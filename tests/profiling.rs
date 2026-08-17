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

// ── Async tracing surface (DESIGN.md §2 async gap) ────────────────────────

#[cfg(feature = "fastrace")]
mod async_tracing {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll};

    use fast_observe::profiling::async_::{
        ObservedFutureExt, extract_traceparent, in_observed_span, inject_traceparent,
    };
    use fastrace::collector::{SpanContext, SpanId, TraceId};

    /// Minimal executor: poll the future on this thread with the std noop
    /// waker until ready.
    fn block_on<F: Future>(f: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut f = pin!(f);
        loop {
            match f.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    /// Current local parent, with a loud failure when absent (feature-gated
    /// fastrace must be built with its `enable` feature for these tests to
    /// observe anything).
    fn local_parent() -> SpanContext {
        let parent = SpanContext::current_local_parent();
        assert!(parent.is_some(), "expected an active local parent");
        parent.unwrap_or_else(SpanContext::random)
    }

    #[test]
    fn root_span_creates_local_parent() {
        assert!(
            SpanContext::current_local_parent().is_none(),
            "no local parent before root_span!"
        );
        {
            let _root = fast_observe::root_span!("request");
            let _ = local_parent();
        }
        assert!(
            SpanContext::current_local_parent().is_none(),
            "guard drop must clear the local parent"
        );
    }

    #[test]
    fn root_span_continues_context() {
        let ctx = {
            let _root = fast_observe::root_span!("a");
            local_parent()
        };
        let _root = fast_observe::root_span!("b", ctx);
        assert_eq!(
            local_parent().trace_id,
            ctx.trace_id,
            "continuation form must keep the incoming trace id"
        );
    }

    #[test]
    fn in_observed_span_smoke() {
        let _root = fast_observe::root_span!("task");
        let out = block_on(
            in_observed_span("load", async {
                // in_span enters the span on poll, so the local parent is
                // active inside the future body.
                let _ = local_parent();
                42
            })
            .in_observed_span("outer"),
        );
        assert_eq!(out, 42);
    }

    #[test]
    fn traceparent_roundtrip() {
        // NB: SpanContext::random() has span_id == 0, which the W3C codec
        // rejects as invalid — build a context with a nonzero span id.
        let ctx = SpanContext::new(TraceId::random(), SpanId::random());
        let header = inject_traceparent(&ctx);
        let back = extract_traceparent(&header);
        assert!(back.is_some(), "roundtrip header must decode: {header}");
        assert_eq!(back.map(|c| c.trace_id), Some(ctx.trace_id));
        assert!(extract_traceparent("not-a-traceparent").is_none());
    }
}
