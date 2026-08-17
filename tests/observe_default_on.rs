//! Integration tests — default-on observability (lazy default hook,
//! error counters, `report`, deref regression, hook reentrancy).

use fast_observe::exn::error_counts;
use fast_observe::{Fault, ResultExt, add_error_hook};
use std::cell::Cell;
use std::error::Error;
use std::fmt;

/// Bump helper: current count for a key (0 when absent).
fn count_of(key: &str) -> u64 {
    error_counts()
        .iter()
        .find(|(k, _)| *k == key)
        .map_or(0, |(_, c)| *c)
}

#[derive(Debug)]
struct UniqueTestError;
impl fmt::Display for UniqueTestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unique test error")
    }
}
impl Error for UniqueTestError {}

/// A `Fault` constructed WITHOUT `init()` must still fire the (lazily
/// installed) default hook — no panic — and must bump the auto error counter.
#[test]
fn fault_without_init_reaches_default_hook_and_counts() {
    let key = std::any::type_name::<UniqueTestError>();
    let before = count_of(key);
    // No observe::init() called — the default hook self-installs on first use.
    let f = Fault::new(UniqueTestError);
    assert!(f.to_string().contains("unique test error"));
    let after = count_of(key);
    assert_eq!(
        after,
        before + 1,
        "error_counts must increment for {key} (before={before}, after={after})"
    );
}

/// `report()` on an Err returns None and increments the "reported" counter.
#[test]
fn report_returns_none_and_counts() {
    let before = count_of("reported");
    let r: Result<(), UniqueTestError> = Err(UniqueTestError);
    let out = r.report("swallowing for the test");
    assert!(out.is_none(), "report on Err must return None");
    let after = count_of("reported");
    assert_eq!(
        after,
        before + 1,
        "reported counter must increment (before={before}, after={after})"
    );

    // Ok passes through untouched.
    let ok: Result<u32, UniqueTestError> = Ok(7);
    assert_eq!(ok.report("unused"), Some(7));
}

/// Deref regression: `Fault::from("boom")` (via `from_boxed`) must not panic
/// on deref — the stored concrete type is `BoxError` after double-boxing.
#[test]
fn deref_on_from_str_fault_does_not_panic() {
    let f: Fault = Fault::from("boom");
    // Forces Deref to BoxError — previously panicked on the downcast.
    let boxed: &fast_observe::BoxError = &f;
    assert!(boxed.to_string().contains("boom"));
    // Auto-deref method call (Error::source via BoxError) must also work.
    let _ = f.source();
}

/// Hook reentrancy: a hook that itself constructs a Fault must not deadlock
/// (the lock is dropped before the callback runs). Depth-guarded against
/// infinite recursion.
#[test]
fn reentrant_hook_does_not_deadlock() {
    thread_local! {
        static DEPTH: Cell<u32> = const { Cell::new(0) };
    }
    add_error_hook(move |_frame| {
        DEPTH.with(|d| {
            let depth = d.get();
            if depth < 2 {
                d.set(depth + 1);
                // Reentrant Fault construction inside the hook callback.
                let inner = Fault::from("reentrant");
                assert!(inner.to_string().contains("reentrant"));
                d.set(depth);
            }
        });
    });

    // Must complete — would deadlock if invoke held the lock across the hook.
    let f: Fault = Fault::from("outer");
    assert!(f.to_string().contains("outer"));
}
