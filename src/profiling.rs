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
//! 6. **`web` feature** — instant spans + browser-console logging on wasm32.
//!
//! The `scope!` / `#[all_functions]` API is compatible with upstream
//! `profiling`.

// ── Backend modules ────────────────────────────────────────────────────────

pub(crate) mod clock;
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
#[cfg(all(feature = "web", target_arch = "wasm32"))]
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

// ── Unified scope guard ────────────────────────────────────────────────────

/// Guard returned by [`scope!`]. Holds one guard per backend; only backends
/// whose bit is set in the runtime mask (and whose feature is compiled in)
/// hold real guards — the rest are ZST stubs.
/// Fields are never read — they exist purely for their Drop side effects.
#[allow(dead_code, reason = "fields held only for Drop side effects")]
pub struct ScopeGuard {
    instant: instant_wrap::InstantGuard,
    fastrace: fastrace_wrap::FastraceGuard,
    puffin: puffin_wrap::PuffinGuard,
    tracy: tracy_wrap::TracyGuard,
    superluminal: superluminal_wrap::SuperluminalGuard,
    tracing: tracing_wrap::TracingGuard,
}

impl ScopeGuard {
    /// Enter a scope for every backend selected in the runtime mask.
    /// Zero allocation — the name is `'static` (a string literal).
    /// Loads the mask ONCE; `Backends::OFF` → all-dummy guard (~2ns).
    ///
    /// `WEB` behaves like `INSTANT` for span timing; the browser-console half
    /// of the web backend is a log appender, not a span sink.
    #[must_use]
    pub fn new_static(name: &'static str, tag: Option<&'static str>) -> Self {
        use crate::config::Backends;
        let mask = crate::config::config().backends();
        Self {
            instant: if mask.contains(Backends::INSTANT) || mask.contains(Backends::WEB) {
                instant_wrap::enter(name, tag)
            } else {
                instant_wrap::dummy()
            },
            fastrace: if mask.contains(Backends::FASTRACE) {
                fastrace_wrap::enter(name, tag)
            } else {
                fastrace_wrap::dummy()
            },
            puffin: if mask.contains(Backends::PUFFIN) {
                puffin_wrap::enter(name, tag)
            } else {
                puffin_wrap::dummy()
            },
            tracy: if mask.contains(Backends::TRACY) {
                tracy_wrap::enter(name, tag)
            } else {
                tracy_wrap::dummy()
            },
            superluminal: if mask.contains(Backends::SUPERLUMINAL) {
                superluminal_wrap::enter(name, tag)
            } else {
                superluminal_wrap::dummy()
            },
            tracing: if mask.contains(Backends::TRACING) {
                tracing_wrap::enter(name, tag)
            } else {
                tracing_wrap::dummy()
            },
        }
    }

    /// Enter a scope with a dynamic name. Interns the string to satisfy
    /// `&'static str` — repeated calls with the same name share one leaked
    /// copy. Cold path — only called from dynamic scope creation (rare).
    #[must_use]
    pub fn new(name: &str, tag: Option<&str>) -> Self {
        Self::new_static(intern(name), tag.map(intern))
    }
}

/// Intern a dynamic scope name: return the existing leaked copy if present,
/// else leak once and insert. Bounded by the number of unique dynamic names
/// (~50 bytes each).
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

// ── profiling! ────────────────────────────────────────────────────────────────

#[macro_export]
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
macro_rules! func_path {
    () => {{
        struct S;
        let type_name = core::any::type_name::<S>();
        &type_name[..type_name.len() - 3]
    }};
}

#[macro_export]
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
use std::sync::LazyLock;

use parking_lot::Mutex;
// Target-selected clock (fastant native / web-time wasm) — see `clock.rs`.
use self::clock::Instant;

thread_local! {
    pub(crate) static CURRENT_SCOPE: RefCell<Vec<(Cow<'static, str>, Instant)>> = const { RefCell::new(Vec::new()) };
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
    logforth::diagnostic::ThreadLocalDiagnostic::insert("scope", name.as_ref());
    FunctionScopeGuard
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
pub struct FunctionScopeGuard;

impl Drop for FunctionScopeGuard {
    fn drop(&mut self) {
        let parent = CURRENT_SCOPE.with(|s| {
            let mut stack = s.borrow_mut();
            stack.pop();
            stack.last().map(|(name, _)| name.clone())
        });
        match parent {
            Some(name) => {
                logforth::diagnostic::ThreadLocalDiagnostic::insert("scope", name.as_ref());
            }
            None => logforth::diagnostic::ThreadLocalDiagnostic::remove("scope"),
        }
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
