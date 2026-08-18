//! Diagnostics — compile-time errors (not exceptions).
//!
//! Used by compilers and mod/asset loaders. Typed errors with
//! labeled source spans + severity, rendered with ariadne.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

use ariadne::{Color, FnCache, Label, Report, ReportKind};
use camino::Utf8PathBuf;

/// Error severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::AsRefStr)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Severity {
    /// A hard failure — the primary report kind.
    Error,
    /// A soft problem — renders with the warning color.
    Warning,
    /// Advisory output.
    Info,
}

/// A source span — file + byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceSpan {
    /// Path to the source file (`camino` serde support rides on its
    /// `serde1` feature, pulled in by this crate's `serde` feature).
    pub file: Utf8PathBuf,
    /// Start byte offset, inclusive.
    pub start: usize,
    /// End byte offset, exclusive.
    pub end: usize,
}

/// A labeled source span. Exactly one label per diagnostic should be `primary`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LabelSpan {
    /// Spanned region.
    pub span: SourceSpan,
    /// Label message; defaults to the severity name when None.
    pub message: Option<String>,
    /// Primary labels get the severity color; secondary labels render in
    /// neutral gray.
    pub primary: bool,
}

/// A diagnostic — a compile-time error with labeled spans + advice.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    /// Stable error code. Owned `String` (not `&'static str`) so the type can
    /// round-trip through serde — `&'static str` cannot be deserialized from
    /// non-`'static` input.
    pub code: String,
    /// Error/warning/info — drives the report kind and label colors.
    pub severity: Severity,
    /// The human-readable error message.
    pub message: String,
    /// Labeled source spans; empty renders a synthetic zero-width
    /// `<unknown>` location.
    #[cfg_attr(feature = "serde", serde(default))]
    pub labels: Vec<LabelSpan>,
    /// Prescriptive advice — rendered as a report note, and the same slot
    /// the fault path uses for the registry entry's advice.
    pub advice: Option<String>,
}

impl Diagnostic {
    /// An error diagnostic with code + message; no spans or advice.
    #[must_use]
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, message, Severity::Error)
    }

    /// A warning diagnostic with code + message; no spans or advice.
    #[must_use]
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, message, Severity::Warning)
    }

    /// An info diagnostic with code + message; no spans or advice.
    #[must_use]
    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, message, Severity::Info)
    }

    fn new(code: &str, message: impl Into<String>, severity: Severity) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message: message.into(),
            labels: Vec::new(),
            advice: None,
        }
    }

    /// Attach a primary source span (one-label convenience).
    #[must_use]
    pub fn with_source(mut self, span: SourceSpan) -> Self {
        self.labels.push(LabelSpan {
            span,
            message: None,
            primary: true,
        });
        self
    }

    /// Attach a secondary labeled span. Use [`Diagnostic::with_label_primary`]
    /// to add another primary label.
    #[must_use]
    pub fn with_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(LabelSpan {
            span,
            message: Some(message.into()),
            primary: false,
        });
        self
    }

    /// Attach an additional primary labeled span.
    #[must_use]
    pub fn with_label_primary(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(LabelSpan {
            span,
            message: Some(message.into()),
            primary: true,
        });
        self
    }

    /// Attach advice (rendered as a report note).
    #[must_use]
    pub fn with_advice(mut self, advice: impl Into<String>) -> Self {
        self.advice = Some(advice.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Anything that can become a [`Diagnostic`] — the bridge between runtime
/// faults and compile-time-style rendered diagnostics. One renderer (the
/// ariadne pipeline in `build_report`) serves both worlds: coded runtime
/// errors render exactly like compile-time diagnostics.
///
/// Deliberately NOT sealed — implement it for your own error/report types
/// to render them through the same pipeline (e.g. a domain type carrying
/// spans that is not a [`Fault`](crate::exn::Fault)).
#[diagnostic::on_unimplemented(
    message = "implement `ToDiagnostic` to render this type as a `Diagnostic`",
    note = "anyhow/eyre/error-stack boundaries provide conversions via the `compat-*` features"
)]
pub trait ToDiagnostic {
    /// Convert to a renderable diagnostic.
    fn to_diagnostic(&self) -> Diagnostic;
}

/// Identity conversion — generic code can accept either a [`Diagnostic`] or
/// a [`Fault`](crate::exn::Fault) (or any user type) through the same bound.
impl ToDiagnostic for Diagnostic {
    fn to_diagnostic(&self) -> Diagnostic {
        self.clone()
    }
}

/// Convert a runtime fault to a diagnostic, reading the ROOT frame only —
/// the cause chain is not folded in (a `Diagnostic` has one message + one
/// advice; overloading them with chain frames would garble both).
///
/// Mapping:
/// - code: the [`ErrorCode`](crate::errors::ErrorCode) the root error
///   provides through `Error::provide` (`error!`-generated types do this
///   automatically). Uncoded faults have no stable code, so the code slot
///   carries the error's short type name — see `uncoded_code` for the
///   heuristic. `Diagnostic.code` is required non-empty and the type name
///   is the most informative stable label available.
/// - message: the root error's bare `Display`
///   ([`Frame::error`](crate::exn::Frame::error)) — NOT the frame's
///   `Display`, which appends the context in parens.
/// - advice: the registry entry's advice for the code, else the root
///   frame's [`Context`](crate::exn::Context) when one is attached.
/// - severity: [`Severity::Error`]; no labels (faults carry no source
///   spans — rendering uses the synthetic zero-width `<unknown>` source).
impl<E: Send + Sync + 'static> ToDiagnostic for crate::exn::Fault<E> {
    fn to_diagnostic(&self) -> Diagnostic {
        let frame = self.frame();
        let provided = core::error::request_value::<crate::errors::ErrorCode>(frame.error());
        let code = provided.map_or_else(|| uncoded_code(self, frame), |code| code.0.to_string());
        let advice = provided
            .and_then(|code| crate::errors::lookup_error(code.0))
            .and_then(|entry| entry.advice.map(str::to_string))
            .or_else(|| {
                let context = frame.context();
                (!matches!(context, crate::exn::Context::None)).then(|| context.to_string())
            });
        let mut diag = Diagnostic::error(&code, frame.error().to_string());
        diag.advice = advice;
        diag
    }
}

/// Code slot for an uncoded fault: the error's short type name.
///
/// Typed faults (`Fault::new(MyError)`) store `type_name::<E>()` on the
/// root frame — the last `::` segment is the short name. Boxed faults
/// (`Fault<BoxError>`, e.g. `Fault::from("boom")`) erase the concrete type
/// at capture: the frame's `type_name` is the `dyn Error + Send + Sync`
/// trait-object name, and `type_name_of_val` on a `&dyn Error` yields the
/// same (no vtable-based concrete lookup). The concrete name is recovered
/// best-effort from the real error's `Debug`, reached through the fault's
/// typed deref (the root frame's own error is a delegating wrapper whose
/// `Debug` starts with the WRAPPER name): derived `Debug` output starts
/// with the type name, so the leading identifier run is the short name
/// (e.g. `InternalError("boom")` → `InternalError`). When recovery fails
/// (a hand-written `Debug` not starting with the type name), falls back to
/// `"Error"`.
fn uncoded_code<E: Send + Sync + 'static>(
    fault: &crate::exn::Fault<E>,
    frame: &crate::exn::Frame,
) -> String {
    let type_name = frame.type_name();
    if !type_name.starts_with("dyn ") {
        return type_name
            .rsplit("::")
            .next()
            .unwrap_or(type_name)
            .to_string();
    }
    // Boxed fault: deref to the typed error (`E` = `BoxError` here) and
    // recover the concrete error's name from its `Debug` leading identifier.
    let typed: &E = fault;
    if let Some(boxed) = (typed as &dyn std::any::Any).downcast_ref::<crate::BoxError>() {
        let real: &(dyn std::error::Error + Send + Sync + 'static) = &**boxed;
        let debug = format!("{real:?}");
        let ident: String = debug
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !ident.is_empty() {
            return ident;
        }
    }
    "Error".to_string()
}

/// Render anything [`ToDiagnostic`] through the ariadne pipeline, returned
/// as a colorless `String` — the same output [`render_diagnostic`] produces,
/// for any convertible type.
#[must_use]
pub fn render_any(x: &impl ToDiagnostic) -> String {
    render_diagnostic(&x.to_diagnostic())
}

/// Render anything [`ToDiagnostic`] through the ariadne pipeline to stderr,
/// with the same tty/`NO_COLOR`/`TERM` color discipline as
/// [`eprint_diagnostic`].
pub fn eprint_any(x: &impl ToDiagnostic) {
    eprint_diagnostic(&x.to_diagnostic());
}

/// In-memory sources registered via [`register_source`], consulted before
/// the filesystem when rendering spans.
static SOURCES: LazyLock<parking_lot::RwLock<HashMap<String, Arc<str>>>> =
    LazyLock::new(|| parking_lot::RwLock::new(HashMap::new()));

/// Register an in-memory source (generated code, embedded assets, virtual
/// paths). Consulted before the filesystem when rendering spans.
pub fn register_source(name: impl Into<String>, contents: impl Into<String>) {
    SOURCES.write().insert(name.into(), contents.into().into());
}

/// Look up a registered in-memory source (crate-internal — the report's
/// source-snippet line consults the same store before the filesystem).
pub(crate) fn registered_source(name: &str) -> Option<String> {
    SOURCES.read().get(name).map(ToString::to_string)
}

/// color decision: `OBSERVE_COLOR=always|never` overrides; `auto` (default)
/// is tty && !`NO_COLOR` && TERM != "dumb"
fn should_color(is_tty: bool, no_color_env: Option<&str>, term_env: Option<&str>) -> bool {
    match crate::config::color_mode() {
        crate::config::ColorMode::Always => true,
        crate::config::ColorMode::Never => false,
        crate::config::ColorMode::Auto => {
            is_tty && no_color_env.is_none() && term_env != Some("dumb")
        }
    }
}

/// Build the ariadne report for a diagnostic.
///
/// Report message is `[<code>] <message>`; severity drives the report kind
/// and primary-label color. All labels are attached: primary labels get the
/// severity color, secondary labels render in neutral gray (fixed 244) so
/// the primary span stands out; a label with no message uses the severity
/// name. When the code is registered in `crate::errors::ERROR_REGISTRY`,
/// the registry entry renders as a note; `advice` renders as its own note.
/// When no span is attached, a synthetic zero-width source is used.
fn build_report(
    diag: &Diagnostic,
    color: bool,
) -> Report<'static, (String, std::ops::Range<usize>)> {
    let (kind, label_color) = match diag.severity {
        Severity::Error => (ReportKind::Error, Color::Red),
        Severity::Warning => (ReportKind::Warning, Color::Yellow),
        Severity::Info => (ReportKind::Advice, Color::Cyan),
    };
    // ariadne asserts `start <= end` on the report anchor and every label;
    // reversed spans are clamped to (min, max), never panicked on.
    let clamped = |start: usize, end: usize| start.min(end)..start.max(end);
    // Anchor the report at the first primary label (else the first label,
    // else a synthetic zero-width source).
    let anchor = diag
        .labels
        .iter()
        .find(|l| l.primary)
        .or_else(|| diag.labels.first());
    let (file, range) = anchor.map_or_else(
        || ("<unknown>".to_string(), 0..0),
        |l| (l.span.file.to_string(), clamped(l.span.start, l.span.end)),
    );

    let mut builder = Report::build(kind, (file.clone(), range.clone()))
        .with_config(ariadne::Config::default().with_color(color))
        .with_message(format!("[{}] {}", diag.code, diag.message));

    if diag.labels.is_empty() {
        builder = builder.with_label(
            Label::new((file, range))
                .with_message(diag.severity.as_ref())
                .with_color(label_color),
        );
    } else {
        for label in &diag.labels {
            let message = label
                .message
                .clone()
                .unwrap_or_else(|| diag.severity.as_ref().to_string());
            builder = builder.with_label(
                Label::new((
                    label.span.file.to_string(),
                    clamped(label.span.start, label.span.end),
                ))
                .with_message(message)
                .with_color(if label.primary {
                    label_color
                } else {
                    Color::Fixed(244)
                }),
            );
        }
    }

    if let Some(entry) = crate::errors::lookup_error(&diag.code) {
        builder = builder.with_note(format!(
            "{} [{}] — {} (category: {})",
            entry.name, entry.code, entry.display, entry.category
        ));
    }
    if let Some(advice) = &diag.advice {
        builder = builder.with_note(advice);
    }
    builder.finish()
}

/// Render a `Diagnostic` as an ariadne report, returned as a `String`
/// (colors disabled — safe to embed in logs, files, and test snapshots).
///
/// Falls back to a plain `[<code>] <message>` line if ariadne fails to
/// render (e.g. unreadable source file).
#[must_use]
pub fn render_diagnostic(diag: &Diagnostic) -> String {
    let mut buf = Vec::new();
    if let Err(e) = build_report(diag, false).write(source_cache(), &mut buf) {
        log::error!(target: crate::log_targets::DIAGNOSTIC, "failed to render diagnostic: {e}");
        return format!("[{}] {}", diag.code, diag.message);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Render a `Diagnostic` as an ariadne report to stderr. Colors are enabled
/// only when stderr is a tty, `NO_COLOR` is unset (any value, even empty,
/// disables per no-color.org), and `TERM` is not `dumb`.
pub fn eprint_diagnostic(diag: &Diagnostic) {
    let color = should_color(
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
        std::env::var_os("NO_COLOR").is_some().then_some("1"),
        std::env::var("TERM").ok().as_deref(),
    );
    if let Err(e) = build_report(diag, color).eprint(source_cache()) {
        log::error!(target: crate::log_targets::DIAGNOSTIC, "failed to render diagnostic: {e}");
    }
}

/// Build a `FnCache` that resolves sources for ariadne rendering: the
/// in-memory [`SOURCES`] store (exact name match) first, then the
/// filesystem.
fn source_cache() -> FnCache<String, impl FnMut(&String) -> Result<String, String>, String> {
    FnCache::new(|id: &String| {
        if let Some(src) = SOURCES.read().get(id) {
            return Ok(src.to_string());
        }
        // Propagate read failures — ariadne renders the error instead of a
        // silently empty source snippet.
        fs_err::read_to_string(id).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::should_color;

    #[test]
    fn should_color_tty_no_no_color_with_term() {
        assert!(should_color(true, None, Some("xterm")));
    }

    #[test]
    fn should_color_no_color_disables_even_empty() {
        assert!(!should_color(true, Some("1"), Some("xterm")));
        assert!(!should_color(true, Some(""), Some("xterm")));
    }

    #[test]
    fn should_color_term_dumb_disables() {
        assert!(!should_color(true, None, Some("dumb")));
    }

    #[test]
    fn should_color_non_tty_disables() {
        assert!(!should_color(false, None, Some("xterm")));
    }
}
