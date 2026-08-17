//! Config: backend selection roundtrips, throttle, global config restore.
//! (See MIGRATING.md for provenance.)

use fast_observe::config::{ObserveConfig, ProfilingBackend, config};

#[test]
fn default_backend_is_fastrace() {
    // Documented default: fastrace.
    assert_eq!(
        ObserveConfig::new().profiling_backend(),
        ProfilingBackend::Fastrace
    );
}

#[test]
fn backend_set_get_roundtrip() {
    let cfg = ObserveConfig::new();
    for backend in [
        ProfilingBackend::Off,
        ProfilingBackend::Instant,
        ProfilingBackend::Fastrace,
        ProfilingBackend::Web,
    ] {
        cfg.set_profiling_backend(backend);
        assert_eq!(cfg.profiling_backend(), backend);
    }
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
    let original = cfg.profiling_backend();
    cfg.set_profiling_backend(ProfilingBackend::Instant);
    assert_eq!(config().profiling_backend(), ProfilingBackend::Instant);
    cfg.set_profiling_backend(original);
}
