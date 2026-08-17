//! Diagnostic rendering: render-to-String + eprint wrapper + serde roundtrip.

#[cfg(feature = "serde")]
use fast_observe::Severity;
use fast_observe::diagnostic::{Diagnostic, SourceSpan, eprint_diagnostic};
use fast_observe::render_diagnostic;

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
    assert_eq!(
        back.source
            .as_ref()
            .map(|s| (s.file.as_str(), s.start, s.end)),
        Some(("src/main.f", 3, 9))
    );
    assert_eq!(back.advice.as_deref(), Some("check the spec"));
}
