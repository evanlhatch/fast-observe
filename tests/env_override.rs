//! `OBSERVE_PROFILE` env-var override. Dedicated test binary: the global
//! config is a `LazyLock` initialized on first access, so this test must be
//! the only one in its process that touches `config()`.

use fast_observe::config::{Backends, config};

#[test]
#[allow(
    unsafe_code,
    reason = "env mutation before first config() access; this binary contains only this test, so no concurrent env readers exist"
)]
fn observe_profile_env_overrides_default() {
    // Safety: single test in this binary; env is set before the global
    // config LazyLock initializes (first `config()` call below).
    unsafe { std::env::set_var("OBSERVE_PROFILE", "instant") };
    assert!(config().backends().contains(Backends::INSTANT));
}
