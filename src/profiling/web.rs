//! Web backend support — browser-console log appender (wasm32 only).
//!
//! The `Web` profiling backend itself is just the instant backend for span
//! timing (see `instant.rs`); this module adds log shipping to the browser
//! console via `web-sys`. Compiled only for
//! `cfg(all(feature = "web", target_arch = "wasm32", target_os = "unknown"))`
//! — on native targets the `web` feature is a no-op, and on wasm32-wasip3
//! there is no browser console (wasm-bindgen's placeholder imports panic
//! there), so the feature degrades to `instant` spans only.

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
