//! The fault report — deterministic, greppable, LLM-agent-readable.
//!
//! One fact per line, `key: value`, fixed section order:
//! `error`, `category`, `location`, `scope`, `attachment*`, `cause N`
//! (preorder over [`Fault::iter`], root = `cause 0`), `trace_id`, `advice`,
//! `action`. No ANSI, no wall-clock timestamps — output is diff-stable and
//! snapshot-testable. Sections with no data are omitted entirely.
//!
//! ```text
//! error: [E100] entity not found: 17
//! category: Content (policy: fix the input; retrying unchanged input will fail)
//! location: src/repo.rs:42:10
//! scope: request → load_entity (elapsed 3ms)
//! attachment: attempt=3
//! cause 0: [E100] entity not found: 17
//! cause 1: No such file or directory (os error 2)
//! trace_id: 4f3c9a2b…
//! advice: check the entity table
//! action: fix the input; retrying unchanged input will fail; see `doctor E100`
//! ```

use core::error::request_value;
use core::fmt;

use crate::ErrorCategory;
use crate::errors::{CategoryTag, ErrorCode, lookup_error};
use crate::exn::{Attachment, Fault, Frame, Placement};

/// The frame's registry code, provided through `Error::provide`. The
/// fault's shared-error wrappers forward `provide()`, so codes stay
/// readable through `&dyn Error`.
fn frame_code(frame: &Frame) -> Option<ErrorCode> {
    request_value::<ErrorCode>(frame.error())
}

/// The frame's category: a provided [`CategoryTag`] first, then a registry
/// lookup by code. `None` when neither source knows one.
fn frame_category(frame: &Frame) -> Option<ErrorCategory> {
    if let Some(tag) = request_value::<CategoryTag>(frame.error()) {
        return Some(tag.0);
    }
    frame_code(frame)
        .and_then(|code| lookup_error(code.0))
        .map(|entry| entry.category)
}

/// True when `key` names a built-in attachment rendered as its own report
/// line (`scope:` from `scope_path` / `scope_elapsed_ms`, `trace_id:`) —
/// these never appear as `attachment:` lines.
fn is_reserved_key(key: Option<&'static str>) -> bool {
    matches!(key, Some("scope_path" | "scope_elapsed_ms" | "trace_id"))
}

/// The first root-frame attachment with `key`, if any.
fn find_keyed<'a>(frame: &'a Frame, key: &str) -> Option<&'a Attachment> {
    frame
        .attachments()
        .iter()
        .find(|a| matches!(a.key(), Some(k) if k == key))
}

/// The registry advice for a code, when the code is registered with advice.
fn advice_for(code: Option<ErrorCode>) -> Option<&'static str> {
    code.and_then(|c| lookup_error(c.0))
        .and_then(|entry| entry.advice)
}

/// Stream the report into `w` — the single rendering path behind
/// [`render_report`] and [`report_display`].
fn write_report<E: Send + Sync + 'static>(
    w: &mut impl fmt::Write,
    fault: &Fault<E>,
) -> fmt::Result {
    let root = fault.frame();
    let code = frame_code(root);
    let category = frame_category(root);

    // error: [CODE] <root Display incl. context suffix>
    w.write_str("error: ")?;
    if let Some(code) = code {
        write!(w, "[{}] ", code.0)?;
    }
    writeln!(w, "{fault}")?;

    // category: Name (policy: advice line)
    if let Some(category) = category {
        writeln!(
            w,
            "category: {category} (policy: {})",
            category.policy().advice_line()
        )?;
    }

    // location: file:line:column
    let loc = root.location();
    writeln!(
        w,
        "location: {}:{}:{}",
        loc.file(),
        loc.line(),
        loc.column()
    )?;

    // scope: outer → … → leaf (elapsed Nms)
    if let Some(path) = find_keyed(root, "scope_path") {
        write!(w, "scope: {}", path.display())?;
        if let Some(ms) = find_keyed(root, "scope_elapsed_ms") {
            write!(w, " (elapsed {}ms)", ms.display())?;
        }
        w.write_char('\n')?;
    }

    // attachment: key=value  (or bare value for unkeyed) — root frame,
    // Inline placement, reserved keys excluded.
    for attachment in root.attachments() {
        if attachment.placement() != Placement::Inline || is_reserved_key(attachment.key()) {
            continue;
        }
        match attachment.key() {
            Some(key) => writeln!(w, "attachment: {key}={}", attachment.display())?,
            None => writeln!(w, "attachment: {}", attachment.display())?,
        }
    }

    // cause N: [CODE] <frame Display incl. context suffix> — preorder,
    // root first (the `error:` line is the root's headed form).
    for (n, frame) in fault.iter().enumerate() {
        write!(w, "cause {n}: ")?;
        if let Some(code) = frame_code(&frame) {
            write!(w, "[{}] ", code.0)?;
        }
        writeln!(w, "{frame}")?;
    }

    // trace_id: <id> — grep key across logs and spans.
    if let Some(trace) = find_keyed(root, "trace_id") {
        writeln!(w, "trace_id: {}", trace.display())?;
    }

    // advice: <registry advice for the code>
    if let Some(advice) = advice_for(code) {
        writeln!(w, "advice: {advice}")?;
    }

    // action: <policy line>; see `doctor CODE` — the agent's next step.
    if let Some(category) = category {
        let policy = category.policy();
        match code {
            Some(code) => writeln!(
                w,
                "action: {}; see `doctor {}`",
                policy.advice_line(),
                code.0
            )?,
            None => writeln!(w, "action: {}", policy.advice_line())?,
        }
    }
    Ok(())
}

/// Render a fault as the standard report text block (see the module
/// docs for the format). Ends with a trailing newline.
#[must_use]
pub fn render_report<E: Send + Sync + 'static>(fault: &Fault<E>) -> String {
    report_display(fault).to_string()
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
        write_report(f, self.fault)
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

/// JSON form of the report. Versioned schema: the object ALWAYS carries
/// `"schema": 1`; consumers must ignore unknown fields. Mirrors the text
/// sections — `error` (with `code`/`category`/`policy`/`advice` when
/// known), `location`, `scope`, `attachments`, `causes` (preorder, root
/// first), `trace_id`, `action`; absent data omits the field.
#[cfg(feature = "serde")]
#[must_use]
pub fn render_report_json<E: Send + Sync + 'static>(fault: &Fault<E>) -> String {
    let root = fault.frame();
    let code = frame_code(root);
    let category = frame_category(root);

    let mut out = String::from("{\"schema\": 1");

    // error: the root, headed form.
    out.push_str(", \"error\": {");
    let mut first = true;
    if let Some(code) = code {
        push_json_field(&mut out, &mut first, "code", code.0);
    }
    push_json_field(&mut out, &mut first, "message", &fault.to_string());
    if let Some(category) = category {
        push_json_field(&mut out, &mut first, "category", category.as_ref());
        push_json_field(
            &mut out,
            &mut first,
            "policy",
            category.policy().advice_line(),
        );
    }
    if let Some(advice) = advice_for(code) {
        push_json_field(&mut out, &mut first, "advice", advice);
    }
    out.push('}');

    let loc = root.location();
    let mut first = false;
    push_json_field(
        &mut out,
        &mut first,
        "location",
        &format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
    );

    if let Some(path) = find_keyed(root, "scope_path") {
        push_json_field(&mut out, &mut first, "scope", path.display());
        if let Some(ms) = find_keyed(root, "scope_elapsed_ms") {
            push_json_field(&mut out, &mut first, "scope_elapsed_ms", ms.display());
        }
    }

    let attachments: Vec<&Attachment> = root
        .attachments()
        .iter()
        .filter(|a| a.placement() == Placement::Inline && !is_reserved_key(a.key()))
        .collect();
    if !attachments.is_empty() {
        out.push_str(", \"attachments\": [");
        for (i, attachment) in attachments.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push('{');
            let mut first = true;
            if let Some(key) = attachment.key() {
                push_json_field(&mut out, &mut first, "key", key);
            }
            push_json_field(&mut out, &mut first, "value", attachment.display());
            out.push('}');
        }
        out.push(']');
    }

    out.push_str(", \"causes\": [");
    for (i, frame) in fault.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('{');
        let mut first = true;
        if let Some(code) = frame_code(&frame) {
            push_json_field(&mut out, &mut first, "code", code.0);
        }
        push_json_field(&mut out, &mut first, "message", &frame.to_string());
        out.push('}');
    }
    out.push(']');

    if let Some(trace) = find_keyed(root, "trace_id") {
        push_json_field(&mut out, &mut first, "trace_id", trace.display());
    }
    if let Some(category) = category {
        let action = match code {
            Some(code) => format!(
                "{}; see `doctor {}`",
                category.policy().advice_line(),
                code.0
            ),
            None => category.policy().advice_line().to_string(),
        };
        push_json_field(&mut out, &mut first, "action", &action);
    }

    out.push('}');
    out
}
