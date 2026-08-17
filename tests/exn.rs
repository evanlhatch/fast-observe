//! `Fault` / `Context` / `bail!` behavior. Display strings here appear in
//! logs and crash dumps — changing them is a breaking observability change.
//! (See MIGRATING.md for provenance.)

use core::fmt;
use std::error::Error;

use fast_observe::exn::{Context, Fault, OptionExt, Result, ResultExt};
use fast_observe::{ErrorCategory, bail};

// ── Context Display stability — these strings appear in logs and crash
// dumps; changing them is a breaking observability change.

#[test]
fn context_display_stability() {
    assert_eq!(Context::None.to_string(), "None");
    assert_eq!(Context::scope("tick").to_string(), "tick");
    assert_eq!(Context::tick(7).to_string(), "tick 7");
    assert_eq!(Context::entity("units", 3).to_string(), "units at tick 3");
    assert_eq!(Context::custom("free").to_string(), "free");
}

#[test]
fn context_default_is_none() {
    assert_eq!(Context::default(), Context::None);
}

// ── ErrorCategory — drives retry/poison/abort policy; names are stable
// API surface for match arms and strum serialization.

#[test]
fn error_category_strum_names_stable() {
    assert_eq!(ErrorCategory::Content.as_ref(), "Content");
    assert_eq!(ErrorCategory::Invariant.as_ref(), "Invariant");
    assert_eq!(ErrorCategory::Transient.as_ref(), "Transient");
    assert_eq!(ErrorCategory::Fatal.as_ref(), "Fatal");
}

#[test]
fn error_category_display_matches_name() {
    assert_eq!(ErrorCategory::Content.to_string(), "Content");
    assert_eq!(ErrorCategory::Fatal.to_string(), "Fatal");
}

// ── Fault construction + context ──────────────────────────────────────

// A concrete error type — Fault's typed API (frame/context/with_context)
// requires E: Error, which Box<dyn Error> does not satisfy here.
#[derive(Debug)]
struct TestErr;
impl fmt::Display for TestErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("boom")
    }
}
impl Error for TestErr {}

#[test]
fn fault_from_str_displays_message() {
    let f = Fault::from("boom");
    assert!(f.to_string().contains("boom"));
}

#[test]
fn fault_with_context_shows_in_display() {
    let f = Fault::new(TestErr).with_context(Context::tick(42));
    let s = f.to_string();
    assert!(s.contains("boom"), "missing error: {s}");
    assert!(s.contains("tick 42"), "missing context: {s}");
    assert_eq!(f.context(), &Context::Tick(42));
}

#[test]
fn fault_frame_records_type_and_location() {
    let f = Fault::new(TestErr);
    let frame = f.frame();
    assert!(frame.type_name.ends_with("TestErr"));
    assert!(frame.location.file().ends_with("exn.rs"));
}

#[test]
fn result_ext_wrap_msg_preserves_cause() {
    let inner: core::result::Result<(), std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "missing file",
    ));
    let err = inner.wrap_msg("loading spec").unwrap_err();
    let s = err.to_string();
    assert!(s.contains("loading spec"), "missing wrapper msg: {s}");
    // The cause is a child frame in the causal tree — visible in Debug.
    let dbg = format!("{err:?}");
    assert!(dbg.contains("missing file"), "missing cause in tree: {dbg}");
}

#[test]
fn option_ext_ok_or_msg() {
    let v: Option<u32> = Some(5);
    assert_eq!(v.ok_or_msg("unused").unwrap(), 5);
    let none: Option<u32> = None;
    let err = none.ok_or_msg("was none").unwrap_err();
    assert!(err.to_string().contains("was none"));
}

#[test]
fn bail_macro_produces_fault_with_message() {
    fn inner() -> Result<()> {
        bail!("bailed 7");
    }
    let err = inner().unwrap_err();
    assert!(err.to_string().contains("bailed 7"));
}
