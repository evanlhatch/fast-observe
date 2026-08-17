//! Deployment init tests. The global `log` logger is per-process, so this
//! binary holds the ONLY `observe().init()` calls in the test suite — every
//! test here shares one init via [`ensure_init`]; other test binaries must
//! not call `init()`.

use fast_observe::deploy::{InitError, InitGuard, observe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};

/// The one live init guard for this process (kept alive until exit so the
/// drop-flush never fires mid-test).
static GUARD: Mutex<Option<InitGuard>> = Mutex::new(None);
static INIT: Once = Once::new();

/// Exactly one `observe().init()` per process. Returns true when THIS call
/// performed the init (vs. blocking on another test's init).
#[allow(clippy::expect_used, reason = "test")]
fn ensure_init() -> bool {
    let mut ours = false;
    INIT.call_once(|| {
        let guard = observe().init().expect("first init must succeed");
        *GUARD.lock().expect("guard mutex not poisoned") = Some(guard);
        ours = true;
    });
    ours
}

#[test]
#[allow(clippy::expect_used, reason = "test")]
fn init_twice_second_errors() {
    ensure_init();
    let second = observe().init();
    // `.err()` not `.expect_err()`: InitGuard has no Debug impl.
    let err = second.err().expect("second init must fail");
    assert_eq!(err, InitError::AlreadyInitialized);
    // First guard intentionally NOT dropped here — dropping flushes
    // fastrace mid-test-run; it lives in `GUARD` until process exit.
}

static PREVIOUS_HOOK_RAN: AtomicBool = AtomicBool::new(false);

#[test]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "deliberate panic to verify panic-hook chaining"
)]
fn panic_hook_chains() {
    // Install the sentinel BEFORE init: the deployment's panic hook
    // (default `panic_hook(true)`) chains to whatever hook `take_hook`
    // returns at install time. cargo test's own
    // panic hook is installed at process start; our sentinel replaces it
    // here, so a correct chain must reach the sentinel.
    std::panic::set_hook(Box::new(|_| {
        PREVIOUS_HOOK_RAN.store(true, Ordering::SeqCst);
    }));

    let ours = ensure_init();

    let result = std::panic::catch_unwind(|| std::panic::panic_any("deliberate test panic"));
    assert!(result.is_err(), "catch_unwind must observe the panic");

    // Ordering across the two tests in this binary is nondeterministic:
    // if init_twice_second_errors won the OnceLock, ITS init (also
    // `panic_hook(true)` by default) chained to whatever hook was current
    // then, and our later set_hook replaced that chain outright. Only when
    // WE performed the init is the chain sentinel-after-our-hook
    // guaranteed, so only then assert the chain reached the sentinel.
    if ours {
        assert!(
            PREVIOUS_HOOK_RAN.load(Ordering::SeqCst),
            "deployment panic hook must chain to the previously installed hook"
        );
    }
}
