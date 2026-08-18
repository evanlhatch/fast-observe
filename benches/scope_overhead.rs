//! Dogfood benchmarks: measure fast-observe's own instrumentation overhead
//! through the public surface (feature `bench`, implies `instant`).
//!
//! Run: `cargo bench --features bench --bench scope_overhead`
//!
//! Numbers verify the documented overhead claims:
//! - `scope!` with `Backends::OFF` ≈ 2ns (one relaxed atomic load + all-dummy
//!   guard; profiling.rs docs),
//! - `scope!` with `Backends::INSTANT` ≈ 100ns (thread-local span record).

use fast_observe::bench::BenchExt;
use fast_observe::config::{Backends, config};
use fast_observe::divan;
use fast_observe::{BoxError, Fault, scope};

fn main() {
    // Absolute-overhead benches measure against a known mask. The default is
    // FASTRACE, so pin OFF up front; `bench_profiled` toggles INSTANT itself
    // and restores the previous mask afterwards.
    config().set_backends(Backends::OFF);
    divan::main();
}

/// Raw `scope!` with `Backends::OFF` — verifies the ~2ns claim.
#[divan::bench]
fn scope_off() {
    let guard = scope!("scope_off");
    divan::black_box_drop(guard);
}

/// Raw `scope!` with `Backends::INSTANT` — verifies the ~100ns claim.
#[divan::bench]
fn scope_instant(bencher: divan::Bencher) {
    let cfg = config();
    let saved = cfg.backends();
    cfg.set_backends(saved | Backends::INSTANT);
    bencher.bench_local(|| {
        let guard = scope!("scope_instant");
        divan::black_box_drop(guard);
    });
    cfg.set_backends(saved);
}

/// Error-path baseline: `Fault::from("x")` construction cost (message
/// boxing + frame capture).
#[divan::bench]
fn fault_from_str() -> Fault<BoxError> {
    Fault::from(divan::black_box("x"))
}

/// `bench_profiled` demo: a scope-heavy closure with two phases, printing
/// the aggregated per-phase `ProfiledRun` breakdown after the sample loop.
#[divan::bench(crate = fast_observe::bench::divan)]
fn profiled_two_phases(bencher: divan::Bencher) {
    let run = bencher.bench_profiled(|| {
        {
            let guard = scope!("phase_a");
            divan::black_box(&guard);
        }
        {
            let guard = scope!("phase_b");
            divan::black_box(&guard);
        }
    });
    run.print();
}
