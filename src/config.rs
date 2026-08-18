//! Global runtime config — profiling backend-set selection, error-hook throttle.
//!
//! Hot-path reads are relaxed atomics (~2ns). Default backend: fastrace.
//!
//! Compiled-in ≠ active: `profile-with-*` cargo features compile backend glue
//! in; the [`Backends`] mask selects which compiled backends are live at
//! runtime.
//!
//! Env split: log-level envs (`OBSERVE_LOG` → `RUST_LOG`) and
//! `OBSERVE_LOG_DIR` are consumed by `crate::deploy`; this module owns
//! `OBSERVE_PROFILE` and `OBSERVE_ERROR_THROTTLE`.
//!
//! The environment variable `OBSERVE_PROFILE` overrides the backend set at
//! startup (first `config()` access). It is a comma-separated,
//! case-insensitive list:
//!
//! - `off` — profiling disabled; `scope!` is a ~2ns no-op. Must appear alone.
//! - `instant` — thread-local span accumulator (see `profiling::instant`).
//! - `fastrace` — fastrace `LocalSpan`s (default).
//! - `web` — instant spans; with feature `web` on wasm32, logs also go to the
//!   browser console.
//! - `puffin`, `tracy`, `superluminal`, `tracing` — Tier-2 backends,
//!   each requiring its `profile-with-*` cargo feature.
//!
//! Setting a bit whose feature is not compiled in logs a one-time warning
//! naming the exact cargo feature to enable.
//!
//! The environment variable `OBSERVE_ERROR_THROTTLE` (a `u32`) sets the
//! error-hook throttle at startup — max hook invocations per error type per
//! second. Unparseable values warn and keep the default (`0` = unlimited).
//!
//! ```ignore
//! use fast_observe::config::{Backends, config};
//! config().set_backends(Backends::FASTRACE | Backends::TRACY);
//! ```

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};

/// Runtime-selected profiling backend set. Compiled-in ≠ active:
/// features compile backends in; this mask selects which are live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Backends(u16);

impl Backends {
    /// Profiling disabled — `scope!` is a ~2ns no-op.
    pub const OFF: Self = Self(0);
    /// Thread-local span accumulator — deterministic per-phase breakdown.
    pub const INSTANT: Self = Self(1 << 0);
    /// fastrace `LocalSpan`s — the default.
    pub const FASTRACE: Self = Self(1 << 1);
    /// Instant spans + browser-console logging (feature `web`, wasm32).
    pub const WEB: Self = Self(1 << 2);
    /// puffin profiler (feature `profile-with-puffin`).
    pub const PUFFIN: Self = Self(1 << 3);
    /// Tracy profiler (feature `profile-with-tracy`).
    pub const TRACY: Self = Self(1 << 4);
    /// Superluminal profiler (feature `profile-with-superluminal`, windows).
    /// NOTE: bit 5 (`1 << 5`) is intentionally reserved/unused — do not
    /// assign it without checking persisted masks elsewhere.
    pub const SUPERLUMINAL: Self = Self(1 << 6);
    /// `tracing` span backend (feature `profile-with-tracing`).
    pub const TRACING: Self = Self(1 << 7);

    /// An empty backend set — `OFF` under a different name, for callers
    /// that prefer the set-semantics vocabulary over the power-off one.
    #[must_use]
    pub const fn empty() -> Self {
        Self::OFF
    }

    /// `true` when every bit of `other` is set in this mask.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// `true` for the empty set (see [`Backends::empty`]).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Parse an `OBSERVE_PROFILE` value: a comma-separated, case-insensitive
    /// list of backend names (`off|instant|fastrace|web|puffin|tracy|
    /// superluminal|tracing`). `off` must appear alone. Returns `None` for
    /// unrecognized values.
    #[must_use]
    pub fn from_env_value(s: &str) -> Option<Self> {
        let mut out = Self::OFF;
        let mut saw_off = false;
        let mut saw_other = false;
        for part in s.split(',') {
            let name = part.trim().to_ascii_lowercase();
            if name == "off" {
                saw_off = true;
                continue;
            }
            let info = BACKENDS_INFO.iter().find(|info| info.name == name)?;
            out |= info.bit;
            saw_other = true;
        }
        // `off` alone → OFF; combined with anything else it is ambiguous.
        if saw_off && saw_other {
            return None;
        }
        Some(out)
    }
}

/// One row per selectable backend: mask bit, env/config name, cargo feature,
/// compiled-in availability.
pub(crate) struct BackendInfo {
    /// The mask bit this row describes.
    pub bit: Backends,
    /// The env/config name matched by [`Backends::from_env_value`].
    pub name: &'static str,
    /// The cargo feature that compiles this backend in.
    pub feature: &'static str,
    /// `true` when the backend's cargo feature (and target constraint, if
    /// any) is compiled in for this build.
    pub available: bool,
}

/// Every selectable backend. `WEB`'s availability is the instant feature's
/// (spans ride on the instant backend; the console half is a log appender).
pub(crate) const BACKENDS_INFO: &[BackendInfo] = &[
    BackendInfo {
        bit: Backends::INSTANT,
        name: "instant",
        feature: "instant",
        available: crate::profiling::instant_wrap::AVAILABLE,
    },
    BackendInfo {
        bit: Backends::FASTRACE,
        name: "fastrace",
        feature: "fastrace",
        available: crate::profiling::fastrace_wrap::AVAILABLE,
    },
    BackendInfo {
        // Deliberate: `WEB` spans ride on the instant backend (the
        // browser-console half is a log appender), so its availability is
        // the instant feature, not `web`.
        bit: Backends::WEB,
        name: "web",
        feature: "web",
        available: crate::profiling::instant_wrap::AVAILABLE,
    },
    BackendInfo {
        bit: Backends::PUFFIN,
        name: "puffin",
        feature: "profile-with-puffin",
        available: crate::profiling::puffin_wrap::AVAILABLE,
    },
    BackendInfo {
        bit: Backends::TRACY,
        name: "tracy",
        feature: "profile-with-tracy",
        available: crate::profiling::tracy_wrap::AVAILABLE,
    },
    BackendInfo {
        bit: Backends::SUPERLUMINAL,
        name: "superluminal",
        feature: "profile-with-superluminal",
        available: crate::profiling::superluminal_wrap::AVAILABLE,
    },
    BackendInfo {
        bit: Backends::TRACING,
        name: "tracing",
        feature: "profile-with-tracing",
        available: crate::profiling::tracing_wrap::AVAILABLE,
    },
];

// NOTE: plain (non-const) trait impls — `impl const` would need the nightly
// `const_trait_impl` feature enabled at the crate root, which this crate
// does not set. The associated consts above are usable in const contexts.
impl std::ops::BitOr for Backends {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Backends {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Global runtime configuration.
pub struct ObserveConfig {
    backends: AtomicU16,
    /// Max error-hook invocations per error type per second. 0 = unlimited.
    error_hook_throttle: AtomicU32,
}

impl ObserveConfig {
    /// Start in the default state: fastrace backends, unlimited error-hook
    /// throttle. Use [`config()`] to get the process-global instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            backends: AtomicU16::new(Backends::FASTRACE.0),
            error_hook_throttle: AtomicU32::new(0),
        }
    }

    /// Set the active profiling backend set at runtime.
    ///
    /// Self-teaching: each requested backend whose cargo feature is not
    /// compiled in triggers a one-time `log::warn!` naming the exact feature
    /// to enable. The mask is stored as requested regardless — unavailable
    /// backends fall back to no-op stubs.
    pub fn set_backends(&self, backends: Backends) {
        let previous = Backends(self.backends.swap(backends.0, Ordering::AcqRel));
        Self::warn_unavailable(backends);
        // Backend enable hooks: puffin no-ops until `set_scopes_on(true)`.
        if backends.contains(Backends::PUFFIN)
            && !previous.contains(Backends::PUFFIN)
            && crate::profiling::puffin_wrap::AVAILABLE
        {
            crate::profiling::puffin_wrap::on_enable();
        }
    }

    /// Read the active profiling backend set (hot-path, relaxed atomic).
    #[must_use]
    pub fn backends(&self) -> Backends {
        Backends(self.backends.load(Ordering::Relaxed))
    }

    /// One `log::warn!` per requested-but-not-compiled-in backend, once per
    /// process, naming the exact cargo feature to enable.
    fn warn_unavailable(requested: Backends) {
        /// Backend bits already warned about (warn once per process).
        static WARNED: AtomicU16 = AtomicU16::new(0);
        for info in BACKENDS_INFO {
            if info.available || !requested.contains(info.bit) {
                continue;
            }
            if WARNED.fetch_or(info.bit.0, Ordering::AcqRel) & info.bit.0 == 0 {
                log::warn!(
                    target: crate::log_targets::CONFIG,
                    "profiling backend '{}' requested but not compiled in — enable cargo feature `{}`",
                    info.name,
                    info.feature
                );
            }
        }
    }

    /// Set the global error-hook throttle: at most `max_per_second` hook
    /// invocations per error type per second. 0 disables throttling
    /// (the default — unlimited).
    pub fn set_error_hook_throttle(&self, max_per_second: u32) {
        self.error_hook_throttle
            .store(max_per_second, Ordering::Release);
    }

    /// Read the error-hook throttle (0 = unlimited).
    #[must_use]
    pub fn error_hook_throttle(&self) -> u32 {
        self.error_hook_throttle.load(Ordering::Relaxed)
    }
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self::new()
    }
}

static CONFIG: LazyLock<ObserveConfig> = LazyLock::new(|| {
    let cfg = ObserveConfig::new();
    if let Ok(value) = std::env::var(crate::env_vars::OBSERVE_PROFILE) {
        match Backends::from_env_value(&value) {
            Some(backends) => cfg.set_backends(backends),
            None => log::warn!(
                target: crate::log_targets::CONFIG,
                "invalid OBSERVE_PROFILE={value:?}; expected comma-separated \
                 off|instant|fastrace|web|puffin|tracy|superluminal|tracing — keeping default"
            ),
        }
    }
    if let Ok(value) = std::env::var(crate::env_vars::OBSERVE_ERROR_THROTTLE) {
        match value.trim().parse::<u32>() {
            Ok(n) => cfg.set_error_hook_throttle(n),
            Err(_) => log::warn!(
                target: crate::log_targets::CONFIG,
                "invalid OBSERVE_ERROR_THROTTLE={value:?}; expected a u32 — keeping default (0 = unlimited)"
            ),
        }
    }
    cfg
});

/// Access the global runtime config. The `OBSERVE_PROFILE` and
/// `OBSERVE_ERROR_THROTTLE` env overrides are applied once, on first access.
#[must_use]
pub fn config() -> &'static ObserveConfig {
    &CONFIG
}

/// Parse a case-insensitive env enum: read `var`, trim + lowercase it, run
/// `parse` on the normalized value. On an unparseable value, warns on target
/// `fast_observe.config` and returns `default`. A missing var returns
/// `default` silently. Shared by every `OBSERVE_*` `LazyLock` (single
/// parse-once discipline — DESIGN.md §9c-ext).
pub(crate) fn env_enum<T: Copy>(
    var: &str,
    parse: impl Fn(&str) -> Option<T>,
    default: T,
    expected: &str,
) -> T {
    let Ok(value) = std::env::var(var) else {
        return default;
    };
    if let Some(parsed) = parse(value.trim().to_ascii_lowercase().as_str()) {
        return parsed;
    }
    log::warn!(
        target: crate::log_targets::CONFIG,
        "invalid {var}={value:?}; expected {expected} — keeping default"
    );
    default
}

// ── Report mode — OBSERVE_REPORT (DESIGN.md §7) ───────────────────────────

/// How the default error hook renders errors (`OBSERVE_REPORT`).
/// `Off` keeps the classic one-line `log::error!`; `Text`/`Json` emit the
/// full report block as one structured event instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ReportMode {
    #[default]
    /// Classic one-line hook log.
    Off,
    /// The full text report block (`report::render_frame_report`).
    Text,
    /// The versioned JSON report (feature `serde`). Falls back to `Text`
    /// without the feature.
    Json,
}

/// The default-hook report mode, resolved ONCE from `OBSERVE_REPORT`
/// (`off|text|json`, case-insensitive; anything else → `Off` + a warning).
static REPORT_MODE: LazyLock<ReportMode> = LazyLock::new(|| {
    env_enum(
        crate::env_vars::OBSERVE_REPORT,
        |name| match name {
            "off" | "" => Some(ReportMode::Off),
            "text" | "1" | "true" => Some(ReportMode::Text),
            "json" => {
                if cfg!(not(feature = "serde")) {
                    log::warn!(
                        target: crate::log_targets::CONFIG,
                        "OBSERVE_REPORT=json requested but cargo feature `serde` is not compiled in — falling back to text"
                    );
                    Some(ReportMode::Text)
                } else {
                    Some(ReportMode::Json)
                }
            }
            _ => None,
        },
        ReportMode::Off,
        "off|text|json",
    )
});

/// The default-hook report mode (see [`ReportMode`]).
#[must_use]
pub fn report_mode() -> ReportMode {
    *REPORT_MODE
}

// ── Color mode — OBSERVE_COLOR (DESIGN.md §3) ─────────────────────────────

/// Color decision for rendered output (`OBSERVE_COLOR`). `Auto` keeps the
/// tty + `NO_COLOR` + `TERM=dumb` discipline; `Always`/`Never` override it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ColorMode {
    #[default]
    /// tty && !`NO_COLOR` && TERM != "dumb".
    Auto,
    /// Force color even off-tty.
    Always,
    /// Force no color even on-tty.
    Never,
}

/// The color mode, resolved ONCE from `OBSERVE_COLOR`
/// (`auto|always|never`, case-insensitive; anything else → `Auto`).
static COLOR_MODE: LazyLock<ColorMode> = LazyLock::new(|| {
    env_enum(
        crate::env_vars::OBSERVE_COLOR,
        |name| match name {
            "always" | "1" | "true" => Some(ColorMode::Always),
            "never" | "0" | "false" => Some(ColorMode::Never),
            _ => None,
        },
        ColorMode::Auto,
        "auto|always|never",
    )
});

/// The color mode (see [`ColorMode`]).
#[must_use]
pub fn color_mode() -> ColorMode {
    *COLOR_MODE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_bits_roundtrip() {
        // The mask stores arbitrary u16 bits verbatim — no repr fallback.
        let cfg = ObserveConfig::new();
        cfg.backends.store(99, Ordering::Release);
        assert_eq!(cfg.backends(), Backends(99));
    }

    #[test]
    fn set_backends_stores_unavailable_bits() {
        // Requesting a backend whose feature is missing still stores the bit
        // (it just warns once); the stub path no-ops.
        let cfg = ObserveConfig::new();
        cfg.set_backends(Backends::FASTRACE | Backends::TRACY);
        assert_eq!(cfg.backends(), Backends::FASTRACE | Backends::TRACY);
    }

    #[test]
    fn parse_env_values() {
        assert_eq!(Backends::from_env_value("off"), Some(Backends::OFF));
        assert_eq!(Backends::from_env_value("OFF"), Some(Backends::OFF));
        assert_eq!(Backends::from_env_value("INSTANT"), Some(Backends::INSTANT));
        assert_eq!(
            Backends::from_env_value(" fastrace "),
            Some(Backends::FASTRACE)
        );
        assert_eq!(Backends::from_env_value("web"), Some(Backends::WEB));
        assert_eq!(
            Backends::from_env_value("fastrace,tracy"),
            Some(Backends::FASTRACE | Backends::TRACY)
        );
        assert_eq!(
            Backends::from_env_value("Instant, PUFFIN ,tracing"),
            Some(Backends::INSTANT | Backends::PUFFIN | Backends::TRACING)
        );
        assert_eq!(
            Backends::from_env_value("superluminal"),
            Some(Backends::SUPERLUMINAL)
        );
        assert_eq!(Backends::from_env_value("tracing"), Some(Backends::TRACING));
        // `off` combined with other backends is ambiguous → invalid.
        assert_eq!(Backends::from_env_value("off,fastrace"), None);
        assert_eq!(Backends::from_env_value("bogus"), None);
        assert_eq!(Backends::from_env_value(""), None);
    }
}
