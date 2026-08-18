//! Web backend support — browser-console log appender + devtools timeline
//! marks (wasm32 only).
//!
//! The `Web` profiling backend itself is just the instant backend for span
//! timing (see `instant.rs`); this module adds log shipping to the browser
//! console via `web-sys` and `performance.mark()/measure()` emission so
//! `scope!` regions appear in the browser devtools Performance timeline
//! (DESIGN.md §11b). Compiled only for
//! `cfg(all(feature = "web", target_arch = "wasm32", target_os = "unknown"))`
//! — on native targets the `web` feature is a no-op, and on wasm32-wasip3
//! there is no browser console (wasm-bindgen's placeholder imports panic
//! there), so the feature degrades to `instant` spans only.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use logforth::append::Append;
use logforth::diagnostic::Diagnostic;
use logforth::record::Record;

/// Logforth appender that forwards log records to `console.log`.
#[derive(Debug, Default)]
pub struct WebConsoleAppend;

impl Append for WebConsoleAppend {
    fn append(
        &self,
        record: &Record,
        _diags: &[Box<dyn Diagnostic>],
    ) -> Result<(), logforth::Error> {
        let message = record.payload().to_string();
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&message));
        Ok(())
    }

    fn flush(&self) -> Result<(), logforth::Error> {
        Ok(())
    }
}

// ── Devtools Performance-timeline marks (DESIGN.md §11b) ──────────────────

/// Unique-per-guard id so overlapping same-name scopes (recursion) get
/// distinct mark names and `measure()` pairs the right start/end marks.
static MARK_ID: AtomicU32 = AtomicU32::new(0);

/// Guard that emits a browser-devtools Performance-timeline entry for a
/// scope: on enter `performance.mark("{name}::{id}::start")`, on drop
/// `mark("{name}::{id}::end")` + `measure(name, start, end)` — the measure
/// carries the bare scope name, so devtools groups same-name regions.
///
/// Not zero-cost like the span backends: two `format!` allocations and up
/// to three JS FFI calls per scope — intended for devtools-visible regions,
/// not hot loops. Missing APIs (no `window`, no `performance`) degrade to
/// no-ops and JS exceptions are swallowed, so non-browser wasm runtimes are
/// safe. A `None` name (from [`dummy_mark`]) is a pure no-op.
pub struct WebMarkGuard {
    name: Option<&'static str>,
    id: u32,
    // Thread-bound marker — matches the `web_wrap` stub's contract so the
    // `!Send`/`!Sync` guarantee holds regardless of target.
    _not_send: PhantomData<*const ()>,
}

/// Enter a named timeline region — see [`WebMarkGuard`].
#[must_use]
pub fn enter_mark(name: &'static str) -> WebMarkGuard {
    let id = MARK_ID.fetch_add(1, Ordering::Relaxed);
    if let Some(performance) = performance() {
        let _ = performance.mark(&mark_name(name, id, "start"));
    }
    WebMarkGuard {
        name: Some(name),
        id,
        _not_send: PhantomData,
    }
}

/// No-op guard for when the runtime mask's WEB bit is off.
#[must_use]
pub const fn dummy_mark() -> WebMarkGuard {
    WebMarkGuard {
        name: None,
        id: 0,
        _not_send: PhantomData,
    }
}

impl Drop for WebMarkGuard {
    fn drop(&mut self) {
        let Some(name) = self.name else { return };
        let Some(performance) = performance() else {
            return;
        };
        let _ = performance.mark(&mark_name(name, self.id, "end"));
        let _ = performance.measure_with_start_mark_and_end_mark(
            name,
            &mark_name(name, self.id, "start"),
            &mark_name(name, self.id, "end"),
        );
    }
}

/// `window.performance` — `None` off-browser (non-DOM global, worker without
/// `performance`), where marks become no-ops.
fn performance() -> Option<web_sys::Performance> {
    web_sys::window().and_then(|w| w.performance())
}

fn mark_name(name: &str, id: u32, kind: &str) -> String {
    format!("{name}::{id}::{kind}")
}

/// Register a one-shot `pagehide` listener flushing fastrace on page exit
/// (tab close / navigation / bfcache eviction), so the tail of the trace is
/// not lost. No-op when no `window` exists. The closure is intentionally
/// leaked via `Closure::forget` — a process-lifetime listener.
///
/// **Integration requirement:** not wired automatically — the deployment
/// layer must call this once during init when `web` + `fastrace` are on.
#[cfg(feature = "fastrace")]
#[expect(
    dead_code,
    reason = "wired by the deployment integration (Deployment::init) on wasm32-unknown-unknown"
)]
pub fn install_unload_flush() {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::<dyn FnMut()>::new(fastrace::flush);
    let options = web_sys::AddEventListenerOptions::new();
    options.set_once(true);
    // `js_sys::Function` is inferred — js-sys is not a direct dependency.
    let _ = window.add_event_listener_with_callback_and_add_event_listener_options(
        "pagehide",
        callback.as_ref().unchecked_ref(),
        &options,
    );
    callback.forget();
}
