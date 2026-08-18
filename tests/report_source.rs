//! `OBSERVE_REPORT_SOURCE=1` — the opt-in `source:` line. Dedicated test
//! binary: the toggle is a `LazyLock` resolved on first report render, so
//! this test must be the only one in its process that renders a report.

use fast_observe::{Fault, render_report};

/// The fault's location is this file (`Fault::from` is `#[track_caller]`,
/// so the recorded location is the call inside this helper).
fn boom() -> Fault {
    Fault::from("source snippet boom")
}

#[test]
#[allow(
    unsafe_code,
    reason = "env mutation before the first render; this binary contains only this test"
)]
fn report_source_line_opt_in() {
    // Safety: single test in this binary; env set before the report module's
    // LazyLock resolves (first render below).
    unsafe {
        std::env::set_var("OBSERVE_REPORT_SOURCE", "1");
    }
    let report = render_report(&boom());
    let Some(source) = report.lines().find(|l| l.starts_with("source: ")) else {
        unreachable!("no source line in report:\n{report}")
    };
    assert!(
        source.contains("Fault::from(\"source snippet boom\")"),
        "source line shows the constructing statement: {source}"
    );
}
