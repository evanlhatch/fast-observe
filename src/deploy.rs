//! Deployment — the configurable one-call wiring of logs + traces + profiling.
//!
//! [`hook::init()`](crate::hook::init) remains the zero-config path; this
//! module is the configurable path (DESIGN.md §3). At integration time
//! `hook::init()` will delegate here.
//!
//! Build via [`observe()`]; every capability is one toggle with a documented
//! default. Compiled-in ≠ active: cargo features compile backend glue in;
//! the toggles here (and the [`Backends`] mask) select what is live.
//!
//! Zero-config works with zero setters — `observe().init()` is the
//! battery-included default (stdout text logs at `Info`, fastrace console
//! reporter, error hooks as usual):
//!
//! ```ignore
//! let _guard = fast_observe::deploy::observe()
//!     .level(log::LevelFilter::Debug)
//!     .file_from_env(true)
//!     .init()?;
//! ```
//!
//! [`Backends`]: crate::config::Backends

use bon::Builder;

/// The observability deployment. Build via [`observe()`]; every capability
/// is one toggle with a documented default. Compiled-in ≠ active.
#[derive(Builder)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "builder toggles — each bool is an independent documented capability"
)]
pub struct Deployment {
    /// Max log level. Default: `OBSERVE_LOG` → `RUST_LOG` → `Info`.
    level: Option<log::LevelFilter>,
    /// Stdout appender. Default: `true`.
    #[builder(default = true)]
    stdout: bool,
    /// Stdout layout. Default: [`LayoutChoice::Text`] ([`LayoutChoice::Json`]
    /// requires feature `json`; without it a warning is logged and text is
    /// used).
    #[builder(default)]
    layout: LayoutChoice,
    /// Rolling file appender from `OBSERVE_LOG_DIR` (feature `file`).
    /// Default: `false`. No-op when the feature or the env var is missing.
    #[builder(default)]
    file_from_env: bool,
    /// Profiling backend mask. Default: [`Backends::FASTRACE`] (or
    /// `OBSERVE_PROFILE`) — applied to [`config()`](crate::config::config)
    /// only when set here.
    ///
    /// [`Backends::FASTRACE`]: crate::config::Backends::FASTRACE
    backends: Option<crate::config::Backends>,
    /// Error-hook throttle (per error type per second). Default: `None` =
    /// leave the global config untouched (0 = unlimited).
    error_hook_throttle: Option<u32>,
    /// fastrace reporter choice (feature `fastrace`). Default:
    /// [`TracesChoice::Console`].
    #[builder(default)]
    traces: TracesChoice,
    /// Panic hook: log panics as structured error events through the log
    /// pipeline, then chain to the previously installed hook (DESIGN.md
    /// §9b.3). Default: `true`. Chaining means a
    /// `console_error_panic_hook` set by a wasm app (or the cargo test
    /// harness hook) still runs after our log — the prior hook is never
    /// stomped.
    #[builder(default = true)]
    panic_hook: bool,
    /// Best-effort fastrace flush on process exit via `libc::atexit` +
    /// SIGTERM/SIGHUP handlers (feature `flush-on-exit`, native targets
    /// only — DESIGN.md §9b.4). Default: `true`. The field is always
    /// present; without the feature (or on wasm) it is a documented no-op.
    #[builder(default = true)]
    flush_on_exit: bool,
}

/// Stdout layout selector for [`Deployment::layout`].
///
/// [`Deployment::layout`]: DeploymentBuilder::layout
#[derive(Debug, Clone, Copy, Default)]
pub enum LayoutChoice {
    /// Human-readable plain text (logforth default layout).
    #[default]
    Text,
    /// One JSON object per line. Requires cargo feature `json`; without it
    /// the deployment warns and falls back to [`LayoutChoice::Text`].
    Json,
}

/// fastrace reporter selector for [`Deployment::traces`].
///
/// [`Deployment::traces`]: DeploymentBuilder::traces
#[derive(Debug, Default)]
pub enum TracesChoice {
    /// `ConsoleReporter` with `Config::default()` — spans print to stdout.
    #[default]
    Console,
    /// No reporter is installed — `set_reporter` is skipped entirely, so
    /// spans accumulate nowhere and the `FastraceEvent` log appender +
    /// `FastraceDiagnostic` are left out of the pipeline.
    Off,
}

/// Begin configuring the observability deployment — the entry point reads
/// as a verb. Finish with [`DeploymentBuilder::init`].
pub fn observe() -> DeploymentBuilder {
    Deployment::builder()
}

impl<S: deployment_builder::State> DeploymentBuilder<S> {
    /// Terminal: wire logs + traces + profiling per the toggles.
    ///
    /// Hand-written (not bon-generated) so it can fail: unlike
    /// [`hook::init()`](crate::hook::init), a logger that is already
    /// installed is reported, not swallowed.
    ///
    /// # Errors
    /// Returns [`InitError::AlreadyInitialized`] when the global `log`
    /// logger was already set — e.g. a second `init()`, or
    /// [`hook::init()`](crate::hook::init) ran first. Config toggles
    /// (`backends`, `error_hook_throttle`) are applied before the logger
    /// check and stay applied.
    pub fn init(self) -> Result<InitGuard, InitError> {
        self.build().wire()
    }
}

impl Deployment {
    fn wire(self) -> Result<InitGuard, InitError> {
        let Self {
            level,
            stdout,
            layout,
            file_from_env,
            backends,
            error_hook_throttle,
            traces,
            panic_hook,
            flush_on_exit,
        } = self;

        // Fields consumed only under their cargo feature — mark them read in
        // the builds where the feature is off.
        #[cfg(not(feature = "file"))]
        let _ = file_from_env;
        #[cfg(not(feature = "fastrace"))]
        let _ = traces;
        #[cfg(not(all(feature = "flush-on-exit", not(target_family = "wasm"))))]
        let _ = flush_on_exit;

        // 1. Runtime config toggles (applied even if logger init fails).
        if let Some(backends) = backends {
            crate::config::config().set_backends(backends);
        }
        if let Some(max_per_second) = error_hook_throttle {
            crate::config::config().set_error_hook_throttle(max_per_second);
        }

        // 2. Assemble appenders. `DispatchBuilder`'s typestate requires at
        // least one append per dispatch, so appenders are collected first;
        // an empty set means "logging compiled out" and we register a
        // dispatch-less logger (records are dropped) instead of failing.
        let mut appends: Vec<Box<dyn logforth::Append>> = Vec::new();

        #[cfg(feature = "fastrace")]
        let traces_on = !matches!(traces, TracesChoice::Off);

        if stdout {
            let base = logforth::append::Stdout::default();
            let stdout = match layout {
                LayoutChoice::Text => base,
                #[cfg(feature = "json")]
                LayoutChoice::Json => base.with_layout(logforth_layout_json::JsonLayout::default()),
                #[cfg(not(feature = "json"))]
                LayoutChoice::Json => {
                    log::warn!(
                        target: "fast_observe.deploy",
                        "LayoutChoice::Json requested but cargo feature `json` is not compiled in — using the text layout"
                    );
                    base
                }
            };
            appends.push(Box::new(stdout));
        }
        #[cfg(feature = "fastrace")]
        if traces_on {
            appends.push(Box::new(logforth_append_fastrace::FastraceEvent::default()));
        }
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        appends.push(Box::new(crate::profiling::web::WebConsoleAppend));
        #[cfg(feature = "file")]
        if file_from_env {
            if let Some(file) = file_appender() {
                appends.push(Box::new(file));
            }
        }

        // 3. Build the logforth pipeline, mirroring `hook::init()`'s
        // composition (multi-dispatch is a later refinement).
        let mut builder = logforth::starter_log::builder();
        let mut appends = appends.into_iter();
        if let Some(first) = appends.next() {
            builder = builder.dispatch(|d| {
                let d = d.diagnostic(logforth::diagnostic::ThreadLocalDiagnostic::default());
                #[cfg(feature = "fastrace")]
                let d = if traces_on {
                    d.diagnostic(logforth_diagnostic_fastrace::FastraceDiagnostic::default())
                } else {
                    d
                };
                let d = d.append(first);
                appends.fold(d, logforth::core::DispatchBuilder::append)
            });
        }

        // 4. Apply. A logger that is already installed is an error here —
        // the behavior difference from `hook::init()`, which ignores it.
        builder
            .try_apply()
            .map_err(|_| InitError::AlreadyInitialized)?;

        // 5. Logger is live: clamp the max level (try_apply sets Trace).
        log::set_max_level(resolve_level(level));

        // 6. Fastrace reporter (feature `fastrace`); `Off` skips
        // `set_reporter` entirely.
        #[cfg(feature = "fastrace")]
        if traces_on {
            fastrace::set_reporter(
                fastrace::collector::ConsoleReporter,
                fastrace::collector::Config::default(),
            );
        }

        // 7. Panic hook — after the logger is live, so panic logs have
        // somewhere to go.
        if panic_hook {
            install_panic_hook();
        }

        // 8. Best-effort flush on process exit (feature `flush-on-exit`) —
        // after the reporter exists, so the flush has somewhere to send
        // spans.
        #[cfg(all(feature = "flush-on-exit", not(target_family = "wasm")))]
        if flush_on_exit {
            install_exit_flush();
        }

        Ok(InitGuard { _private: () })
    }
}

/// Install a panic hook that logs the panic as a structured error event
/// THROUGH the log pipeline (target `fast_observe.panic`, kv fields
/// `panic_file`/`panic_line`, message = the panic payload string), then
/// CHAINS to the previously installed hook — composability: never stomp.
///
/// Category mapping: a panic is an unrecovered bug surfaced at runtime,
/// conceptually [`ErrorCategory::Fatal`](crate::ErrorCategory::Fatal)
/// events; logging them as structured errors makes a panic and a returned
/// error indistinguishable in a crash log (DESIGN.md §9b.3).
///
/// Chaining is `take_hook` + `set_hook`, not `std::panic::update_hook`:
/// `update_hook` is still unstable (feature `panic_update_hook`,
/// [rust#92649]) and this crate only enables
/// `error_generic_member_access`. The pair is not atomic — a concurrent
/// `set_hook` between them would be lost; `init()` runs at startup, before
/// user hooks, so this is acceptable. On wasm the chaining preserves a
/// `console_error_panic_hook` the app may have set — it runs after our
/// log, unchanged.
///
/// [rust#92649]: https://github.com/rust-lang/rust/issues/92649
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string payload>");
        match info.location() {
            Some(location) => log::error!(
                target: "fast_observe.panic",
                panic_file = location.file(),
                panic_line = location.line();
                "panic: {payload}"
            ),
            None => log::error!(target: "fast_observe.panic", "panic: {payload}"),
        }
        // Chain — never stomp the previous hook (the cargo test harness
        // hook, a wasm `console_error_panic_hook`, …).
        previous(info);
    }));
}

/// `extern "C"` trampoline invoked by `atexit` and the SIGTERM/SIGHUP
/// handlers: flush fastrace best-effort. Registered with `libc::atexit`
/// (normal exit) and `libc::signal` (SIGTERM/SIGHUP — servers die by
/// signal, not by Drop, so [`InitGuard`]'s drop flush never runs there).
/// The signal path is technically NOT async-signal-safe (`fastrace::flush`
/// allocates) and may misbehave if a signal lands mid-allocation; accepted
/// as a best-effort flush, documented in DESIGN.md §9b.4.
#[cfg(all(feature = "flush-on-exit", not(target_family = "wasm")))]
extern "C" fn fastrace_flush_trampoline() {
    fastrace::flush();
}

/// Register [`fastrace_flush_trampoline`] with `libc::atexit` and as the
/// SIGTERM/SIGHUP handler. Best-effort: a failed `atexit` registration
/// only logs a warning.
///
/// # Safety
/// `libc::atexit`/`libc::signal` are FFI calls with a C contract: `atexit`
/// takes an `extern "C" fn()` callback and `signal` takes the handler as a
/// `sighandler_t` (a `usize` alias). `fastrace_flush_trampoline` is
/// `extern "C" fn()` — matching the `atexit` contract exactly, and matching
/// the C signal-handler ABI on all supported targets (the caller passes one
/// `c_int` argument, which a zero-argument `extern "C"` callee safely
/// ignores).
#[cfg(all(feature = "flush-on-exit", not(target_family = "wasm")))]
#[allow(
    unsafe_code,
    function_casts_as_integer,
    reason = "libc::atexit/libc::signal FFI contract: sighandler_t is a usize alias"
)]
fn install_exit_flush() {
    // SAFETY: see the fn-level Safety section — the trampoline matches the
    // C ABI contract of both registration points.
    unsafe {
        if libc::atexit(fastrace_flush_trampoline) != 0 {
            log::warn!(
                target: "fast_observe.deploy",
                "libc::atexit registration failed — exit flush will not run on normal exit"
            );
        }
        libc::signal(
            libc::SIGTERM,
            fastrace_flush_trampoline as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGHUP,
            fastrace_flush_trampoline as libc::sighandler_t,
        );
    }
}

/// Resolve the max log level: explicit toggle → `OBSERVE_LOG` → `RUST_LOG` →
/// `Info`. Unparseable env values warn and fall through to the next source.
fn resolve_level(explicit: Option<log::LevelFilter>) -> log::LevelFilter {
    if let Some(level) = explicit {
        return level;
    }
    for var in ["OBSERVE_LOG", "RUST_LOG"] {
        if let Ok(value) = std::env::var(var) {
            match value.trim().parse::<log::LevelFilter>() {
                Ok(level) => return level,
                Err(_) => log::warn!(
                    target: "fast_observe.deploy",
                    "invalid {var}={value:?}; expected off|error|warn|info|debug|trace — falling through"
                ),
            }
        }
    }
    log::LevelFilter::Info
}

/// Rolling file appender, enabled by setting `OBSERVE_LOG_DIR` to a writable
/// directory. Logs roll into `<dir>/app.log`.
#[cfg(feature = "file")]
fn file_appender() -> Option<logforth_append_file::File> {
    let dir = std::env::var("OBSERVE_LOG_DIR").ok()?;
    match logforth_append_file::FileBuilder::new(dir, "app.log").build() {
        Ok(file) => Some(file),
        Err(e) => {
            log::error!(target: "fast_observe.deploy", "failed to build file appender: {e}");
            None
        }
    }
}

/// Guard returned by [`DeploymentBuilder::init`]. Keep it alive for the
/// process lifetime; dropping it flushes fastrace (feature `fastrace`,
/// best-effort) so the last spans are not lost at shutdown. Without the
/// feature, drop is a no-op.
#[must_use = "dropping the guard flushes fastrace — keep it alive until shutdown"]
pub struct InitGuard {
    _private: (),
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        #[cfg(feature = "fastrace")]
        fastrace::flush();
    }
}

/// Failure mode of [`DeploymentBuilder::init`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// The global `log` logger was already set — a second `init()`, or
    /// [`hook::init()`](crate::hook::init) ran first.
    AlreadyInitialized,
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => f.write_str(
                "observability already initialized: the global `log` logger is already set",
            ),
        }
    }
}

impl std::error::Error for InitError {}

#[cfg(test)]
mod tests {
    use super::*;

    // `build()` only — never `init()`: installing the global logger here
    // would race every other test in the process.

    #[test]
    fn zero_setters_build_defaults() {
        let d = observe().build();
        assert!(d.level.is_none());
        assert!(d.stdout);
        assert!(matches!(d.layout, LayoutChoice::Text));
        assert!(!d.file_from_env);
        assert!(d.backends.is_none());
        assert!(d.error_hook_throttle.is_none());
        assert!(matches!(d.traces, TracesChoice::Console));
        assert!(d.panic_hook);
        assert!(d.flush_on_exit);
    }

    #[test]
    fn setters_chain_in_any_combination() {
        let d = observe()
            .level(log::LevelFilter::Debug)
            .stdout(false)
            .layout(LayoutChoice::Json)
            .file_from_env(true)
            .backends(crate::config::Backends::OFF)
            .error_hook_throttle(10)
            .traces(TracesChoice::Off)
            .panic_hook(false)
            .flush_on_exit(false)
            .build();
        assert_eq!(d.level, Some(log::LevelFilter::Debug));
        assert!(!d.stdout);
        assert!(matches!(d.layout, LayoutChoice::Json));
        assert!(d.file_from_env);
        assert_eq!(d.backends, Some(crate::config::Backends::OFF));
        assert_eq!(d.error_hook_throttle, Some(10));
        assert!(matches!(d.traces, TracesChoice::Off));
        assert!(!d.panic_hook);
        assert!(!d.flush_on_exit);
    }

    #[test]
    fn level_resolution_order() {
        assert_eq!(
            resolve_level(Some(log::LevelFilter::Warn)),
            log::LevelFilter::Warn
        );
        // Env vars are process-global and racy under the test harness; only
        // assert the no-env fallback when the vars are absent.
        if std::env::var_os("OBSERVE_LOG").is_none() && std::env::var_os("RUST_LOG").is_none() {
            assert_eq!(resolve_level(None), log::LevelFilter::Info);
        }
    }
}
