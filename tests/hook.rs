//! New-behavior tests: multi-sink fan-out, hook panic containment, and the
//! per-type per-second hook throttle.

use fast_observe::exn::Fault;
use fast_observe::hook::{clear_error_hooks, hooks_len, set_default_hook_enabled};
use fast_observe::{add_error_hook, config};
use std::error::Error;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

// Serializes tests that mutate the global throttle config or assert on
// hook-fire counts: cargo test runs them on parallel threads, so without
// this they can interleave and flake.
static TEST_LOCK: Mutex<()> = Mutex::new(());

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
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn multiple_hooks_all_fire() {
    let _serial = TEST_LOCK.lock().unwrap();
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
    clippy::items_after_statements,
    clippy::unwrap_used,
    reason = "the test deliberately installs a panicking hook to verify containment; unwrap is test idiom"
)]
fn panicking_hook_is_contained_and_later_hooks_still_run() {
    let _serial = TEST_LOCK.lock().unwrap();
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
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn throttle_caps_identical_type_hooks_per_second() {
    let _serial = TEST_LOCK.lock().unwrap();
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
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn throttle_zero_is_unlimited() {
    let _serial = TEST_LOCK.lock().unwrap();
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

// ── Hook management API ─────────────────────────────────────────────────

#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn clear_error_hooks_resets_to_default_only() {
    let _serial = TEST_LOCK.lock().unwrap();
    let before = hooks_len();
    add_error_hook(|_| {});
    assert_eq!(
        hooks_len(),
        before + 1,
        "adding a hook grows the count by 1"
    );
    clear_error_hooks();
    assert_eq!(hooks_len(), 1, "clear resets to only the default hook");
}

unique_error!(DefaultToggleError, "default toggle");

#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn default_hook_can_be_disabled_and_reenabled() {
    let _serial = TEST_LOCK.lock().unwrap();
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    add_error_hook(|frame| {
        if frame.type_name.ends_with("DefaultToggleError") {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
    });

    set_default_hook_enabled(false);
    let before = COUNT.load(Ordering::Relaxed);
    let _f = Fault::new(DefaultToggleError);
    let fired = COUNT.load(Ordering::Relaxed) - before;
    set_default_hook_enabled(true);

    assert_eq!(
        fired, 1,
        "custom hook must still fire while default hook disabled, got {fired}"
    );
}
