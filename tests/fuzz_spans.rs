//! Fuzz target: instant span-stack model test (feature `instant`).
#![cfg(feature = "instant")]
//!
//! Drives `fast_observe::profiling::instant` (`enter`, `dummy`,
//! `finish_frame`, `drain`, `drain_frames`, `clear`, `peek_recent`) with a
//! bolero-generated op sequence, checked against an exact model: the
//! expected OPEN stack (names), the expected FINISHED buffer (finish
//! order), the absolute drain base (mirrors `DRAIN_BASE`), and absolute
//! frame boundaries including `finish_frame`'s 60-frame eviction window.
//!
//! Guards are stored with their RECORDED DEPTH (stack length at enter
//! time): `DropIdx` applies the guard's finalize-above-depth semantics to
//! the model, `Forget` (`mem::forget`) removes the guard but leaves its
//! span open — a lower guard's later drop finalizes it.
//!
//! Invariants per case:
//! - no panic anywhere (implicit — a panic fails the bolero case),
//! - `drain()` names == the model's finished buffer EXACTLY,
//! - `drain_frames()` == the model's rebased boundaries (hence monotonic
//!   non-decreasing),
//! - `peek_recent(n)` == the last n finished names, accumulator
//!   untouched,
//! - every drained span has `end_ns >= start_ns`.
//!
//! State hygiene: bolero runs many cases in one process on one thread, so
//! each case starts and ends with `clear()`. NOTE: `clear()` does not
//! reset the span STACK — a forgotten guard below every live guard leaks
//! its open span for the rest of the thread's life. Harmless here:
//! leaked spans never reach FINISHED, and they shift every recorded depth
//! and the stack length uniformly, so `while len > depth` decisions —
//! and therefore the model — stay exact.

use bolero::generator::TypeGenerator;
use fast_observe::profiling::instant::{
    InstantGuard, clear, drain, drain_frames, dummy, enter, finish_frame, peek_recent,
};

/// Span names — a small static bag (`enter` takes `&'static str`).
const NAMES: &[&str] = &["alpha", "beta", "gamma", "delta"];

/// Mirror of the crate-private `RETAINED_FRAMES` eviction window.
const RETAINED_FRAMES: usize = 60;

/// One span-stack op.
#[derive(Debug, Clone, TypeGenerator)]
enum SpanOp {
    /// `enter` a span (name + tag from selector bits).
    Enter(u8),
    /// Drop a live guard (selector mod live count) — finalizes its span
    /// and every span above its recorded depth, deepest first.
    DropIdx(u8),
    /// `mem::forget` a live guard — the span stays open until a lower
    /// guard's drop finalizes it (or the thread ends).
    Forget(u8),
    /// Drop a `dummy()` guard — must never pop anything.
    Dummy,
    /// `finish_frame()` — pushes a boundary, maybe evicts old spans.
    FinishFrame,
    /// `drain()` — must equal the model's finished buffer exactly.
    Drain,
    /// `drain_frames()` — must equal the model's rebased boundaries.
    DrainFrames,
    /// `peek_recent(n)` — must equal the last n finished names.
    Peek(u8),
}

#[derive(Debug, Clone, TypeGenerator)]
struct SpanCase {
    ops: Vec<SpanOp>,
}

/// The exact expected thread-local state.
#[derive(Default)]
struct Model {
    /// Expected OPEN stack (names), bottom first.
    open: Vec<&'static str>,
    /// Expected FINISHED buffer, finish order (oldest first).
    finished: Vec<&'static str>,
    /// Absolute count of spans ever removed from the finished buffer
    /// (mirrors `DRAIN_BASE`).
    base: usize,
    /// Frame boundaries, absolute (mirrors `FRAME_BOUNDARIES`).
    boundaries: Vec<usize>,
}

impl Model {
    /// Mirror `finish_frame`: push the absolute boundary, then evict the
    /// spans older than the oldest retained frame past the window.
    fn finish_frame(&mut self) {
        let abs = self.base + self.finished.len();
        self.boundaries.push(abs);
        if self.boundaries.len() > RETAINED_FRAMES {
            let keep = self.boundaries.len() - RETAINED_FRAMES;
            let cutoff = self.boundaries[keep];
            let n = cutoff.saturating_sub(self.base);
            if n > 0 {
                self.finished.drain(0..n.min(self.finished.len()));
            }
            self.base = self.base.max(cutoff);
            self.boundaries.drain(0..keep);
        }
    }

    /// Mirror `drain_frames`: drop stale boundaries, rebase, consume.
    fn drain_frames(&mut self) -> Vec<usize> {
        let rebased: Vec<usize> = self
            .boundaries
            .iter()
            .filter(|b| **b >= self.base)
            .map(|b| b - self.base)
            .collect();
        self.boundaries.clear();
        rebased
    }
}

/// `sel` mod `len`, when `len` is non-zero.
fn index(sel: u8, len: usize) -> Option<usize> {
    (len > 0).then(|| usize::from(sel) % len)
}

fn run_case(ops: &[SpanOp]) {
    clear();
    let mut model = Model::default();
    // Live guards with their recorded depth (stack length at enter time).
    // After a `Forget` the vec index no longer matches the stack — drops
    // must go BY RECORDED DEPTH, never by vec index.
    let mut guards: Vec<(InstantGuard, usize)> = Vec::new();

    for op in ops {
        match *op {
            SpanOp::Enter(sel) => {
                let name = NAMES[usize::from(sel) % NAMES.len()];
                let tag = if sel & 0x10 == 0 { None } else { Some("tag") };
                let depth = model.open.len();
                guards.push((enter(name, tag), depth));
                model.open.push(name);
            }
            SpanOp::DropIdx(sel) => {
                if let Some(idx) = index(sel, guards.len()) {
                    let (guard, depth) = guards.remove(idx);
                    drop(guard);
                    // Finalize-above-depth: every span above the guard's
                    // recorded depth, then the guard's own span.
                    while model.open.len() > depth {
                        let Some(name) = model.open.pop() else {
                            break;
                        };
                        model.finished.push(name);
                    }
                }
            }
            SpanOp::Forget(sel) => {
                if let Some(idx) = index(sel, guards.len()) {
                    let (guard, _) = guards.remove(idx);
                    std::mem::forget(guard);
                }
            }
            SpanOp::Dummy => drop(dummy()),
            SpanOp::FinishFrame => {
                finish_frame();
                model.finish_frame();
            }
            SpanOp::Drain => {
                let drained = drain();
                let names: Vec<&str> = drained.iter().map(|s| s.name).collect();
                assert_eq!(
                    names, model.finished,
                    "drain must return exactly the modeled finished spans"
                );
                for span in &drained {
                    assert!(
                        span.end_ns >= span.start_ns,
                        "span end must not precede start: {span:?}"
                    );
                }
                model.base += model.finished.len();
                model.finished.clear();
            }
            SpanOp::DrainFrames => {
                let frames = drain_frames();
                let expected = model.drain_frames();
                assert_eq!(frames, expected, "frame boundaries must match the model");
                assert!(
                    frames.windows(2).all(|w| w[0] <= w[1]),
                    "frame boundaries must be non-decreasing: {frames:?}"
                );
            }
            SpanOp::Peek(n) => {
                let n = usize::from(n);
                let peeked = peek_recent(n);
                let names: Vec<&str> = peeked.iter().map(|s| s.name).collect();
                let start = model.finished.len().saturating_sub(n);
                assert_eq!(
                    names,
                    model.finished[start..],
                    "peek_recent({n}) must return the last finished names"
                );
            }
        }
    }

    // Drop all remaining guards BEFORE clear(): a guard dropped after
    // clear() would finalize into the freshly-wiped FINISHED buffer and
    // leak into the next case.
    for (guard, _) in guards {
        drop(guard);
    }
    clear();
}

#[test]
fn fuzz_span_stack_model() {
    bolero::check!()
        .with_type::<SpanCase>()
        .for_each(|case: &SpanCase| run_case(&case.ops));
}
