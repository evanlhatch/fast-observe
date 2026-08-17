//! Profiling — forked from `profiling` (MIT) with improvements:
//!
//! 1. **`fastrace` feature** — fastrace replaces `tracing`.
//! 2. **`instant` feature** — thread-local span accumulator that **coexists**
//!    with other backends. Replaces HotProfile-style ad-hoc timers.
//! 3. **Unified `scope!`** — one macro feeds the config-selected backend.
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
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub(crate) mod web;

// ── Wrap modules: feature-gated re-exports + ZST stubs ─────────────────────

macro_rules! profiling_backend {
    ($wrap:ident, $backend:ident, $guard:ident, feat = $feat:literal $(, $finish_frame:ident)?) => {
        pub mod $wrap {
            #[cfg(feature = $feat)]
            pub use super::$backend::{dummy, enter, $($finish_frame,)? $guard};
            #[cfg(not(feature = $feat))]
            pub struct $guard;
            #[cfg(not(feature = $feat))]
            pub const fn enter<'a>(_name: &'a str, _tag: Option<&'a str>) -> $guard {
                $guard
            }
            #[cfg(not(feature = $feat))]
            pub const fn dummy() -> $guard {
                $guard
            }
            $(
            #[cfg(not(feature = $feat))]
            pub const fn $finish_frame() {}
            )?
        }
    };
}

profiling_backend!(
    instant_wrap,
    instant,
    InstantGuard,
    feat = "instant",
    finish_frame
);

profiling_backend!(fastrace_wrap, fastrace, FastraceGuard, feat = "fastrace");

// ── Unified scope guard ────────────────────────────────────────────────────

/// Guard returned by [`scope!`]. Holds the guards for all enabled backends.
/// Fields are never read — they exist purely for their Drop side effects.
#[allow(dead_code, reason = "fields held only for Drop side effects")]
pub struct ScopeGuard(instant_wrap::InstantGuard, fastrace_wrap::FastraceGuard);

impl ScopeGuard {
    /// Enter a scope for the config-selected backend only.
    /// Zero allocation — the name is `'static` (a string literal).
    ///
    /// `Web` behaves like `Instant` for span timing; the browser-console half
    /// of the web backend is a log appender, not a span sink.
    #[must_use]
    pub fn new_static(name: &'static str, tag: Option<&'static str>) -> Self {
        match crate::config::config().profiling_backend() {
            crate::config::ProfilingBackend::Instant | crate::config::ProfilingBackend::Web => {
                Self(instant_wrap::enter(name, tag), fastrace_wrap::dummy())
            }
            crate::config::ProfilingBackend::Fastrace => {
                Self(instant_wrap::dummy(), fastrace_wrap::enter(name, tag))
            }
            crate::config::ProfilingBackend::Off => {
                Self(instant_wrap::dummy(), fastrace_wrap::dummy())
            }
        }
    }

    /// Enter a scope with a dynamic name. Leaks the string to satisfy `&'static str`.
    /// Cold path — only called from dynamic scope creation (very rare).
    /// The leak is bounded by the number of unique dynamic scopes (~50 bytes each).
    #[must_use]
    pub fn new(name: &str, tag: Option<&str>) -> Self {
        fn to_static(s: &str) -> &'static str {
            Box::leak(s.to_owned().into_boxed_str())
        }
        Self::new_static(to_static(name), tag.map(to_static))
    }
}

// ── The unified scope! macro ──────────────────────────────────────────────

/// Create a profiling scope. Only constructs the guard for the config-selected backend.
/// Returns `Option<ScopeGuard>` — `None` when profiling is Off.
/// Zero-cost when `Off` (relaxed atomic load + predictable branch, ~2ns).
#[macro_export]
macro_rules! scope {
    ($name:expr) => {{
        let _guard = if $crate::config::config().profiling_backend()
            != $crate::config::ProfilingBackend::Off
        {
            Some($crate::profiling::ScopeGuard::new_static($name, None))
        } else {
            None
        };
        _guard
    }};
    ($name:expr, $tag:expr) => {{
        let _guard = if $crate::config::config().profiling_backend()
            != $crate::config::ProfilingBackend::Off
        {
            Some($crate::profiling::ScopeGuard::new_static(
                $name,
                Some($tag.as_ref()),
            ))
        } else {
            None
        };
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

#[macro_export]
macro_rules! finish_frame {
    () => {
        if matches!(
            $crate::config::config().profiling_backend(),
            $crate::config::ProfilingBackend::Instant | $crate::config::ProfilingBackend::Web
        ) {
            $crate::profiling::instant_wrap::finish_frame();
        }
    };
}

// ── Function scope tracking (for automatic error context) ─────────────────

use std::borrow::Cow;
use std::cell::RefCell;
// web_time, not std: `std::time::Instant::now` panics on wasm32-unknown-unknown.
use web_time::Instant;

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

pub use profiling_procmacros::all_functions;
pub use profiling_procmacros::skip;
