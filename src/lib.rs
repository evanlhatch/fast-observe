// Nightly is required (see README). Gates, with tracking issues:
// - error_generic_member_access: Error::provide/request_ref — codes,
//   categories and attachments readable through &dyn Error
//   (https://github.com/rust-lang/rust/issues/99301)
// - backtrace_frames: structured BacktraceFrame iteration for the report's
//   line-oriented backtrace attachments
//   (https://github.com/rust-lang/rust/issues/79676)
#![cfg_attr(
    feature = "backtrace",
    feature(error_generic_member_access, backtrace_frames)
)]
#![cfg_attr(not(feature = "backtrace"), feature(error_generic_member_access))]
// - error_iter: Error::sources — the cause-chain iterator used by
//   exn::walk_sources (https://github.com/rust-lang/rust/issues/58520)
#![feature(error_iter)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(docsrs, doc_auto_cfg)]
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
#[cfg(feature = "int-tokio")]
pub mod tokio_ext;

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
    lookup_error, register_statics,
};
pub use exn::{
    Attachment, Backoff, BoxError, BuiltinKey, Context, ErrorExt, Fault, FaultCollection, Frame,
    FrameIter, FrameKind, InternalError, OptionExt, Placement, Result, ResultExt, error_counts,
    error_counts_by_category, retry_with_backoff, retry_with_policy,
};
pub use hook::{
    HookId, add_capture_hook, add_error_hook, capture_hooks_len, clear_error_hooks, hooks_len,
    init, remove_error_hook, set_default_hook_enabled,
};
/// Nanoseconds from the target-selected monotonic clock (DESIGN.md §2):
/// fastant (TSC) native, web-time (→ std on WASI, `performance.now()` in
/// browsers) on wasm. Needed by wasm consumers, where no other public
/// API exposes a raw clock read.
pub use profiling::Nanos;
pub use profiling::clock::now_ns;
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
pub use fast_observe_macros::instrument_async;
pub use fast_observe_macros::main;

#[cfg(feature = "anyhow-boundary")]
pub use compat::anyhow_boundary;

/// The error category — drives retry/poison/abort policy.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    strum::AsRefStr,
    strum::IntoStaticStr,
    derive_more::Display,
)]
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
#[non_exhaustive]
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
        add_capture_hook, add_error_hook, all_functions, bail, doctor, ensure, error, error_counts,
        error_registry, finish_frame, init, instrument, instrument_async, lookup_error, main,
        observe, render_report, report_display, scope, skip,
    };
}

// ── Log targets (crate-internal) ───────────────────────────────────────────

/// The `fast_observe.*` log targets — one const per channel so a typo in a
/// literal can't silently split the log stream (a record under a mistyped
/// target matches nobody's filter).
pub(crate) mod log_targets {
    /// Error-hook events (the default hook, `ResultExt::report`).
    pub const ERROR: &str = "fast_observe.error";
    /// Deployment wiring (appender failures, init warnings).
    pub const DEPLOY: &str = "fast_observe.deploy";
    /// Runtime config (env parse warnings, backend availability).
    pub const CONFIG: &str = "fast_observe.config";
    /// Diagnostic (ariadne) rendering failures.
    pub const DIAGNOSTIC: &str = "fast_observe.diagnostic";
}

/// The `OBSERVE_*` (and honored `RUST_*`) env-var names — one const each,
/// so a typo in a literal can't silently disconnect a knob.
pub(crate) mod env_vars {
    /// Log level / rustlog spec (`OBSERVE_LOG` → `RUST_LOG`).
    pub const OBSERVE_LOG: &str = "OBSERVE_LOG";
    /// `env_logger`-style fallback log spec.
    pub const RUST_LOG: &str = "RUST_LOG";
    /// Rolling file appender directory (feature `file`).
    pub const OBSERVE_LOG_DIR: &str = "OBSERVE_LOG_DIR";
    /// Profiling backend mask (see [`crate::config::Backends`]).
    pub const OBSERVE_PROFILE: &str = "OBSERVE_PROFILE";
    /// Error-hook throttle (per type per second).
    pub const OBSERVE_ERROR_THROTTLE: &str = "OBSERVE_ERROR_THROTTLE";
    /// Default-hook report mode (`off|text|json`).
    pub const OBSERVE_REPORT: &str = "OBSERVE_REPORT";
    /// Color decision (`auto|always|never`).
    pub const OBSERVE_COLOR: &str = "OBSERVE_COLOR";
    /// Backtrace capture override (feature `backtrace`).
    #[cfg(feature = "backtrace")]
    pub const OBSERVE_BACKTRACE: &str = "OBSERVE_BACKTRACE";
    /// std backtrace knob, honored when `OBSERVE_BACKTRACE` is unset.
    #[cfg(feature = "backtrace")]
    pub const RUST_BACKTRACE: &str = "RUST_BACKTRACE";
    /// Report source-snippet toggle (see `report::render_report`).
    pub const OBSERVE_REPORT_SOURCE: &str = "OBSERVE_REPORT_SOURCE";
}

// ── Macro support internals (not public API) ──────────────────────────────

#[doc(hidden)]
pub mod __private {
    // Whole-crate re-export: linkme's macro expansion references
    // `::linkme::` paths; its `#[linkme(crate = ...)]` override redirects
    // them here so consumers need NO linkme dependency.
    #[cfg(not(target_family = "wasm"))]
    pub use linkme;
    #[cfg(not(target_family = "wasm"))]
    pub use linkme::distributed_slice;
    // Whole-crate re-export for `#[fast_observe::instrument_async]`'s
    // expansion (`#[fastrace::trace(...)]` with the crate path baked in) —
    // consumers need NO direct fastrace dependency.
    #[cfg(feature = "fastrace")]
    pub use ::fastrace;
}

/// Flush pending traces best-effort (DESIGN.md §4). Forwards to
/// `fastrace::flush()` when feature `fastrace` is compiled in; no-op
/// otherwise. Call at natural shutdown points — drop of the [`InitGuard`]
/// already does this, this is for early/forced flushes.
pub fn flush() {
    #[cfg(feature = "fastrace")]
    fastrace::flush();
}

// ── Feature re-exports (plugin crates, opt-in) ────────────────────────────

/// Crate-root alias for [`bench::divan`]: consumer bench files can write
/// `use fast_observe::divan;` without going through `bench::`.
#[cfg(feature = "bench")]
pub use bench::divan;
#[cfg(feature = "int-axum")]
pub use fastrace_axum;
#[cfg(feature = "reporter-datadog")]
pub use fastrace_datadog;
#[cfg(feature = "reporter-jaeger")]
pub use fastrace_jaeger;
#[cfg(feature = "int-poem")]
pub use fastrace_poem;
#[cfg(feature = "http")]
pub use fastrace_reqwest;
#[cfg(feature = "int-tonic")]
pub use fastrace_tonic;
#[cfg(feature = "int-tower")]
pub use fastrace_tower;
#[cfg(feature = "bridge-tracing")]
pub use fastrace_tracing;
#[cfg(feature = "otel")]
pub use hook::init_otel;
#[cfg(feature = "otel")]
pub use {fastrace_opentelemetry, logforth_append_opentelemetry};
