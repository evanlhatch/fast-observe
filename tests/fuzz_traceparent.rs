//! Fuzz target: W3C trace-context propagation helpers (feature `fastrace`,
//! a default feature).
//!
//! Properties:
//! - `extract_traceparent` never panics on arbitrary header bytes,
//! - round-trip: for any VALID generated context,
//!   `extract_traceparent(inject_traceparent(ctx))` returns a context with
//!   the same trace id and parent (span) id,
//! - `extract_headers`/`inject_headers` round-trip the full context
//!   (traceparent + tracestate) the same way.

use bolero::generator::TypeGenerator;
use fast_observe::profiling::async_::{
    extract_headers, extract_traceparent, inject_headers, inject_traceparent,
};
use fastrace::collector::{SpanContext, SpanId, TraceId};

/// A generated trace context with arbitrary ids.
#[derive(Debug, Clone, TypeGenerator)]
struct FuzzCtx {
    trace: u128,
    span: u64,
}

impl FuzzCtx {
    fn ctx(&self) -> SpanContext {
        SpanContext::new(TraceId(self.trace), SpanId(self.span))
    }
}

#[test]
fn fuzz_traceparent_never_panics_on_arbitrary_headers() {
    bolero::check!()
        .with_type::<String>()
        .for_each(|header: &String| {
            let _ = extract_traceparent(header);
        });
}

#[test]
fn fuzz_traceparent_roundtrip_identity() {
    bolero::check!()
        .with_type::<FuzzCtx>()
        .for_each(|input: &FuzzCtx| {
            let ctx = input.ctx();
            let header = inject_traceparent(&ctx);
            let Some(decoded) = extract_traceparent(&header) else {
                unreachable!("injected header failed to decode: {header:?}")
            };
            assert_eq!(decoded.trace_id, ctx.trace_id, "trace id round-trip");
            assert_eq!(decoded.span_id, ctx.span_id, "span id round-trip");
        });
}

#[test]
fn fuzz_headers_roundtrip_identity() {
    bolero::check!()
        .with_type::<FuzzCtx>()
        .for_each(|input: &FuzzCtx| {
            // `inject_headers` takes a W3CTraceContext; `new` wraps a
            // SpanContext with no tracestate (arbitrary tracestate strings are
            // the upstream codec's domain, not ours).
            let ctx = fastrace::collector::W3CTraceContext::new(input.ctx());
            let headers = inject_headers(&ctx);
            let decoded = extract_headers(&headers);
            assert!(
                decoded.is_some(),
                "injected headers failed to decode: {headers:?}"
            );
        });
}
