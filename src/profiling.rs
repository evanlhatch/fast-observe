//! Profiling — forked from `profiling` (MIT) with improvements:
//!
//! 1. **`fastrace` feature** — fastrace replaces `tracing`.
//! 2. **`instant` feature** — thread-local span accumulator that **coexists**
//!    with other backends. Replaces HotProfile-style ad-hoc timers.
//! 3. **Unified `scope!`** — one macro feeds the runtime-selected backend
//!    SET (`config::Backends` mask). Tier-2 backends (puffin, tracy,
//!    superluminal, tracing) are compiled in via `profile-with-*` features
//!    and selected at runtime — compiled-in ≠ active.
//! 4. **Automatic error context** — `profiling!()` sets a thread-local scope
//!    name; the error path reads it and auto-attaches `Context::Scope(name)`.
//! 5. **`finish_frame!`** — marks a tick/frame boundary in the instant backend.
//! 6. **`web` feature** — instant spans + browser-console logging +
//!    devtools Performance-timeline marks on wasm32.
//!
//! The `scope!` / `#[all_functions]` API is compatible with upstream
//! `profiling`.
//!
//! # Async
//!
//! [`scope!`] guards are thread-bound (`!Send`-adjacent: they enter/exit
//! thread-local span stacks), so **never hold one across `.await`** — the
//! guard would record against whatever thread the task is next polled on.
//! Sync scopes *between* awaits are fine. `#[instrument]` is sync-only for
//! the same reason: applying it to an `async fn` is a compile error.
//!
//! For async code (feature `fastrace`):
//!
//! - bind [`root_span!`] once per task/request to establish the trace
//!   context (optionally continuing an incoming one), then
//! - wrap the future with
//!   [`profiling::async_::in_observed_span`](async_::in_observed_span) /
//!   [`ObservedFutureExt`](async_::ObservedFutureExt) — the span is entered
//!   on every poll and follows the task across threads, and
//! - for `Stream`s, use the `fastrace_futures` re-export
//!   ([`async_::futures`](async_::futures)) under feature `int-futures`.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "fastrace")] {
//! use fast_observe::profiling::async_::ObservedFutureExt;
//!
//! let _root = fast_observe::root_span!("request");
//! # let work = async {};
//! let task = work.in_observed_span("load");
//! # drop(task);
//! # }
//! ```

// ── Backend modules ────────────────────────────────────────────────────────

#[cfg(feature = "fastrace")]
pub mod async_;
pub(crate) mod clock;
// Unit-carrying timestamp type — reachable as `fast_observe::profiling::Nanos`.
pub use self::clock::Nanos;
#[cfg(feature = "fastrace")]
pub(crate) mod fastrace;
#[cfg(feature = "instant")]
pub mod instant;
#[cfg(feature = "profile-with-puffin")]
pub(crate) mod puffin;
#[cfg(all(feature = "profile-with-superluminal", windows))]
pub(crate) mod superluminal;
#[cfg(feature = "profile-with-tracing")]
pub(crate) mod tracing_scope;
#[cfg(feature = "profile-with-tracy")]
pub(crate) mod tracy;
// Browser-only: `target_os = "unknown"` excludes wasm32-wasip3 — no
// browser console exists on WASI, and wasm-bindgen's placeholder imports
// panic there (proven by wasmtron's observe-spike: WebConsoleAppend →
// __wbindgen_describe → "function not implemented" → abort).
#[cfg(all(feature = "web", target_arch = "wasm32", target_os = "unknown"))]
pub(crate) mod web;

// ── Wrap modules: feature-gated re-exports + ZST stubs ─────────────────────

macro_rules! profiling_backend {
    (
        // `enabled` is ONE parenthesized cfg predicate (a single token tree),
        // e.g. `(feature = "profile-with-puffin")` or
        // `(all(feature = "profile-with-superluminal", windows))`.
        $wrap:ident, $backend:ident, $guard:ident, enabled = $enabled:tt
        $(, finish_frame = $finish_frame:ident)?
        $(, on_enable = $on_enable:ident)?
        $(,)?
    ) => {
        // `pub` (not `pub(crate)`) is load-bearing for macro hygiene:
        // `finish_frame!` expands to `$crate::profiling::<wrap>::…` in
        // CONSUMER crates. Pure implementation detail — hidden from docs.
        #[doc(hidden)]
        pub mod $wrap {
            #[cfg $enabled]
            pub use super::$backend::{dummy, enter, $($finish_frame,)? $($on_enable,)? $guard};
            #[cfg(not $enabled)]
            pub struct $guard;
            #[cfg(not $enabled)]
            pub const fn enter<'a>(_name: &'a str, _tag: Option<&'a str>) -> $guard {
                $guard
            }
            #[cfg(not $enabled)]
            pub const fn dummy() -> $guard {
                $guard
            }
            $(
            #[cfg(not $enabled)]
            pub const fn $finish_frame() {}
            )?
            $(
            #[cfg(not $enabled)]
            pub const fn $on_enable() {}
            )?
            /// `true` when this backend's cargo feature is compiled in (and
            /// its target constraint, if any, matches the build target).
            pub const AVAILABLE: bool = cfg! $enabled;
        }
    };
}

profiling_backend!(
    instant_wrap,
    instant,
    InstantGuard,
    enabled = (feature = "instant"),
    finish_frame = finish_frame,
);

profiling_backend!(
    fastrace_wrap,
    fastrace,
    FastraceGuard,
    enabled = (feature = "fastrace"),
);

profiling_backend!(
    puffin_wrap,
    puffin,
    PuffinGuard,
    enabled = (feature = "profile-with-puffin"),
    finish_frame = finish_frame,
    on_enable = on_enable,
);

profiling_backend!(
    tracy_wrap,
    tracy,
    TracyGuard,
    enabled = (feature = "profile-with-tracy"),
);

profiling_backend!(
    superluminal_wrap,
    superluminal,
    SuperluminalGuard,
    enabled = (all(feature = "profile-with-superluminal", windows)),
);

profiling_backend!(
    tracing_wrap,
    tracing_scope,
    TracingGuard,
    enabled = (feature = "profile-with-tracing"),
);

// Browser timeline marks (DESIGN.md §11b): the Web backend's span timing is
// the instant backend, but on wasm32-unknown-unknown a scope ALSO emits
// performance.mark/measure pairs. The mark guard's enter signature (name
// only, no tag) doesn't fit `profiling_backend!`, so this wrap module is
// hand-written with the same real/stub pattern — ZST stub off-browser, so
// native/test behavior is unchanged.
#[doc(hidden)]
pub mod web_wrap {
    #[cfg(all(feature = "web", target_arch = "wasm32", target_os = "unknown"))]
    pub use super::web::{WebMarkGuard, dummy_mark, enter_mark};

    #[cfg(not(all(feature = "web", target_arch = "wasm32", target_os = "unknown")))]
    pub struct WebMarkGuard {
        // Thread-bound marker — mirrors the real (wasm-only) guard so the
        // stub keeps the same `!Send`/`!Sync` contract.
        _not_send: std::marker::PhantomData<*const ()>,
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32", target_os = "unknown")))]
    #[must_use]
    pub const fn enter_mark(_name: &'static str) -> WebMarkGuard {
        WebMarkGuard {
            _not_send: std::marker::PhantomData,
        }
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32", target_os = "unknown")))]
    #[must_use]
    pub const fn dummy_mark() -> WebMarkGuard {
        WebMarkGuard {
            _not_send: std::marker::PhantomData,
        }
    }
}

// ── Unified scope guard ────────────────────────────────────────────────────

/// Guard returned by [`scope!`]. Holds one guard per backend; only backends
/// whose bit is set in the runtime mask (and whose feature is compiled in)
/// hold real guards — the rest are ZST stubs.
/// Fields are never read — they exist purely for their Drop side effects.
/// `#[must_use]`: writing `scope!("name");` as a bare statement drops the
/// guard immediately — a zero-length span. Bind it (`let _s = scope!(...)`).
#[allow(
    dead_code,
    reason = "fields held only for Drop side effects; `_not_send` pins the guard to its creating thread"
)]
#[must_use = "a scope guard records on Drop — bind it (let _s = scope!(...)) or the span is zero-length"]
pub struct ScopeGuard {
    instant: instant_wrap::InstantGuard,
    web_mark: web_wrap::WebMarkGuard,
    fastrace: fastrace_wrap::FastraceGuard,
    puffin: puffin_wrap::PuffinGuard,
    tracy: tracy_wrap::TracyGuard,
    superluminal: superluminal_wrap::SuperluminalGuard,
    tracing: tracing_wrap::TracingGuard,
    // Thread-bound marker: every backend guard pops a THREAD-LOCAL span
    // stack on drop, so a cross-thread drop would corrupt the receiving
    // thread's stack. Previously `!Send` only by accident (the `fastrace`
    // feature pulled in an `Rc`) — `PhantomData<*const ()>` makes the
    // `!Send`/`!Sync` contract feature-independent.
    _not_send: PhantomData<*const ()>,
}

impl ScopeGuard {
    /// Enter a scope for every backend selected in the runtime mask.
    /// Zero allocation — the name is `'static` (a string literal).
    /// Loads the mask ONCE; `Backends::OFF` → all-dummy guard (~2ns).
    ///
    /// `WEB` behaves like `INSTANT` for span timing; additionally, on
    /// wasm32-unknown-unknown it holds a [`web_wrap::WebMarkGuard`] that
    /// emits `performance.mark()/measure()` pairs for the browser devtools
    /// Performance timeline. The browser-console half of the web backend is
    /// a log appender, not a span sink.
    // No `#[must_use]` here — the struct itself carries the `#[must_use]`
    // message; a method-level one is `clippy::double_must_use`.
    pub fn new_static(name: &'static str, tag: Option<&'static str>) -> Self {
        use crate::config::Backends;
        let mask = crate::config::config().backends();

        // Pure DRY: one repetition over the backends whose enter/dummy
        // signatures match exactly (`name` + `tag`) — per field it emits the
        // uniform `if mask.contains(Backends::…) { enter(name, tag) } else {
        // dummy() }` initializer. `instant` and `web_mark` stay written
        // explicitly — they have special predicates/signatures (INSTANT|WEB
        // split, name-only mark signature).
        //
        // NOTE: a `macro_rules!` call cannot expand to a struct-LITERAL field
        // (rustc limitation — the struct parser rejects `ident!` in field
        // position), so the macro emits the complete `Self { .. }` literal.
        macro_rules! backend_guard_field {
            ($(($field:ident, $backend:ident, $wrap:ident)),* $(,)?) => {
                Self {
                    instant: if mask.contains(Backends::INSTANT) || mask.contains(Backends::WEB) {
                        instant_wrap::enter(name, tag)
                    } else {
                        instant_wrap::dummy()
                    },
                    web_mark: if mask.contains(Backends::WEB) {
                        web_wrap::enter_mark(name)
                    } else {
                        web_wrap::dummy_mark()
                    },
                    $($field: if mask.contains(Backends::$backend) {
                        $wrap::enter(name, tag)
                    } else {
                        $wrap::dummy()
                    },)*
                    _not_send: PhantomData,
                }
            };
        }
        backend_guard_field! {
            (fastrace, FASTRACE, fastrace_wrap),
            (puffin, PUFFIN, puffin_wrap),
            (tracy, TRACY, tracy_wrap),
            (superluminal, SUPERLUMINAL, superluminal_wrap),
            (tracing, TRACING, tracing_wrap),
        }
    }

    /// Enter a scope with a dynamic name. Interns the string to satisfy
    /// `&'static str` — repeated calls with the same name share one leaked
    /// copy. Cold path — only called from dynamic scope creation (rare).
    // `#[must_use]` inherited from the struct (see `new_static`).
    pub fn new(name: &str, tag: Option<&str>) -> Self {
        Self::new_static(intern(name), tag.map(intern))
    }
}

/// Intern a dynamic scope name: return the existing leaked copy if present,
/// else leak once and insert. Bounded by the number of unique dynamic names
/// (~50 bytes each).
///
/// The bound covers unique dynamic scope NAMES *and TAGS* — [`ScopeGuard::new`]
/// interns both. A high-cardinality dynamic tag (request IDs, user IDs)
/// grows the leak without bound, so `ScopeGuard::new` is for low-cardinality
/// names/tags only; use [`enter_function_scope`] (`Cow::Owned`, no leak) for
/// dynamic function-scope names instead.
pub(crate) fn intern(s: &str) -> &'static str {
    static INTERN: LazyLock<Mutex<HashSet<&'static str>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));
    let mut set = INTERN.lock();
    if let Some(&existing) = set.get(s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    set.insert(leaked);
    leaked
}

// ── The unified scope! macro ──────────────────────────────────────────────

/// Create a profiling scope. Evaluates to a `ScopeGuard` holding one guard
/// per runtime-selected backend.
/// Zero-cost when the mask is `OFF`: one relaxed atomic load inside
/// `ScopeGuard::new_static` + all-dummy guards (~2ns).
#[macro_export]
macro_rules! scope {
    ($name:expr) => {{
        let _guard = $crate::profiling::ScopeGuard::new_static($name, None);
        _guard
    }};
    ($name:expr, $tag:expr) => {{
        let _guard = $crate::profiling::ScopeGuard::new_static($name, Some($tag.as_ref()));
        _guard
    }};
}

/// Start (or continue) a fastrace trace and install it as the thread-local
/// parent. Async-correct per task: bind the guard before the first `.await`
/// and keep it alive for the task's whole lifetime.
///
/// ```ignore
/// root_span!("request")        // new trace (random SpanContext)
/// root_span!("request", ctx)   // continue an incoming trace (SpanContext)
/// ```
///
/// Both forms bind `let _root = …;` and evaluate to the
/// [`LocalParentGuard`](fastrace::local::LocalParentGuard). Only available
/// with feature `fastrace`.
#[cfg(feature = "fastrace")]
#[macro_export]
macro_rules! root_span {
    ($name:expr) => {{
        let _root = $crate::profiling::async_::root_span($name);
        _root
    }};
    ($name:expr, $ctx:expr) => {{
        let _root = $crate::profiling::async_::root_span_with($name, $ctx);
        _root
    }};
}

// ── profiling! ────────────────────────────────────────────────────────────────

#[macro_export]
/// Enter a function scope named by `func_path!()` — the `profiling`-crate
/// compatibility form of `function_scope!()`. Pushes the thread-local scope
/// stack and sets the logforth `scope` diagnostic; the guard pops both on
/// drop. See [`enter_function_scope`].
macro_rules! profiling {
    () => {
        let _func_scope = $crate::profiling::enter_function_scope(::std::borrow::Cow::Borrowed(
            $crate::func_path!(),
        ));
    };
    ($data:expr) => {
        let _func_scope = $crate::profiling::enter_function_scope_with_tag(
            ::std::borrow::Cow::Borrowed($crate::func_path!()),
            $data,
        );
    };
}

#[macro_export]
/// The full `module::path::fn_name` of the enclosing function — the same
/// heuristic `profiling` uses (a zero-size local struct's `type_name` minus
/// the trailing `::S`).
macro_rules! func_path {
    () => {{
        struct S;
        let type_name = core::any::type_name::<S>();
        &type_name[..type_name.len() - 3]
    }};
}

#[macro_export]
/// Enter a function scope for the enclosing function (alias of
/// [`profiling!`] with identical semantics).
macro_rules! function_scope {
    () => {
        $crate::profiling!()
    };
    ($data:expr) => {
        let _func_scope = $crate::profiling::enter_function_scope_with_tag(
            ::std::borrow::Cow::Borrowed($crate::func_path!()),
            $data,
        );
    };
}

/// Mark a tick/frame boundary: `INSTANT`/`WEB` advance the instant
/// accumulator's frame counter; `PUFFIN` calls `puffin`'s per-frame hook
/// (upstream `profiling::finish_frame!` semantics).
#[macro_export]
macro_rules! finish_frame {
    () => {{
        let mask = $crate::config::config().backends();
        if mask.contains($crate::config::Backends::INSTANT)
            || mask.contains($crate::config::Backends::WEB)
        {
            $crate::profiling::instant_wrap::finish_frame();
        }
        if mask.contains($crate::config::Backends::PUFFIN) {
            $crate::profiling::puffin_wrap::finish_frame();
        }
    }};
}

// ── Function scope tracking (for automatic error context) ─────────────────

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::LazyLock;

use parking_lot::Mutex;
// Target-selected clock (fastant native / web-time wasm) — see `clock.rs`.
use self::clock::Instant;

thread_local! {
    pub(crate) static CURRENT_SCOPE: RefCell<Vec<(Cow<'static, str>, Instant)>> = const { RefCell::new(Vec::new()) };
    /// Cache of the last value written to the logforth `scope` diagnostic.
    /// logforth's `ThreadLocalDiagnostic::insert` is `Into<String>` over a
    /// `BTreeMap<String, String>` (~2 `String` allocations per call) — the
    /// hot path (`profiling!()` on every function entry) must not
    /// re-allocate when the value is unchanged.
    static LAST_SCOPE_DIAGNOSTIC: RefCell<Option<Cow<'static, str>>> = const { RefCell::new(None) };
}

/// Set the logforth `scope` diagnostic, skipping the logforth call when the
/// cached value already equals `name` (see [`LAST_SCOPE_DIAGNOSTIC`]).
/// `None` removes the key (skipped too when the cache is already `None`).
/// Silently skips during TLS teardown: this runs from
/// [`FunctionScopeGuard::drop`], where a `LocalKey::with` panic inside
/// `Drop` would abort the process during unwinding.
fn set_scope_diagnostic(name: Option<&str>) {
    let changed = LAST_SCOPE_DIAGNOSTIC
        .try_with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.as_deref() == name {
                return false;
            }
            *cache = name.map(|n| Cow::Owned(n.to_owned()));
            true
        })
        .unwrap_or(false);
    if !changed {
        return;
    }
    match name {
        Some(n) => logforth::diagnostic::ThreadLocalDiagnostic::insert("scope", n),
        None => logforth::diagnostic::ThreadLocalDiagnostic::remove("scope"),
    }
}

/// Enter a function scope — pushes `(name, now)` onto the thread-local scope
/// stack and sets the logforth `scope` diagnostic to the leaf name.
/// Returns a guard that pops the stack and restores both on drop.
/// Takes `Cow<'static, str>` — static names are Borrowed, dynamic names are Owned (no leak).
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the name is stored in the thread-local — taking by value avoids a double clone at the call site"
)]
pub fn enter_function_scope(name: Cow<'static, str>) -> FunctionScopeGuard {
    CURRENT_SCOPE.with(|s| s.borrow_mut().push((name.clone(), Instant::now())));
    set_scope_diagnostic(Some(name.as_ref()));
    FunctionScopeGuard {
        _not_send: PhantomData,
    }
}

/// Enter a function scope with a tag. The tag is appended to the scope name.
/// The tag may be any `impl AsRef<str>` (`&str`, `String`, etc.).
/// Uses `Cow::Owned` — no leak.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the name is stored in the thread-local — taking by value avoids a double clone at the call site"
)]
pub fn enter_function_scope_with_tag(
    name: Cow<'static, str>,
    tag: impl AsRef<str>,
) -> FunctionScopeGuard {
    let full = Cow::Owned(format!("{}:{}", name, tag.as_ref()));
    enter_function_scope(full)
}

/// Guard that pops the thread-local scope stack and restores the logforth
/// `scope` diagnostic on drop. Guards are created/dropped LIFO in well-formed
/// code, so `pop()` removes exactly this guard's entry. An out-of-order drop
/// (e.g. a `mem::forget`ed guard dropped later) pops whatever entry is on
/// top — same LIFO discipline as the instant span stack; don't leak guards.
pub struct FunctionScopeGuard {
    // Thread-bound marker: drop pops the THREAD-LOCAL scope stack, so a
    // cross-thread drop would pop the receiving thread's stack —
    // `PhantomData<*const ()>` makes that a compile error.
    _not_send: PhantomData<*const ()>,
}

impl Drop for FunctionScopeGuard {
    fn drop(&mut self) {
        // TLS teardown: the scope stack may already be destroyed — skip
        // silently. `LocalKey::with` panics in that state, and a panic
        // inside `Drop` during unwinding aborts the process.
        let Ok(parent) = CURRENT_SCOPE.try_with(|s| {
            let mut stack = s.borrow_mut();
            stack.pop();
            stack.last().map(|(name, _)| name.clone())
        }) else {
            return;
        };
        set_scope_diagnostic(parent.as_deref());
    }
}

/// Get the current (leaf) function scope name (if any).
/// Returns a `Cow<'static, str>` — use `.as_ref()` or `.as_deref()` for `&str`.
#[must_use]
pub fn current_scope_name() -> Option<Cow<'static, str>> {
    CURRENT_SCOPE.with(|s| s.borrow().last().map(|(name, _)| name.clone()))
}

/// The full scope path from outermost to innermost, e.g. `["request", "load_config", "parse_sql"]`.
/// Empty when no scope is active.
#[must_use]
pub fn scope_path() -> Vec<Cow<'static, str>> {
    CURRENT_SCOPE.with(|s| s.borrow().iter().map(|(name, _)| name.clone()).collect())
}

/// Milliseconds elapsed since the leaf scope was entered, if any scope is active.
#[must_use]
pub fn current_scope_elapsed_ms() -> Option<u128> {
    CURRENT_SCOPE.with(|s| {
        s.borrow()
            .last()
            .map(|(_, entered)| entered.elapsed().as_millis())
    })
}

// ── Re-export the proc macros ─────────────────────────────────────────────

pub use fast_observe_macros::{all_functions, instrument, skip};

#[cfg(test)]
mod tests {
    #[test]
    fn intern_dedups_dynamic_names() {
        let a = super::intern("same_name");
        let b = super::intern("same_name");
        assert!(
            std::ptr::eq(a, b),
            "same input must return the same pointer"
        );
        let c = super::intern("other_name");
        assert!(!std::ptr::eq(a, c), "different inputs must not alias");
    }
}
