//! `Fault` / `Context` / `bail!` behavior. Display strings here appear in
//! logs and crash dumps — changing them is a breaking observability change.
//! (See MIGRATING.md for provenance.)
// Integration tests are separate crates — the nightly gate must be enabled
// here, not inherited from the library root (needed by the `error!` macro's
// generated `Error::provide` overrides).
#![feature(error_generic_member_access)]

use core::fmt;
use std::cell::Cell;
use std::error::Error;
use std::process::ExitCode;

use fast_observe::exn::{
    Context, Fault, FaultCollection, OptionExt, Placement, Result, ResultExt,
    error_counts_by_category, retry_with_policy,
};
use fast_observe::{ErrorCategory, Policy, bail};

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

// ── Tree traversal: iter + root_cause ────────────────────────────────

#[test]
fn iter_visits_preorder() {
    let f = Fault::new(chain(&["top", "mid", "leaf"]));
    let names: Vec<String> = f.iter().map(|fr| fr.error().to_string()).collect();
    assert_eq!(names, ["top", "mid", "leaf"]);
}

#[test]
fn iter_with_branching_visits_all() {
    // Tree shape:
    //   capA
    //   |-- topA -> midA -> leafA   (wrapper's source chain, first child)
    //   `-- outerB -> midB -> leafB (wrapped fault's root, second child)
    let b = Fault::new(chain(&["outerB", "midB", "leafB"]));
    let big = b.wrap(chain(&["capA", "topA", "midA", "leafA"]));
    let names: Vec<String> = big.iter().map(|fr| fr.error().to_string()).collect();
    assert_eq!(names.len(), 7, "all 7 frames visited: {names:?}");
    for name in ["capA", "topA", "midA", "leafA", "outerB", "midB", "leafB"] {
        assert!(names.iter().any(|n| n == name), "missing {name}: {names:?}");
    }
    // Pre-order, children left-to-right: the wrapper's source chain comes
    // before the wrapped fault's root.
    assert_eq!(
        &names[..5],
        ["capA", "topA", "midA", "leafA", "outerB"],
        "pre-order start: {names:?}"
    );
}

#[test]
fn root_cause_is_deepest_first_branch() {
    let b = Fault::new(chain(&["outerB", "midB", "leafB"]));
    let big = b.wrap(chain(&["capA", "topA", "midA", "leafA"]));
    assert_eq!(big.root_cause().error().to_string(), "leafA");

    let single = Fault::new(chain(&["only"]));
    assert_eq!(single.root_cause().error().to_string(), "only");
}

// ── Attachments: typed data on frames ───────────────────────────────

#[test]
fn attach_and_downcast_roundtrip() {
    let f = Fault::new(TestErr)
        .attach_key("attempt", 3u32)
        .attach(String::from("payload"));
    assert_eq!(f.find_attachment::<u32>(), Some(&3));
    let atts = f.frame().attachments();
    assert_eq!(atts.len(), 2);
    assert_eq!(atts[0].key(), Some("attempt"));
    assert_eq!(atts[0].display(), "3");
    assert_eq!(atts[0].placement(), Placement::Inline);
    assert_eq!(atts[0].to_string(), "attempt: 3");
    assert_eq!(atts[1].key(), None);
    assert_eq!(atts[1].display(), "payload");
    assert_eq!(atts[1].to_string(), "payload");
    assert_eq!(
        f.frame().find_attachment::<String>().map(String::as_str),
        Some("payload")
    );
}

#[test]
fn attach_with_lazy_on_ok() {
    use std::cell::Cell;
    let flag = Cell::new(false);

    let ok: core::result::Result<u32, TestErr> = Ok(7);
    let v = ok
        .attach_with(|| {
            flag.set(true);
            1u32
        })
        .unwrap();
    assert_eq!(v, 7);
    assert!(!flag.get(), "closure must not run on Ok");

    let err: core::result::Result<u32, TestErr> = Err(TestErr);
    let e = err
        .attach_with(|| {
            flag.set(true);
            42u32
        })
        .unwrap_err();
    assert!(flag.get(), "closure runs on Err");
    assert_eq!(e.find_attachment::<u32>(), Some(&42));
}

#[test]
fn attachments_render_in_debug_tree() {
    let f = Fault::new(TestErr)
        .attach_key("attempt", 3u32)
        .attach_placed("secret-token", Placement::Hidden);
    let dbg = format!("{f:?}");
    // Only attachment: it is the last pseudo-child → ``-- `.
    assert!(dbg.contains("`-- * attempt: 3"), "inline attachment: {dbg}");
    assert!(
        dbg.contains(" (+1 more attachments)"),
        "hidden attachment counted on frame line: {dbg}"
    );
    assert!(
        !dbg.contains("secret-token"),
        "hidden value must not render: {dbg}"
    );
}

#[test]
fn non_last_frame_attachments_use_continuation() {
    // Attached frames always land as the LAST sibling (attach mutates the
    // root pre-wrap, wrap pushes it last), so the continuation bar `|   `
    // shows on the NON-last sibling's subtree, and the attachment is the
    // non-last PSEUDO-child of its own frame (before the real child).
    // Tree shape:
    //   capA
    //   |-- topA
    //   |   `-- midA
    //   |       `-- leafA
    //   `-- outerB (+1 more attachments would appear if non-inline)
    //       |-- * attempt: 3        <- non-last pseudo-child: `|-- `
    //       `-- midB                <- real child is last: ``-- `
    //           `-- leafB
    let b = Fault::new(chain(&["outerB", "midB", "leafB"])).attach_key("attempt", 3u32);
    let big = b.wrap(chain(&["capA", "topA", "midA", "leafA"]));
    let dbg = format!("{big:?}");
    assert!(dbg.contains("|-- topA"), "first sibling: {dbg}");
    assert!(
        dbg.contains("|   `-- midA"),
        "non-last sibling's child keeps continuation bar: {dbg}"
    );
    assert!(dbg.contains("`-- outerB"), "last sibling: {dbg}");
    assert!(
        dbg.contains("    |-- * attempt: 3"),
        "attachment is non-last pseudo-child under outerB: {dbg}"
    );
    assert!(
        dbg.contains("    `-- midB"),
        "real child after attachment is last: {dbg}"
    );
}

// ── Error::source chaining into the tree ──────────────────────────────

#[test]
fn fault_source_chains_into_tree() {
    let fault = Fault::new(chain(&["a", "b", "c"]));
    // Walking `Error::source` from the Fault descends the tree: root's first
    // child is "b", its first child is "c", then the chain ends.
    let b = Error::source(&fault).expect("root has a source");
    assert_eq!(b.to_string(), "b");
    let c = b.source().expect("b frame has a source");
    assert_eq!(c.to_string(), "c");
    assert!(c.source().is_none(), "c frame is the leaf");
}

// ── FaultCollection — multi-failure aggregation ────────────────────────

#[test]
fn fault_collection_into_fault_children() {
    let mut collection = FaultCollection::new();
    assert!(collection.is_empty());
    collection.push(Fault::new(chain(&["e1a", "e1b"])));
    collection.push(Fault::new(TestErr));
    assert_eq!(collection.len(), 2);
    assert!(!collection.is_empty());

    let agg = collection.into_fault(chain(&["agg"]));
    assert_eq!(agg.to_string(), "agg");
    let root = agg.frame();
    assert_eq!(
        root.children().len(),
        2,
        "both collected roots are children"
    );
    // iter() visits agg + both subtrees: agg, e1a, e1b, boom.
    let names: Vec<String> = agg.iter().map(|fr| fr.error().to_string()).collect();
    assert_eq!(names, ["agg", "e1a", "e1b", "boom"]);
}

#[test]
fn collect_adapter() {
    let results: Vec<core::result::Result<i32, Fault<TestErr>>> = vec![
        Ok(1),
        Err(Fault::new(TestErr)),
        Ok(2),
        Err(Fault::new(TestErr)),
    ];
    let failures = results
        .into_iter()
        .filter_map(core::result::Result::err)
        .collect::<FaultCollection>();
    assert_eq!(failures.len(), 2);
}

#[test]
fn into_fault_msg_wraps_collected_under_plain_message() {
    let failures = vec![Fault::new(TestErr)]
        .into_iter()
        .collect::<FaultCollection>();
    let agg = failures.into_fault_msg("batch failed");
    assert!(agg.to_string().contains("batch failed"));
    assert_eq!(agg.frame().children().len(), 1);
    assert_eq!(agg.iter().count(), 2, "message root + one subtree");
}

// ── Coded errors: code-in-tree, policy, exit codes, retry ────────────────

// Coded test errors via the real `error!` macro — exercises the registry
// registration + `Error::provide` path end-to-end. The VARIANT STRUCTS
// (`TransientBoom` / `ContentBoom`) are what tests construct: their
// `type_name` leaf matches the registry entry's `name`, which
// `error_counts_by_category` relies on.
fast_observe::error! {
    /// Errors for code/policy/retry tests.
    #[derive(Debug)]
    pub enum RetryTestError {
        /// transient failure — retryable
        #[error("transient boom")]
        #[code = "E901", category = Transient]
        TransientBoom {},

        /// content failure — fix the input
        #[error("content boom")]
        #[code = "E902", category = Content]
        ContentBoom {},
    }
}

#[test]
fn debug_tree_shows_codes() {
    // Root is coded, wrapped child is coded: both lines carry `[CODE] `.
    let inner = Fault::new(TransientBoom {});
    let wrapped = inner.wrap(ContentBoom {});
    let dbg = format!("{wrapped:?}");
    assert!(
        dbg.contains("[E902] content boom, at"),
        "root line carries the code prefix: {dbg}"
    );
    assert!(
        dbg.contains("`-- [E901] transient boom, at"),
        "child line carries the code prefix after the connector: {dbg}"
    );
}

#[test]
fn policy_and_exit_code() {
    let transient = Fault::new(TransientBoom {});
    assert_eq!(transient.policy(), Some(Policy::Retry));
    assert_eq!(transient.exit_code(), ExitCode::from(75));

    let content = Fault::new(ContentBoom {});
    assert_eq!(content.policy(), Some(Policy::FixInput));
    assert_eq!(content.exit_code(), ExitCode::from(65));

    let plain = Fault::from("plain");
    assert_eq!(plain.policy(), None);
    assert_eq!(plain.exit_code(), ExitCode::from(1));
}

#[test]
fn retry_with_policy_retries_transient() {
    let counter = Cell::new(0u32);
    let result: Result<u32, TransientBoom> = retry_with_policy("flaky-op", 3, || {
        counter.set(counter.get() + 1);
        if counter.get() < 3 {
            Err(TransientBoom {})
        } else {
            Ok(7)
        }
    });
    assert_eq!(result.unwrap(), 7);
    assert_eq!(counter.get(), 3, "two failures + one success");
}

#[test]
fn retry_with_policy_no_retry_on_content() {
    let counter = Cell::new(0u32);
    let result: Result<u32, ContentBoom> = retry_with_policy("rigid-op", 5, || {
        counter.set(counter.get() + 1);
        Err(ContentBoom {})
    });
    let fault = result.unwrap_err();
    assert_eq!(counter.get(), 1, "Content policy is FixInput — no retries");
    // Immediate return: the fault is unchanged (no exhaustion message).
    assert_eq!(fault.to_string(), "content boom");

    // Uncoded errors (policy None) also get no retries.
    let uncoded = Cell::new(0u32);
    let result: Result<u32, TestErr> = retry_with_policy("plain-op", 5, || {
        uncoded.set(uncoded.get() + 1);
        Err(TestErr)
    });
    assert!(result.is_err());
    assert_eq!(uncoded.get(), 1, "uncoded errors return immediately");
}

#[test]
fn retry_with_policy_collects_on_exhaustion() {
    let counter = Cell::new(0u32);
    let result: Result<u32, TransientBoom> = retry_with_policy("doomed-op", 3, || {
        counter.set(counter.get() + 1);
        Err(TransientBoom {})
    });
    let fault = result.unwrap_err();
    assert_eq!(counter.get(), 3, "max_attempts is the total attempt count");
    // One fault wrapping all attempts: the final attempt is the root, the
    // earlier attempts are children of its frame.
    assert_eq!(fault.iter().count(), 3, "all 3 attempts in one tree");
    let msg = fault.to_string();
    assert!(msg.contains("doomed-op"), "label in message: {msg}");
    assert!(
        msg.contains("failed after 3 attempts"),
        "attempt count in message: {msg}"
    );
    let dbg = format!("{fault:?}");
    assert_eq!(
        dbg.matches("[E901]").count(),
        3,
        "every attempt frame carries its code: {dbg}"
    );
}

#[test]
fn error_counts_by_category_buckets() {
    // Construct one coded + one plain error so both bucket kinds exist.
    // (ERROR_COUNTS is process-global — assertions are presence-based.)
    let _coded = Fault::new(TransientBoom {});
    let _plain = Fault::new(TestErr);
    let buckets = error_counts_by_category();
    assert!(
        buckets
            .iter()
            .any(|(cat, n)| *cat == Some(ErrorCategory::Transient) && *n >= 1),
        "coded type bucketed under Some(Transient): {buckets:?}"
    );
    assert!(
        buckets.iter().any(|(cat, n)| cat.is_none() && *n >= 1),
        "plain types land in the None bucket: {buckets:?}"
    );
}
