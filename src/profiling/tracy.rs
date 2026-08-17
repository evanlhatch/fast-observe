//! Tracy backend — `scope!` → tracy zone (feature `profile-with-tracy`).
//!
//! Mirrors upstream `profiling`'s tracy impl. Upstream uses the `span!` macro
//! for string literals and `Client::span_alloc` for dynamic names; our names
//! are runtime values, so only the `span_alloc` path exists here.
//!
//! Unlike upstream (`.expect("scope! without a running tracy_client::Client")`),
//! a non-running client no-ops instead of panicking — runtime selection means
//! the bit can be set before the user starts the client. The user still owns
//! `tracy_client::Client::start()`; with tracy-client's `enable` feature off,
//! `Client::running()` yields a no-op client handle by design.

/// Enter a tracy zone with a runtime name. `tag` is attached as zone text.
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> TracyGuard {
    let span = tracy_client::Client::running().map(|client| {
        // callstack_depth 0: non-zero depth has significant overhead (upstream note).
        // `function`/`file`/`line` point at this glue — tracy shows `name` prominently.
        let span = client.span_alloc(Some(name), "fast_observe::scope", file!(), line!(), 0);
        if let Some(t) = tag {
            span.emit_text(t);
        }
        span
    });
    TracyGuard(span)
}

/// Construct a no-op `TracyGuard` (no real zone, does nothing on drop).
#[must_use]
pub const fn dummy() -> TracyGuard {
    TracyGuard(None)
}

/// Guard — the tracy zone ends on drop (when the inner `Span` is `Some`).
#[allow(dead_code, reason = "field held only for its Drop side effect")]
pub struct TracyGuard(Option<tracy_client::Span>);
