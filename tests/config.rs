//! Config: backend-set selection roundtrips, throttle, global config restore.
//! (See MIGRATING.md for provenance.)

use fast_observe::config::{Backends, ObserveConfig, config};

/// Every named backend bit set.
fn all_backends() -> Backends {
    Backends::INSTANT
        | Backends::FASTRACE
        | Backends::WEB
        | Backends::PUFFIN
        | Backends::TRACY
        | Backends::SUPERLUMINAL
        | Backends::TRACING
}

#[test]
fn default_backend_is_fastrace() {
    // Documented default: fastrace. Fresh config — the global one may be
    // env-overridden in other test binaries.
    assert_eq!(ObserveConfig::new().backends(), Backends::FASTRACE);
}

#[test]
fn backends_set_get_roundtrip() {
    let cfg = ObserveConfig::new();
    for combo in [
        Backends::OFF,
        Backends::INSTANT,
        Backends::FASTRACE,
        Backends::WEB,
        Backends::FASTRACE | Backends::INSTANT,
        Backends::FASTRACE | Backends::PUFFIN | Backends::TRACY,
        all_backends(),
    ] {
        cfg.set_backends(combo);
        assert_eq!(cfg.backends(), combo);
    }
}

#[test]
fn off_is_empty() {
    assert_eq!(Backends::OFF, Backends::empty());
    assert!(Backends::OFF.is_empty());
    assert!(!Backends::FASTRACE.is_empty());
    assert!(all_backends().contains(Backends::TRACING));
    assert!(!Backends::OFF.contains(Backends::INSTANT));
}

#[test]
fn from_env_value_parses_comma_list() {
    assert_eq!(Backends::from_env_value("off"), Some(Backends::OFF));
    assert_eq!(
        Backends::from_env_value("Fastrace,TRACY"),
        Some(Backends::FASTRACE | Backends::TRACY)
    );
    assert_eq!(Backends::from_env_value("nope"), None);
    assert_eq!(Backends::from_env_value("off,instant"), None);
}

#[test]
fn throttle_set_get_roundtrip() {
    let cfg = ObserveConfig::new();
    assert_eq!(cfg.error_hook_throttle(), 0, "default: unlimited");
    cfg.set_error_hook_throttle(10);
    assert_eq!(cfg.error_hook_throttle(), 10);
    cfg.set_error_hook_throttle(0);
    assert_eq!(cfg.error_hook_throttle(), 0);
}

#[test]
fn global_config_default_then_restore() {
    let cfg = config();
    let original = cfg.backends();
    cfg.set_backends(Backends::INSTANT);
    assert_eq!(config().backends(), Backends::INSTANT);
    cfg.set_backends(original);
}
