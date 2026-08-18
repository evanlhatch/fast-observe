//! Tracing backend — `scope!` → tracing span (feature `profile-with-tracing`).
//!
//! Named `tracing_scope` (not `tracing`) to avoid clashing with the `tracing`
//! crate name in module paths.
//!
//! Dynamic span names: tracing span names are static callsite metadata, so
//! every scope uses the fixed span name `observe.scope` and records the
//! runtime `name` (and `tag`) as span fields.

use tracing::span::EnteredSpan;

/// Enter a tracing span; the runtime name is the `name` field.
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> TracingGuard {
    let span = if let Some(t) = tag {
        tracing::info_span!("observe.scope", name, tag = t)
    } else {
        tracing::info_span!("observe.scope", name)
    };
    TracingGuard(Some(span.entered()))
}

/// Construct a no-op `TracingGuard` (no real span, does nothing on drop).
#[must_use]
pub const fn dummy() -> TracingGuard {
    TracingGuard(None)
}

/// Guard — the entered span exits on drop (when the inner span is `Some`).
#[expect(dead_code, reason = "field held only for its Drop side effect")]
pub struct TracingGuard(Option<EnteredSpan>);
