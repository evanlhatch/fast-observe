//! Cheap monotonic clock for span timing (DESIGN.md §2).
//!
//! Split by target:
//! - native: [`fastant::Instant`] — TSC reads (~2–5ns) on Linux `x86_64`,
//!   automatic fallback to `std::time` where TSC is unstable.
//! - wasm: [`web_time::Instant`] — browser `performance.now()`; fastant
//!   would fall back to `std::time::Instant::now`, which panics on
//!   wasm32-unknown-unknown.

use std::sync::OnceLock;

#[cfg(not(target_family = "wasm"))]
use fastant::Instant as InstantImpl;
#[cfg(target_family = "wasm")]
use web_time::Instant as InstantImpl;

/// The target-selected monotonic instant type, exported for the profiling
/// facade (scope stack timestamps) so it shares one clock source with
/// [`now_ns`].
pub type Instant = InstantImpl;

/// Nanosecond timestamp from the target-selected monotonic clock — carries
/// the unit in the type so ns/ms mixing is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nanos(pub u64);

impl Nanos {
    /// Read the clock: nanoseconds since an arbitrary process-monotonic origin.
    #[inline]
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "value is clamped to u64::MAX immediately before the cast"
    )]
    pub fn now() -> Nanos {
        static ORIGIN: OnceLock<InstantImpl> = OnceLock::new();
        Nanos(
            ORIGIN
                .get_or_init(Instant::now)
                .elapsed()
                .as_nanos()
                .min(u128::from(u64::MAX)) as u64,
        )
    }

    /// The raw nanosecond count.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// As a [`std::time::Duration`].
    #[must_use]
    pub const fn as_duration(self) -> std::time::Duration {
        std::time::Duration::from_nanos(self.0)
    }
}

/// Nanoseconds since an arbitrary process-monotonic origin.
#[inline]
#[must_use]
pub fn now_ns() -> u64 {
    Nanos::now().as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn clock_advances() {
        let a = now_ns();
        std::thread::sleep(Duration::from_millis(2));
        let b = now_ns();
        assert!(b > a, "clock must advance (a={a}, b={b})");
        assert!(
            b.saturating_sub(a) >= 1_000_000,
            "2ms sleep should read ≥1ms: {}",
            b.saturating_sub(a)
        );
    }

    #[test]
    fn clock_monotonic_in_loop() {
        let mut prev = now_ns();
        for _ in 0..10_000 {
            let cur = now_ns();
            assert!(cur >= prev, "non-monotonic read: {cur} < {prev}");
            prev = cur;
        }
    }
}
