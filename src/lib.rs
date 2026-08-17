#![doc = include_str!("../README.md")]

#[cfg(feature = "instant")]
pub mod breakdown;
pub mod config;
pub mod diagnostic;
pub mod errors;
pub mod exn;
pub mod hook;
pub mod profiling;

#[cfg(feature = "instant")]
pub use breakdown::{drain_spans, print_breakdown};
pub use diagnostic::{Diagnostic, Severity, SourceSpan, eprint_diagnostic, render_diagnostic};
pub use errors::{ERROR_REGISTRY, ErrorRegistryEntry, error_registry, lookup_error};
pub use exn::{
    BoxError, Context, ErrorExt, Fault, Frame, InternalError, OptionExt, Result, ResultExt,
    SimpleError, error_counts,
};
pub use hook::{add_error_hook, init};
#[cfg(feature = "instant")]
pub use profiling::instant::SpanRecord;
pub use profiling::{all_functions, current_scope_name, skip};

/// The error category — drives retry/poison/abort policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::AsRefStr, derive_more::Display)]
#[non_exhaustive]
pub enum ErrorCategory {
    Content,
    Invariant,
    Transient,
    Fatal,
}

pub use ErrorCategory as Category;

// ── Feature re-exports (plugin crates, opt-in) ────────────────────────────

#[cfg(feature = "http")]
pub use fastrace_reqwest;
#[cfg(feature = "bridge-tracing")]
pub use fastrace_tracing;
#[cfg(feature = "otel")]
pub use {fastrace_opentelemetry, logforth_append_opentelemetry};
