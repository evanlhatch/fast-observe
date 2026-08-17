//! `OBSERVE_PROFILE` + `OBSERVE_ERROR_THROTTLE` env-var overrides. Dedicated
//! test binary: the global config is a `LazyLock` initialized on first
//! access, so this test must be the only one in its process that touches
//! `config()` — both env vars are set before that first access.

use fast_observe::config::{Backends, config};

#[test]
#[allow(
    unsafe_code,
    reason = "env mutation before first config() access; this binary contains only this test, so no concurrent env readers exist"
)]
fn env_vars_override_config_defaults() {
    // Safety: single test in this binary; env is set before the global
    // config LazyLock initializes (first `config()` call below).
    unsafe {
        std::env::set_var("OBSERVE_PROFILE", "instant");
        std::env::set_var("OBSERVE_ERROR_THROTTLE", "7");
    }
    assert!(config().backends().contains(Backends::INSTANT));
    assert_eq!(config().error_hook_throttle(), 7);
}
