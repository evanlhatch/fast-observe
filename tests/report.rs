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
use fast_observe::{Context, ErrorCategory, Fault, ResultExt};

/// Fingerprint fixture helper — one construction site = one location.
fn boom() -> Fault {
    Fault::from("boom")
}

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
    .set_context(Context::custom("loading entity"))
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
    // Fixed section order: report marker, error, category, location, scope,
    // attachment, cause N (with [type] + location), fingerprint, action,
    // hint (advice/trace_id omitted: E777 is not in the registry and no
    // fastrace span is active).
    assert_eq!(lines[0], "report: fast-observe/1", "format marker first");
    assert_eq!(
        lines[1],
        "error: [E777] [report::CodedBoom] coded boom (loading entity)"
    );
    assert_eq!(
        lines[2],
        "category: Content (policy: fix the input; retrying unchanged input will fail)"
    );
    assert!(
        lines[3].starts_with("location: tests/report.rs:"),
        "location line: {}",
        lines[3]
    );
    assert!(
        lines[4].starts_with("scope: load_entity (elapsed ") && lines[4].ends_with("ms)"),
        "scope line: {}",
        lines[4]
    );
    assert_eq!(lines[5], "attachment: attempt=3");
    assert!(
        lines[6].starts_with(
            "cause 0: [E777] [report::CodedBoom] coded boom (loading entity), at tests/report.rs:"
        ),
        "cause 0 line: {}",
        lines[6]
    );
    // cause 1 arrived via `source()` — stringified at capture, so its
    // concrete type is erased and the [type] segment is omitted.
    assert!(
        lines[7].starts_with("cause 1: inner blew up, at tests/report.rs:"),
        "cause 1 line: {}",
        lines[7]
    );
    assert!(
        lines[8].starts_with("fingerprint: ") && lines[8].len() == "fingerprint: ".len() + 8,
        "fingerprint line: {}",
        lines[8]
    );
    assert_eq!(
        lines[9],
        "action: fix the input; retrying unchanged input will fail"
    );
    assert_eq!(lines[10], "hint: run `doctor E777`");
    assert_eq!(lines.len(), 11, "no extra sections: {first}");

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
    assert!(!report.contains("hint:"), "hint needs a code");
    assert!(
        report.contains("error: boom"),
        "error line present (type omitted: boxed root erases it): {report}"
    );
    assert!(
        report.lines().any(|line| line.starts_with("location: ")),
        "location line present"
    );
    assert!(
        report.contains("cause 0: boom, at "),
        "cause line carries location: {report}"
    );
    // The fingerprint is stable for a given construction site (one helper
    // = one location) and differs across sites.
    let fp = |r: &str| {
        r.lines()
            .find(|l| l.starts_with("fingerprint: "))
            .map(str::to_owned)
    };
    let same_site = render_report(&boom());
    let same_site_twice = render_report(&boom());
    assert_eq!(
        fp(&same_site),
        fp(&same_site_twice),
        "same site, same fingerprint"
    );
    assert_ne!(
        fp(&report),
        fp(&same_site),
        "different sites, different fingerprints"
    );
}

/// Regression: data-controlled newlines must not forge report lines — the
/// one-fact-per-line contract is what agents grep against. Every
/// interpolated value is line-sanitized (`\n` → `\\n`).
#[test]
fn report_is_injection_safe() {
    let result: fast_observe::Result<()> =
        Err::<(), _>(std::io::Error::other("raw\ncause: forged"))
            .context("line one\naction: forged");
    let fault = result.unwrap_err();
    let report = render_report(&fault);
    for line in report.lines() {
        let first = line.split(':').next().unwrap_or("");
        assert!(
            line.starts_with("  ")
                || matches!(
                    first,
                    "report"
                        | "error"
                        | "category"
                        | "location"
                        | "scope"
                        | "attachment"
                        | "trace_id"
                        | "fingerprint"
                        | "advice"
                        | "action"
                        | "hint"
                )
                || first.starts_with("cause ")
                || first.starts_with("original ")
                || first.starts_with("attempt ")
                || first.starts_with("failure "),
            "forged report line: {line:?}"
        );
    }
    assert!(
        report.contains("line one\\naction: forged"),
        "newline escaped, not expanded: {report}"
    );
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
    // Schema v2: structured location, typed fields, fingerprint/hint.
    assert_eq!(obj["schema"], 2);
    assert_eq!(obj["report"], "fast-observe/1");
    assert_eq!(obj["error"]["code"], "E777");
    assert_eq!(obj["error"]["category"], "Content");
    assert_eq!(obj["error"]["type"], "report::CodedBoom");
    assert_eq!(obj["location"]["file"], "tests/report.rs");
    assert!(obj["location"]["line"].is_u64(), "line is a number");
    assert!(
        obj["scope"]["elapsed_ms"].is_u64(),
        "elapsed_ms is a number, not a string"
    );
    assert_eq!(obj["causes"][0]["code"], "E777");
    assert_eq!(obj["causes"][0]["type"], "report::CodedBoom");
    assert_eq!(obj["causes"][1]["message"], "inner blew up");
    assert_eq!(obj["causes"][1]["kind"], "source");
    assert!(
        obj["causes"][1]["type"].is_null(),
        "source-chain frame: concrete type erased at capture"
    );
    assert!(obj["fingerprint"].is_string());
    assert_eq!(obj["hint"], "run `doctor E777`");
}
