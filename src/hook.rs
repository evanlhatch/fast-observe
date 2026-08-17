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
//! error type per second. [`init`] is the idempotent zero-config path; it
//! delegates to the deployment builder in [`deploy`](crate::deploy), which
//! owns the logforth/fastrace composition. `observe().init()?` is the
//! checked path (reports `AlreadyInitialized` instead of ignoring it).
//!
//! A second hook family — [`add_capture_hook`] — runs DURING frame
//! construction (before the frame is `Arc`'d) and may mutate the frame
//! (attach data). Capture hooks are NOT throttled (they carry data; keep
//! them cheap — the throttle governs sinks only).
//!
//! [`ObserveConfig::set_error_hook_throttle`]: crate::config::ObserveConfig::set_error_hook_throttle

use crate::exn::{Attachment, Frame};
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
        // Structured kv fields (before the `;`) so JsonLayout/OTel emit
        // fields, not strings. `Context::None` Displays as the literal
        // "None"; omit the context part entirely for context-less errors
        // instead of logging "— None".
        if matches!(frame.context, crate::exn::Context::None) {
            log::error!(
                target: "fast_observe.error",
                error_type = frame.type_name(),
                error_file = frame.location().file(),
                error_line = frame.location().line();
                "{}",
                frame.error(),
            );
        } else {
            log::error!(
                target: "fast_observe.error",
                error_type = frame.type_name(),
                error_file = frame.location().file(),
                error_line = frame.location().line();
                "{} — {}",
                frame.error(),
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

// ── Capture hooks — run DURING frame construction, may mutate the frame ────

type CaptureHook = Arc<dyn Fn(&mut Frame) + Send + Sync + 'static>;

// Same snapshot-Arc pattern as `HOOKS`. INVARIANT: the initial vec always
// holds the built-in capture hooks (trace context under feature `fastrace`,
// then scope path, then backtrace under feature `backtrace`), so they run
// for every frame without setup; `add_capture_hook` only appends.
static CAPTURE_HOOKS: OnceLock<Mutex<Arc<Vec<CaptureHook>>>> = OnceLock::new();

fn capture_hooks() -> &'static Mutex<Arc<Vec<CaptureHook>>> {
    CAPTURE_HOOKS.get_or_init(|| {
        let built_ins: Vec<CaptureHook> = vec![
            #[cfg(feature = "fastrace")]
            trace_context_capture_hook(),
            scope_path_capture_hook(),
            #[cfg(feature = "backtrace")]
            backtrace_capture_hook(),
        ];
        Mutex::new(Arc::new(built_ins))
    })
}

/// Built-in capture hook (feature `fastrace`): attach the current trace id
/// and emit an `error` span event so the error lands IN the trace timeline.
/// Both parts no-op when no local parent span is active.
#[cfg(feature = "fastrace")]
fn trace_context_capture_hook() -> CaptureHook {
    Arc::new(|frame: &mut Frame| {
        if let Some(ctx) = fastrace::collector::SpanContext::current_local_parent() {
            frame.push_attachment(Attachment::with_key("trace_id", ctx.trace_id));
        }
        // No-op when no local parent span is active (checked inside).
        fastrace::local::LocalSpan::add_event(fastrace::Event::new("error").with_properties(
            || {
                use std::borrow::Cow;
                [
                    (Cow::Borrowed("type"), Cow::Borrowed(frame.type_name())),
                    (
                        Cow::Borrowed("location"),
                        Cow::Owned(format!(
                            "{}:{}",
                            frame.location().file(),
                            frame.location().line()
                        )),
                    ),
                ]
            },
        ));
    })
}

/// Built-in capture hook (always installed): attach the profiling scope path
/// (`outer → … → leaf`) and the leaf scope's elapsed milliseconds. Pushes
/// nothing when no scope is active.
fn scope_path_capture_hook() -> CaptureHook {
    Arc::new(|frame: &mut Frame| {
        let path = crate::profiling::scope_path();
        if !path.is_empty() {
            frame.push_attachment(Attachment::with_key("scope_path", path.join(" → ")));
        }
        if let Some(ms) = crate::profiling::current_scope_elapsed_ms() {
            frame.push_attachment(Attachment::with_key("scope_elapsed_ms", ms));
        }
    })
}

// ── Backtrace capture hook (feature `backtrace`) ─────────────────────────

/// Env decision for backtrace capture, factored pure for testing: capture
/// when `RUST_BACKTRACE` is `1`/`full`, or when `OBSERVE_BACKTRACE` is set
/// to a truthy value (`1`/`true`/`full`). `OBSERVE_BACKTRACE`, when set,
/// overrides `RUST_BACKTRACE` in both directions (a falsy override disables
/// capture even with `RUST_BACKTRACE=1`). `read` abstracts `std::env::var`
/// so the matrix is unit-testable without mutating process env (unsafe on
/// edition 2024).
#[cfg(feature = "backtrace")]
fn backtrace_enabled(read: impl Fn(&str) -> Option<String>) -> bool {
    if let Some(override_value) = read("OBSERVE_BACKTRACE") {
        return matches!(
            override_value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "full"
        );
    }
    match read("RUST_BACKTRACE") {
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "full"),
        None => false,
    }
}

/// Resolved once — a process does not toggle `RUST_BACKTRACE` mid-flight in
/// practice, and resolving once keeps the hot path a single shared load.
#[cfg(feature = "backtrace")]
static BACKTRACE_ENABLED: LazyLock<bool> =
    LazyLock::new(|| backtrace_enabled(|name| std::env::var(name).ok()));

/// Backtrace attachment value — `Display` delegates to the captured
/// backtrace's own `Display`.
#[cfg(feature = "backtrace")]
struct BacktraceAttachment(std::backtrace::Backtrace);

#[cfg(feature = "backtrace")]
impl std::fmt::Display for BacktraceAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Built-in capture hook (feature `backtrace`): attach a forced backtrace
/// under the `backtrace` key with [`Placement::Appendix`] — the report
/// renders only Inline attachments, so the backtrace is counted, not
/// inlined. Zero cost when the env knob is off: the resolved flag is
/// checked before any capture happens.
///
/// [`Placement::Appendix`]: crate::exn::Placement::Appendix
#[cfg(feature = "backtrace")]
fn backtrace_capture_hook() -> CaptureHook {
    Arc::new(|frame: &mut Frame| {
        if !*BACKTRACE_ENABLED {
            return;
        }
        frame.push_attachment(
            Attachment::with_key(
                "backtrace",
                BacktraceAttachment(std::backtrace::Backtrace::force_capture()),
            )
            .with_placement(crate::exn::Placement::Appendix),
        );
    })
}

/// Append a capture hook. Capture hooks run DURING frame construction (on
/// `&mut Frame`, before the frame is shared) and may attach data via
/// [`Frame::push_attachment`]. The built-in hooks (trace context, scope
/// path) always run first. A panicking capture hook is contained and does
/// not prevent error construction or later hooks.
///
/// Capture hooks are NOT throttled — they carry data, not notifications.
/// Keep them cheap; the throttle governs sink hooks ([`add_error_hook`])
/// only. Call at startup; safe to call multiple times.
pub fn add_capture_hook(hook: impl Fn(&mut Frame) + Send + Sync + 'static) {
    let mut guard = capture_hooks().lock();
    let mut new_vec = (**guard).clone();
    new_vec.push(Arc::new(hook));
    *guard = Arc::new(new_vec);
}

/// Run all capture hooks on a frame under construction. Called by
/// `Frame::capture` BEFORE the frame is `Arc`'d and before [`invoke`].
///
/// Reentrancy discipline matches `invoke`: the snapshot Arc is cloned and
/// the lock dropped BEFORE invoking, so a capture hook that itself
/// constructs a `Fault` (reentrant `run_capture_hooks`) cannot deadlock.
pub(crate) fn run_capture_hooks(frame: &mut Frame) {
    let snapshot: Arc<Vec<CaptureHook>> = Arc::clone(&capture_hooks().lock());
    for hook in snapshot.iter() {
        // A panicking capture hook must never break error construction.
        let _ = catch_unwind(AssertUnwindSafe(|| hook(frame)));
    }
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

/// Initialize the observability stack — the idempotent zero-config path.
///
/// Delegates to the deployment builder ([`observe()`](crate::deploy::observe)),
/// which owns the composition (logforth appenders/diagnostics per cargo
/// feature + the fastrace console reporter). Equivalent to
/// `observe().init()` with an ignored result: [`InitError::AlreadyInitialized`]
/// (a second `init()`, or a logger installed by someone else) is IGNORED.
/// Call once at startup; harmless if called again. The error hooks need no
/// setup — the default hook self-installs on first use.
///
/// For the checked path (and configurable toggles), use
/// `observe()….init()?` instead.
///
/// [`InitError::AlreadyInitialized`]: crate::deploy::InitError::AlreadyInitialized
pub fn init() {
    // Dropping an Ok guard flushes fastrace immediately — harmless at init
    // (no spans collected yet) and keeps this path allocation-free.
    drop(crate::deploy::observe().init());
}

#[cfg(all(test, feature = "backtrace"))]
mod tests {
    use super::backtrace_enabled;

    /// Fake env for the pure `backtrace_enabled` decision.
    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn backtrace_enabled_matrix() {
        // Neither set → off.
        assert!(!backtrace_enabled(env(&[])));
        // RUST_BACKTRACE: only 1/full count (0 does not).
        assert!(!backtrace_enabled(env(&[("RUST_BACKTRACE", "0")])));
        assert!(backtrace_enabled(env(&[("RUST_BACKTRACE", "1")])));
        assert!(backtrace_enabled(env(&[("RUST_BACKTRACE", "full")])));
        assert!(!backtrace_enabled(env(&[("RUST_BACKTRACE", "yes")])));
        // OBSERVE_BACKTRACE alone: truthy values 1/true/full (case-insensitive).
        assert!(backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "1")])));
        assert!(backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "true")])));
        assert!(backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "full")])));
        assert!(backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "TRUE")])));
        assert!(!backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "0")])));
        assert!(!backtrace_enabled(env(&[("OBSERVE_BACKTRACE", "no")])));
        // OBSERVE_BACKTRACE overrides RUST_BACKTRACE in both directions.
        assert!(!backtrace_enabled(env(&[
            ("RUST_BACKTRACE", "1"),
            ("OBSERVE_BACKTRACE", "0"),
        ])));
        assert!(backtrace_enabled(env(&[
            ("RUST_BACKTRACE", "0"),
            ("OBSERVE_BACKTRACE", "1"),
        ])));
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
