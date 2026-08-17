//! Diagnostics — compile-time errors (not exceptions).
//!
//! Used by compilers and mod/asset loaders. Typed errors with
//! source spans + severity, rendered with ariadne.

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

/// A diagnostic — a compile-time error with source location + advice.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    /// Stable error code. Owned `String` (not `&'static str`) so the type can
    /// round-trip through serde — `&'static str` cannot be deserialized from
    /// non-`'static` input.
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub source: Option<SourceSpan>,
    pub advice: Option<String>,
}

impl Diagnostic {
    /// An error diagnostic with code + message; no span or advice.
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.into(),
            source: None,
            advice: None,
        }
    }

    /// Attach a source span.
    #[must_use]
    pub fn with_source(mut self, source: SourceSpan) -> Self {
        self.source = Some(source);
        self
    }

    /// Attach advice (rendered as a report note).
    #[must_use]
    pub fn with_advice(mut self, advice: impl Into<String>) -> Self {
        self.advice = Some(advice.into());
        self
    }
}

/// Build the ariadne report for a diagnostic.
///
/// Report message is `[<code>] <message>`; severity drives the report kind
/// and label color; `advice` renders as a note. When no span is attached,
/// a synthetic zero-width source is used.
fn build_report(
    diag: &Diagnostic,
    color: bool,
) -> Report<'static, (String, std::ops::Range<usize>)> {
    let (kind, label_color) = match diag.severity {
        Severity::Error => (ReportKind::Error, Color::Red),
        Severity::Warning => (ReportKind::Warning, Color::Yellow),
        Severity::Info => (ReportKind::Advice, Color::Cyan),
    };
    let (file, range) = diag.source.as_ref().map_or_else(
        || ("<unknown>".to_string(), 0..0),
        |s| (s.file.to_string(), s.start..s.end),
    );

    let mut builder = Report::build(kind, (file.clone(), range.clone()))
        .with_config(ariadne::Config::default().with_color(color))
        .with_message(format!("[{}] {}", diag.code, diag.message))
        .with_label(
            Label::new((file, range))
                .with_message(diag.severity.as_ref())
                .with_color(label_color),
        );
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

/// Render a `Diagnostic` as an ariadne report to stderr (colors enabled).
pub fn eprint_diagnostic(diag: &Diagnostic) {
    if let Err(e) = build_report(diag, true).eprint(source_cache()) {
        log::error!(target: "fast_observe.diagnostic", "failed to render diagnostic: {e}");
    }
}

/// Build a `FnCache` that reads source files on demand for ariadne rendering.
fn source_cache() -> FnCache<String, impl FnMut(&String) -> Result<String, String>, String> {
    FnCache::new(|id: &String| {
        // Propagate read failures — ariadne renders the error instead of a
        // silently empty source snippet.
        fs_err::read_to_string(id).map_err(|e| e.to_string())
    })
}
