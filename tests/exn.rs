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
    assert!(frame.type_name().ends_with("TestErr"));
    assert!(frame.location().file().ends_with("exn.rs"));
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

// ── Causal tree structure: nesting + tree rendering ────────────────────

/// A chainable test error: display name + optional boxed source.
#[derive(Debug)]
struct ChainErr(&'static str, Option<Box<ChainErr>>);
impl fmt::Display for ChainErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for ChainErr {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.1.as_deref().map(|e| e as &(dyn Error + 'static))
    }
}
/// `chain(&["a", "b", "c"])` = a with source b with source c.
fn chain(names: &[&'static str]) -> ChainErr {
    let (first, rest) = names.split_first().unwrap();
    ChainErr(first, (!rest.is_empty()).then(|| Box::new(chain(rest))))
}

#[test]
fn source_chain_nests_instead_of_flattening() {
    let f = Fault::new(chain(&["top", "mid", "leaf"]));
    let dbg = format!("{f:?}");
    // Nested chain: mid is a child of top, leaf a child of mid.
    assert!(dbg.contains("`-- mid"), "mid should nest under top: {dbg}");
    assert!(
        dbg.contains("    `-- leaf"),
        "leaf should nest under mid: {dbg}"
    );
}

#[test]
fn debug_tree_uses_continuation_prefixes_for_nested_children() {
    // Chain B becomes a child via wrap; chain A arrives via the wrapper's
    // own source chain. Tree shape:
    //   capA
    //   |-- topA          <- non-last sibling WITH children: needs `|   `
    //   |   `-- midA
    //   |       `-- leafA
    //   `-- outerB
    //       `-- midB
    //           `-- leafB
    let b = Fault::new(chain(&["outerB", "midB", "leafB"]));
    let big = b.wrap(chain(&["capA", "topA", "midA", "leafA"]));
    let dbg = format!("{big:?}");
    assert!(dbg.contains("|-- topA"), "first sibling: {dbg}");
    assert!(
        dbg.contains("|   `-- midA"),
        "non-last sibling's child needs continuation bar: {dbg}"
    );
    assert!(
        dbg.contains("|       `-- leafA"),
        "continuation bar holds at depth 3 (midA is last child of topA, so spaces after the bar): {dbg}"
    );
    assert!(dbg.contains("`-- outerB"), "last sibling: {dbg}");
    assert!(
        dbg.contains("    `-- midB"),
        "last sibling's child gets plain indent: {dbg}"
    );
}

#[test]
fn wrap_msg_preserves_nested_source_chain() {
    let inner: core::result::Result<(), ChainErr> = Err(chain(&["mid", "leaf"]));
    let err = inner.wrap_msg("wrapping").unwrap_err();
    let dbg = format!("{err:?}");
    assert!(dbg.contains("wrapping"), "wrapper message: {dbg}");
    // The original error's source chain survives, nested (was: dropped).
    assert!(dbg.contains("`-- mid"), "original error is a child: {dbg}");
    assert!(
        dbg.contains("    `-- leaf"),
        "original error's source nests beneath it: {dbg}"
    );
}
