//! Error hooks — auto-observability for every error.
//!
//! The default hook (profiling scope + structured log) is installed lazily on
//! first use — observability is default-on, no `init()` call required.
//! [`add_error_hook`] appends a sink and returns a [`HookId`], usable with
//! [`remove_error_hook`] to retract that sink later: every constructed
//! `Fault` fans out to ALL registered hooks. Each hook invocation is wrapped
//! in `catch_unwind`, so a panicking hook never kills the error path.
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

use crate::exn::{Attachment, BuiltinKey, Frame};
use parking_lot::Mutex;

/// The span-event name emitted for every constructed fault (feature
/// `fastrace`) — shared with `reporter::has_error_event`, whose
/// always-keep-errors sampling rule matches on it. A typo here or there
/// would silently sample error traces away; the const makes the coupling
/// compile-time.
#[cfg(feature = "fastrace")]
pub(crate) const ERROR_EVENT: &str = "error";

/// Nanoseconds per second — the throttle window.
const NS_PER_SEC: u64 = 1_000_000_000;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, OnceLock};

type Hook = Arc<dyn Fn(&Frame) + Send + Sync + 'static>;

/// Opaque identifier for a registered error hook, returned by
/// [`add_error_hook`] and accepted by [`remove_error_hook`]. Ids are handed
/// out from a process-wide counter starting ABOVE 0 — [`HookId`] 0 is
/// reserved for the built-in default hook, which is not removable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HookId(u64);

/// Reserved id of the default hook — the one user-facing id that
/// [`remove_error_hook`] always refuses.
const DEFAULT_HOOK_ID: HookId = HookId(0);

/// Process-wide hook id counter. Starts at 1: id 0 is reserved for the
/// default hook (see [`DEFAULT_HOOK_ID`]).
static NEXT_HOOK_ID: AtomicU64 = AtomicU64::new(1);

/// An append-mostly hook registry behind an immutable snapshot Arc: readers
/// clone the Arc (one refbump) instead of the Vec; writers rebuild.
/// `default` builds the initial/built-in hook set. `F: Clone` — writers
/// rebuild the Vec by cloning the previous snapshot.
struct HookRegistry<F> {
    default: fn() -> Vec<F>,
    slots: Mutex<Arc<Vec<F>>>,
}

impl<F: Clone> HookRegistry<F> {
    fn new(default: fn() -> Vec<F>) -> Self {
        Self {
            default,
            slots: Mutex::new(Arc::new(default())),
        }
    }

    /// Clone the snapshot Arc (one refbump). Callers drop the lock BEFORE
    /// invoking the hooks — the reentrancy discipline documented at
    /// [`invoke`] and [`run_capture_hooks`].
    fn snapshot(&self) -> Arc<Vec<F>> {
        Arc::clone(&self.slots.lock())
    }

    /// Append `hook` by rebuilding the snapshot Vec under the lock.
    fn push(&self, hook: F) {
        let mut guard = self.slots.lock();
        let mut new_vec = (**guard).clone();
        new_vec.push(hook);
        *guard = Arc::new(new_vec);
    }

    /// Remove every entry matching `pred` (rebuild under the lock).
    /// Returns false when nothing matched.
    fn remove_where(&self, pred: impl Fn(&F) -> bool) -> bool {
        let mut guard = self.slots.lock();
        let mut new_vec = (**guard).clone();
        let before = new_vec.len();
        new_vec.retain(|entry| !pred(entry));
        if new_vec.len() == before {
            return false;
        }
        *guard = Arc::new(new_vec);
        true
    }

    /// Restore the built-in hook set (`(self.default)()`).
    fn reset(&self) {
        *self.slots.lock() = Arc::new((self.default)());
    }

    /// Number of hooks in the current snapshot.
    fn len(&self) -> usize {
        self.slots.lock().len()
    }
}

// The sink registry stores (HookId, Hook) pairs: the id enables removal via
// `remove_error_hook`; the default hook carries the reserved `DEFAULT_HOOK_ID`.
//
// INVARIANT: index 0 is ALWAYS the default hook. `hooks()` installs it at
// position 0, `add_error_hook` only appends, `remove_error_hook` refuses
// `DEFAULT_HOOK_ID`, and `clear_error_hooks` resets to exactly
// `[default_hook()]`. `set_default_hook_enabled(false)` relies on
// this to skip index 0 in `invoke()`.
static HOOKS: OnceLock<HookRegistry<(HookId, Hook)>> = OnceLock::new();

/// The built-in sink set: exactly the default hook, at index 0.
fn default_sink_hooks() -> Vec<(HookId, Hook)> {
    vec![(DEFAULT_HOOK_ID, default_hook())]
}

fn hooks() -> &'static HookRegistry<(HookId, Hook)> {
    HOOKS.get_or_init(|| HookRegistry::new(default_sink_hooks))
}

/// The default error hook — profiling scope + structured log, or the full
/// report block under `OBSERVE_REPORT` (DESIGN.md §7, §9 decision #4).
/// The envelope: volatile per-event process state carried as structured kv
/// fields on the log record (never in the report body, which must stay a
/// pure function of the fault). `occurrence` = constructions of this error
/// type so far INCLUDING this one (`invoke` bumps the counter before
/// hooks run) — novel-vs-chronic is the first thing a reader should know.
struct Envelope {
    occurrence: u64,
    thread: std::borrow::Cow<'static, str>,
    uptime_ms: u64,
}

impl Envelope {
    fn capture(type_name: &'static str) -> Self {
        let occurrence = crate::exn::error_counts()
            .iter()
            .find(|(k, _)| *k == type_name)
            .map_or(0, |(_, c)| *c);
        let thread = std::thread::current();
        Self {
            occurrence,
            thread: thread
                .name()
                .map_or(std::borrow::Cow::Borrowed("<unnamed>"), |n| {
                    std::borrow::Cow::Owned(n.to_string())
                }),
            uptime_ms: crate::profiling::clock::now_ns() / 1_000_000,
        }
    }
}

fn default_hook() -> Hook {
    Arc::new(|frame: &Frame| {
        // Hold the scope guard across the log — dropping it before the log
        // loses the span timing for the error path.
        let _span = crate::scope!("error");
        let envelope = Envelope::capture(frame.type_name());
        match crate::config::report_mode() {
            crate::config::ReportMode::Off => {
                // Structured kv fields (before the `;`) so JsonLayout/OTel
                // emit fields, not strings. `Context::None` Displays as the
                // literal "None"; omit the context part entirely for
                // context-less errors instead of logging "— None".
                let message = if matches!(frame.context, crate::exn::Context::None) {
                    frame.error().to_string()
                } else {
                    format!("{} — {}", frame.error(), frame.context)
                };
                log::error!(
                    target: "fast_observe.error",
                    error_type = frame.type_name(),
                    error_file = frame.location().file(),
                    error_line = frame.location().line(),
                    occurrence = envelope.occurrence,
                    thread = &*envelope.thread,
                    uptime_ms = envelope.uptime_ms;
                    "{message}",
                );
            }
            crate::config::ReportMode::Text => log_report(frame, &envelope, false),
            crate::config::ReportMode::Json => log_report(frame, &envelope, true),
        }
    })
}

/// Emit the full report block (text or versioned JSON) as ONE structured
/// error event — the agent-readable form (OBSERVE.md). The kv fields stay
/// structured so JsonLayout/OTel still get `error_type`/`error_file`/
/// `error_line` alongside the block payload.
///
/// `json` requests the versioned JSON form; without feature `serde` it
/// falls back to the text block.
fn log_report(frame: &Frame, envelope: &Envelope, json: bool) {
    #[cfg(feature = "serde")]
    let report = if json {
        crate::report::render_frame_report_json(frame)
    } else {
        crate::report::render_frame_report(frame)
    };
    #[cfg(not(feature = "serde"))]
    let report = {
        let _ = json;
        crate::report::render_frame_report(frame)
    };
    log::error!(
        target: "fast_observe.error",
        error_type = frame.type_name(),
        error_file = frame.location().file(),
        error_line = frame.location().line(),
        occurrence = envelope.occurrence,
        thread = &*envelope.thread,
        uptime_ms = envelope.uptime_ms;
        "{report}"
    );
}

/// Append an error hook. Called on every `Fault::new()` — fans out with all
/// previously registered hooks (the lazily installed default hook included).
/// A panicking hook is contained and does not prevent later hooks from
/// running. Call at startup; safe to call multiple times.
///
/// Returns the [`HookId`] of the new sink, for later retraction via
/// [`remove_error_hook`]. Ignoring the id is legitimate (fire-and-forget
/// registration) — the hook then lives until [`clear_error_hooks`].
pub fn add_error_hook(hook: impl Fn(&Frame) + Send + Sync + 'static) -> HookId {
    // Relaxed: the id is just a unique token, no ordering against other data.
    let id = HookId(NEXT_HOOK_ID.fetch_add(1, Ordering::Relaxed));
    hooks().push((id, Arc::new(hook)));
    id
}

/// Remove the error hook registered under `id` (returned by
/// [`add_error_hook`]). Returns false for an unknown id — including the
/// reserved id of the default hook, which is not removable (disable it with
/// [`set_default_hook_enabled`] instead).
#[must_use]
pub fn remove_error_hook(id: HookId) -> bool {
    if id == DEFAULT_HOOK_ID {
        return false;
    }
    hooks().remove_where(|(entry_id, _)| *entry_id == id)
}

/// Remove all user-registered error hooks, resetting the registry to contain
/// ONLY the lazily installed default hook. The default hook survives because
/// it is not user-registered (see the index-0 invariant at `HOOKS`); disable
/// it separately with [`set_default_hook_enabled`].
pub fn clear_error_hooks() {
    hooks().reset();
}

/// Number of registered error hooks (default hook included).
/// Intended for testing/introspection.
#[must_use]
pub fn hooks_len() -> usize {
    hooks().len()
}

// ── Capture hooks — run DURING frame construction, may mutate the frame ────

type CaptureHook = Arc<dyn Fn(&mut Frame) + Send + Sync + 'static>;

// Same snapshot-Arc registry as `HOOKS` (capture hooks stay plain `F`,
// no removal ids). INVARIANT: the initial vec always holds the built-in
// capture hooks (trace context under feature `fastrace`,
// then scope path, then span trail under feature `instant`, then backtrace
// under feature `backtrace`), so they run
// for every frame without setup; `add_capture_hook` only appends.
static CAPTURE_HOOKS: OnceLock<HookRegistry<CaptureHook>> = OnceLock::new();

/// The built-in capture hook set (see the `CAPTURE_HOOKS` invariant for the
/// order contract).
fn default_capture_hooks() -> Vec<CaptureHook> {
    vec![
        #[cfg(feature = "fastrace")]
        trace_context_capture_hook(),
        scope_path_capture_hook(),
        #[cfg(feature = "instant")]
        span_trail_capture_hook(),
        #[cfg(feature = "backtrace")]
        backtrace_capture_hook(),
    ]
}

fn capture_hooks() -> &'static HookRegistry<CaptureHook> {
    CAPTURE_HOOKS.get_or_init(|| HookRegistry::new(default_capture_hooks))
}

/// Built-in capture hook (feature `fastrace`): attach the current trace id
/// and emit an `error` span event so the error lands IN the trace timeline.
/// Both parts no-op when no local parent span is active.
#[cfg(feature = "fastrace")]
fn trace_context_capture_hook() -> CaptureHook {
    Arc::new(|frame: &mut Frame| {
        if let Some(ctx) = fastrace::collector::SpanContext::current_local_parent() {
            frame.push_attachment(Attachment::with_key(
                BuiltinKey::TraceId.as_str(),
                ctx.trace_id,
            ));
        }
        // No-op when no local parent span is active (checked inside).
        fastrace::local::LocalSpan::add_event(fastrace::Event::new(ERROR_EVENT).with_properties(
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
            frame.push_attachment(Attachment::with_key(
                BuiltinKey::ScopePath.as_str(),
                path.join(" → "),
            ));
        }
        if let Some(ms) = crate::profiling::current_scope_elapsed_ms() {
            frame.push_attachment(Attachment::with_key(
                BuiltinKey::ScopeElapsedMs.as_str(),
                ms,
            ));
        }
    })
}

// ── Span-trail breadcrumb capture hook (feature `instant`) ───────────────

/// Number of recent finished spans attached as the `span_trail` breadcrumb.
#[cfg(feature = "instant")]
const SPAN_TRAIL_LEN: usize = 8;

/// Breadcrumb attachment value: the recent finished-span trail captured at
/// fault time. `Display` renders one line, chronological (oldest finished
/// first): `name(12µs); name2(3ms); …`.
#[cfg(feature = "instant")]
struct SpanTrail(Vec<crate::profiling::instant::SpanRecord>);

#[cfg(feature = "instant")]
impl std::fmt::Display for SpanTrail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for span in &self.0 {
            if !first {
                f.write_str("; ")?;
            }
            first = false;
            write!(
                f,
                "{}({})",
                span.name,
                humantime::format_duration(span.duration())
            )?;
        }
        Ok(())
    }
}

/// Built-in capture hook (feature `instant`): attach the recent finished-span
/// trail under the `span_trail` key with [`Placement::Appendix`] — the "what
/// was happening just before it broke" breadcrumbs. Skips cheaply (one
/// atomic load) when the backends mask contains neither `INSTANT` nor `WEB`:
/// the instant accumulator is always empty then. Non-destructive —
/// [`peek_recent`] snapshots, the accumulator is undisturbed.
///
/// v1 contract: the report renders only Inline attachments, so the trail is
/// counted-not-shown there. It IS reachable programmatically via
/// [`Frame::attachments`] (key `span_trail`; [`Attachment::display`] carries
/// the one-line rendering).
///
/// [`Placement::Appendix`]: crate::exn::Placement::Appendix
/// [`peek_recent`]: crate::profiling::instant::peek_recent
#[cfg(feature = "instant")]
fn span_trail_capture_hook() -> CaptureHook {
    Arc::new(|frame: &mut Frame| {
        use crate::config::Backends;
        let backends = crate::config::config().backends();
        if !backends.contains(Backends::INSTANT) && !backends.contains(Backends::WEB) {
            return;
        }
        let trail = crate::profiling::instant::peek_recent(SPAN_TRAIL_LEN);
        if trail.is_empty() {
            return;
        }
        frame.push_attachment(
            Attachment::with_key(BuiltinKey::SpanTrail.as_str(), SpanTrail(trail))
                .with_placement(crate::exn::Placement::Appendix),
        );
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
    if let Some(override_value) = read(crate::env_vars::OBSERVE_BACKTRACE) {
        return matches!(
            override_value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "full"
        );
    }
    match read(crate::env_vars::RUST_BACKTRACE) {
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
    /// Structured, line-oriented frames when the `backtrace_frames` nightly
    /// gate is enabled at the crate root (DESIGN.md §11c: "LLM-readable
    /// backtraces"): `frame 3: { fn: "...", file: ..., line: ... }` per line.
    /// Falls back to the raw `Debug` text dump if the gate is unavailable.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "backtrace")]
        {
            // The gate is required for `frames()`; if the pinned nightly
            // lacks it, fall back to the raw dump. `frames()` returns
            // `&[BacktraceFrame]` (Debug = `fn/file/line` struct lines).
            let frames = self.0.frames();
            for (i, frame) in frames.iter().enumerate() {
                writeln!(f, "frame {i}: {frame:?}")?;
            }
            Ok(())
        }
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
                BuiltinKey::Backtrace.as_str(),
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
    capture_hooks().push(Arc::new(hook));
}

/// Number of registered capture hooks (built-ins included).
/// Intended for testing/introspection.
#[must_use]
pub fn capture_hooks_len() -> usize {
    capture_hooks().len()
}

/// Run all capture hooks on a frame under construction. Called by
/// `Frame::capture` BEFORE the frame is `Arc`'d and before [`invoke`].
///
/// Reentrancy discipline matches `invoke`: the snapshot Arc is cloned and
/// the lock dropped BEFORE invoking, so a capture hook that itself
/// constructs a `Fault` (reentrant `run_capture_hooks`) cannot deadlock.
pub(crate) fn run_capture_hooks(frame: &mut Frame) {
    let snapshot: Arc<Vec<CaptureHook>> = capture_hooks().snapshot();
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

/// A per-key fixed-window rate limiter. The clock is injected (`now_ns`)
/// so tests drive time explicitly — no sleeping.
struct RateLimiter {
    state: Mutex<HashMap<&'static str, ThrottleState>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
        }
    }

    /// True when `key` is over `limit` per second (0 = unlimited — never throttled).
    fn over_budget(&self, key: &'static str, limit: u32, now_ns: u64) -> bool {
        if limit == 0 {
            return false; // 0 = unlimited (default)
        }
        let mut map = self.state.lock();
        let state = map.entry(key).or_insert(ThrottleState {
            window_start_ns: now_ns,
            count: 0,
        });
        if now_ns.saturating_sub(state.window_start_ns) >= NS_PER_SEC {
            state.window_start_ns = now_ns;
            state.count = 0;
        }
        if state.count >= limit {
            return true;
        }
        state.count += 1;
        false
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

static THROTTLE: LazyLock<RateLimiter> = LazyLock::new(RateLimiter::new);

/// Returns true when this error type is over its per-second hook budget.
fn throttled(type_name: &'static str) -> bool {
    THROTTLE.over_budget(
        type_name,
        crate::config::config().error_hook_throttle(),
        crate::profiling::clock::now_ns(),
    )
}

/// Invoke all hooks on a frame. Called by `Fault::new` and friends.
/// Installs the default hook on first use.
pub(crate) fn invoke(frame: &Frame) {
    crate::exn::record_error(frame.type_name);
    #[cfg(feature = "metrics-facade")]
    crate::exn::record_error_metrics(frame.type_name);
    if throttled(frame.type_name) {
        return;
    }
    // Clone the snapshot Arc (one refbump) and drop the lock BEFORE
    // invoking: hooks can log via logforth, which can construct a Fault →
    // reentrant invoke → deadlock if the lock were held across the callbacks.
    let snapshot: Arc<Vec<(HookId, Hook)>> = hooks().snapshot();
    let skip_default = !DEFAULT_HOOK_ENABLED.load(Ordering::Relaxed);
    for (i, (_id, sink)) in snapshot.iter().enumerate() {
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

#[cfg(test)]
mod rate_limiter_tests {
    use super::{NS_PER_SEC, RateLimiter};

    #[test]
    fn under_limit_passes() {
        let limiter = RateLimiter::new();
        assert!(!limiter.over_budget("key", 2, 0));
        assert!(!limiter.over_budget("key", 2, 1));
    }

    #[test]
    fn over_limit_blocks() {
        let limiter = RateLimiter::new();
        assert!(!limiter.over_budget("key", 2, 0));
        assert!(!limiter.over_budget("key", 2, 1));
        assert!(limiter.over_budget("key", 2, 2));
        assert!(limiter.over_budget("key", 2, 3));
    }

    #[test]
    fn window_rolls_after_one_second() {
        let limiter = RateLimiter::new();
        assert!(!limiter.over_budget("key", 1, 0));
        assert!(limiter.over_budget("key", 1, 1));
        // Window rolls exactly at NS_PER_SEC elapsed → budget opens again.
        assert!(!limiter.over_budget("key", 1, NS_PER_SEC));
        assert!(limiter.over_budget("key", 1, NS_PER_SEC + 1));
    }

    #[test]
    fn limit_zero_never_blocks() {
        let limiter = RateLimiter::new();
        for now in 0..100u64 {
            assert!(!limiter.over_budget("key", 0, now));
        }
    }

    #[test]
    fn keys_are_independent() {
        let limiter = RateLimiter::new();
        assert!(!limiter.over_budget("a", 1, 0));
        assert!(limiter.over_budget("a", 1, 0));
        // A saturated "a" window must not throttle "b".
        assert!(!limiter.over_budget("b", 1, 0));
    }
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
        assert!(!backtrace_enabled(env(&[(
            crate::env_vars::RUST_BACKTRACE,
            "0"
        )])));
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::RUST_BACKTRACE,
            "1"
        )])));
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::RUST_BACKTRACE,
            "full"
        )])));
        assert!(!backtrace_enabled(env(&[(
            crate::env_vars::RUST_BACKTRACE,
            "yes"
        )])));
        // OBSERVE_BACKTRACE alone: truthy values 1/true/full (case-insensitive).
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "1"
        )])));
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "true"
        )])));
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "full"
        )])));
        assert!(backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "TRUE"
        )])));
        assert!(!backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "0"
        )])));
        assert!(!backtrace_enabled(env(&[(
            crate::env_vars::OBSERVE_BACKTRACE,
            "no"
        )])));
        // OBSERVE_BACKTRACE overrides RUST_BACKTRACE in both directions.
        assert!(!backtrace_enabled(env(&[
            (crate::env_vars::RUST_BACKTRACE, "1"),
            (crate::env_vars::OBSERVE_BACKTRACE, "0"),
        ])));
        assert!(backtrace_enabled(env(&[
            (crate::env_vars::RUST_BACKTRACE, "0"),
            (crate::env_vars::OBSERVE_BACKTRACE, "1"),
        ])));
    }
}

/// Install an `OpenTelemetry` reporter for fastrace (feature `otel`).
///
/// The reporter needs app-specific `OTel` SDK config (endpoint,
/// protocol, resource), so it is constructed by the caller; this helper just
/// wires it into fastrace. Pair with the re-exported
/// `logforth_append_opentelemetry` appender for log correlation.
#[cfg(feature = "otel")]
pub fn init_otel(reporter: fastrace_opentelemetry::OpenTelemetryReporter) {
    fastrace::set_reporter(reporter, fastrace::collector::Config::default());
}
