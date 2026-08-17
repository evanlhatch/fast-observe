//! Benchmarks on the same instrumentation: divan for "how fast", the instant
//! span accumulator for "where" (DESIGN.md §9d).
//!
//! Available with feature `bench` (implies `instant`).
//!
//! # Use with divan
//!
//! This module re-exports [`divan`] as a convenience so consumer bench files
//! need no direct divan dependency:
//!
//! ```ignore
//! // benches/my_bench.rs — [[bench]] harness = false
//! use fast_observe::bench::{BenchExt, divan};
//!
//! #[divan::bench]
//! fn parse(bencher: divan::Bencher) {
//!     let report = bencher.bench_profiled(|| {
//!         let _s = fast_observe::scope!("parse");
//!         // work...
//!     });
//!     report.print();
//! }
//!
//! fn main() {
//!     divan::main();
//! }
//! ```
//!
//! `#[divan::bench]` accepts a `crate` option for when divan only reaches the
//! file through a re-export: `#[divan::bench(crate = fast_observe::bench::divan)]`.
//! Depending on divan directly works too — the re-export is convenience only.
//! The attribute macro itself is NOT wrapped.
//!
//! # Semantics
//!
//! [`BenchExt::bench_profiled`] runs the measured closure through
//! [`divan::Bencher::bench_local`] with the instant span accumulator enabled,
//! then drains and aggregates per span name. Numbers INCLUDE scope
//! instrumentation overhead (~100ns/scope) — use plain `#[divan::bench]` for
//! absolute numbers, `bench_profiled` for phase attribution.
//!
//! divan [`ItemsCount`](divan::counter::ItemsCount) throughput counters are
//! deliberately NOT wired: divan counters are per-iteration values set BEFORE
//! the sample loop runs, but the error-count delta is only known AFTER. The
//! error delta rides on [`ProfiledRun::errors_delta`] and its `Display`
//! instead.
//!
//! Outside divan (unit tests, ad-hoc timing), use [`measure_breakdown`] —
//! a fixed-iteration loop with the same force/clear/drain/aggregate sequence.

use std::time::Duration;

use crate::config::{Backends, config};
use crate::profiling::instant::{self, SpanRecord};

// Convenience re-export — see module docs. NOT a macro wrapper.
pub use divan;

/// One aggregated span group: every recorded span with this name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanAgg {
    /// Span name passed to `scope!`.
    pub name: &'static str,
    /// Sum of all span wall durations.
    pub total: Duration,
    /// Number of recorded spans with this name.
    pub calls: u64,
    /// `total / calls`.
    pub avg: Duration,
}

/// Aggregated result of a profiled benchmark run.
///
/// Produced by [`BenchExt::bench_profiled`] and [`measure_breakdown`].
/// Print with the `Display` impl or [`Self::print`].
#[derive(Debug, Clone, Default)]
pub struct ProfiledRun {
    /// Per-span-name aggregation, sorted by total time descending.
    pub spans: Vec<SpanAgg>,
    /// Error-construction count delta across the run: `(type_name, delta)`,
    /// sorted by delta descending. Empty when no errors were constructed.
    pub errors_delta: Vec<(&'static str, u64)>,
}

impl ProfiledRun {
    /// Print the breakdown table to stdout.
    pub fn print(&self) {
        println!("{self}");
    }
}

impl std::fmt::Display for ProfiledRun {
    /// Pinned format (snapshot tests rely on it):
    ///
    /// ```text
    /// profiled breakdown:
    ///                     parse: 123ns (100 calls, total 12.3µs)
    /// errors: my_crate::MyError +100
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use humantime::format_duration;
        writeln!(f, "profiled breakdown:")?;
        for s in &self.spans {
            writeln!(
                f,
                "  {:>20}: {} ({} calls, total {})",
                s.name,
                format_duration(s.avg),
                s.calls,
                format_duration(s.total),
            )?;
        }
        for (ty, n) in &self.errors_delta {
            writeln!(f, "errors: {ty} +{n}")?;
        }
        Ok(())
    }
}

/// Aggregate spans by name: total/calls/avg, sorted by total descending.
///
/// Duplicates `breakdown::print_tree`'s grouping (~15 lines) — that one
/// prints to stdout; this one returns the data for [`ProfiledRun`].
/// `breakdown.rs` is not editable from here; keep the two in sync manually.
fn aggregate(spans: &[SpanRecord]) -> Vec<SpanAgg> {
    let mut groups: std::collections::BTreeMap<&'static str, (u128, u64)> =
        std::collections::BTreeMap::new();
    for s in spans {
        let e = groups.entry(s.name).or_default();
        e.0 += s.duration().as_nanos();
        e.1 += 1;
    }
    let mut out: Vec<SpanAgg> = groups
        .into_iter()
        .map(|(name, (total_ns, calls))| {
            let total_ns = u64::try_from(total_ns).unwrap_or(u64::MAX);
            let total = Duration::from_nanos(total_ns);
            // calls fits u32 in any real run; saturate rather than truncate.
            let divisor = u32::try_from(calls).unwrap_or(u32::MAX);
            SpanAgg {
                name,
                total,
                calls,
                avg: total / divisor,
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.total));
    out
}

/// Diff two [`error_counts`](crate::error_counts) snapshots: entries whose
/// count grew, sorted by delta descending.
fn error_delta(
    before: &[(&'static str, u64)],
    after: Vec<(&'static str, u64)>,
) -> Vec<(&'static str, u64)> {
    let mut out: Vec<(&'static str, u64)> = after
        .into_iter()
        .filter_map(|(ty, n)| {
            let prev = before.iter().find(|(t, _)| *t == ty).map_or(0, |(_, c)| *c);
            (n > prev).then_some((ty, n - prev))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    out
}

/// Restore the saved backend set on drop — a panicking measured closure must
/// not leak the `INSTANT` bit into the caller's config.
struct Restore(Backends);

impl Drop for Restore {
    fn drop(&mut self) {
        config().set_backends(self.0);
    }
}

/// Run `f` `iterations` times with the instant span accumulator enabled,
/// then return the aggregated per-phase breakdown.
///
/// The standalone (non-divan) counterpart of [`BenchExt::bench_profiled`] —
/// same force/clear/drain/aggregate sequence, useful in unit tests and
/// ad-hoc timing. The previous backend set is restored afterwards, even on
/// panic.
///
/// Concurrency caveat: the backend mask is process-global while span storage
/// is thread-local. Concurrent callers (parallel tests) can restore the mask
/// under each other and silently stop span recording — serialize callers.
pub fn measure_breakdown(iterations: usize, mut f: impl FnMut()) -> ProfiledRun {
    let cfg = config();
    let saved = cfg.backends();
    cfg.set_backends(saved | Backends::INSTANT);
    let _restore = Restore(saved);
    let errors_before = crate::error_counts();
    instant::clear();

    for _ in 0..iterations {
        f();
    }

    let spans = instant::drain();
    let errors_after = crate::error_counts();
    ProfiledRun {
        spans: aggregate(&spans),
        errors_delta: error_delta(&errors_before, errors_after),
    }
}

/// Extension methods for [`divan::Bencher`].
pub trait BenchExt {
    /// Like [`divan::Bencher::bench_local`], but with the instant span
    /// accumulator enabled for the closure's duration; returns the aggregated
    /// per-phase breakdown afterwards.
    ///
    /// Numbers INCLUDE scope instrumentation overhead (~100ns/scope) — use
    /// plain `bench` for absolute numbers, `bench_profiled` for attribution.
    ///
    /// # Signature deviation from the design sketch
    ///
    /// Takes `self` BY VALUE, not `&mut self`: divan 0.1's
    /// `bench`/`bench_local` consume the `Bencher`, and `Bencher` offers
    /// neither a reborrow nor `Default`, so a `&mut self` receiver cannot
    /// soundly hand ownership to divan. Method-call syntax
    /// (`bencher.bench_profiled(f)`) is identical either way.
    ///
    /// Uses `bench_local` (not `bench`): span accumulation is thread-local,
    /// and `bench_local` keeps the sample loop on the current thread, so
    /// `clear()` before and `drain()` after observe the closure's spans.
    /// With divan's `threads` option the benchmark body itself may run on
    /// other threads — those spans are not collected.
    fn bench_profiled<T>(self, f: impl FnMut() -> T) -> ProfiledRun;
}

impl BenchExt for divan::Bencher<'_, '_> {
    fn bench_profiled<T>(self, f: impl FnMut() -> T) -> ProfiledRun {
        let cfg = config();
        let saved = cfg.backends();
        cfg.set_backends(saved | Backends::INSTANT);
        let _restore = Restore(saved);
        let errors_before = crate::error_counts();
        instant::clear();

        self.bench_local(f);

        let spans = instant::drain();
        let errors_after = crate::error_counts();
        ProfiledRun {
            spans: aggregate(&spans),
            errors_delta: error_delta(&errors_before, errors_after),
        }
    }
}
