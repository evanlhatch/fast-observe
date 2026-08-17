//! Function-scope tracking + unified `ScopeGuard` across all backends.
//! (See MIGRATING.md for provenance.)

use std::borrow::Cow;

use fast_observe::config::ProfilingBackend;
use fast_observe::profiling::{
    ScopeGuard, current_scope_name, enter_function_scope, enter_function_scope_with_tag,
};

#[test]
fn scope_name_none_outside_function_scope() {
    assert!(current_scope_name().is_none());
}

#[test]
fn function_scope_sets_and_clears_name() {
    {
        let _g = enter_function_scope(Cow::Borrowed("my_fn"));
        assert_eq!(current_scope_name().as_deref(), Some("my_fn"));
    }
    assert!(current_scope_name().is_none(), "guard drop must clear");
}

#[test]
fn function_scope_with_tag_appends() {
    let _g = enter_function_scope_with_tag(Cow::Borrowed("my_fn"), "tick");
    assert_eq!(current_scope_name().as_deref(), Some("my_fn:tick"));
}

#[test]
fn scope_guard_static_constructs() {
    // The unified guard must construct for every backend without panic.
    let cfg = fast_observe::config::config();
    let original = cfg.profiling_backend();
    for backend in [
        ProfilingBackend::Off,
        ProfilingBackend::Instant,
        ProfilingBackend::Fastrace,
        ProfilingBackend::Web,
    ] {
        cfg.set_profiling_backend(backend);
        drop(ScopeGuard::new_static("test_scope", None));
    }
    cfg.set_profiling_backend(original);
}

#[cfg(feature = "instant")]
#[test]
fn scope_macro_records_with_instant_backend() {
    let cfg = fast_observe::config::config();
    let original = cfg.profiling_backend();
    cfg.set_profiling_backend(ProfilingBackend::Instant);
    fast_observe::profiling::instant::clear();
    {
        let _g = fast_observe::scope!("macro_scope");
    }
    let spans = fast_observe::drain_spans();
    cfg.set_profiling_backend(original);
    assert!(
        spans.iter().any(|s| s.name == "macro_scope"),
        "scope! must record to the instant backend when selected: {spans:?}"
    );
}
