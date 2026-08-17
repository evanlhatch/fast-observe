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
        } = self;

        // Fields consumed only under their cargo feature — mark them read in
        // the builds where the feature is off.
        #[cfg(not(feature = "file"))]
        let _ = file_from_env;
        #[cfg(not(feature = "fastrace"))]
        let _ = traces;

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

        Ok(InitGuard { _private: () })
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
            .build();
        assert_eq!(d.level, Some(log::LevelFilter::Debug));
        assert!(!d.stdout);
        assert!(matches!(d.layout, LayoutChoice::Json));
        assert!(d.file_from_env);
        assert_eq!(d.backends, Some(crate::config::Backends::OFF));
        assert_eq!(d.error_hook_throttle, Some(10));
        assert!(matches!(d.traces, TracesChoice::Off));
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
