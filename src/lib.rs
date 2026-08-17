// Nightly is required (see README). Gates, with tracking issues:
// - error_generic_member_access: Error::provide/request_ref — codes,
//   categories and attachments readable through &dyn Error
//   (https://github.com/rust-lang/rust/issues/99301)
#![feature(error_generic_member_access)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "instant")]
pub mod breakdown;
pub mod compat;
pub mod config;
pub mod deploy;
pub mod diagnostic;
pub mod errors;
pub mod exn;
pub mod hook;
pub mod profiling;
pub mod report;

#[cfg(feature = "fastrace")]
pub mod reporter;

#[cfg(feature = "instant")]
pub use breakdown::{drain_spans, print_breakdown};
pub use deploy::{Deployment, InitError, InitGuard, observe};
pub use diagnostic::{
    Diagnostic, LabelSpan, Severity, SourceSpan, eprint_diagnostic, register_source,
    render_diagnostic,
};
pub use errors::{
    CategoryTag, Coded, ERROR_REGISTRY, ErrorCode, ErrorRegistryEntry, doctor, error_registry,
    lookup_error,
};
pub use exn::{
    Attachment, BoxError, Context, ErrorExt, Fault, FaultCollection, Frame, FrameIter,
    InternalError, OptionExt, Placement, Result, ResultExt, SimpleError, error_counts,
};
pub use hook::{
    add_capture_hook, add_error_hook, clear_error_hooks, hooks_len, init, set_default_hook_enabled,
};
#[cfg(feature = "instant")]
pub use profiling::instant::SpanRecord;
pub use profiling::{
    all_functions, current_scope_elapsed_ms, current_scope_name, instrument, scope_path, skip,
};
#[cfg(feature = "serde")]
pub use report::render_report_json;
pub use report::{render_report, report_display};

/// Declarative error definitions — thiserror-compatible attributes plus
/// `#[code]`/`#[category]`/`#[advice]` registry integration.
pub use fast_observe_macros::error;

#[cfg(feature = "anyhow-boundary")]
pub use compat::anyhow_boundary;

/// The error category — drives retry/poison/abort policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::AsRefStr, derive_more::Display)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// Bad input — fix the caller's data.
    Content,
    /// Internal invariant broken — state may be corrupt.
    Invariant,
    /// Temporary failure — safe to retry.
    Transient,
    /// Unrecoverable — abort the process.
    Fatal,
}

impl ErrorCategory {
    /// What to DO about an error of this category.
    #[must_use]
    pub const fn policy(self) -> Policy {
        match self {
            Self::Content => Policy::FixInput,
            Self::Transient => Policy::Retry,
            Self::Invariant => Policy::Poison,
            Self::Fatal => Policy::Abort,
        }
    }
}

/// What to DO about an error — category made behavioral. Drives the
/// report's action line and retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// The input is wrong; retrying it unchanged will fail again.
    FixInput,
    /// Transient failure; safe to retry with backoff.
    Retry,
    /// State may be corrupt; unwind to a recovery boundary.
    Poison,
    /// Fatal invariant violation; do not continue this process.
    Abort,
}

impl Policy {
    /// The prescriptive sentence rendered in reports/doctor.
    #[must_use]
    pub const fn advice_line(self) -> &'static str {
        match self {
            Self::FixInput => "fix the input; retrying unchanged input will fail",
            Self::Retry => "safe to retry with backoff; if persistent, escalate",
            Self::Poison => "state may be corrupt; unwind to a recovery boundary and reinitialize",
            Self::Abort => "fatal invariant violation; do not continue this process",
        }
    }
}

pub use ErrorCategory as Category;

/// One-import surface: the canonical vocabulary (OBSERVE.md §5b).
/// `Result` here is `fast_observe::Result` — the anyhow-style default.
pub mod prelude {
    pub use crate::config::{Backends, config};
    pub use crate::{
        Attachment, BoxError, Category, Coded, Context, ErrorCategory, ErrorExt, Fault,
        FaultCollection, Frame, InternalError, OptionExt, Placement, Policy, Result, ResultExt,
        SimpleError, add_capture_hook, add_error_hook, all_functions, bail, define_errors, doctor,
        ensure, error, error_counts, error_registry, finish_frame, init, instrument, lookup_error,
        observe, render_report, report_display, scope, skip,
    };
}

// ── Macro support internals (not public API) ──────────────────────────────

#[doc(hidden)]
#[cfg(not(target_family = "wasm"))]
pub mod __private {
    pub use linkme::distributed_slice;
}

// ── Feature re-exports (plugin crates, opt-in) ────────────────────────────

#[cfg(feature = "http")]
pub use fastrace_reqwest;
#[cfg(feature = "bridge-tracing")]
pub use fastrace_tracing;
#[cfg(feature = "otel")]
pub use {fastrace_opentelemetry, logforth_append_opentelemetry};
