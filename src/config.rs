//! Global runtime config — profiling backend selection, error-hook throttle.
//!
//! Hot-path reads are relaxed atomics (~2ns). Default backend: fastrace.
//!
//! The environment variable `OBSERVE_PROFILE` overrides the profiling backend
//! at startup (first `config()` access):
//!
//! - `off` — profiling disabled; `scope!` is a ~2ns no-op.
//! - `instant` — thread-local span accumulator (see `profiling::instant`).
//! - `fastrace` — fastrace `LocalSpan`s (default).
//! - `web` — instant spans; with feature `web` on wasm32, logs also go to the
//!   browser console.
//!
//! ```ignore
//! use fast_observe::config::{config, ProfilingBackend};
//! config().set_profiling_backend(ProfilingBackend::Instant);
//! ```

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// Profiling backend selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProfilingBackend {
    Off = 0,
    Instant = 1,
    Fastrace = 2,
    /// Instant spans + browser-console logging (feature `web`, wasm32).
    /// On native targets behaves like `Instant`.
    Web = 3,
}

impl ProfilingBackend {
    /// Parse an `OBSERVE_PROFILE` value (`off|instant|fastrace|web`,
    /// case-insensitive). Returns `None` for unrecognized values.
    #[must_use]
    pub fn from_env_value(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "instant" => Some(Self::Instant),
            "fastrace" => Some(Self::Fastrace),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

/// Global runtime configuration.
pub struct ObserveConfig {
    profiling_backend: AtomicU8,
    /// Max error-hook invocations per error type per second. 0 = unlimited.
    error_hook_throttle: AtomicU32,
}

impl ObserveConfig {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            profiling_backend: AtomicU8::new(ProfilingBackend::Fastrace as u8),
            error_hook_throttle: AtomicU32::new(0),
        }
    }

    /// Set the active profiling backend at runtime.
    pub fn set_profiling_backend(&self, backend: ProfilingBackend) {
        self.profiling_backend
            .store(backend as u8, Ordering::Release);
    }

    /// Read the active profiling backend (hot-path, relaxed atomic).
    pub fn profiling_backend(&self) -> ProfilingBackend {
        match self.profiling_backend.load(Ordering::Relaxed) {
            0 => ProfilingBackend::Off,
            1 => ProfilingBackend::Instant,
            3 => ProfilingBackend::Web,
            _ => ProfilingBackend::Fastrace,
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
        match ProfilingBackend::from_env_value(&value) {
            Some(backend) => cfg.set_profiling_backend(backend),
            None => log::warn!(
                target: "fast_observe.config",
                "invalid OBSERVE_PROFILE={value:?}; expected off|instant|fastrace|web — keeping default"
            ),
        }
    }
    cfg
});

/// Access the global runtime config. The `OBSERVE_PROFILE` env override is
/// applied once, on first access.
#[must_use]
pub fn config() -> &'static ObserveConfig {
    &CONFIG
}

// `unknown_discriminant_falls_back_to_fastrace` stays inline: it writes
// the private `profiling_backend` atomic directly (raw discriminant test).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_discriminant_falls_back_to_fastrace() {
        let cfg = ObserveConfig::new();
        cfg.profiling_backend.store(99, Ordering::Release);
        assert_eq!(cfg.profiling_backend(), ProfilingBackend::Fastrace);
    }

    #[test]
    fn parse_env_values() {
        assert_eq!(
            ProfilingBackend::from_env_value("off"),
            Some(ProfilingBackend::Off)
        );
        assert_eq!(
            ProfilingBackend::from_env_value("INSTANT"),
            Some(ProfilingBackend::Instant)
        );
        assert_eq!(
            ProfilingBackend::from_env_value(" fastrace "),
            Some(ProfilingBackend::Fastrace)
        );
        assert_eq!(
            ProfilingBackend::from_env_value("web"),
            Some(ProfilingBackend::Web)
        );
        assert_eq!(ProfilingBackend::from_env_value("bogus"), None);
    }
}
