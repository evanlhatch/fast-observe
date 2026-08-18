//! Fuzz target: report renderer robustness against hostile strings.
//!
//! Builds faults via the public API only — `Fault::from(String)` /
//! `Fault::new(HostileError)` leaves, `set_context` (`Context`
//! constructors), `attach`/`attach_key`/`attach_placed` (`Placement`),
//! and `ResultExt::context` on a failing `std::io::Error` (one-level
//! cause) — driven by a bolero-generated op sequence. Depth is NOT what
//! this target tests; hostile STRINGS are: messages/keys/values are
//! biased toward control chars, raw `\n`/`\r`, quotes, braces, unicode,
//! and long runs, via a bag of nasty fragments plus arbitrary data.
//!
//! Invariants per case:
//! - `render_report` never panics and renders twice byte-identically,
//! - LINE GRAMMAR: every output line either starts with two spaces
//!   (appendix continuation), matches
//!   `^(report|error|category|location|scope|attachment|trace_id|fingerprint|advice|action|hint|appendix)( |:)`,
//!   or matches `^(cause|original|attempt|failure) [0-9]+: ` — the
//!   one-fact-per-line contract: hostile values must not forge lines
//!   (this IS the newline-injection check),
//! - (feature `serde`) `render_report_json` parses as JSON and carries
//!   `"schema": 2`.
//!
//! NOTE: the nasty `attach_key` keys carry no raw `\n`/`\r` on purpose.
//! Keys are developer-chosen `&'static str` and `write_report` does NOT
//! escape them (only values pass through `Line`) — a newline in a key
//! would forge a line, but that is developer error at a compile-time
//! call site, not hostile runtime data, so it is out of scope here.

use std::error::Error;
use std::fmt;

use bolero::generator::TypeGenerator;
use fast_observe::exn::Fault;
#[cfg(feature = "serde")]
use fast_observe::render_report_json;
use fast_observe::{BoxError, Context, Placement, ResultExt, render_report};

/// Nasty string fragments biased toward line-forging payloads.
const FRAGS: &[&str] = &[
    "\naction: forged",
    "\nerror: forged",
    "\nfingerprint: deadbeef",
    "\r\n",
    "\r",
    "\"}",
    "{\"schema\": 99}",
    "\u{0}",
    "\u{7}",
    "\t",
    "  indented continuation forgery",
    "cause 99: forged",
    "☃ héllo wörld",
    "\u{2028}\u{2029}",
    "\\n literal backslash-n",
];

/// Static keys for `attach_key` (the API requires `&'static str`).
/// Includes the report's reserved keys on purpose — they exercise the
/// reserved-key filtering and the `scope:`/`trace_id:` line paths with
/// hostile values. No raw `\n`/`\r` (see the module docs).
const KEYS: &[&str] = &[
    "scope_path",
    "scope_elapsed_ms",
    "trace_id",
    "span_trail",
    "backtrace",
    "k",
    "weird key",
    "a:b",
    "\"}",
    "\u{0}null",
    "emoji-☃",
    "tab\there",
];

/// A plain leaf error with no source chain (like `fuzz_fault_tree`'s
/// `TreeError`) — keeps the cause-chain shape predictable.
#[derive(Debug)]
struct HostileError(String);

impl fmt::Display for HostileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for HostileError {}

/// One hostile fragment — from the bag, a long run, or raw data.
#[derive(Debug, Clone, TypeGenerator)]
enum Frag {
    /// A hand-picked nasty fragment (index into [`FRAGS`]).
    Nasty(u8),
    /// A 1024-byte run — exercises long-value paths.
    Long,
    /// Raw arbitrary string data.
    Raw(String),
}

/// A hostile string: concatenation of nasty fragments and raw data.
#[derive(Debug, Clone, TypeGenerator)]
struct Hostile(Vec<Frag>);

impl Hostile {
    fn build(&self) -> String {
        let mut out = String::new();
        for frag in &self.0 {
            match frag {
                Frag::Nasty(sel) => out.push_str(FRAGS[usize::from(*sel) % FRAGS.len()]),
                Frag::Long => out.push_str(&"x".repeat(1024)),
                Frag::Raw(s) => out.push_str(s),
            }
        }
        out
    }
}

/// One fault-building op, applied to a work stack of faults.
#[derive(Debug, Clone, TypeGenerator)]
enum ReportOp {
    /// Push `Fault::from(String)` — a `Fault<BoxError>` leaf.
    LeafStr(Hostile),
    /// Push `Fault::new(HostileError)` — a typed leaf.
    LeafTyped(Hostile),
    /// Push a fault raised from a failing `std::io::Error` through
    /// `ResultExt::context` — boxed root, one-level typed cause.
    Raise(Hostile),
    /// `set_context` on the stack top (variant 0 = `Context::None`).
    SetContext(u8, Hostile, u64),
    /// `attach` on the stack top.
    Attach(Hostile),
    /// `attach_key` on the stack top (key from [`KEYS`]).
    AttachKey(u8, Hostile),
    /// `attach_placed` on the stack top (placement from index).
    AttachPlaced(u8, Hostile),
}

#[derive(Debug, Clone, TypeGenerator)]
struct ReportCase {
    ops: Vec<ReportOp>,
}

/// One fault on the work stack (typed or boxed root).
enum Node {
    Typed(Fault<HostileError>),
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

/// Raise a failing `std::io::Error` through `ResultExt::context`.
fn raise_io(msg: String) -> Fault<BoxError> {
    let result: Result<(), std::io::Error> = Err(std::io::Error::other("fuzz io cause"));
    match result.context(msg) {
        Ok(()) => Fault::from("io error unexpectedly Ok".to_string()),
        Err(fault) => fault,
    }
}

/// Fixed section heads, valid when followed by a space or colon.
const HEADS: &[&str] = &[
    "report",
    "error",
    "category",
    "location",
    "scope",
    "attachment",
    "trace_id",
    "fingerprint",
    "advice",
    "action",
    "hint",
    "appendix",
];

/// The one-fact-per-line contract, as a predicate over output lines.
fn line_is_valid(line: &str) -> bool {
    // (a) Appendix continuation lines are indented by two spaces.
    if line.starts_with("  ") {
        return true;
    }
    // (b) Fixed section heads, followed by a space or colon.
    for head in HEADS {
        if let Some(rest) = line.strip_prefix(head)
            && (rest.starts_with(' ') || rest.starts_with(':'))
        {
            return true;
        }
    }
    // (c) Cause lines: `cause|original|attempt|failure N: …`.
    for word in ["cause", "original", "attempt", "failure"] {
        let Some(rest) = line.strip_prefix(word) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && rest[digits..].starts_with(": ") {
            return true;
        }
    }
    false
}

/// Assert every report invariant on one fault.
fn assert_report_invariants<E: Send + Sync + 'static>(fault: &Fault<E>) {
    let out = render_report(fault);
    assert!(!out.is_empty(), "report must be non-empty");
    let again = render_report(fault);
    assert_eq!(out, again, "report rendering must be deterministic");
    for line in out.lines() {
        assert!(
            line_is_valid(line),
            "line breaks the one-fact-per-line grammar: {line:?}\nfull report:\n{out}"
        );
    }
    #[cfg(feature = "serde")]
    {
        let json = render_report_json(fault);
        let parsed = serde_json::from_str::<serde_json::Value>(&json);
        assert!(
            parsed.is_ok(),
            "report JSON must parse: {:?}\n{json}",
            parsed.err()
        );
        if let Ok(value) = parsed {
            assert_eq!(value["schema"], 2, "report JSON schema must be 2");
        }
    }
}

fn apply_op(stack: &mut Vec<Node>, op: &ReportOp) {
    match op {
        ReportOp::LeafStr(h) => stack.push(Node::Boxed(Fault::from(h.build()))),
        ReportOp::LeafTyped(h) => stack.push(Node::Typed(Fault::new(HostileError(h.build())))),
        ReportOp::Raise(h) => stack.push(Node::Boxed(raise_io(h.build()))),
        ReportOp::SetContext(sel, h, n) => {
            if let Some(top) = stack.pop() {
                match context_from(*sel, &h.build(), *n) {
                    Some(ctx) => stack.push(top.set_context(ctx)),
                    None => stack.push(top.set_context(Context::None)),
                }
            }
        }
        ReportOp::Attach(h) => {
            if let Some(top) = stack.pop() {
                stack.push(top.attach(h.build()));
            }
        }
        ReportOp::AttachKey(sel, h) => {
            if let Some(top) = stack.pop() {
                let key = KEYS[usize::from(*sel) % KEYS.len()];
                stack.push(top.attach_key(key, h.build()));
            }
        }
        ReportOp::AttachPlaced(sel, h) => {
            if let Some(top) = stack.pop() {
                stack.push(top.attach_placed(h.build(), placement_from(*sel)));
            }
        }
    }
}

#[test]
fn fuzz_render_report_hostile_strings() {
    bolero::check!()
        .with_type::<ReportCase>()
        .for_each(|case: &ReportCase| {
            let mut stack: Vec<Node> = Vec::new();
            for op in &case.ops {
                apply_op(&mut stack, op);
            }
            // Always exercise the invariants at least once per case.
            if stack.is_empty() {
                stack.push(Node::Boxed(Fault::from(String::new())));
            }
            for node in &stack {
                match node {
                    Node::Typed(f) => assert_report_invariants(f),
                    Node::Boxed(f) => assert_report_invariants(f),
                }
            }
        });
}
