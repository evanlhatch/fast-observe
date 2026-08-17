//! Insta snapshots for the report text block and the `Debug` tree — the two
//! contract surfaces of `src/report.rs` / `src/exn.rs` that agents and
//! tooling consume verbatim.
//!
//! Location lines (`file:line:column`) churn on edits, so they are redacted
//! to `[LOCATION]` before snapshotting. insta's `filters` feature is not
//! enabled (plain `insta = "1"` dev-dep), so the redaction is a small
//! hand-rolled scanner instead of `with_settings!({filters => ...})`.
//! Everything else in the snapshot is pinned verbatim.
// Integration tests are separate crates — the nightly gate must be enabled
// here, not inherited from the library root.
#![feature(error_generic_member_access)]

use std::error::Error;
use std::fmt;

use fast_observe::errors::{CategoryTag, ErrorCode};
use fast_observe::report::render_report;
use fast_observe::{Context, ErrorCategory, Fault};

/// Leaf cause — no code, no category, plain display.
#[derive(Debug)]
struct SnapLeaf;

impl fmt::Display for SnapLeaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("snap leaf blew up")
    }
}

impl Error for SnapLeaf {}

/// Snapshot error providing a fixed code + category through `Error::provide`
/// (same pattern as `CodedBoom` in tests/report.rs).
#[derive(Debug)]
struct SnapBoom {
    source: Option<SnapLeaf>,
}

impl fmt::Display for SnapBoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("snap boom")
    }
}

impl Error for SnapBoom {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|inner| inner as &(dyn Error + 'static))
    }

    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        request.provide_value(ErrorCode("E999"));
        request.provide_value(CategoryTag(ErrorCategory::Content));
    }
}

/// The deterministic fixture: coded root with a source child, a context,
/// one keyed attachment. Deliberately UNSCOPED — `scope_path` /
/// `scope_elapsed_ms` are non-deterministic (wall-clock elapsed) and only
/// appear inside a function scope; `trace_id` needs an active span. E999 is
/// not in the registry, so `advice:` is omitted while `action:` still shows
/// the category policy line.
fn snap_fault() -> Fault<SnapBoom> {
    Fault::new(SnapBoom {
        source: Some(SnapLeaf),
    })
    .set_context(Context::custom("loading snapshot"))
    .attach_key("attempt", 3)
}

/// Replace every `tests/report_snapshot.rs:LINE:COLUMN` location with
/// `[LOCATION]` — line/column numbers churn on edits; the rest of the
/// output is pinned verbatim.
fn redact_location(text: &str) -> String {
    const NEEDLE: &str = "tests/report_snapshot.rs";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(NEEDLE) {
        let end = start + NEEDLE.len();
        let after = &rest[end..];
        // A location tail is `:LINE:COLUMN` — two runs of digits.
        let bytes = after.as_bytes();
        let mut idx = 0;
        let mut is_location = true;
        for _ in 0..2 {
            if bytes.get(idx) == Some(&b':') {
                idx += 1;
                let digits_start = idx;
                while bytes.get(idx).is_some_and(u8::is_ascii_digit) {
                    idx += 1;
                }
                if idx == digits_start {
                    is_location = false;
                    break;
                }
            } else {
                is_location = false;
                break;
            }
        }
        if is_location {
            out.push_str(&rest[..start]);
            out.push_str("[LOCATION]");
            rest = &after[idx..];
        } else {
            out.push_str(&rest[..end]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn report_text_snapshot() {
    let fault = snap_fault();
    insta::assert_snapshot!(redact_location(&render_report(&fault)));
}

#[test]
fn fault_debug_tree_snapshot() {
    let fault = snap_fault();
    insta::assert_snapshot!(redact_location(&format!("{fault:?}")));
}

#[test]
fn redact_location_only_touches_locations() {
    assert_eq!(
        redact_location("location: tests/report_snapshot.rs:12:34\nrest"),
        "location: [LOCATION]\nrest"
    );
    // A bare path mention without a line/column tail is left alone.
    assert_eq!(
        redact_location("see tests/report_snapshot.rs for the fixture"),
        "see tests/report_snapshot.rs for the fixture"
    );
}
