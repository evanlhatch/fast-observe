//! Function-scope tracking + unified `ScopeGuard` across all backends.
//! (See MIGRATING.md for provenance.)

use std::borrow::Cow;

use fast_observe::config::Backends;
use fast_observe::profiling::{
    ScopeGuard, current_scope_elapsed_ms, current_scope_name, enter_function_scope,
    enter_function_scope_with_tag, scope_path,
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
    // The unified guard must construct for every backend set without panic.
    let cfg = fast_observe::config::config();
    let original = cfg.backends();
    let all = Backends::INSTANT
        | Backends::FASTRACE
        | Backends::WEB
        | Backends::PUFFIN
        | Backends::TRACY
        | Backends::SUPERLUMINAL
        | Backends::TRACING;
    for combo in [
        Backends::OFF,
        Backends::INSTANT,
        Backends::FASTRACE,
        Backends::FASTRACE | Backends::PUFFIN,
        all,
    ] {
        cfg.set_backends(combo);
        drop(ScopeGuard::new_static("test_scope", None));
    }
    cfg.set_backends(original);
}

#[cfg(feature = "instant")]
#[test]
fn scope_macro_records_with_instant_backend() {
    let cfg = fast_observe::config::config();
    let original = cfg.backends();
    cfg.set_backends(Backends::INSTANT);
    fast_observe::profiling::instant::clear();
    {
        let _g = fast_observe::scope!("macro_scope");
    }
    let spans = fast_observe::drain_spans();
    cfg.set_backends(original);
    assert!(
        spans.iter().any(|s| s.name == "macro_scope"),
        "scope! must record to the instant backend when selected: {spans:?}"
    );
}

#[test]
fn nested_scopes_maintain_path() {
    let outer = enter_function_scope(Cow::Borrowed("outer"));
    let inner = enter_function_scope(Cow::Borrowed("inner"));

    assert_eq!(current_scope_name().as_deref(), Some("inner"));
    assert_eq!(
        scope_path(),
        [Cow::Borrowed("outer"), Cow::Borrowed("inner")]
    );
    assert!(current_scope_elapsed_ms().is_some());

    drop(inner);
    assert_eq!(current_scope_name().as_deref(), Some("outer"));
    assert_eq!(scope_path(), [Cow::Borrowed("outer")]);

    drop(outer);
    assert_eq!(scope_path(), Vec::<Cow<'static, str>>::new());
    assert!(current_scope_name().is_none());
    assert!(current_scope_elapsed_ms().is_none());
}

#[test]
fn leaf_elapsed_is_sane() {
    let _g = enter_function_scope(Cow::Borrowed("slept"));
    std::thread::sleep(std::time::Duration::from_millis(2));
    let elapsed = current_scope_elapsed_ms();
    assert!(
        elapsed.is_some_and(|ms| ms >= 1),
        "elapsed {elapsed:?} must be Some(>= 1ms) after 2ms sleep"
    );
}
