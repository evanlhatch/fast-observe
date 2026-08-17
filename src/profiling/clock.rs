//! Cheap monotonic clock for span timing.
//!
//! Uses [`web_time::Instant`] — a drop-in `std::time::Instant` replacement
//! that also works on wasm32 (browser `performance.now()`), so the instant
//! backend compiles and runs everywhere.

use std::sync::OnceLock;
use web_time::Instant;

/// Nanoseconds since an arbitrary process-monotonic origin.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    reason = "value is clamped to u64::MAX immediately before the cast"
)]
pub fn now_ns() -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
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
