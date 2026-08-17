//! Integration tests — `bench` module (divan folded into the scope model).
//!
//! Exercises [`fast_observe::bench::measure_breakdown`] directly — the
//! standalone path behind `BenchExt::bench_profiled` — because divan's
//! `Bencher` has no public constructor outside the divan harness.
//!
//! An actual `#[divan::bench]` binary (benches/ + `divan::main()`) is
//! integration's call, not this test's.
#![cfg(feature = "bench")]

use std::error::Error;
use std::fmt;
use std::sync::Mutex;

use fast_observe::Fault;
use fast_observe::bench::measure_breakdown;
use fast_observe::config::config;

/// `measure_breakdown` toggles the PROCESS-GLOBAL backend mask (INSTANT on,
/// then restore) — concurrent tests in this binary would restore the mask
/// under each other and silently stop span recording mid-loop. Serialize.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn measure_breakdown_collects_spans_and_restores_backends() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let backends_before = config().backends();

    let run = measure_breakdown(100, || {
        let _s = fast_observe::scope!("inner_phase");
        std::hint::black_box(());
    });

    let agg = run
        .spans
        .iter()
        .find(|s| s.name == "inner_phase")
        .expect("spans must contain inner_phase");
    assert_eq!(agg.calls, 100, "one span per iteration");
    assert!(agg.avg.as_nanos() > 0, "avg must be nonzero: {agg:?}");
    assert!(agg.total >= agg.avg, "total must cover avg: {agg:?}");
    // Sorted by total descending.
    assert!(
        run.spans.windows(2).all(|w| w[0].total >= w[1].total),
        "spans must be sorted by total desc: {:?}",
        run.spans
    );
    // NOTE: no `errors_delta.is_empty()` assertion — tests share one
    // process and `error_counts` is process-global, so a concurrent test's
    // Faults can land in this run's delta window.

    // Backends restored after the run.
    assert_eq!(
        config().backends(),
        backends_before,
        "backend set must be restored"
    );
}

#[derive(Debug)]
struct BenchBoom;
impl fmt::Display for BenchBoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bench boom")
    }
}
impl Error for BenchBoom {}

#[test]
fn measure_breakdown_reports_error_delta() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let run = measure_breakdown(10, || {
        let _f = Fault::new(BenchBoom);
    });

    let key = std::any::type_name::<BenchBoom>();
    let delta = run
        .errors_delta
        .iter()
        .find(|(ty, _)| *ty == key)
        .map(|(_, n)| *n);
    assert_eq!(
        delta,
        Some(10),
        "one Fault per iteration → delta 10 for {key}: {:?}",
        run.errors_delta
    );

    // Display renders the error line.
    let rendered = run.to_string();
    assert!(
        rendered.contains(&format!("errors: {key} +10")),
        "Display must render the error delta line, got:\n{rendered}"
    );
}

#[test]
fn profiled_run_display_format() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let run = measure_breakdown(3, || {
        let _s = fast_observe::scope!("phase_x");
    });
    let rendered = run.to_string();
    let mut lines = rendered.lines();
    assert_eq!(
        lines.next(),
        Some("profiled breakdown:"),
        "header line pinned, got:\n{rendered}"
    );
    let span_line = lines.next().expect("span line missing");
    assert!(
        span_line.contains("phase_x:") && span_line.contains("(3 calls, total "),
        "span line shape pinned, got: {span_line:?}"
    );
}
