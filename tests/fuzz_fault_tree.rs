//! Fuzz target: fault tree construction + rendering, public API only.
//!
//! Builds arbitrary fault trees from `Fault::from(String)` /
//! `Fault::new(TreeError)` leaves via `wrap`, `ResultExt::context`,
//! `set_context`, `attach`/`attach_key`/`attach_placed`, and
//! `FaultCollection` merges — arbitrary branching, depth capped at 64.
//!
//! A model tracks each tree's exact frame count, first-child spine length,
//! height, and root Display text. Invariants per iteration:
//! - no panic anywhere (implicit — a panic fails the bolero case),
//! - `iter().count()` == modeled size,
//! - Debug terminates, is non-empty, and renders identically twice,
//! - Display terminates and equals the modeled root Display exactly,
//! - `render_report` terminates, is non-empty, and renders identically
//!   twice (determinism).

use std::error::Error;
use std::fmt;

use bolero::generator::TypeGenerator;
use fast_observe::exn::{Fault, FaultCollection};
use fast_observe::report::render_report;
use fast_observe::{BoxError, Context, Placement, ResultExt};

/// Trees deeper than this are not grown (keeps the render cheap; Debug
/// rendering is iterative, but depth still costs stack-adjacent work).
const MAX_DEPTH: usize = 64;
/// Trees larger than this are not grown (keeps one iteration fast).
const MAX_SIZE: usize = 8192;

/// Static keys for `attach_key` (the API requires `&'static str`). Includes
/// the report's reserved keys on purpose — they exercise the
/// reserved-key filtering in `render_report`.
const KEYS: &[&str] = &["attempt", "entity", "trace_id", "scope_path", "k"];

/// A plain leaf error with no source chain — keeps the frame-count model
/// exact (capture walks `source()`, so a source here would add frames).
#[derive(Debug)]
struct TreeError(String);

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for TreeError {}

#[derive(Debug, Clone, TypeGenerator)]
enum TreeOp {
    /// `Fault::from(String)` — a `Fault<BoxError>` leaf.
    LeafStr(String),
    /// `Fault::new(TreeError)` — a typed leaf.
    LeafTyped(String),
    /// `wrap` the top fault under a new typed root (typed tops only).
    Wrap(String),
    /// Raise the top fault through `ResultExt::context` — the fault's
    /// first-child spine becomes a STRINGIFIED child chain under a new
    /// boxed root (typed tops only).
    ContextMsg(String),
    /// `set_context` on the top fault (variant 0 = `Context::None`).
    SetContext(u8, String, u64),
    /// `attach` on the top fault.
    Attach(String),
    /// `attach_key` on the top fault (key from [`KEYS`]).
    AttachKey(u8, String),
    /// `attach_placed` on the top fault (placement from index).
    AttachPlaced(u8, String),
    /// Pop two faults, merge under one root via [`FaultCollection`]
    /// (typed or message root chosen by the payload byte).
    Merge(u8),
}

/// One fault on the work stack. `wrap`/`context` need `E: Error`, which
/// `BoxError` is not — hence the two forms.
enum Node {
    Typed(Fault<TreeError>),
    Boxed(Fault<BoxError>),
}

impl Node {
    fn set_context(self, ctx: Context) -> Self {
        match self {
            Self::Typed(f) => Self::Typed(f.set_context(ctx)),
            Self::Boxed(f) => Self::Boxed(f.set_context(ctx)),
        }
    }

    fn attach(self, value: String) -> Self {
        match self {
            Self::Typed(f) => Self::Typed(f.attach(value)),
            Self::Boxed(f) => Self::Boxed(f.attach(value)),
        }
    }

    fn attach_key(self, key: &'static str, value: String) -> Self {
        match self {
            Self::Typed(f) => Self::Typed(f.attach_key(key, value)),
            Self::Boxed(f) => Self::Boxed(f.attach_key(key, value)),
        }
    }

    fn attach_placed(self, value: String, placement: Placement) -> Self {
        match self {
            Self::Typed(f) => Self::Typed(f.attach_placed(value, placement)),
            Self::Boxed(f) => Self::Boxed(f.attach_placed(value, placement)),
        }
    }

    /// Assert every invariant against the model (dispatches per variant).
    fn check_invariants(&self, model: &Model) {
        match self {
            Self::Typed(f) => check(f, model),
            Self::Boxed(f) => check(f, model),
        }
    }
}

/// The invariant battery for a finished tree.
fn check<E: Send + Sync + 'static>(fault: &Fault<E>, model: &Model) {
    assert_eq!(
        fault.iter().count(),
        model.size,
        "iter().count() == modeled frame count"
    );
    let dbg = format!("{fault:?}");
    assert!(!dbg.is_empty(), "Debug renders non-empty");
    assert_eq!(dbg, format!("{fault:?}"), "Debug is deterministic");
    assert_eq!(
        fault.to_string(),
        model.root_display(),
        "Display == modeled root Display"
    );
    let report = render_report(fault);
    assert!(!report.is_empty(), "report renders non-empty");
    assert_eq!(report, render_report(fault), "report is deterministic");
}

/// Exact structural model of a [`Node`].
#[derive(Clone)]
struct Model {
    /// Total frames in the tree (what `iter().count()` must return).
    size: usize,
    /// Frames on the first-child spine, root included. Drives the size of
    /// the stringified chain `ResultExt::context` produces.
    spine: usize,
    /// Longest root-to-leaf path, in frames.
    height: usize,
    /// Root frame's error Display text.
    root_msg: String,
    /// Root frame's context (drives the Display ` (ctx)` suffix).
    root_ctx: Option<Context>,
}

impl Model {
    fn leaf(msg: &str) -> Self {
        Self {
            size: 1,
            spine: 1,
            height: 1,
            root_msg: msg.to_string(),
            root_ctx: None,
        }
    }

    /// The modeled root Display: `msg` plus the ` (ctx)` suffix when a
    /// non-None context is attached.
    fn root_display(&self) -> String {
        match &self.root_ctx {
            None | Some(Context::None) => self.root_msg.clone(),
            Some(ctx) => format!("{} ({ctx})", self.root_msg),
        }
    }
}

fn context_from(sel: u8, s: &str, n: u64) -> Option<Context> {
    match sel % 5 {
        0 => None,
        1 => Some(Context::scope(s.to_string())),
        2 => Some(Context::tick(n)),
        3 => Some(Context::entity(s.to_string(), n)),
        _ => Some(Context::custom(s.to_string())),
    }
}

fn placement_from(sel: u8) -> Placement {
    match sel % 4 {
        0 => Placement::Inline,
        1 => Placement::Appendix,
        2 => Placement::Opaque,
        _ => Placement::Hidden,
    }
}

/// Merge two nodes under one fresh root. The first-popped node's root
/// becomes the FIRST child (collection push order), so the merged spine
/// grows from `a`'s spine only. Mirrors [`FaultCollection::into_fault`] /
/// [`FaultCollection::into_fault_msg`]: the fresh root adds exactly one
/// frame and has no captured source chain (`TreeError`/`InternalError`
/// have no sources).
fn merge_nodes(a: (Node, Model), b: (Node, Model), typed_root: bool) -> (Node, Model) {
    let (a_node, a_model) = a;
    let (b_node, b_model) = b;
    let mut collection = FaultCollection::new();
    match a_node {
        Node::Typed(f) => collection.push(f),
        Node::Boxed(f) => collection.push(f),
    }
    match b_node {
        Node::Typed(f) => collection.push(f),
        Node::Boxed(f) => collection.push(f),
    }
    assert_eq!(collection.len(), 2, "collection holds both merged faults");
    let model = Model {
        size: 1 + a_model.size + b_model.size,
        spine: 1 + a_model.spine,
        height: 1 + a_model.height.max(b_model.height),
        root_msg: "merged".to_string(),
        root_ctx: None,
    };
    let node = if typed_root {
        Node::Typed(collection.into_fault(TreeError("merged".to_string())))
    } else {
        Node::Boxed(collection.into_fault_msg("merged"))
    };
    (node, model)
}

/// Run one op sequence, then assert every invariant on every live tree.
fn run_ops(ops: &[TreeOp]) {
    let mut stack: Vec<(Node, Model)> = Vec::new();

    for op in ops {
        match op {
            TreeOp::LeafStr(msg) => {
                stack.push((Node::Boxed(Fault::from(msg.clone())), Model::leaf(msg)));
            }
            TreeOp::LeafTyped(msg) => {
                stack.push((
                    Node::Typed(Fault::new(TreeError(msg.clone()))),
                    Model::leaf(msg),
                ));
            }
            TreeOp::Wrap(msg) => {
                // Typed tops only: `wrap` needs `E: Error`. Over budget or
                // boxed top: put it back untouched.
                let Some((node, model)) = stack.pop() else {
                    continue;
                };
                match node {
                    Node::Typed(f) if model.height < MAX_DEPTH && model.size < MAX_SIZE => {
                        let model = Model {
                            size: model.size + 1,
                            spine: model.spine + 1,
                            height: model.height + 1,
                            root_msg: msg.clone(),
                            root_ctx: None,
                        };
                        stack.push((Node::Typed(f.wrap(TreeError(msg.clone()))), model));
                    }
                    node => stack.push((node, model)),
                }
            }
            TreeOp::ContextMsg(msg) => {
                // Typed tops only (`E: Error` bound). `context` builds a new
                // boxed root whose child frame wraps the fault; the fault's
                // first-child SPINE is stringified into a nested chain —
                // off-spine branches are dropped. Result: a pure chain of
                // `1 + old.spine` frames (new root + frame for the fault +
                // old.spine - 1 stringified sources).
                let Some((node, model)) = stack.pop() else {
                    continue;
                };
                match node {
                    Node::Typed(f) if model.height < MAX_DEPTH && model.size < MAX_SIZE => {
                        let Err(fault) = Err::<(), Fault<TreeError>>(f).context(msg.clone()) else {
                            unreachable!("we built an Err")
                        };
                        let model = Model {
                            size: model.spine + 1,
                            spine: model.spine + 1,
                            height: model.spine + 1,
                            root_msg: msg.clone(),
                            root_ctx: None,
                        };
                        stack.push((Node::Boxed(fault), model));
                    }
                    node => stack.push((node, model)),
                }
            }
            TreeOp::SetContext(sel, s, n) => {
                if let Some((node, mut model)) = stack.pop() {
                    let ctx = context_from(*sel, s, *n);
                    model.root_ctx.clone_from(&ctx);
                    stack.push((node.set_context(ctx.unwrap_or(Context::None)), model));
                }
            }
            TreeOp::Attach(value) => {
                if let Some((node, model)) = stack.pop() {
                    stack.push((node.attach(value.clone()), model));
                }
            }
            TreeOp::AttachKey(sel, value) => {
                if let Some((node, model)) = stack.pop() {
                    let key = KEYS[usize::from(*sel) % KEYS.len()];
                    stack.push((node.attach_key(key, value.clone()), model));
                }
            }
            TreeOp::AttachPlaced(sel, value) => {
                if let Some((node, model)) = stack.pop() {
                    let placement = placement_from(*sel);
                    stack.push((node.attach_placed(value.clone(), placement), model));
                }
            }
            TreeOp::Merge(sel) => {
                if stack.len() < 2 {
                    continue;
                }
                let b = stack.pop();
                let a = stack.pop();
                let (Some(a), Some(b)) = (a, b) else {
                    unreachable!("len checked above")
                };
                stack.push(merge_nodes(a, b, sel % 2 == 0));
            }
        }
    }

    for (node, model) in &stack {
        node.check_invariants(model);
    }
}

#[test]
fn fuzz_fault_tree_invariants() {
    bolero::check!()
        .with_type::<Vec<TreeOp>>()
        .for_each(|ops: &Vec<TreeOp>| run_ops(ops));
}
