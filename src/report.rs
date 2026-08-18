//! The fault report — deterministic, greppable, LLM-agent-readable.
//!
//! One fact per line, `key: value`, fixed section order:
//! `report` (format marker), `error`, `category`, `location`, `scope`,
//! `attachment*`, cause lines (preorder over the tree, root = `cause 0`),
//! `trace_id`, `fingerprint`, `advice`, `action`, `hint`, `appendix*`.
//! No ANSI, no wall-clock timestamps — output is diff-stable and
//! snapshot-testable. Sections with no data are omitted entirely.
//!
//! Two invariants make the text form machine-safe:
//!
//! - **Line hygiene** — every interpolated value passes through [`Line`],
//!   which escapes `\n`/`\r`. Data-controlled text can never forge a report
//!   line (a message containing `\naction: …` renders as a literal `\n`).
//! - **Typed causes** — every cause line carries the frame's type name,
//!   location, and edge [`FrameKind`](crate::exn::FrameKind) label
//!   (`cause` / `original` / `attempt` / `failure`), so readers can tell
//!   "the underlying OS error" from "retry attempt 2".
//!
//! The report body is a PURE function of the fault. Volatile process state
//! (occurrence count, thread name, uptime) rides in the hook's log-event
//! envelope, never here — see `hook::default_hook`.
//!
//! ```text
//! report: fast-observe/1
//! error: [E100] [my_crate::Repo::NotFound] entity not found: 17
//! category: Content (policy: fix the input; retrying unchanged input will fail)
//! location: src/repo.rs:42:10
//! scope: request → load_entity (elapsed 3ms)
//! attachment: attempt=3
//! cause 0: [E100] [my_crate::Repo::NotFound] entity not found: 17, at src/repo.rs:42:10
//! cause 1: [std::io::Error] No such file or directory (os error 2), at src/repo.rs:42:10
//! trace_id: 4f3c9a2b…
//! fingerprint: 9f86d081
//! advice: check the entity table
//! action: fix the input; retrying unchanged input will fail
//! hint: run `doctor E100`
//! ```

use core::error::request_value;
use core::fmt;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use crate::ErrorCategory;
use crate::errors::{ErrorCode, lookup_error};
use crate::exn::{Attachment, BuiltinKey, Fault, Frame, FrameKind, Placement, frame_category};

/// Text-report format marker, rendered as the first line
/// (`report: fast-observe/1`) so consumers can pattern-match the format
/// before parsing. Bump when the line set or order changes.
pub const REPORT_FORMAT: &str = "fast-observe/1";

/// JSON schema version — the object ALWAYS carries `"schema": 2`. v2 vs v1:
/// `location` is a `{file, line, column}` object with real numbers,
/// `scope_elapsed_ms` is a number, causes are objects with
/// `type`/`location`/`kind`, and `fingerprint`/`hint`/`report` were added.
/// Consumers must ignore unknown fields.
#[cfg(feature = "serde")]
pub const REPORT_SCHEMA_VERSION: u32 = 2;

/// Max lines rendered per Appendix attachment (the backtrace is the big
/// one) — an uncapped backtrace is a token-budget attack on the LLM context
/// this report is designed for.
const APPENDIX_MAX_LINES: usize = 32;

/// Max chars of the optional `source:` snippet line.
const SOURCE_LINE_MAX_CHARS: usize = 200;

/// Env knob (`OBSERVE_REPORT_SOURCE=1|true`): attach the source line at the
/// root location to the report. Off by default — it costs a filesystem read
/// per rendered report (cold path) and snapshot stability is unaffected
/// (off in tests).
static REPORT_SOURCE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    crate::config::env_enum(
        crate::env_vars::OBSERVE_REPORT_SOURCE,
        |name| match name {
            "1" | "true" => Some(true),
            "0" | "false" | "" => Some(false),
            _ => None,
        },
        false,
        "1|true|0|false",
    )
});

/// The source line at `location` (1-based line number), trimmed and
/// char-capped: from the in-memory source registry first, then the
/// filesystem. `None` when the knob is off or the source is unreadable.
fn source_line_at(location: &std::panic::Location) -> Option<String> {
    if !*REPORT_SOURCE {
        return None;
    }
    let file = location.file();
    let contents =
        crate::diagnostic::registered_source(file).or_else(|| fs_err::read_to_string(file).ok())?;
    let line = contents
        .lines()
        .nth(location.line() as usize - 1)?
        .trim_end();
    let capped: String = line.chars().take(SOURCE_LINE_MAX_CHARS).collect();
    Some(capped)
}

// ── Line — the one-fact-per-line contract as a type ────────────────────────

/// A report fragment guaranteed free of raw newlines/carriage returns.
/// Constructible only through [`Line::new`], which escapes `\n` and `\r` —
/// so no interpolated value can forge a report line. Every dynamic string
/// the text renderer emits goes through this type.
struct Line<'a>(Cow<'a, str>);

impl<'a> Line<'a> {
    /// Escape `\n`/`\r` (the report's line delimiters). Allocation-free when
    /// the input is already clean — the common case.
    fn new(s: &'a str) -> Self {
        if s.contains(['\n', '\r']) {
            Self(Cow::Owned(s.replace('\n', "\\n").replace('\r', "\\r")))
        } else {
            Self(Cow::Borrowed(s))
        }
    }
}

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── ReportData — one collection pass, two renderers ────────────────────────

/// The concrete type name when KNOWN. Frames captured from a `&dyn Error`
/// (boxed roots, stringified `source()` chains) store the trait-object name
/// (`dyn core::error::Error + …`) — the concrete type is unrecoverable
/// there, and displaying the trait-object name would be noise. `None` =
/// type erased at capture.
fn concrete_type_name(type_name: &'static str) -> Option<&'static str> {
    (!type_name.starts_with("dyn ")).then_some(type_name)
}

/// One preorder cause row (root = index 0).
struct CauseRow {
    code: Option<ErrorCode>,
    /// [`None`] when the concrete type was erased at capture.
    type_name: Option<&'static str>,
    /// Frame Display (message + context suffix), unescaped.
    message: String,
    location: &'static std::panic::Location<'static>,
    /// Edge kind to the parent; `None` on the root.
    kind: Option<FrameKind>,
}

/// Everything a report render needs, collected from the root frame in ONE
/// pass. The text and JSON forms are pure renderers over this — they cannot
/// drift, because there is only one traversal.
struct ReportData {
    code: Option<ErrorCode>,
    category: Option<ErrorCategory>,
    /// [`None`] when the concrete type was erased at capture (boxed roots).
    type_name: Option<&'static str>,
    /// Root frame Display (message + context suffix), unescaped.
    message: String,
    location: &'static std::panic::Location<'static>,
    /// The source line at the root location (`OBSERVE_REPORT_SOURCE`).
    source_line: Option<String>,
    /// `(path, elapsed_ms)` from the built-in scope attachments.
    scope: Option<(String, Option<String>)>,
    /// Typed elapsed milliseconds (for the JSON numeric form).
    scope_elapsed_ms: Option<u128>,
    /// Root-frame Inline attachments minus reserved keys.
    attachments: Vec<(Option<&'static str>, String)>,
    /// Preorder, root first (`causes[0]` IS the root, like text `cause 0`).
    causes: Vec<CauseRow>,
    trace_id: Option<String>,
    advice: Option<&'static str>,
    /// Policy line or variant `#[action]` text (no doctor suffix).
    action: Option<String>,
    /// Runnable next step — ``run `doctor CODE` `` when coded.
    hint: Option<String>,
    /// Stable hash of (code|type, root location, root-cause type) —
    /// dedupes "same error, new log line" across runs.
    fingerprint: String,
    /// Root-frame Appendix attachments `(key, display)` — span trail,
    /// backtrace. Opaque/Hidden placements never appear.
    appendix: Vec<(&'static str, String)>,
}

/// The frame's registry code, provided through `Error::provide`. The
/// fault's shared-error wrappers forward `provide()`, so codes stay
/// readable through `&dyn Error`.
fn frame_code(frame: &Frame) -> Option<ErrorCode> {
    request_value::<ErrorCode>(frame.error())
}

/// The first root-frame attachment with `key`, if any.
fn find_keyed(frame: &Frame, key: BuiltinKey) -> Option<&Attachment> {
    frame
        .attachments()
        .iter()
        .find(|a| matches!(a.key(), Some(k) if k == key.as_str()))
}

/// True when `key` names a built-in attachment rendered as its own report
/// line (`scope:` from `scope_path` / `scope_elapsed_ms`, `trace_id:`) —
/// these never appear as `attachment:` lines.
fn is_reserved_key(key: Option<&'static str>) -> bool {
    [
        BuiltinKey::ScopePath,
        BuiltinKey::ScopeElapsedMs,
        BuiltinKey::TraceId,
    ]
    .into_iter()
    .any(|k| Some(k.as_str()) == key)
}

/// The registry advice for a code, when the code is registered with advice.
fn advice_for(code: Option<ErrorCode>) -> Option<&'static str> {
    code.and_then(|c| lookup_error(c.0))
        .and_then(|entry| entry.advice)
}

/// The registry action text for a code, when the code is registered with a
/// variant-specific `#[action]` override.
fn action_for(code: Option<ErrorCode>) -> Option<&'static str> {
    code.and_then(|c| lookup_error(c.0))
        .and_then(|entry| entry.action)
}

/// What the `action:` line carries: the variant-specific `#[action]` text
/// when the registry has one, else the category's generic policy line.
fn base_action(category: ErrorCategory, code: Option<ErrorCode>) -> &'static str {
    action_for(code).unwrap_or_else(|| category.policy().advice_line())
}

/// Stable fingerprint: hash of the error identity (code when coded, else
/// root type), the root location, and the root cause's type. Deliberately
/// excludes messages/context — those vary per occurrence; the fingerprint
/// answers "have we seen THIS failure site before".
/// `DefaultHasher::new()` uses fixed keys — deterministic across runs.
#[allow(
    clippy::cast_possible_truncation,
    reason = "deliberate: the fingerprint is the low 32 bits of the hash"
)]
fn fingerprint_of(root: &Frame) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match frame_code(root) {
        Some(code) => code.0.hash(&mut hasher),
        None => root.type_name().hash(&mut hasher),
    }
    let loc = root.location();
    (loc.file(), loc.line(), loc.column()).hash(&mut hasher);
    // Root cause type: deepest first-branch frame.
    let mut cause = root;
    while let Some((_, child)) = cause.child_edges().next() {
        cause = child;
    }
    cause.type_name().hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:08x}", hash as u32)
}

impl ReportData {
    /// The single collection pass over the fault tree.
    fn collect(root: &Frame) -> Self {
        let code = frame_code(root);
        let category = frame_category(root);

        let scope = find_keyed(root, BuiltinKey::ScopePath).map(|path| {
            (
                path.display().to_string(),
                find_keyed(root, BuiltinKey::ScopeElapsedMs).map(|ms| ms.display().to_string()),
            )
        });
        let scope_elapsed_ms = find_keyed(root, BuiltinKey::ScopeElapsedMs)
            .and_then(Attachment::downcast::<u128>)
            .copied();

        let attachments = root
            .attachments()
            .iter()
            .filter(|a| a.placement() == Placement::Inline && !is_reserved_key(a.key()))
            .map(|a| (a.key(), a.display().to_string()))
            .collect();

        // Preorder over the tree — explicit stack, root first.
        let mut causes = Vec::new();
        let mut stack: Vec<(Option<FrameKind>, &Frame)> = vec![(None, root)];
        while let Some((kind, frame)) = stack.pop() {
            causes.push(CauseRow {
                code: frame_code(frame),
                type_name: concrete_type_name(frame.type_name()),
                message: frame.to_string(),
                location: frame.location(),
                kind,
            });
            for (kind, child) in frame.child_edges().rev() {
                stack.push((Some(kind), child));
            }
        }

        let appendix = root
            .attachments()
            .iter()
            .filter(|a| a.placement() == Placement::Appendix)
            .filter_map(|a| a.key().map(|k| (k, a.display().to_string())))
            .collect();

        let action = category.map(|c| base_action(c, code).to_string());
        let hint = code.map(|c| format!("run `doctor {}`", c.0));

        Self {
            code,
            category,
            type_name: concrete_type_name(root.type_name()),
            message: root.to_string(),
            location: root.location(),
            source_line: source_line_at(root.location()),
            scope,
            scope_elapsed_ms,
            attachments,
            causes,
            trace_id: find_keyed(root, BuiltinKey::TraceId).map(|t| t.display().to_string()),
            advice: advice_for(code),
            action,
            hint,
            fingerprint: fingerprint_of(root),
            appendix,
        }
    }
}

// ── Text rendering ─────────────────────────────────────────────────────────

/// The text label for a cause line: `cause` for source-chain frames (and
/// the root), `original` for wrapped-over errors, `attempt` for retry
/// attempts, `failure` for batch merges.
fn cause_label(kind: Option<FrameKind>, n: usize) -> String {
    let word = match kind {
        None | Some(FrameKind::Source) => "cause",
        Some(FrameKind::Wrap) => "original",
        Some(FrameKind::Attempt) => "attempt",
        Some(FrameKind::Batch) => "failure",
    };
    format!("{word} {n}")
}

/// Write one cause row: `cause N: [CODE] [type] message, at file:line:col`
/// (the `[type]` segment is omitted when the concrete type was erased).
fn write_cause_line(w: &mut impl fmt::Write, n: usize, cause: &CauseRow) -> fmt::Result {
    write!(w, "{}: ", cause_label(cause.kind, n))?;
    if let Some(code) = cause.code {
        write!(w, "[{}] ", code.0)?;
    }
    if let Some(type_name) = cause.type_name {
        write!(w, "[{type_name}] ")?;
    }
    write!(w, "{}", Line::new(&cause.message))?;
    let loc = cause.location;
    writeln!(w, ", at {}:{}:{}", loc.file(), loc.line(), loc.column())
}

/// Stream the text report into `w` — the single text path behind
/// [`render_report`], [`render_frame_report`] and [`report_display`].
fn write_report(w: &mut impl fmt::Write, root: &Frame) -> fmt::Result {
    let data = ReportData::collect(root);

    // Format marker first — consumers pattern-match it before parsing.
    writeln!(w, "report: {REPORT_FORMAT}")?;

    // error: [CODE] [type] <root Display incl. context suffix>
    w.write_str("error: ")?;
    if let Some(code) = data.code {
        write!(w, "[{}] ", code.0)?;
    }
    if let Some(type_name) = data.type_name {
        write!(w, "[{type_name}] ")?;
    }
    writeln!(w, "{}", Line::new(&data.message))?;

    // category: Name (policy: advice line)
    if let Some(category) = data.category {
        writeln!(
            w,
            "category: {category} (policy: {})",
            category.policy().advice_line()
        )?;
    }

    // location: file:line:column
    let loc = data.location;
    writeln!(
        w,
        "location: {}:{}:{}",
        loc.file(),
        loc.line(),
        loc.column()
    )?;

    // source: <the line at location> — opt-in via OBSERVE_REPORT_SOURCE.
    if let Some(source) = &data.source_line {
        writeln!(w, "source: {}", Line::new(source))?;
    }

    // scope: outer → … → leaf (elapsed Nms). Prefer the TYPED elapsed
    // value (u128, downcast from the attachment) over its display string —
    // typed-first rendering keeps the two from drifting.
    if let Some((path, elapsed)) = &data.scope {
        write!(w, "scope: {}", Line::new(path))?;
        match data.scope_elapsed_ms {
            Some(ms) => write!(w, " (elapsed {ms}ms)")?,
            None => {
                if let Some(ms) = elapsed {
                    write!(w, " (elapsed {}ms)", Line::new(ms))?;
                }
            }
        }
        w.write_char('\n')?;
    }

    // attachment: key=value  (or bare value for unkeyed). Keys are
    // developer-chosen literals (debug-asserted clean in `with_key`), but
    // release builds still sanitize — belt and braces on the line contract.
    for (key, display) in &data.attachments {
        match key {
            Some(key) => writeln!(w, "attachment: {}={}", Line::new(key), Line::new(display))?,
            None => writeln!(w, "attachment: {}", Line::new(display))?,
        }
    }

    // cause N: [CODE] [type] <frame Display>, at file:line:col — preorder,
    // root first (the `error:` line is the root's headed form).
    for (n, cause) in data.causes.iter().enumerate() {
        write_cause_line(w, n, cause)?;
    }

    // trace_id: <id> — grep key across logs and spans.
    if let Some(trace) = &data.trace_id {
        writeln!(w, "trace_id: {}", Line::new(trace))?;
    }

    // fingerprint: <stable hash> — dedupe across runs.
    writeln!(w, "fingerprint: {}", data.fingerprint)?;

    // advice: <registry advice for the code>
    if let Some(advice) = data.advice {
        writeln!(w, "advice: {advice}")?;
    }

    // action: <policy or #[action] text> — what to DO.
    if let Some(action) = &data.action {
        writeln!(w, "action: {}", Line::new(action))?;
    }

    // hint: run `doctor CODE` — the agent's literal next command.
    if let Some(hint) = &data.hint {
        writeln!(w, "hint: {hint}")?;
    }

    // appendix <key>: — Appendix attachments (span trail, backtrace),
    // line-capped. Multi-line displays indent continuation lines.
    for (key, display) in &data.appendix {
        let mut lines = display.trim_end().lines();
        let mut shown = 0usize;
        let mut first = true;
        for line in lines.by_ref().take(APPENDIX_MAX_LINES) {
            if first {
                writeln!(w, "appendix {key}: {}", Line::new(line))?;
                first = false;
            } else {
                writeln!(w, "  {}", Line::new(line))?;
            }
            shown += 1;
        }
        let remaining = lines.count();
        if remaining > 0 {
            writeln!(w, "  … ({remaining} more lines)")?;
        }
        let _ = shown;
    }
    Ok(())
}

/// Render a fault as the standard report text block (see the module
/// docs for the format). Ends with a trailing newline.
#[must_use]
pub fn render_report<E: Send + Sync + 'static>(fault: &Fault<E>) -> String {
    report_display(fault).to_string()
}

/// Render the report from a root frame alone — used by the default error
/// hook under `OBSERVE_REPORT` (the hook only sees a `&Frame`, not the
/// typed `Fault`).
#[must_use]
pub fn render_frame_report(root: &Frame) -> String {
    let mut out = String::new();
    // Infallible on String.
    let _ = write_report(&mut out, root);
    out
}

/// Zero-allocation streaming form of [`render_report`]: an [`fmt::Display`]
/// that writes the same text straight into any writer.
#[must_use]
pub fn report_display<E: Send + Sync + 'static>(fault: &Fault<E>) -> impl fmt::Display + '_ {
    ReportDisplay { fault }
}

/// Hand-rolled wrapper behind [`report_display`] — streams the report into
/// the formatter without building the report string first.
struct ReportDisplay<'a, E: Send + Sync + 'static> {
    fault: &'a Fault<E>,
}

impl<E: Send + Sync + 'static> fmt::Display for ReportDisplay<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_report(f, self.fault.frame())
    }
}

// ── JSON form (feature `serde`) ────────────────────────────────────────────

/// Append `"value"` to `out`, JSON-escaped. Hand-rolled: `serde_json` is a
/// dev-dependency only, and the report must not gain a serializer for one
/// flat object.
#[cfg(feature = "serde")]
fn push_json_escaped(out: &mut String, value: &str) {
    use fmt::Write as _;
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                // Infallible on String.
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append a `"key": "value"` field, preceded by a comma unless `first`.
#[cfg(feature = "serde")]
fn push_json_field(out: &mut String, first: &mut bool, key: &str, value: &str) {
    if !*first {
        out.push_str(", ");
    }
    *first = false;
    push_json_escaped(out, key);
    out.push_str(": ");
    push_json_escaped(out, value);
}

/// Append a `"key": <number>` field (numbers are not quoted).
#[cfg(feature = "serde")]
fn push_json_number(out: &mut String, first: &mut bool, key: &str, value: impl fmt::Display) {
    use fmt::Write as _;
    if !*first {
        out.push_str(", ");
    }
    *first = false;
    push_json_escaped(out, key);
    out.push_str(": ");
    // Infallible on String.
    let _ = write!(out, "{value}");
}

/// Append a `"location": {"file": …, "line": N, "column": N}` object field.
#[cfg(feature = "serde")]
fn push_json_location(out: &mut String, first: &mut bool, loc: &std::panic::Location) {
    out.push_str(if *first { "" } else { ", " });
    *first = false;
    out.push_str("\"location\": {");
    let mut inner = true;
    push_json_field(out, &mut inner, "file", loc.file());
    push_json_number(out, &mut inner, "line", loc.line());
    push_json_number(out, &mut inner, "column", loc.column());
    out.push('}');
}

/// JSON form of the report (schema [`REPORT_SCHEMA_VERSION`]). Mirrors the
/// text sections — `report`/`error` (with `code`/`type`/`category`/`policy`/
/// `advice` when known), `location` (object, numeric line/column), `scope`
/// (object, numeric `elapsed_ms`), `attachments`, `causes` (preorder, root
/// first; objects with `code`/`type`/`message`/`kind`/`location`),
/// `trace_id`, `fingerprint`, `action`, `hint`, `appendix`; absent data
/// omits the field.
#[cfg(feature = "serde")]
#[must_use]
pub fn render_report_json<E: Send + Sync + 'static>(fault: &Fault<E>) -> String {
    render_frame_report_json(fault.frame())
}

/// JSON form of a report from a root frame alone — the hook path for
/// `OBSERVE_REPORT=json`.
#[cfg(feature = "serde")]
#[must_use]
pub fn render_frame_report_json(root: &Frame) -> String {
    let data = ReportData::collect(root);
    let mut out = String::from("{");
    let mut first = true;

    push_json_number(&mut out, &mut first, "schema", REPORT_SCHEMA_VERSION);
    push_json_field(&mut out, &mut first, "report", REPORT_FORMAT);

    // error: the root, headed form.
    out.push_str(", \"error\": {");
    let mut inner = true;
    if let Some(code) = data.code {
        push_json_field(&mut out, &mut inner, "code", code.0);
    }
    if let Some(type_name) = data.type_name {
        push_json_field(&mut out, &mut inner, "type", type_name);
    }
    push_json_field(&mut out, &mut inner, "message", &data.message);
    if let Some(category) = data.category {
        push_json_field(&mut out, &mut inner, "category", category.as_ref());
        push_json_field(
            &mut out,
            &mut inner,
            "policy",
            category.policy().advice_line(),
        );
    }
    if let Some(advice) = data.advice {
        push_json_field(&mut out, &mut inner, "advice", advice);
    }
    out.push('}');

    push_json_location(&mut out, &mut first, data.location);
    if let Some(source) = &data.source_line {
        push_json_field(&mut out, &mut first, "source", source);
    }

    if let Some((path, _)) = &data.scope {
        out.push_str(", \"scope\": {");
        let mut inner = true;
        push_json_field(&mut out, &mut inner, "path", path);
        if let Some(ms) = data.scope_elapsed_ms {
            push_json_number(&mut out, &mut inner, "elapsed_ms", ms);
        }
        out.push('}');
    }

    if !data.attachments.is_empty() {
        out.push_str(", \"attachments\": [");
        for (i, (key, display)) in data.attachments.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push('{');
            let mut inner = true;
            if let Some(key) = key {
                push_json_field(&mut out, &mut inner, "key", key);
            }
            push_json_field(&mut out, &mut inner, "value", display);
            out.push('}');
        }
        out.push(']');
    }

    out.push_str(", \"causes\": [");
    for (i, cause) in data.causes.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('{');
        let mut inner = true;
        if let Some(code) = cause.code {
            push_json_field(&mut out, &mut inner, "code", code.0);
        }
        if let Some(type_name) = cause.type_name {
            push_json_field(&mut out, &mut inner, "type", type_name);
        }
        push_json_field(&mut out, &mut inner, "message", &cause.message);
        if let Some(kind) = cause.kind {
            push_json_field(&mut out, &mut inner, "kind", kind.as_ref());
        }
        push_json_location(&mut out, &mut inner, cause.location);
        out.push('}');
    }
    out.push(']');

    if let Some(trace) = &data.trace_id {
        push_json_field(&mut out, &mut first, "trace_id", trace);
    }
    push_json_field(&mut out, &mut first, "fingerprint", &data.fingerprint);
    if let Some(action) = &data.action {
        push_json_field(&mut out, &mut first, "action", action);
    }
    if let Some(hint) = &data.hint {
        push_json_field(&mut out, &mut first, "hint", hint);
    }

    if !data.appendix.is_empty() {
        out.push_str(", \"appendix\": {");
        let mut inner = true;
        for (key, display) in &data.appendix {
            push_json_field(&mut out, &mut inner, key, display);
        }
        out.push('}');
    }

    out.push('}');
    out
}
