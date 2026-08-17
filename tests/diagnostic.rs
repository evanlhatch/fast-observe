//! Diagnostic rendering: render-to-String + eprint wrapper + serde roundtrip.

use fast_observe::diagnostic::{Diagnostic, SourceSpan, eprint_diagnostic, register_source};
use fast_observe::{Severity, define_errors, render_diagnostic};

/// Test error enum — registers one code so the registry-note path in
/// `build_report` is exercised with a live entry (this binary links only
/// fast-observe, so the registry contents here are exactly what this file
/// registers).
#[derive(Debug)]
pub enum DiagTestError {
    BadWidget(BadWidget),
}

define_errors! {
    enum DiagTestError {
        (BadWidget, "E777", Content, "bad widget", "bad widget: {name}", {
            name: String,
        });
    }
}

impl std::error::Error for DiagTestError {}

#[test]
fn render_diagnostic_returns_nonempty_string_with_code() {
    let diag =
        Diagnostic::error("E042", "something broke").with_advice("try turning it off and on again");
    let out = render_diagnostic(&diag);
    assert!(!out.is_empty(), "rendered report must be non-empty");
    assert!(
        out.contains("E042"),
        "rendered report must contain the code: {out:?}"
    );
    assert!(
        out.contains("something broke"),
        "rendered report must contain the message: {out:?}"
    );
    // String rendering disables ANSI colors.
    assert!(
        !out.contains('\u{1b}'),
        "rendered String must not contain ANSI escapes: {out:?}"
    );
}

#[test]
fn render_diagnostic_with_span_references_file() {
    // ariadne reads the source file through the cache — use a real temp file.
    let path = std::env::temp_dir().join(format!("fast_observe_diag_{}.txt", std::process::id()));
    std::fs::write(&path, "abc def ghi\n").unwrap();
    let file = camino::Utf8PathBuf::from_path_buf(path.clone()).unwrap();

    let diag = Diagnostic::error("E100", "bad token").with_source(SourceSpan {
        file: file.clone(),
        start: 0,
        end: 3,
    });
    let out = render_diagnostic(&diag);
    std::fs::remove_file(&path).ok();
    assert!(out.contains("E100"), "missing code: {out:?}");
    assert!(out.contains(file.as_str()), "missing file id: {out:?}");
    // The spanned source line is rendered.
    assert!(out.contains("abc"), "missing source line: {out:?}");
}

#[test]
fn eprint_diagnostic_does_not_panic() {
    let diag = Diagnostic::error("E007", "to stderr");
    eprint_diagnostic(&diag);
}

#[test]
fn multi_label_renders_both() {
    register_source("virtual:///multi.txt", "alpha beta gamma");
    let diag = Diagnostic::error("E300", "mismatch")
        .with_source(SourceSpan {
            file: "virtual:///multi.txt".into(),
            start: 0,
            end: 5,
        })
        .with_label(
            SourceSpan {
                file: "virtual:///multi.txt".into(),
                start: 6,
                end: 10,
            },
            "referenced here",
        );
    let out = render_diagnostic(&diag);
    assert!(
        out.contains("referenced here"),
        "missing secondary label message: {out:?}"
    );
    assert!(
        out.contains("virtual:///multi.txt"),
        "missing file name: {out:?}"
    );
    // The primary label (message None) defaults to the severity name.
    assert!(out.contains("Error"), "missing primary label: {out:?}");
    assert_eq!(diag.labels.len(), 2);
    assert!(diag.labels[0].primary);
    assert!(!diag.labels[1].primary);
}

#[test]
fn in_memory_source_renders_without_disk() {
    let name = "virtual:///generated.flat";
    assert!(
        !std::path::Path::new(name).exists(),
        "virtual path must not exist on disk"
    );
    register_source(name, "alpha beta gamma");
    let diag = Diagnostic::error("E310", "bad word").with_source(SourceSpan {
        file: name.into(),
        start: 6,
        end: 10,
    });
    let out = render_diagnostic(&diag);
    assert!(out.contains("beta"), "missing spanned word: {out:?}");
}

#[test]
fn registry_note_appears_for_known_code() {
    // Notes render only when a source group resolves — attach a span backed
    // by an in-memory source (the synthetic `<unknown>` span fails to fetch).
    register_source("virtual:///widget.txt", "widget gizmo");
    let diag = Diagnostic::error("E777", "compile failed").with_source(SourceSpan {
        file: "virtual:///widget.txt".into(),
        start: 0,
        end: 6,
    });
    let out = render_diagnostic(&diag);
    assert!(
        out.contains("BadWidget [E777] — bad widget (category: Content)"),
        "missing registry note: {out:?}"
    );

    // Unknown code: no registry note, message still renders.
    let diag = Diagnostic::error("E999", "compile failed");
    let out = render_diagnostic(&diag);
    assert!(out.contains("[E999] compile failed"), "{out:?}");
    assert!(
        !out.contains("category:"),
        "unexpected registry note: {out:?}"
    );
}

#[test]
fn warning_and_info_constructors() {
    let w = Diagnostic::warning("W001", "careful");
    assert_eq!(w.severity, Severity::Warning);
    assert_eq!(w.code, "W001");
    assert_eq!(w.message, "careful");
    assert!(w.labels.is_empty());
    assert!(w.advice.is_none());

    let i = Diagnostic::info("I001", "fyi");
    assert_eq!(i.severity, Severity::Info);
    assert_eq!(i.code, "I001");
    assert_eq!(i.message, "fyi");
}

#[test]
fn diagnostic_display_and_error_impl() {
    let diag = Diagnostic::error("E042", "something broke");
    assert_eq!(diag.to_string(), "[E042] something broke");
    fn assert_error<T: std::error::Error>(_: &T) {}
    assert_error(&diag);
}

#[cfg(feature = "serde")]
#[test]
fn diagnostic_serde_roundtrip() {
    let diag = Diagnostic::error("E500", "roundtrip")
        .with_source(SourceSpan {
            file: "src/main.f".into(),
            start: 3,
            end: 9,
        })
        .with_advice("check the spec");
    let json = serde_json::to_string(&diag).unwrap();
    let back: Diagnostic = serde_json::from_str(&json).unwrap();
    assert_eq!(back.code, "E500");
    assert_eq!(back.severity, Severity::Error);
    assert_eq!(back.message, "roundtrip");
    assert_eq!(back.labels.len(), 1);
    let label = &back.labels[0];
    assert!(label.primary);
    assert_eq!(label.message, None);
    assert_eq!(label.span.file.as_str(), "src/main.f");
    assert_eq!(label.span.start, 3);
    assert_eq!(label.span.end, 9);
    assert_eq!(back.advice.as_deref(), Some("check the spec"));
}
