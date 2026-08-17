//! The fault report (src/report.rs): deterministic `key: value` lines,
//! fixed section order, no ANSI. The report text appears in logs and is
//! read by agents — changing it is a breaking observability change.
// Integration tests are separate crates — the nightly gate must be enabled
// here, not inherited from the library root.
#![feature(error_generic_member_access)]

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use fast_observe::errors::{CategoryTag, ErrorCode};
use fast_observe::profiling::enter_function_scope;
use fast_observe::report::{render_report, report_display};
use fast_observe::{Context, ErrorCategory, Fault};

/// Leaf cause — no code, no category, plain display.
#[derive(Debug)]
struct InnerBoom;

impl fmt::Display for InnerBoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("inner blew up")
    }
}

impl Error for InnerBoom {}

/// Test error providing a code + category through `Error::provide`.
#[derive(Debug)]
struct CodedBoom {
    source: Option<InnerBoom>,
}

impl fmt::Display for CodedBoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("coded boom")
    }
}

impl Error for CodedBoom {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|inner| inner as &(dyn Error + 'static))
    }

    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        request.provide_value(ErrorCode("E777"));
        request.provide_value(CategoryTag(ErrorCategory::Content));
    }
}

/// The standard fixture: coded root error with a source child, context, and
/// one keyed attachment. Construct INSIDE a scope guard so the built-in
/// capture hook attaches `scope_path` / `scope_elapsed_ms`.
fn coded_fault() -> Fault<CodedBoom> {
    Fault::new(CodedBoom {
        source: Some(InnerBoom),
    })
    .with_context(Context::custom("loading entity"))
    .attach_key("attempt", 3)
}

#[test]
fn report_is_deterministic_and_complete() {
    let _scope = enter_function_scope(Cow::Borrowed("load_entity"));
    let fault = coded_fault();

    let first = render_report(&fault);
    let second = render_report(&fault);
    assert_eq!(first, second, "report must be deterministic");
    assert!(!first.contains('\u{1b}'), "no ANSI escapes in the report");

    let lines: Vec<&str> = first.lines().collect();
    // Fixed section order: error, category, location, scope, attachment,
    // cause N, action (advice/trace_id omitted: E777 is not in the
    // registry and no fastrace span is active).
    assert_eq!(lines[0], "error: [E777] coded boom (loading entity)");
    assert_eq!(
        lines[1],
        "category: Content (policy: fix the input; retrying unchanged input will fail)"
    );
    assert!(
        lines[2].starts_with("location: tests/report.rs:"),
        "location line: {}",
        lines[2]
    );
    assert!(
        lines[3].starts_with("scope: load_entity (elapsed ") && lines[3].ends_with("ms)"),
        "scope line: {}",
        lines[3]
    );
    assert_eq!(lines[4], "attachment: attempt=3");
    assert_eq!(lines[5], "cause 0: [E777] coded boom (loading entity)");
    assert_eq!(lines[6], "cause 1: inner blew up");
    assert_eq!(
        lines[7],
        "action: fix the input; retrying unchanged input will fail; see `doctor E777`"
    );
    assert_eq!(lines.len(), 8, "no extra sections: {first}");

    assert!(
        !first.contains("trace_id:"),
        "no span active — line omitted"
    );
    assert!(
        !first.contains("advice:"),
        "E777 not registered — advice omitted"
    );
}

#[test]
fn report_without_code_omits_code_lines() {
    let fault = Fault::from("boom");
    let report = render_report(&fault);

    assert!(!report.contains("[E"), "no code prefix: {report}");
    assert!(!report.contains("category:"), "category line omitted");
    assert!(!report.contains("action:"), "action needs a category");
    assert!(report.contains("error: boom"), "error line present");
    assert!(
        report.lines().any(|line| line.starts_with("location: ")),
        "location line present"
    );
    assert!(report.contains("cause 0: boom"), "cause line present");
}

#[test]
fn report_display_streams() {
    use std::fmt::Write as _;

    let _scope = enter_function_scope(Cow::Borrowed("stream_scope"));
    let fault = coded_fault();

    let mut streamed = String::new();
    assert!(write!(streamed, "{}", report_display(&fault)).is_ok());
    assert_eq!(streamed, render_report(&fault));
}

#[cfg(feature = "serde")]
#[test]
fn json_schema_version() {
    let _scope = enter_function_scope(Cow::Borrowed("json_scope"));
    let fault = coded_fault();

    let json = fast_observe::report::render_report_json(&fault);
    let obj: serde_json::Value = serde_json::from_str(&json).expect("report json parses");
    assert_eq!(obj["schema"], 1);
    assert_eq!(obj["error"]["code"], "E777");
    assert_eq!(obj["error"]["category"], "Content");
    assert_eq!(obj["causes"][0]["code"], "E777");
    assert_eq!(obj["causes"][1]["message"], "inner blew up");
}
