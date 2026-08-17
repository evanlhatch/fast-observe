//! Async tracing surface (DESIGN.md §2 async gap).
//!
//! [`scope!`](crate::scope) guards are thread-bound: they enter/exit via
//! thread-local stacks, so a guard held across an `.await` point records
//! against whichever thread the task happens to be polled on. This module
//! provides the async-correct equivalents:
//!
//! - [`root_span!`](crate::root_span) — start (or continue) a trace and
//!   install it as the thread-local parent. Bind it per task, before the
//!   first `.await`.
//! - [`in_observed_span`] / [`ObservedFutureExt`] — wrap a future in a
//!   fastrace span that is entered on every poll, so the span follows the
//!   task across threads and suspension points.
//! - [`extract_traceparent`] / [`inject_traceparent`] — W3C
//!   `traceparent` propagation helpers (DESIGN.md §11b), thin wrappers over
//!   fastrace's codec.
//!
//! With feature `int-futures`, the [`futures`] module re-exports
//! `fastrace-futures` (`StreamExt::in_span` etc.) for `Stream`/`Sink`
//! instrumentation; `FutureExt` itself lives in fastrace core.

use std::future::Future;

use fastrace::collector::SpanContext;
use fastrace::future::FutureExt as _;
use fastrace::local::LocalParentGuard;
use sealed::sealed;

/// `Stream`/`Sink` tracing adapters from `fastrace-futures`
/// (`StreamExt::in_span`, …). `Future` instrumentation needs no extra crate —
/// use [`in_observed_span`].
#[cfg(feature = "int-futures")]
pub use fastrace_futures as futures;

/// Start a new trace: create a root span with a random
/// [`SpanContext`] and install it as the thread-local parent.
///
/// The returned guard keeps the trace context as the local parent until
/// dropped — child spans (`scope!`, `LocalSpan`, [`in_observed_span`])
/// attach to it. The root span itself is a zero-duration marker submitted
/// immediately; the guard is what carries the trace.
///
/// Drop order matters like any guard: bind it (`let _root = …`) and keep it
/// alive for the whole task/request.
#[must_use]
pub fn root_span(name: &'static str) -> LocalParentGuard {
    fastrace::Span::root(name, SpanContext::random()).set_local_parent()
}

/// Continue an incoming trace: create a root span whose parent is `ctx`
/// (same trace id) and install it as the thread-local parent.
///
/// Use at service boundaries with a context from [`extract_traceparent`].
/// Same drop semantics as [`root_span`].
#[must_use]
pub fn root_span_with(name: &'static str, ctx: SpanContext) -> LocalParentGuard {
    fastrace::Span::root(name, ctx).set_local_parent()
}

/// Wrap a future in a fastrace span that enters on poll (async-correct —
/// unlike [`scope!`](crate::scope) guards, which are thread-bound and must
/// never cross `.await`).
///
/// Implemented via [`fastrace::future::FutureExt::in_span`] with
/// [`Span::enter_with_local_parent`](fastrace::Span::enter_with_local_parent):
/// on every `poll` the span becomes the local parent, so nested `scope!` /
/// `LocalSpan` calls inside the future attach to it regardless of which
/// thread the executor polls on. Without an active local parent (see
/// [`root_span!`](crate::root_span)) the span is a no-op, matching fastrace
/// semantics.
pub fn in_observed_span<F: Future>(name: &'static str, f: F) -> impl Future<Output = F::Output> {
    f.in_span(fastrace::Span::enter_with_local_parent(name))
}

/// Extension trait so futures can be wrapped fluently:
/// `my_future.in_observed_span("load")`.
///
/// See [`in_observed_span`] for semantics. Sealed: blanket-implemented for
/// every [`Future`] — not intended for user implementation.
#[sealed]
pub trait ObservedFutureExt: Future + Sized {
    /// Wrap `self` in a fastrace span entered on every poll.
    fn in_observed_span(self, name: &'static str) -> impl Future<Output = Self::Output>;
}

impl<F: Future> __seal_observed_future_ext::Sealed for F {}
impl<F: Future> ObservedFutureExt for F {
    fn in_observed_span(self, name: &'static str) -> impl Future<Output = F::Output> {
        in_observed_span(name, self)
    }
}

/// Extract a [`SpanContext`] from a W3C `traceparent` header value.
///
/// Thin wrapper over [`SpanContext::decode_w3c_traceparent`]; returns `None`
/// for malformed headers. Only the `traceparent` portion is handled —
/// `tracestate` is not carried by [`SpanContext`] (see fastrace's
/// `W3CTraceContext` if state is needed).
#[must_use]
pub fn extract_traceparent(header: &str) -> Option<SpanContext> {
    SpanContext::decode_w3c_traceparent(header)
}

/// Render a [`SpanContext`] as a W3C `traceparent` header value
/// (`00-<trace-id>-<span-id>-<flags>`). Thin wrapper over
/// [`SpanContext::encode_w3c_traceparent`].
#[must_use]
pub fn inject_traceparent(ctx: &SpanContext) -> String {
    ctx.encode_w3c_traceparent()
}
