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
    /// Syslog appender on the unix socket `/dev/log` (feature `log-syslog`).
    /// Default: `false`. The field is always present; without the feature
    /// (or off-unix, where the crate has no unix-socket sender) `wire()`
    /// warns on target `fast_observe.deploy` and skips the appender. A
    /// missing/unreachable `/dev/log` also warns and skips.
    #[builder(default)]
    syslog: bool,
    /// systemd journald appender (feature `log-journald`, unix-only — the
    /// crate is an empty stub off-unix). Default: `false`. The field is
    /// always present; without the feature (or off-unix) `wire()` warns on
    /// target `fast_observe.deploy` and skips the appender. An unreachable
    /// journald socket also warns and skips.
    #[builder(default)]
    journald: bool,
    /// Wrap the stdout and file appenders in
    /// `logforth_append_async::AsyncBuilder` (feature `log-async`) — logging
    /// and flushing happen on a background worker thread. Default: `false`.
    /// Without the feature `wire()` warns on target `fast_observe.deploy`
    /// and the appenders stay synchronous.
    #[builder(default)]
    async_append: bool,
    /// Add `logforth_diagnostic_task_local::TaskLocalDiagnostic` to the
    /// dispatch diagnostics (feature `diag-task-local`). Default: `false`.
    /// Without the feature `wire()` warns on target `fast_observe.deploy`
    /// and skips the diagnostic.
    #[builder(default)]
    task_local_diagnostic: bool,
    /// Use `logforth_filter_rustlog::RustLogFilter` (feature
    /// `filter-rustlog`) as the dispatch filter INSTEAD of the fixed level
    /// filter, reading the spec from `OBSERVE_LOG` → `RUST_LOG` (default
    /// `info`). Default: `false`. `log::set_max_level` is still applied as
    /// today, so it remains the global ceiling. Without the feature
    /// `wire()` warns on target `fast_observe.deploy` and skips the filter.
    #[builder(default)]
    rust_log_filter: bool,
    /// Route records at or above this level to a dedicated `Stderr`
    /// appender instead of the main appenders (DESIGN.md §3
    /// `.stderr_from(Level::Error)` — the standard ops split: errors to
    /// stderr, everything else to stdout/file). Default: `None` =
    /// single-dispatch (current behavior).
    stderr_from: Option<log::Level>,
    /// Static key-value context stamped on EVERY log record (app name,
    /// version, deployment id, …). Zero TLS cost (logforth
    /// `StaticDiagnostic`). Default: none.
    #[builder(default)]
    static_diag: Vec<(String, String)>,
}

/// Stdout layout selector for [`Deployment::layout`].
///
/// [`Deployment::layout`]: DeploymentBuilder::layout
#[derive(Debug, Clone, Copy, Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum LayoutChoice {
    /// Human-readable plain text (logforth default layout).
    #[default]
    Text,
    /// One JSON object per line. Requires cargo feature `json`; without it
    /// the deployment warns and falls back to [`LayoutChoice::Text`].
    Json,
    /// `key=value` logfmt lines. Requires cargo feature `layout-logfmt`;
    /// without it the deployment warns and falls back to
    /// [`LayoutChoice::Text`].
    Logfmt,
    /// Google Cloud Logging JSON (severity/trace fields per the GCL
    /// structured-logging contract). Requires cargo feature `layout-gcl`;
    /// without it the deployment warns and falls back to
    /// [`LayoutChoice::Text`].
    Gcl,
}

/// fastrace reporter selector for [`Deployment::traces`].
///
/// The `ConsoleWith`/`Custom` variants exist only under cargo feature
/// `fastrace`; without it the enum still compiles (the variants are
/// `#[cfg]`'d out) and `wire()` ignores the field entirely.
///
/// [`Deployment::traces`]: DeploymentBuilder::traces
#[derive(Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum TracesChoice {
    /// `ConsoleReporter` with `Config::default()` — spans print to stdout.
    #[default]
    Console,
    /// `ConsoleReporter` with a tuned
    /// [`fastrace::collector::Config`] — `report_interval` controls the
    /// batch flush cadence (memory/latency tradeoff on a busy trace;
    /// DESIGN.md §9c-ext). Not expressible as a string/`OBSERVE_*` env
    /// value (a config object), so `#[strum(disabled)]`.
    #[cfg(feature = "fastrace")]
    #[strum(disabled)]
    ConsoleWith(fastrace::collector::Config),
    /// Any custom reporter (`ConsoleReporter`, the `otel` feature's
    /// `OpenTelemetryReporter`, a [`crate::reporter::MultiReporter`]
    /// fan-out, …) with its own tuned
    /// [`fastrace::collector::Config`]. Not expressible as a string, so
    /// `#[strum(disabled)]`.
    #[cfg(feature = "fastrace")]
    #[strum(disabled)]
    Custom(
        Box<dyn fastrace::collector::Reporter>,
        fastrace::collector::Config,
    ),
    /// No reporter is installed — `set_reporter` is skipped entirely, so
    /// spans accumulate nowhere and the `FastraceEvent` log appender +
    /// `FastraceDiagnostic` are left out of the pipeline.
    Off,
}

/// The standard "toggle requested but its cargo feature is not compiled
/// in" warning on target `fast_observe.deploy` — one shape for the five
/// feature-gated deployment toggles (syslog, journald, async appends,
/// task-local diagnostic, rustlog filter).
///
/// `$toggle`: the field name as the user wrote it; `$feature`: the cargo
/// feature; `$tail`: what happens instead ("skipping … ", "appenders stay
/// synchronous", …).
///
/// Under `--all-features` every gate is open and none of the call sites
/// compile in — the macro itself is then unused, which is expected.
#[allow(
    unused_macros,
    reason = "used when any of the five feature-gated deployment toggles' cargo features is off"
)]
macro_rules! missing_feature_warn {
    ($toggle:literal, $feature:literal, $tail:literal) => {{
        log::warn!(
            target: crate::log_targets::DEPLOY,
            concat!(
                $toggle,
                " requested but cargo feature `",
                $feature,
                "` is not compiled in — ",
                $tail
            )
        );
    }};
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

impl DeploymentConfig {
    /// Build the config from the `OBSERVE_*` environment variables:
    ///
    /// | Env var               | Field                |
    /// |-----------------------|----------------------|
    /// | `OBSERVE_LOG`         | `level`              |
    /// | `OBSERVE_PROFILE`     | `backends`           |
    /// | `OBSERVE_ERROR_THROTTLE` | `error_hook_throttle` |
    /// | `OBSERVE_LOG_DIR`     | `file_from_env`      |
    ///
    /// Unset variables stay `None`. Values are NOT validated here —
    /// [`apply`](Self::apply) collects the parse errors. This is the
    /// single env-read point (DESIGN.md §3 "single `EnvConfig`, parsed
    /// once"): `wire()`/`config()` resolve every `OBSERVE_*` value
    /// through `LazyLock` statics or through this method, never ad-hoc
    /// re-reads.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            level: std::env::var(crate::env_vars::OBSERVE_LOG).ok(),
            stdout: None,
            layout: None,
            file_from_env: std::env::var_os(crate::env_vars::OBSERVE_LOG_DIR)
                .is_some()
                .then_some(true),
            backends: std::env::var(crate::env_vars::OBSERVE_PROFILE).ok(),
            error_hook_throttle: std::env::var(crate::env_vars::OBSERVE_ERROR_THROTTLE)
                .ok()
                .and_then(|v| v.parse().ok()),
            traces: None,
            panic_hook: None,
            flush_on_exit: None,
            syslog: None,
            journald: None,
            async_append: None,
            task_local_diagnostic: None,
            rust_log_filter: None,
        }
    }
}

impl Deployment {
    /// A [`Deployment`] configured from the `OBSERVE_*` environment
    /// variables — `OBSERVE_LOG` (level), `OBSERVE_PROFILE` (backend
    /// mask), `OBSERVE_ERROR_THROTTLE`, `OBSERVE_LOG_DIR` (file appender
    /// on). Unset variables leave the builder defaults. Unparseable values
    /// are reported as [`ConfigError`]s — nothing is applied.
    ///
    /// # Errors
    /// See [`DeploymentConfig::apply`].
    pub fn from_env() -> Result<Self, Vec<ConfigError>> {
        DeploymentConfig::from_env().apply(observe())
    }

    /// Begin configuring from a [`DeploymentConfig`] — equivalent to
    /// `cfg.apply(observe())`. Finish with [`Deployment::init`].
    ///
    /// # Errors
    /// Returns the collected [`ConfigError`]s when any field fails to parse;
    /// nothing is applied in that case.
    pub fn from_config(cfg: DeploymentConfig) -> Result<Self, Vec<ConfigError>> {
        cfg.apply(observe())
    }

    /// Terminal: wire logs + traces + profiling per the toggles — the
    /// [`DeploymentBuilder::init`] equivalent for a [`Deployment`] produced
    /// by [`DeploymentConfig::apply`] / [`Deployment::from_config`].
    ///
    /// # Errors
    /// Same as [`DeploymentBuilder::init`]: [`InitError::AlreadyInitialized`]
    /// when the global `log` logger was already set.
    pub fn init(self) -> Result<InitGuard, InitError> {
        self.wire()
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one sequential pipeline: config → appenders → dispatch → reporter → hooks — splitting would scatter a single logical flow"
    )]
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
            syslog,
            journald,
            async_append,
            task_local_diagnostic,
            rust_log_filter,
            stderr_from,
            static_diag,
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
        let traces_on = !matches!(&traces, TracesChoice::Off);

        if stdout {
            let base = logforth::append::Stdout::default();
            let stdout = match layout {
                LayoutChoice::Text => base,
                #[cfg(feature = "json")]
                LayoutChoice::Json => base.with_layout(logforth_layout_json::JsonLayout::default()),
                #[cfg(not(feature = "json"))]
                LayoutChoice::Json => {
                    log::warn!(
                        target: crate::log_targets::DEPLOY,
                        "LayoutChoice::Json requested but cargo feature `json` is not compiled in — using the text layout"
                    );
                    base
                }
                #[cfg(feature = "layout-logfmt")]
                LayoutChoice::Logfmt => {
                    base.with_layout(logforth_layout_logfmt::LogfmtLayout::default())
                }
                #[cfg(not(feature = "layout-logfmt"))]
                LayoutChoice::Logfmt => {
                    log::warn!(
                        target: crate::log_targets::DEPLOY,
                        "LayoutChoice::Logfmt requested but cargo feature `layout-logfmt` is not compiled in — using the text layout"
                    );
                    base
                }
                #[cfg(feature = "layout-gcl")]
                LayoutChoice::Gcl => base.with_layout(
                    logforth_layout_google_cloud_logging::GoogleCloudLoggingLayout::default(),
                ),
                #[cfg(not(feature = "layout-gcl"))]
                LayoutChoice::Gcl => {
                    log::warn!(
                        target: crate::log_targets::DEPLOY,
                        "LayoutChoice::Gcl requested but cargo feature `layout-gcl` is not compiled in — using the text layout"
                    );
                    base
                }
            };
            #[cfg(feature = "log-async")]
            appends.push(trap(
                maybe_async("fast-observe-log-stdout", stdout, async_append),
                "stdout",
            ));
            #[cfg(not(feature = "log-async"))]
            appends.push(trap(stdout, "stdout"));
        }
        #[cfg(feature = "fastrace")]
        if traces_on {
            appends.push(trap(
                logforth_append_fastrace::FastraceEvent::default(),
                "fastrace-events",
            ));
        }
        // Browser-only (`target_os = "unknown"` — see the `mod web` gate in
        // profiling.rs): on WASI the appender would call into wasm-bindgen
        // placeholder imports and panic the guest.
        #[cfg(all(feature = "web", target_arch = "wasm32", target_os = "unknown"))]
        appends.push(trap(crate::profiling::web::WebConsoleAppend, "web-console"));
        #[cfg(feature = "file")]
        if file_from_env && let Some(file) = file_appender() {
            #[cfg(feature = "log-async")]
            appends.push(trap(
                maybe_async("fast-observe-log-file", file, async_append),
                "file",
            ));
            #[cfg(not(feature = "log-async"))]
            appends.push(trap(file, "file"));
        }
        #[cfg(all(feature = "log-syslog", unix))]
        if syslog {
            match logforth_append_syslog::SyslogBuilder::unix("/dev/log") {
                Ok(builder) => appends.push(trap(builder.build(), "syslog")),
                Err(e) => log::warn!(
                    target: crate::log_targets::DEPLOY,
                    "failed to connect syslog socket /dev/log: {e} — skipping the syslog appender"
                ),
            }
        }
        #[cfg(not(all(feature = "log-syslog", unix)))]
        if syslog {
            missing_feature_warn!("syslog", "log-syslog", "skipping the syslog appender");
        }
        #[cfg(all(feature = "log-journald", unix))]
        if journald {
            match logforth_append_journald::Journald::new() {
                Ok(journald) => appends.push(trap(journald, "journald")),
                Err(e) => log::warn!(
                    target: crate::log_targets::DEPLOY,
                    "journald unavailable: {e} — skipping the journald appender"
                ),
            }
        }
        #[cfg(not(all(feature = "log-journald", unix)))]
        if journald {
            missing_feature_warn!("journald", "log-journald", "skipping the journald appender");
        }
        #[cfg(not(feature = "log-async"))]
        if async_append {
            missing_feature_warn!("async_append", "log-async", "appenders stay synchronous");
        }
        #[cfg(not(feature = "diag-task-local"))]
        if task_local_diagnostic {
            missing_feature_warn!(
                "task_local_diagnostic",
                "diag-task-local",
                "skipping the diagnostic"
            );
        }
        #[cfg(not(feature = "filter-rustlog"))]
        if rust_log_filter {
            missing_feature_warn!("rust_log_filter", "filter-rustlog", "skipping the filter");
        }

        // 3. Build the logforth pipeline (multi-dispatch when
        // `stderr_from` is set: the main appenders get records below the
        // split level, a dedicated `Stderr` appender gets records at/above
        // it — the standard ops split, DESIGN.md §3).
        let split_level = stderr_from.map(to_logforth_level);
        let mut builder = logforth::starter_log::builder();
        // Shared dispatch config (diagnostics + optional rustlog filter)
        // — used by the main dispatch and the stderr split dispatch alike.
        let common = |d: logforth::core::DispatchBuilder<false>| {
            let d = d.diagnostic(logforth::diagnostic::ThreadLocalDiagnostic::default());
            #[cfg(feature = "fastrace")]
            let d = if traces_on {
                d.diagnostic(logforth_diagnostic_fastrace::FastraceDiagnostic::default())
            } else {
                d
            };
            #[cfg(feature = "diag-task-local")]
            let d = if task_local_diagnostic {
                d.diagnostic(logforth_diagnostic_task_local::TaskLocalDiagnostic::default())
            } else {
                d
            };
            let d = if static_diag.is_empty() {
                d
            } else {
                d.diagnostic(logforth::diagnostic::StaticDiagnostic::new(
                    static_diag.iter().cloned().collect(),
                ))
            };
            #[cfg(feature = "filter-rustlog")]
            let d = if rust_log_filter {
                d.filter(build_rust_log_filter())
            } else {
                d
            };
            d
        };

        let mut appends = appends.into_iter();
        if let Some(first) = appends.next() {
            let main = |d: logforth::core::DispatchBuilder<false>| {
                let d = match split_level {
                    Some(lv) => d.filter(logforth::record::LevelFilter::MoreVerbose(lv)),
                    None => d,
                };
                let d = common(d);
                let d = d.append(first);
                appends.fold(d, logforth::core::DispatchBuilder::append)
            };
            builder = builder.dispatch(main);

            if let Some(lv) = split_level {
                builder = builder.dispatch(|d| {
                    let d = d.filter(logforth::record::LevelFilter::MoreSevereEqual(lv));
                    let d = common(d);
                    d.append(trap(logforth::append::Stderr::default(), "stderr-split"))
                });
            }
        } else if split_level.is_some() && !static_diag.is_empty() {
            // No main appenders (logging compiled out), but the caller still
            // configured a split and static context — keep a stderr-only
            // dispatch so error records aren't silently dropped.
            builder = builder.dispatch(|d| {
                let d = common(d);
                d.append(trap(logforth::append::Stderr::default(), "stderr-split"))
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
        // `set_reporter` entirely. `ConsoleWith`/`Custom` carry a tuned
        // [`Config`](fastrace::collector::Config) (`report_interval` — batch
        // flush cadence); `Custom` takes the reporter too.
        #[cfg(feature = "fastrace")]
        if traces_on {
            match traces {
                TracesChoice::Console | TracesChoice::Off => fastrace::set_reporter(
                    fastrace::collector::ConsoleReporter,
                    fastrace::collector::Config::default(),
                ),
                TracesChoice::ConsoleWith(config) => {
                    fastrace::set_reporter(fastrace::collector::ConsoleReporter, config);
                }
                TracesChoice::Custom(reporter, config) => {
                    fastrace::set_reporter(ReporterAdapter(reporter), config);
                }
            }
        }

        // 6b. wasm32-unknown-unknown (browser): register the pagehide
        // listener so the tail of the trace is flushed when the tab closes
        // or navigates away (feature `web` — where `install_unload_flush`
        // only exists + fastrace is on).
        #[cfg(all(
            feature = "web",
            feature = "fastrace",
            target_arch = "wasm32",
            target_os = "unknown"
        ))]
        crate::profiling::web::install_unload_flush();

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

/// Plain-data mirror of the builder: every field `Option`, every choice a
/// string-parsable value. Apps embed this in their own config
/// (figment/config-rs/toml) and convert — fast-observe stays
/// figment-compatible, NOT figment-dependent (SURFACE.md §2). Feature
/// `serde` gives `Serialize`/`Deserialize`; the type exists without it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default, deny_unknown_fields))]
pub struct DeploymentConfig {
    /// Max log level: `off|error|warn|info|debug|trace` (case-insensitive),
    /// parsed via [`log::LevelFilter::from_str`](std::str::FromStr).
    pub level: Option<String>,
    /// Stdout appender toggle.
    pub stdout: Option<bool>,
    /// Stdout layout: `text` | `json` | `logfmt` | `gcl` (case-insensitive).
    pub layout: Option<String>,
    /// Rolling file appender from `OBSERVE_LOG_DIR` (feature `file`).
    pub file_from_env: Option<bool>,
    /// Profiling backend mask — `OBSERVE_PROFILE` syntax: a comma-separated,
    /// case-insensitive list (`off|instant|fastrace|web|puffin|tracy|
    /// superluminal|tracing`), parsed via
    /// [`Backends::from_env_value`](crate::config::Backends::from_env_value).
    pub backends: Option<String>,
    /// Error-hook throttle (per error type per second).
    pub error_hook_throttle: Option<u32>,
    /// fastrace reporter choice: `console` | `off` (case-insensitive).
    pub traces: Option<String>,
    /// Panic hook toggle.
    pub panic_hook: Option<bool>,
    /// Best-effort fastrace flush on process exit toggle (feature
    /// `flush-on-exit`, native only).
    pub flush_on_exit: Option<bool>,
    /// Syslog appender toggle (feature `log-syslog`, unix socket sender).
    pub syslog: Option<bool>,
    /// systemd journald appender toggle (feature `log-journald`, unix only).
    pub journald: Option<bool>,
    /// Async stdout/file appender composition toggle (feature `log-async`).
    pub async_append: Option<bool>,
    /// Task-local diagnostic toggle (feature `diag-task-local`).
    pub task_local_diagnostic: Option<bool>,
    /// `RUST_LOG`-style per-module filter toggle (feature `filter-rustlog`);
    /// replaces the fixed level filter.
    pub rust_log_filter: Option<bool>,
}

impl DeploymentConfig {
    /// Overlay onto a builder — `Some` fields win, `None` leaves the
    /// builder's current value (the default, or whatever earlier setters
    /// established). Works on any builder state: bon 3's setters transition
    /// the typestate (`S::X: IsUnset` → `SetX<S>`), so conditional
    /// application cannot go through the setters; instead the builder is
    /// finished and the parsed values are overlaid on the built
    /// [`Deployment`]. Parse errors are collected and returned; nothing is
    /// applied on `Err`. Finish with [`Deployment::init`].
    ///
    /// # Errors
    /// Returns every [`ConfigError`] for the fields that failed to parse
    /// (not first-wins).
    pub fn apply<S: deployment_builder::State>(
        self,
        builder: DeploymentBuilder<S>,
    ) -> Result<Deployment, Vec<ConfigError>> {
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
            syslog,
            journald,
            async_append,
            task_local_diagnostic,
            rust_log_filter,
        } = self;
        let mut errors = Vec::new();

        // Parse first; the builder is only finished once every field is
        // known-good, so `Err` leaves nothing applied.
        let level = level.and_then(|value| {
            if let Ok(parsed) = value.trim().parse::<log::LevelFilter>() {
                Some(parsed)
            } else {
                errors.push(ConfigError {
                    field: "level",
                    value,
                    reason: "expected off|error|warn|info|debug|trace",
                });
                None
            }
        });
        let layout = layout.and_then(|value| {
            if let Ok(layout) = value.trim().to_ascii_lowercase().parse::<LayoutChoice>() {
                Some(layout)
            } else {
                errors.push(ConfigError {
                    field: "layout",
                    value,
                    reason: "expected text|json|logfmt|gcl",
                });
                None
            }
        });
        let backends = backends.and_then(|value| {
            if let Some(parsed) = crate::config::Backends::from_env_value(&value) {
                Some(parsed)
            } else {
                errors.push(ConfigError {
                    field: "backends",
                    value,
                    reason: "expected comma-separated off|instant|fastrace|web|puffin|tracy|superluminal|tracing (`off` alone)",
                });
                None
            }
        });
        let traces = traces.and_then(|value| {
            if let Ok(traces) = value.trim().to_ascii_lowercase().parse::<TracesChoice>() {
                Some(traces)
            } else {
                errors.push(ConfigError {
                    field: "traces",
                    value,
                    reason: "expected console|off",
                });
                None
            }
        });

        if !errors.is_empty() {
            return Err(errors);
        }

        // Every field parsed — overlay onto the built deployment; `None`
        // leaves the builder's value untouched.
        let base = builder.build();
        Ok(Deployment {
            level: level.or(base.level),
            stdout: stdout.unwrap_or(base.stdout),
            layout: layout.unwrap_or(base.layout),
            file_from_env: file_from_env.unwrap_or(base.file_from_env),
            backends: backends.or(base.backends),
            error_hook_throttle: error_hook_throttle.or(base.error_hook_throttle),
            traces: traces.unwrap_or(base.traces),
            panic_hook: panic_hook.unwrap_or(base.panic_hook),
            flush_on_exit: flush_on_exit.unwrap_or(base.flush_on_exit),
            syslog: syslog.unwrap_or(base.syslog),
            journald: journald.unwrap_or(base.journald),
            async_append: async_append.unwrap_or(base.async_append),
            task_local_diagnostic: task_local_diagnostic.unwrap_or(base.task_local_diagnostic),
            rust_log_filter: rust_log_filter.unwrap_or(base.rust_log_filter),
            stderr_from: base.stderr_from,
            static_diag: base.static_diag,
        })
    }
}

/// One failed [`DeploymentConfig`] field parse. `field` names the config
/// key, `value` is the offending input, `reason` names the expected values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The config field that failed to parse (e.g. `"level"`).
    pub field: &'static str,
    /// The offending input value.
    pub value: String,
    /// What the field expects (e.g. `"expected text|json"`).
    pub reason: &'static str,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            field,
            value,
            reason,
        } = self;
        write!(
            f,
            "invalid observe config field `{field}` = {value:?} — {reason}"
        )
    }
}

impl std::error::Error for ConfigError {}

/// Install a panic hook that logs the panic as a structured error event
/// Install a panic hook that routes the panic through the ERROR pipeline:
/// it constructs a [`Fault<PanicError>`] so the panic is counted in
/// `error_counts`, fires capture hooks (backtrace attachment), and is
/// logged/rendered by the error hooks per `OBSERVE_REPORT` — a panic and a
/// returned error are indistinguishable in a crash log (DESIGN.md §9b.3).
/// The real panic location rides as the `panic_location` attachment
/// (a `std::panic::Location` cannot be fabricated from the borrowed
/// runtime `PanicHookInfo` location). Then CHAINS to the previously
/// installed hook — composability: never stomp.
///
/// Category mapping: a panic is an unrecovered bug surfaced at runtime —
/// [`PanicError`] provides [`ErrorCategory::Fatal`](crate::ErrorCategory::Fatal),
/// so the report carries the fatal policy line.
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
/// A panic surfaced as an error — routes panics through the SAME pipeline
/// as returned errors (DESIGN.md §9b.3: indistinguishable in a crash log):
/// constructing a `Fault<PanicError>` in the panic hook fires the capture
/// hooks (backtrace attachment when enabled), the error hooks (the default
/// hook logs/renders per `OBSERVE_REPORT`), and the error counters under
/// this type name. `provide` marks the category [`ErrorCategory::Fatal`]
/// ([`ErrorCategory::Fatal`](crate::ErrorCategory::Fatal)), so the report
/// carries the fatal policy line.
#[derive(Debug)]
struct PanicError {
    payload: String,
}

impl std::fmt::Display for PanicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "panic: {}", self.payload)
    }
}

impl std::error::Error for PanicError {
    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        request.provide_value(crate::errors::CategoryTag(crate::ErrorCategory::Fatal));
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = crate::exn::payload_str(info.payload())
            .unwrap_or("<non-string payload>")
            .to_string();
        let location = info.location().map_or_else(
            || "<unknown>".to_string(),
            |l| format!("{}:{}", l.file(), l.line()),
        );
        // Contain: a panic while REPORTING a panic (e.g. OOM in fault
        // construction) must not abort inside the hook — chain regardless.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // The frame's `location:` is this crate's construction site
            // (a `std::panic::Location` cannot be fabricated from the
            // runtime-reported `PanicHookInfo::location`, which is neither
            // `'static` nor the same type) — the REAL panic location rides
            // as the `panic_location` attachment.
            let _fault = crate::exn::Fault::new(PanicError { payload })
                .attach_key("panic_location", location);
        }));
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
                target: crate::log_targets::DEPLOY,
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
    for var in [crate::env_vars::OBSERVE_LOG, crate::env_vars::RUST_LOG] {
        if let Ok(value) = std::env::var(var) {
            match value.trim().parse::<log::LevelFilter>() {
                Ok(level) => return level,
                Err(_) => log::warn!(
                    target: crate::log_targets::DEPLOY,
                    "invalid {var}={value:?}; expected off|error|warn|info|debug|trace — falling through"
                ),
            }
        }
    }
    log::LevelFilter::Info
}

/// Map a `log::Level` to logforth's extended `record::Level`: the log
/// facade only ever produces the five standard levels, while logforth's
/// enum carries OTel-style offsets (Trace2..Fatal4) — the standard levels
/// map to their base discriminants. Used for the `stderr_from` split
/// filters.
fn to_logforth_level(level: log::Level) -> logforth::record::Level {
    match level {
        log::Level::Error => logforth::record::Level::Error,
        log::Level::Warn => logforth::record::Level::Warn,
        log::Level::Info => logforth::record::Level::Info,
        log::Level::Debug => logforth::record::Level::Debug,
        log::Level::Trace => logforth::record::Level::Trace,
    }
}

/// Route an appender's failures through the log facade + fastrace event
/// stream instead of logforth-core's hardcoded stderr dump (D21: "a broken
/// appender must never break the app"). The FIRST failure per appender is
/// reported; later failures are swallowed, so a wedged appender cannot
/// starve the caller (and the report cannot recursively re-enter the same
/// failing appender through our own `log::error!`).
///
/// [`TrapAppender`] wraps every deployment appender — see [`TrapAppender`].
fn trap(
    append: impl Into<Box<dyn logforth::Append>>,
    name: &'static str,
) -> Box<dyn logforth::Append> {
    Box::new(TrapAppender::new(append.into(), name))
}

/// An [`Append`](logforth::Append) wrapper that converts a failed
/// `append`/`flush` into a structured `fast_observe.deploy` error event
/// (+ fastrace `error` span event under feature `fastrace`) instead of
/// logforth-core's unconditional stderr fallback.
///
/// Isolation story (DESIGN.md §3): a broken destination (full disk, dead
/// syslog socket, …) must not break the application — the error is still
/// RECORDED (observability), but the record is dropped at this appender.
#[derive(Debug)]
struct TrapAppender {
    inner: Box<dyn logforth::Append>,
    name: &'static str,
    reported: std::sync::OnceLock<()>,
}

impl TrapAppender {
    fn new(inner: Box<dyn logforth::Append>, name: &'static str) -> Self {
        Self {
            inner,
            name,
            reported: std::sync::OnceLock::new(),
        }
    }

    /// Report an appender failure — once per appender, through the log
    /// facade + the fastrace event stream under `fastrace`.
    fn report(&self, err: &logforth::Error) {
        let _: &() = self.reported.get_or_init(|| {
            log::error!(
                target: crate::log_targets::DEPLOY,
                appender = self.name;
                "log appender failed: {err} — records to this destination are being dropped",
            );
            #[cfg(feature = "fastrace")]
            fastrace::local::LocalSpan::add_event(
                fastrace::Event::new("log_appender_failure").with_properties(|| {
                    use std::borrow::Cow;
                    [
                        (Cow::Borrowed("appender"), Cow::Borrowed(self.name)),
                        (Cow::Borrowed("error"), Cow::Owned(err.to_string())),
                    ]
                }),
            );
        });
    }
}

impl logforth::Append for TrapAppender {
    fn append(
        &self,
        record: &logforth::record::Record,
        diags: &[Box<dyn logforth::diagnostic::Diagnostic>],
    ) -> Result<(), logforth::Error> {
        match self.inner.append(record, diags) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.report(&err);
                // Swallow: a broken appender must never propagate failure
                // into the log caller (isolation), but the failure is now
                // visible through observability.
                Ok(())
            }
        }
    }

    fn flush(&self) -> Result<(), logforth::Error> {
        match self.inner.flush() {
            Ok(()) => Ok(()),
            Err(err) => {
                self.report(&err);
                Ok(())
            }
        }
    }
}

/// Object-safe bridge: fastrace's `set_reporter` takes `impl Reporter` (a
/// concrete type), but [`TracesChoice::Custom`] stores a `Box<dyn Reporter>`
/// — this adapter restores the concrete impl by delegating.
#[cfg(feature = "fastrace")]
struct ReporterAdapter(Box<dyn fastrace::collector::Reporter>);

#[cfg(feature = "fastrace")]
impl fastrace::collector::Reporter for ReporterAdapter {
    fn report(&mut self, spans: Vec<fastrace::collector::SpanRecord>) {
        self.0.report(spans);
    }
}

/// Wrap an appender in `logforth_append_async::AsyncBuilder` when `enabled`
/// (feature `log-async`): logging and flushing move to a named background
/// worker thread. Pass-through when `enabled` is false.
#[cfg(feature = "log-async")]
fn maybe_async(
    thread_name: &'static str,
    append: impl Into<Box<dyn logforth::Append>>,
    enabled: bool,
) -> Box<dyn logforth::Append> {
    if enabled {
        Box::new(
            logforth_append_async::AsyncBuilder::new(thread_name)
                .append(append)
                .build(),
        )
    } else {
        append.into()
    }
}

/// The `RUST_LOG`-style per-module filter (feature `filter-rustlog`): spec
/// from `OBSERVE_LOG`, else `RUST_LOG`, else `info`. Malformed directives
/// are ignored by the filter builder (it warns to stderr).
#[cfg(feature = "filter-rustlog")]
fn build_rust_log_filter() -> logforth_filter_rustlog::RustLogFilter {
    use logforth_filter_rustlog::RustLogFilterBuilder;
    match std::env::var(crate::env_vars::OBSERVE_LOG) {
        Ok(spec) => RustLogFilterBuilder::from_spec(spec).build(),
        Err(_) => RustLogFilterBuilder::from_default_env_or("info").build(),
    }
}

/// Rolling file appender, enabled by setting `OBSERVE_LOG_DIR` to a writable
/// directory. Logs roll into `<dir>/app.log`.
#[cfg(feature = "file")]
fn file_appender() -> Option<logforth_append_file::File> {
    let dir = std::env::var(crate::env_vars::OBSERVE_LOG_DIR).ok()?;
    match logforth_append_file::FileBuilder::new(dir, "app.log").build() {
        Ok(file) => Some(file),
        Err(e) => {
            log::error!(target: crate::log_targets::DEPLOY, "failed to build file appender: {e}");
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
            .syslog(true)
            .journald(true)
            .async_append(true)
            .task_local_diagnostic(true)
            .rust_log_filter(true)
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
        assert!(d.syslog);
        assert!(d.journald);
        assert!(d.async_append);
        assert!(d.task_local_diagnostic);
        assert!(d.rust_log_filter);
    }

    #[test]
    fn toggles_default_off() {
        let d = observe().build();
        assert!(!d.syslog);
        assert!(!d.journald);
        assert!(!d.async_append);
        assert!(!d.task_local_diagnostic);
        assert!(!d.rust_log_filter);
    }

    #[test]
    fn config_apply_sets_fields() {
        let cfg = DeploymentConfig {
            level: Some("debug".to_owned()),
            stdout: Some(false),
            layout: Some("json".to_owned()),
            file_from_env: Some(true),
            backends: Some("fastrace,tracy".to_owned()),
            error_hook_throttle: Some(7),
            traces: Some("off".to_owned()),
            panic_hook: Some(false),
            flush_on_exit: Some(false),
            syslog: Some(true),
            journald: Some(true),
            async_append: Some(true),
            task_local_diagnostic: Some(true),
            rust_log_filter: Some(true),
        };
        // Never `.init()` — the global logger is per-process.
        let Ok(d) = cfg.apply(observe()) else {
            unreachable!("all-Some config must apply")
        };
        assert_eq!(d.level, Some(log::LevelFilter::Debug));
        assert!(!d.stdout);
        assert!(matches!(d.layout, LayoutChoice::Json));
        assert!(d.file_from_env);
        assert_eq!(
            d.backends,
            Some(crate::config::Backends::FASTRACE | crate::config::Backends::TRACY)
        );
        assert_eq!(d.error_hook_throttle, Some(7));
        assert!(matches!(d.traces, TracesChoice::Off));
        assert!(!d.panic_hook);
        assert!(!d.flush_on_exit);
        assert!(d.syslog);
        assert!(d.journald);
        assert!(d.async_append);
        assert!(d.task_local_diagnostic);
        assert!(d.rust_log_filter);
    }

    #[test]
    fn config_apply_collects_errors_and_leaves_defaults() {
        let cfg = DeploymentConfig {
            level: Some("bogus".to_owned()),
            backends: Some("nope".to_owned()),
            ..DeploymentConfig::default()
        };
        let Err(errors) = cfg.apply(observe()) else {
            unreachable!("bad values must not apply")
        };
        assert_eq!(errors.len(), 2, "errors collected, not first-wins");
        assert!(
            errors
                .iter()
                .any(|e| e.field == "level" && e.value == "bogus")
        );
        assert!(
            errors
                .iter()
                .any(|e| e.field == "backends" && e.value == "nope")
        );

        // All-None config is a no-op overlay: builder keeps its defaults.
        let Ok(d) = DeploymentConfig::default().apply(observe()) else {
            unreachable!("all-None config must apply")
        };
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
    fn config_apply_overlays_preset_builder() {
        // `Some` wins over an earlier setter; `None` preserves it.
        let builder = observe()
            .level(log::LevelFilter::Error)
            .error_hook_throttle(3);
        let cfg = DeploymentConfig {
            level: Some("debug".to_owned()),
            ..DeploymentConfig::default()
        };
        let Ok(d) = cfg.apply(builder) else {
            unreachable!("valid config must apply")
        };
        assert_eq!(d.level, Some(log::LevelFilter::Debug), "Some wins");
        assert_eq!(d.error_hook_throttle, Some(3), "None preserves the setter");
    }

    #[test]
    fn from_config_matches_apply_on_observe() {
        let cfg = DeploymentConfig {
            level: Some("warn".to_owned()),
            ..DeploymentConfig::default()
        };
        let Ok(d) = Deployment::from_config(cfg) else {
            unreachable!("valid config must apply")
        };
        assert_eq!(d.level, Some(log::LevelFilter::Warn));
    }

    #[test]
    fn level_resolution_order() {
        assert_eq!(
            resolve_level(Some(log::LevelFilter::Warn)),
            log::LevelFilter::Warn
        );
        // Env vars are process-global and racy under the test harness; only
        // assert the no-env fallback when the vars are absent.
        if std::env::var_os(crate::env_vars::OBSERVE_LOG).is_none()
            && std::env::var_os(crate::env_vars::RUST_LOG).is_none()
        {
            assert_eq!(resolve_level(None), log::LevelFilter::Info);
        }
    }
}
