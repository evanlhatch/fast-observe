//! New-behavior tests: multi-sink fan-out, hook panic containment, and the
//! per-type per-second hook throttle.

use fast_observe::exn::{Attachment, Fault};
use fast_observe::hook::{
    add_capture_hook, clear_error_hooks, hooks_len, set_default_hook_enabled,
};
use fast_observe::profiling::enter_function_scope;
use fast_observe::{add_error_hook, config};
use std::borrow::Cow;
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
        if frame.type_name().ends_with("FanoutError") {
            A.fetch_add(1, Ordering::Relaxed);
        }
    });
    add_error_hook(|frame| {
        if frame.type_name().ends_with("FanoutError") {
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
        if frame.type_name().ends_with("PanicError") {
            panic!("deliberate hook panic");
        }
    });
    add_error_hook(|frame| {
        if frame.type_name().ends_with("PanicError") {
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
        if frame.type_name().ends_with("ThrottleError") {
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
        if frame.type_name().ends_with("FanoutError") {
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
        if frame.type_name().ends_with("DefaultToggleError") {
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

// ── Capture hooks — run during frame construction, may attach data ────────

unique_error!(CaptureMarkerError, "capture marker");

// Unique newtype for find_attachment — a bare `String` value would be
// ambiguous against the built-in hooks' `String` attachments.
struct Marker;
impl fmt::Display for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("yes")
    }
}

#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn capture_hooks_attach_data() {
    let _serial = TEST_LOCK.lock().unwrap();

    add_capture_hook(|frame| {
        if frame.type_name().ends_with("CaptureMarkerError") {
            frame.push_attachment(Attachment::with_key("marker", Marker));
        }
    });

    let f = Fault::new(CaptureMarkerError);
    let marker = f.frame().find_attachment::<Marker>();
    assert!(
        marker.is_some(),
        "capture hook attachment must be present on the root frame"
    );
    assert_eq!(
        marker.map(ToString::to_string).as_deref(),
        Some("yes"),
        "attached Marker must display as yes"
    );
}

unique_error!(CapturePanicError, "capture panic");

#[test]
#[allow(
    clippy::panic,
    clippy::manual_assert,
    clippy::items_after_statements,
    clippy::unwrap_used,
    reason = "the test deliberately installs a panicking capture hook to verify containment; unwrap is test idiom"
)]
fn capture_hook_panic_is_contained() {
    let _serial = TEST_LOCK.lock().unwrap();
    static AFTER: AtomicUsize = AtomicUsize::new(0);

    add_capture_hook(|frame| {
        if frame.type_name().ends_with("CapturePanicError") {
            panic!("deliberate capture hook panic");
        }
    });
    add_capture_hook(|frame| {
        if frame.type_name().ends_with("CapturePanicError") {
            AFTER.fetch_add(1, Ordering::Relaxed);
        }
    });

    let before = AFTER.load(Ordering::Relaxed);
    // Construction must succeed despite the panicking capture hook.
    let f = Fault::new(CapturePanicError);
    assert!(f.to_string().contains("capture panic"));
    assert_eq!(
        AFTER.load(Ordering::Relaxed) - before,
        1,
        "capture hook registered after the panicking one must still run"
    );
}

unique_error!(ScopedCaptureError, "scoped capture");

// ── Span-trail breadcrumbs (feature `instant`) ────────────────────────────

#[cfg(feature = "instant")]
unique_error!(BreadcrumbError, "breadcrumb trail");

#[cfg(feature = "instant")]
#[test]
#[allow(
    clippy::items_after_statements,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test"
)]
fn breadcrumb_trail_attached_on_fault_with_instant_backend() {
    let _serial = TEST_LOCK.lock().unwrap();
    let cfg = config::config();
    let original = cfg.backends();
    cfg.set_backends(config::Backends::INSTANT);
    fast_observe::profiling::instant::clear();
    // The default hook wraps its log in `scope!("error")`, which would record
    // into FINISHED and skew the post-fault drain count — disable it for the
    // duration of this test (restored below).
    set_default_hook_enabled(false);

    {
        let _outer = fast_observe::scope!("crumb_outer");
        let _inner = fast_observe::scope!("crumb_inner");
    }
    let f = Fault::new(BreadcrumbError);

    set_default_hook_enabled(true);
    cfg.set_backends(original);

    let trail = f
        .frame()
        .attachments()
        .iter()
        .find(|a| a.key() == Some("span_trail"))
        .expect("built-in breadcrumb hook must attach span_trail");
    assert_eq!(
        trail.placement(),
        fast_observe::Placement::Appendix,
        "span_trail must be Appendix (counted, not inlined)"
    );
    let rendered = trail.display();
    assert!(
        rendered.contains("crumb_outer"),
        "trail must name the outer scope: {rendered}"
    );
    assert!(
        rendered.contains("crumb_inner"),
        "trail must name the inner scope: {rendered}"
    );
    // Non-destructive: the breadcrumb peek must not have drained FINISHED.
    assert_eq!(
        fast_observe::profiling::instant::drain().len(),
        2,
        "span_trail capture must leave the accumulator undisturbed"
    );
}

#[cfg(feature = "instant")]
unique_error!(NoBreadcrumbError, "no breadcrumb");

#[cfg(feature = "instant")]
#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn no_breadcrumbs_when_backend_off() {
    let _serial = TEST_LOCK.lock().unwrap();
    let cfg = config::config();
    let original = cfg.backends();
    cfg.set_backends(config::Backends::OFF);
    fast_observe::profiling::instant::clear();

    let f = Fault::new(NoBreadcrumbError);
    cfg.set_backends(original);

    assert!(
        f.frame()
            .attachments()
            .iter()
            .all(|a| a.key() != Some("span_trail")),
        "no span_trail attachment when the instant backend is off"
    );
}

#[cfg(feature = "instant")]
#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn peek_recent_is_non_destructive_and_bounded() {
    let _serial = TEST_LOCK.lock().unwrap();
    use fast_observe::profiling::instant::{clear, drain, enter, peek_recent};
    clear();
    // Static names — `enter` needs &'static str.
    const NAMES: [&str; 10] = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9"];
    for name in NAMES {
        drop(enter(name, None));
    }

    let peeked = peek_recent(3);
    let names: Vec<_> = peeked.iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        vec!["p7", "p8", "p9"],
        "peek_recent(3) must return the 3 newest spans, oldest→newest"
    );
    assert_eq!(
        drain().len(),
        10,
        "peek must be non-destructive — drain still yields all 10 spans"
    );
}

#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn scope_path_attached_when_scoped() {
    let _serial = TEST_LOCK.lock().unwrap();

    let _scope = enter_function_scope(Cow::Borrowed("test_scope"));
    let f = Fault::new(ScopedCaptureError);

    let has_scope_path = f
        .frame()
        .attachments()
        .iter()
        .any(|a| a.key() == Some("scope_path"));
    assert!(
        has_scope_path,
        "built-in scope hook must attach scope_path when a scope is active"
    );
}

// ── Backtrace capture hook (feature `backtrace`) ──────────────────────────

#[cfg(feature = "backtrace")]
unique_error!(BacktraceMarkerError, "backtrace marker");

#[cfg(feature = "backtrace")]
#[test]
#[allow(clippy::items_after_statements, clippy::unwrap_used, reason = "test")]
fn backtrace_attachment_when_env_set() {
    let _serial = TEST_LOCK.lock().unwrap();

    // The env decision (RUST_BACKTRACE / OBSERVE_BACKTRACE) is resolved once
    // in a LazyLock at first use — process-global and possibly already
    // initialized by an earlier test thread, so env manipulation here would
    // be racy (and `set_var` is unsafe on edition 2024). The full decision
    // matrix is unit-tested against the pure `backtrace_enabled` fn in
    // src/hook.rs. This integration test asserts only deterministic
    // properties: construction must not panic, and any `backtrace`
    // attachment must be Appendix-placed (counted, not inlined by the
    // report).
    let f = Fault::new(BacktraceMarkerError);
    assert!(f.to_string().contains("backtrace marker"));
    for attachment in f.frame().attachments() {
        if attachment.key() == Some("backtrace") {
            assert_eq!(
                attachment.placement(),
                fast_observe::Placement::Appendix,
                "backtrace attachment must be Appendix (counted, not inlined)"
            );
        }
    }
}
