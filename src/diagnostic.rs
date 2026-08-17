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
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter, strum::AsRefStr)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A source span — file + byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceSpan {
    /// Path to the source file (`camino` serde support rides on its
    /// `serde1` feature, pulled in by this crate's `serde` feature).
    pub file: Utf8PathBuf,
    pub start: usize,
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
    pub severity: Severity,
    pub message: String,
    /// Labeled source spans; empty renders a synthetic zero-width
    /// `<unknown>` location.
    #[cfg_attr(feature = "serde", serde(default))]
    pub labels: Vec<LabelSpan>,
    pub advice: Option<String>,
}

impl Diagnostic {
    /// An error diagnostic with code + message; no spans or advice.
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, message, Severity::Error)
    }

    /// A warning diagnostic with code + message; no spans or advice.
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, message, Severity::Warning)
    }

    /// An info diagnostic with code + message; no spans or advice.
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

/// In-memory sources registered via [`register_source`], consulted before
/// the filesystem when rendering spans.
static SOURCES: LazyLock<parking_lot::RwLock<HashMap<String, Arc<str>>>> =
    LazyLock::new(|| parking_lot::RwLock::new(HashMap::new()));

/// Register an in-memory source (generated code, embedded assets, virtual
/// paths). Consulted before the filesystem when rendering spans.
pub fn register_source(name: impl Into<String>, contents: impl Into<String>) {
    SOURCES.write().insert(name.into(), contents.into().into());
}

/// color decision: tty && !NO_COLOR && TERM != "dumb"
fn should_color(is_tty: bool, no_color_env: Option<&str>, term_env: Option<&str>) -> bool {
    is_tty && no_color_env.is_none() && term_env != Some("dumb")
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
    // Anchor the report at the first primary label (else the first label,
    // else a synthetic zero-width source).
    let anchor = diag
        .labels
        .iter()
        .find(|l| l.primary)
        .or(diag.labels.first());
    let (file, range) = anchor.map_or_else(
        || ("<unknown>".to_string(), 0..0),
        |l| (l.span.file.to_string(), l.span.start..l.span.end),
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
                    label.span.start..label.span.end,
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
        log::error!(target: "fast_observe.diagnostic", "failed to render diagnostic: {e}");
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
        log::error!(target: "fast_observe.diagnostic", "failed to render diagnostic: {e}");
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
