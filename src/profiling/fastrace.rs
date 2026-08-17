//! Fastrace backend — `scope!` → fastrace `LocalSpan`.
//!
//! Coexists with the instant backend.

use fastrace::local::LocalSpan;

/// Enter a fastrace span. Returns a guard that ends the span on drop.
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> FastraceGuard {
    let mut span = LocalSpan::enter_with_local_parent(name);
    if let Some(t) = tag {
        span = span.with_property(|| ("tag", t));
    }
    FastraceGuard { span: Some(span) }
}

/// Construct a no-op `FastraceGuard` (no real span, does nothing on drop).
#[must_use]
pub fn dummy() -> FastraceGuard {
    FastraceGuard { span: None }
}

/// Guard — the `LocalSpan` ends on drop (when `span` is `Some`).
#[allow(dead_code, reason = "field held only for its Drop side effect")]
pub struct FastraceGuard {
    span: Option<LocalSpan>,
}
