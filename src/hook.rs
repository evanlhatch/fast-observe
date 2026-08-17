//! Error hooks — auto-observability for every error.
//!
//! The default hook (profiling scope + structured log) is installed lazily on
//! first use — observability is default-on, no `init()` call required.
//! [`add_error_hook`] appends a sink: every constructed `Fault` fans out to
//! ALL registered hooks. Each hook invocation is wrapped in
//! `catch_unwind`, so a panicking hook never kills the error path.
//!
//! An optional global throttle (see
//! [`ObserveConfig::set_error_hook_throttle`]) caps hook invocations to N per
//! error type per second. [`init`] additionally wires logforth (only the
//! appenders/diagnostics enabled by cargo features) and fastrace.
//!
//! [`ObserveConfig::set_error_hook_throttle`]: crate::config::ObserveConfig::set_error_hook_throttle

use crate::exn::Frame;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, OnceLock};

type Hook = Arc<dyn Fn(&Frame) + Send + Sync + 'static>;

// The hook registry is an immutable snapshot behind a single Arc: add/clear
// swap in a fresh Arc, readers clone the Arc (one refbump) instead of
// cloning the whole Vec (N refbumps) per error.
//
// INVARIANT: index 0 is ALWAYS the default hook. `hooks()` installs it at
// position 0, `add_error_hook` only appends, and `clear_error_hooks` resets
// to exactly `[default_hook()]`. `set_default_hook_enabled(false)` relies on
// this to skip index 0 in `invoke()`.
static HOOKS: OnceLock<Mutex<Arc<Vec<Hook>>>> = OnceLock::new();

fn hooks() -> &'static Mutex<Arc<Vec<Hook>>> {
    HOOKS.get_or_init(|| Mutex::new(Arc::new(vec![default_hook()])))
}

/// The default error hook — profiling scope + structured log.
fn default_hook() -> Hook {
    Arc::new(|frame: &Frame| {
        // Hold the scope guard across the log — dropping it before the log
        // loses the span timing for the error path.
        let _span = crate::scope!("error");
        // `Context::None` Displays as the literal "None"; omit the context
        // part entirely for context-less errors instead of logging "— None".
        if matches!(frame.context, crate::exn::Context::None) {
            log::error!(
                target: "fast_observe.error",
                "{} at {}:{}",
                frame.error,
                frame.location.file(),
                frame.location.line(),
            );
        } else {
            log::error!(
                target: "fast_observe.error",
                "{} at {}:{} — {}",
                frame.error,
                frame.location.file(),
                frame.location.line(),
                frame.context,
            );
        }
    })
}

/// Append an error hook. Called on every `Fault::new()` — fans out with all
/// previously registered hooks (the lazily installed default hook included).
/// A panicking hook is contained and does not prevent later hooks from
/// running. Call at startup; safe to call multiple times.
pub fn add_error_hook(hook: impl Fn(&Frame) + Send + Sync + 'static) {
    let mut guard = hooks().lock();
    let mut new_vec = (**guard).clone();
    new_vec.push(Arc::new(hook));
    *guard = Arc::new(new_vec);
}

/// Remove all user-registered error hooks, resetting the registry to contain
/// ONLY the lazily installed default hook. The default hook survives because
/// it is not user-registered (see the index-0 invariant at `HOOKS`); disable
/// it separately with [`set_default_hook_enabled`].
pub fn clear_error_hooks() {
    *hooks().lock() = Arc::new(vec![default_hook()]);
}

/// Number of registered error hooks (default hook included).
/// Intended for testing/introspection.
pub fn hooks_len() -> usize {
    hooks().lock().len()
}

static DEFAULT_HOOK_ENABLED: AtomicBool = AtomicBool::new(true);

/// Enable or disable the default hook (profiling scope + structured log).
/// Enabled by default. Custom hooks registered via [`add_error_hook`] are
/// unaffected. Relies on the index-0 invariant documented at `HOOKS`:
/// the default hook is always the first entry in the registry snapshot.
pub fn set_default_hook_enabled(enabled: bool) {
    DEFAULT_HOOK_ENABLED.store(enabled, Ordering::Relaxed);
}

// ── Throttle — max N hook invocations per error type per second ────────────

struct ThrottleState {
    window_start_ns: u64,
    count: u32,
}

static THROTTLE: LazyLock<Mutex<HashMap<&'static str, ThrottleState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns true when this error type is over its per-second hook budget.
fn throttled(type_name: &'static str) -> bool {
    let limit = crate::config::config().error_hook_throttle();
    if limit == 0 {
        return false; // 0 = unlimited (default)
    }
    let now = crate::profiling::clock::now_ns();
    let mut map = THROTTLE.lock();
    let state = map.entry(type_name).or_insert(ThrottleState {
        window_start_ns: now,
        count: 0,
    });
    if now.saturating_sub(state.window_start_ns) >= 1_000_000_000 {
        state.window_start_ns = now;
        state.count = 0;
    }
    if state.count >= limit {
        return true;
    }
    state.count += 1;
    false
}

/// Invoke all hooks on a frame. Called by `Fault::new` and friends.
/// Installs the default hook on first use.
pub(crate) fn invoke(frame: &Frame) {
    crate::exn::record_error(frame.type_name);
    if throttled(frame.type_name) {
        return;
    }
    // Clone the snapshot Arc (one refbump) and drop the lock BEFORE
    // invoking: hooks can log via logforth, which can construct a Fault →
    // reentrant invoke → deadlock if the lock were held across the callbacks.
    let snapshot: Arc<Vec<Hook>> = Arc::clone(&hooks().lock());
    let skip_default = !DEFAULT_HOOK_ENABLED.load(Ordering::Relaxed);
    for (i, sink) in snapshot.iter().enumerate() {
        // Index 0 is the default hook by construction (see HOOKS invariant).
        if i == 0 && skip_default {
            continue;
        }
        // A panicking hook must never kill the error path (the error itself
        // is usually in flight). Contain it and keep fanning out.
        let _ = catch_unwind(AssertUnwindSafe(|| sink(frame)));
    }
}

/// Initialize the observability stack:
///
/// 1. **Logforth** — `ThreadLocalDiagnostic` always; composes only the
///    appenders/diagnostics enabled by cargo features:
///    - `fastrace` → `FastraceDiagnostic` + `FastraceEvent` appender.
///    - `json` → JSON layout on the stdout appender.
///    - `file` → rolling file appender when `OBSERVE_LOG_DIR` is set.
///    - `web` (wasm32 only) → browser-console appender.
/// 2. **Fastrace** — `ConsoleReporter` (feature `fastrace`).
///
/// The error hooks need no setup — the default hook self-installs on first
/// use. Call once at startup; harmless if called again (a second
/// `try_apply` failure is ignored).
pub fn init() {
    let result = logforth::starter_log::builder()
        .dispatch(|d| {
            let d = d.diagnostic(logforth::diagnostic::ThreadLocalDiagnostic::default());
            #[cfg(feature = "fastrace")]
            let d = d.diagnostic(logforth_diagnostic_fastrace::FastraceDiagnostic::default());

            #[cfg(feature = "json")]
            let stdout = logforth::append::Stdout::default()
                .with_layout(logforth_layout_json::JsonLayout::default());
            #[cfg(not(feature = "json"))]
            let stdout = logforth::append::Stdout::default();

            let d = d.append(stdout);
            #[cfg(feature = "fastrace")]
            let d = d.append(logforth_append_fastrace::FastraceEvent::default());
            #[cfg(all(feature = "web", target_arch = "wasm32"))]
            let d = d.append(crate::profiling::web::WebConsoleAppend);
            #[cfg(feature = "file")]
            let d = match file_appender() {
                Some(f) => d.append(f),
                None => d,
            };
            d
        })
        .try_apply();

    if result.is_ok() {
        // Clamp to Info so hot-path trace/debug macros compile out to no-ops.
        log::set_max_level(log::LevelFilter::Info);
    }

    #[cfg(feature = "fastrace")]
    fastrace::set_reporter(
        fastrace::collector::ConsoleReporter,
        fastrace::collector::Config::default(),
    );
}

/// Rolling file appender, enabled by setting `OBSERVE_LOG_DIR` to a writable
/// directory. Logs roll into `<dir>/app.log`.
#[cfg(feature = "file")]
fn file_appender() -> Option<logforth_append_file::File> {
    let dir = std::env::var("OBSERVE_LOG_DIR").ok()?;
    match logforth_append_file::FileBuilder::new(dir, "app.log").build() {
        Ok(f) => Some(f),
        Err(e) => {
            log::error!(target: "fast_observe.hook", "failed to build file appender: {e}");
            None
        }
    }
}

/// Install an `OpenTelemetry` reporter for fastrace (feature `otel`).
///
/// The reporter needs app-specific OTel SDK config (endpoint,
/// protocol, resource), so it is constructed by the caller; this helper just
/// wires it into fastrace. Pair with the re-exported
/// `logforth_append_opentelemetry` appender for log correlation.
#[cfg(feature = "otel")]
pub fn init_otel(reporter: fastrace_opentelemetry::OpenTelemetryReporter) {
    fastrace::set_reporter(reporter, fastrace::collector::Config::default());
}
