//! Deterministic timing breakdown — replaces ad-hoc span profilers.
//!
//! Derives the same per-phase nanosecond breakdown from the instant backend's
//! span tree. Groups by name, sorted by total time.
//!
//! Available with feature `instant`.

use crate::profiling::instant::SpanRecord;

/// Print the accumulated span breakdown (per-phase nanosecond table).
/// Available with feature `instant`.
pub fn print_breakdown() {
    let spans = drain_spans();
    if spans.is_empty() {
        println!("  (no spans recorded)");
        return;
    }
    print_tree(&spans);
}

/// Drain all recorded spans from the thread-local accumulator.
/// Available with feature `instant`.
#[must_use]
pub fn drain_spans() -> Vec<SpanRecord> {
    crate::profiling::instant::drain()
}

fn print_tree(spans: &[SpanRecord]) {
    use humantime::format_duration;
    use nu_ansi_term::Color;
    use std::time::Duration;

    // One shared group-by-name pass (`instant::group_by_name`); `bench`
    // uses the same helper and sums instead of formatting.
    let groups = crate::profiling::instant::group_by_name(spans);

    println!("\n  SPAN BREAKDOWN ({} spans):", spans.len());
    let mut sorted: Vec<_> = groups.iter().collect();
    sorted.sort_by(|a, b| {
        b.1.iter()
            .map(Duration::as_nanos)
            .sum::<u128>()
            .cmp(&a.1.iter().map(Duration::as_nanos).sum::<u128>())
    });

    for (name, durations) in &sorted {
        let total: u128 = durations.iter().map(Duration::as_nanos).sum();
        let avg = total / durations.len() as u128;
        let color = if avg < 1_000 {
            Color::Green
        } else if avg < 10_000 {
            Color::Yellow
        } else {
            Color::Red
        };
        let avg_ns = u64::try_from(avg).unwrap_or(u64::MAX);
        println!(
            "  {:>20}: {} ({} calls)",
            name,
            color.paint(format_duration(Duration::from_nanos(avg_ns)).to_string()),
            durations.len(),
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn drain_spans_snapshot_roundtrip() {
        crate::profiling::instant::clear();
        // Record two spans via the instant backend directly.
        drop(crate::profiling::instant::enter("phase_a", None));
        drop(crate::profiling::instant::enter("phase_b", Some("t")));

        let spans = super::drain_spans();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].name, "phase_a");
        assert_eq!(spans[1].name, "phase_b");
        assert_eq!(spans[1].tag, Some("t"));

        // Snapshot is destructive — second drain sees nothing.
        assert!(super::drain_spans().is_empty());
    }

    #[test]
    fn print_breakdown_handles_empty() {
        crate::profiling::instant::clear();
        super::print_breakdown(); // must not panic on empty accumulator
    }
}
