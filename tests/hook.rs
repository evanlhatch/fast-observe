//! New-behavior tests: multi-sink fan-out, hook panic containment, and the
//! per-type per-second hook throttle.

use fast_observe::exn::Fault;
use fast_observe::{add_error_hook, config};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

macro_rules! unique_error {
    ($name:ident, $msg:literal) => {
        #[derive(Debug)]
        struct $name;
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($msg)
            }
        }
        impl Error for $name {}
    };
}

// ── Multi-sink fan-out ──────────────────────────────────────────────────

unique_error!(FanoutError, "fanout");

#[test]
fn multiple_hooks_all_fire() {
    static A: AtomicUsize = AtomicUsize::new(0);
    static B: AtomicUsize = AtomicUsize::new(0);

    add_error_hook(|frame| {
        if frame.type_name.ends_with("FanoutError") {
            A.fetch_add(1, Ordering::Relaxed);
        }
    });
    add_error_hook(|frame| {
        if frame.type_name.ends_with("FanoutError") {
            B.fetch_add(1, Ordering::Relaxed);
        }
    });

    let before_a = A.load(Ordering::Relaxed);
    let before_b = B.load(Ordering::Relaxed);

    let f = Fault::new(FanoutError);
    assert!(f.to_string().contains("fanout"));

    assert_eq!(
        A.load(Ordering::Relaxed) - before_a,
        1,
        "first added hook must fire"
    );
    assert_eq!(
        B.load(Ordering::Relaxed) - before_b,
        1,
        "second added hook must fire"
    );
}

// ── Panic containment ───────────────────────────────────────────────────

unique_error!(PanicError, "panic path");

#[test]
#[allow(
    clippy::panic,
    clippy::manual_assert,
    reason = "the test deliberately installs a panicking hook to verify containment"
)]
fn panicking_hook_is_contained_and_later_hooks_still_run() {
    static AFTER: AtomicUsize = AtomicUsize::new(0);

    add_error_hook(|frame| {
        if frame.type_name.ends_with("PanicError") {
            panic!("deliberate hook panic");
        }
    });
    add_error_hook(|frame| {
        if frame.type_name.ends_with("PanicError") {
            AFTER.fetch_add(1, Ordering::Relaxed);
        }
    });

    let before = AFTER.load(Ordering::Relaxed);
    // Must not propagate the hook's panic.
    let f = Fault::new(PanicError);
    assert!(f.to_string().contains("panic path"));
    assert_eq!(
        AFTER.load(Ordering::Relaxed) - before,
        1,
        "hook registered after the panicking one must still run"
    );
}

// ── Throttle ────────────────────────────────────────────────────────────

unique_error!(ThrottleError, "throttle me");

#[test]
fn throttle_caps_identical_type_hooks_per_second() {
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    add_error_hook(|frame| {
        if frame.type_name.ends_with("ThrottleError") {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
    });

    let cfg = config::config();
    let original = cfg.error_hook_throttle();
    cfg.set_error_hook_throttle(2);

    let before = COUNT.load(Ordering::Relaxed);
    for _ in 0..5 {
        let _f = Fault::new(ThrottleError);
    }
    let fired = COUNT.load(Ordering::Relaxed) - before;

    cfg.set_error_hook_throttle(original);

    assert_eq!(
        fired, 2,
        "throttle limit 2/sec: 5 same-type errors must invoke hooks twice, got {fired}"
    );
}

#[test]
fn throttle_zero_is_unlimited() {
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    add_error_hook(|frame| {
        if frame.type_name.ends_with("FanoutError") {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Default config is unlimited (0). Other tests never raise it above 0
    // without restoring, but set explicitly for determinism.
    let cfg = config::config();
    let original = cfg.error_hook_throttle();
    cfg.set_error_hook_throttle(0);

    let before = COUNT.load(Ordering::Relaxed);
    for _ in 0..4 {
        let _f = Fault::new(FanoutError);
    }
    let fired = COUNT.load(Ordering::Relaxed) - before;

    cfg.set_error_hook_throttle(original);

    assert_eq!(fired, 4, "throttle 0 = unlimited, got {fired}");
}
