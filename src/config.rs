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
    pub const OFF: Self = Self(0);
    pub const INSTANT: Self = Self(1 << 0);
    pub const FASTRACE: Self = Self(1 << 1);
    /// Instant spans + browser-console logging (feature `web`, wasm32).
    pub const WEB: Self = Self(1 << 2);
    pub const PUFFIN: Self = Self(1 << 3);
    pub const TRACY: Self = Self(1 << 4);
    pub const SUPERLUMINAL: Self = Self(1 << 6);
    pub const TRACING: Self = Self(1 << 7);

    pub const fn empty() -> Self {
        Self::OFF
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

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
        let mut tokens = 0_u32;
        let mut saw_off = false;
        for part in s.split(',') {
            tokens += 1;
            match part.trim().to_ascii_lowercase().as_str() {
                "off" => saw_off = true,
                "instant" => out |= Self::INSTANT,
                "fastrace" => out |= Self::FASTRACE,
                "web" => out |= Self::WEB,
                "puffin" => out |= Self::PUFFIN,
                "tracy" => out |= Self::TRACY,
                "superluminal" => out |= Self::SUPERLUMINAL,
                "tracing" => out |= Self::TRACING,
                _ => return None,
            }
        }
        // `off` alone → OFF; combined with anything else it is ambiguous.
        if saw_off && tokens > 1 {
            return None;
        }
        Some(out)
    }
}

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
    pub fn backends(&self) -> Backends {
        Backends(self.backends.load(Ordering::Relaxed))
    }

    /// One `log::warn!` per requested-but-not-compiled-in backend, once per
    /// process, naming the exact cargo feature to enable.
    fn warn_unavailable(requested: Backends) {
        /// Backend bits already warned about (warn once per process).
        static WARNED: AtomicU16 = AtomicU16::new(0);
        // `WEB` spans ride on the instant backend (the browser-console half is
        // a log appender), so its availability is the instant feature.
        let table = [
            (
                Backends::INSTANT,
                "instant",
                "instant",
                crate::profiling::instant_wrap::AVAILABLE,
            ),
            (
                Backends::FASTRACE,
                "fastrace",
                "fastrace",
                crate::profiling::fastrace_wrap::AVAILABLE,
            ),
            (
                Backends::WEB,
                "web",
                "web",
                crate::profiling::instant_wrap::AVAILABLE,
            ),
            (
                Backends::PUFFIN,
                "puffin",
                "profile-with-puffin",
                crate::profiling::puffin_wrap::AVAILABLE,
            ),
            (
                Backends::TRACY,
                "tracy",
                "profile-with-tracy",
                crate::profiling::tracy_wrap::AVAILABLE,
            ),
            (
                Backends::SUPERLUMINAL,
                "superluminal",
                "profile-with-superluminal",
                crate::profiling::superluminal_wrap::AVAILABLE,
            ),
            (
                Backends::TRACING,
                "tracing",
                "profile-with-tracing",
                crate::profiling::tracing_wrap::AVAILABLE,
            ),
        ];
        for (bit, name, feature, available) in table {
            if available || !requested.contains(bit) {
                continue;
            }
            if WARNED.fetch_or(bit.0, Ordering::AcqRel) & bit.0 == 0 {
                log::warn!(
                    target: "fast_observe.config",
                    "profiling backend '{name}' requested but not compiled in — enable cargo feature `{feature}`"
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
    if let Ok(value) = std::env::var("OBSERVE_PROFILE") {
        match Backends::from_env_value(&value) {
            Some(backends) => cfg.set_backends(backends),
            None => log::warn!(
                target: "fast_observe.config",
                "invalid OBSERVE_PROFILE={value:?}; expected comma-separated \
                 off|instant|fastrace|web|puffin|tracy|superluminal|tracing — keeping default"
            ),
        }
    }
    if let Ok(value) = std::env::var("OBSERVE_ERROR_THROTTLE") {
        match value.trim().parse::<u32>() {
            Ok(n) => cfg.set_error_hook_throttle(n),
            Err(_) => log::warn!(
                target: "fast_observe.config",
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
