//! Puffin backend — `scope!` → puffin `ProfilerScope` (feature `profile-with-puffin`).
//!
//! Mirrors upstream `profiling`'s puffin impl, as a runtime-name function.
//! Requires [`on_enable`] (`puffin::set_scopes_on(true)`) or every scope
//! no-ops; `set_backends` calls it when the `PUFFIN` bit flips on.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Scope-id cache. `ScopeId`s are registered per thread (each thread's
    /// `ThreadProfiler` owns its scope details), so the cache is thread-local.
    static SCOPE_IDS: RefCell<HashMap<&'static str, puffin::ScopeId>> =
        RefCell::new(HashMap::new());
}

fn scope_id(name: &'static str) -> puffin::ScopeId {
    if let Some(id) = SCOPE_IDS.with(|m| m.borrow().get(name).copied()) {
        return id;
    }
    let id = puffin::ThreadProfiler::call(|tp| tp.register_named_scope(name, "", "", 0));
    SCOPE_IDS.with(|m| m.borrow_mut().insert(name, id));
    id
}

/// Enter a puffin scope. `tag` becomes puffin's per-scope `data` string.
/// No-ops when puffin scopes are off (mirrors `puffin::profile_scope_if!`).
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> PuffinGuard {
    let scope = if puffin::are_scopes_on() {
        Some(puffin::ProfilerScope::new(
            scope_id(name),
            tag.unwrap_or(""),
        ))
    } else {
        None
    };
    PuffinGuard { scope }
}

/// Construct a no-op `PuffinGuard` (no real scope, does nothing on drop).
#[must_use]
pub const fn dummy() -> PuffinGuard {
    PuffinGuard { scope: None }
}

/// Puffin per-frame boundary — mirrors upstream `profiling::finish_frame!`.
pub fn finish_frame() {
    puffin::GlobalProfiler::lock().new_frame();
}

/// REQUIRED on first enable: puffin scopes no-op until this is called.
pub fn on_enable() {
    puffin::set_scopes_on(true);
}

/// Guard — the `ProfilerScope` ends on drop (when `scope` is `Some`).
#[expect(dead_code, reason = "field held only for its Drop side effect")]
pub struct PuffinGuard {
    scope: Option<puffin::ProfilerScope>,
}
