//! Deployment init tests. The global `log` logger is per-process, so this
//! binary holds the ONLY `observe().init()` calls in the test suite — every
//! test here shares one init via [`ensure_init`]; other test binaries must
//! not call `init()`.

use fast_observe::deploy::{DeploymentConfig, InitError, InitGuard, observe};
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

// DeploymentConfig tests: apply-only, never init() — the builder's fields
// are private, so assertions are success/failure of apply plus the serde
// roundtrip. Field-level behavior is covered by the unit tests in
// src/deploy.rs (same module = private-field access).

fn full_config() -> DeploymentConfig {
    DeploymentConfig {
        level: Some("debug".to_owned()),
        stdout: Some(false),
        layout: Some("json".to_owned()),
        file_from_env: Some(true),
        backends: Some("fastrace,tracy".to_owned()),
        error_hook_throttle: Some(7),
        traces: Some("off".to_owned()),
        panic_hook: Some(false),
        flush_on_exit: Some(false),
    }
}

#[test]
#[allow(clippy::expect_used, reason = "test")]
fn config_apply_sets_fields() {
    // Deployment has no Debug impl, but the Err side (Vec<ConfigError>)
    // does, so `expect` works for the Ok-expecting direction.
    let _deployment = full_config()
        .apply(observe())
        .expect("all-Some valid config must apply");
    // Deployment::from_config is the same overlay on observe().
    let cfg = DeploymentConfig {
        level: Some("warn".to_owned()),
        ..DeploymentConfig::default()
    };
    let _deployment = fast_observe::Deployment::from_config(cfg)
        .expect("valid config must apply via from_config");
}

#[test]
#[allow(clippy::expect_used, reason = "test")]
fn config_apply_rejects_bad_values() {
    // `.err()` not `.expect_err()`: Deployment has no Debug impl.
    let bad_level = DeploymentConfig {
        level: Some("bogus".to_owned()),
        ..DeploymentConfig::default()
    };
    let errors = bad_level
        .apply(observe())
        .err()
        .expect("bogus level must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field, "level");
    assert_eq!(errors[0].value, "bogus");

    let bad_backends = DeploymentConfig {
        backends: Some("nope".to_owned()),
        ..DeploymentConfig::default()
    };
    let errors = bad_backends
        .apply(observe())
        .err()
        .expect("bogus backends must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field, "backends");
    assert_eq!(errors[0].value, "nope");

    // Multiple bad fields → collected, not first-wins.
    let both_bad = DeploymentConfig {
        level: Some("bogus".to_owned()),
        backends: Some("nope".to_owned()),
        ..DeploymentConfig::default()
    };
    let errors = both_bad
        .apply(observe())
        .err()
        .expect("two bogus fields must fail");
    assert_eq!(errors.len(), 2, "errors collected, not first-wins");
    assert!(errors.iter().any(|e| e.field == "level"));
    assert!(errors.iter().any(|e| e.field == "backends"));
}

#[test]
#[allow(clippy::expect_used, reason = "test")]
fn config_apply_none_fields_are_noops() {
    let _deployment = DeploymentConfig::default()
        .apply(observe())
        .expect("all-None config must apply");
}

#[cfg(feature = "serde")]
#[test]
#[allow(clippy::expect_used, reason = "test")]
fn config_roundtrip_json() {
    let cfg = full_config();
    let json = serde_json::to_string(&cfg).expect("serialize");
    let back: DeploymentConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(cfg, back);

    // `deny_unknown_fields` rejects a bogus key.
    let bogus: Result<DeploymentConfig, _> = serde_json::from_str(r#"{"levl": "info"}"#);
    assert!(bogus.is_err(), "unknown field `levl` must be rejected");
}
