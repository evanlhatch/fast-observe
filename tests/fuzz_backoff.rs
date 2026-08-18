//! Fuzz target: `Backoff` schedule purity.
//!
//! `fast_observe::Backoff` (`None` / `Fixed` / `Exponential`) schedules
//! are pure, side-effect-free iterators — attempt 1 first, one item per
//! attempt, forever. Driven with arbitrary `base`/`max` nanoseconds
//! (including `u64::MAX`) and arbitrary `factor` (including 0, 1,
//! `u32::MAX`), taking the first 64 items.
//!
//! Invariants per case:
//! - never panics, always yields (the iterator is infinite),
//! - every `Some(d)` satisfies `d <= max` (Exponential's cap),
//! - Exponential delays are non-decreasing and the first delay equals
//!   `min(base, max)`,
//! - `None` yields only `None`; `Fixed(d)` yields `Some(d)` forever.

use std::time::Duration;

use bolero::generator::TypeGenerator;
use fast_observe::Backoff;

/// How many schedule items each case inspects.
const TAKE: usize = 64;

#[derive(Debug, Clone, Copy, TypeGenerator)]
struct BackoffFuzz {
    /// Variant selector: 0 = None, 1 = Fixed, 2 = Exponential.
    variant: u8,
    /// `base` (Exponential) / the fixed delay (Fixed), in nanos.
    base_ns: u64,
    /// Exponential multiplier — 0 and 1 coerce to 2 inside `delay`.
    factor: u32,
    /// Exponential cap, in nanos.
    max_ns: u64,
}

impl BackoffFuzz {
    fn build(&self) -> Backoff {
        let base = Duration::from_nanos(self.base_ns);
        let max = Duration::from_nanos(self.max_ns);
        match self.variant % 3 {
            0 => Backoff::None,
            1 => Backoff::Fixed(base),
            _ => Backoff::Exponential {
                base,
                factor: self.factor,
                max,
            },
        }
    }
}

fn check_schedule(input: &BackoffFuzz) {
    let backoff = input.build();
    let items: Vec<Option<Duration>> = backoff.schedule().take(TAKE).collect();
    assert_eq!(items.len(), TAKE, "the schedule must yield items forever");
    match backoff {
        Backoff::None => {
            assert!(
                items.iter().all(Option::is_none),
                "Backoff::None must yield only None"
            );
        }
        Backoff::Fixed(d) => {
            assert!(
                items.iter().all(|item| *item == Some(d)),
                "Backoff::Fixed must yield Some({d:?}) forever"
            );
        }
        Backoff::Exponential { base, max, .. } => {
            let mut prev: Option<Duration> = None;
            for (attempt, item) in items.iter().enumerate() {
                assert!(
                    item.is_some(),
                    "Exponential attempt {} must yield Some",
                    attempt + 1
                );
                let Some(d) = item else {
                    continue;
                };
                assert!(
                    d <= &max,
                    "delay {d:?} exceeds max {max:?} at attempt {}",
                    attempt + 1
                );
                if let Some(p) = prev {
                    assert!(
                        *d >= p,
                        "Exponential delays must be non-decreasing: {p:?} then {d:?}"
                    );
                }
                prev = Some(*d);
            }
            let first = items.first().copied().flatten();
            assert_eq!(
                first,
                Some(base.min(max)),
                "the first Exponential delay must be min(base, max)"
            );
        }
    }
}

#[test]
fn fuzz_backoff_schedule() {
    bolero::check!()
        .with_type::<BackoffFuzz>()
        .for_each(check_schedule);
}
