//! Instant backend — deterministic per-phase timing via thread-local ticks.
//!
//! Coexists with fastrace/tracy/puffin-style backends (unlike upstream
//! `profiling` backends which are mutually exclusive).
//!
//! `scope!("dispatch")` records to the thread-local accumulator when the
//! `instant` (or `web`) backend is selected. `drain()` returns all finished
//! spans. `print_breakdown()` (in `crate::breakdown`) derives the per-phase
//! nanosecond breakdown from the span tree.
//!
//! Timing uses `web_time::Instant` (see `clock.rs`) — wasm-safe. Storage
//! is `Cell<Vec>` (take/set) — no `RefCell` borrow states, so instrumentation
//! can never panic on reentrancy.

use super::clock;
use std::cell::Cell;
use std::marker::PhantomData;
use std::time::Duration;

thread_local! {
    static FINISHED: Cell<Vec<SpanRecord>> = Cell::new(Vec::new());
    static STACK: Cell<Vec<SpanRecord>> = Cell::new(Vec::new());
    static FRAME_BOUNDARIES: Cell<Vec<usize>> = Cell::new(Vec::new());
    /// Absolute count of spans ever removed from `FINISHED`. Frame
    /// boundaries are stored absolute; subtract this to rebase.
    static DRAIN_BASE: Cell<usize> = const { Cell::new(0) };
}

/// Take/mutate/set dance for a thread-local `Cell<Vec<T>>` — reentrancy
/// panic-safe: the cell is empty while `f` runs, so nested access sees an
/// empty vec instead of a `RefCell`-style borrow panic.
///
/// Uses `try_with` and silently skips when the TLS slot is already
/// destroyed: `InstantGuard::drop` can run during thread teardown, and
/// `LocalKey::with` would panic there — a panic inside `Drop` during
/// unwinding aborts the process. A skipped finalize loses at most the
/// exiting thread's tail spans — acceptable.
fn with_tl<T>(cell: &'static std::thread::LocalKey<Cell<Vec<T>>>, f: impl FnOnce(&mut Vec<T>)) {
    let _ = cell.try_with(|c| {
        let mut v = c.take();
        f(&mut v);
        c.set(v);
    });
}

/// One recorded span — name, optional tag, start/end (ns since a process
/// origin), depth.
#[derive(Debug, Clone)]
pub struct SpanRecord {
    /// The scope name passed to `enter`/`scope!`.
    pub name: &'static str,
    /// Optional tag — a secondary label for the span.
    pub tag: Option<&'static str>,
    /// Start timestamp, in ns since the clock's process origin
    /// ([`crate::profiling::clock::now_ns`]).
    pub start_ns: u64,
    /// End timestamp, in ns since the same origin. Set on guard drop;
    /// equals `start_ns` for zero-length spans.
    pub end_ns: u64,
    /// Nesting depth — 0 for spans entered at the top of the stack.
    pub depth: u32,
}

impl SpanRecord {
    /// Wall duration, in real nanoseconds.
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_nanos(self.end_ns.saturating_sub(self.start_ns))
    }
}

/// Enter a scope — push onto the stack. Returns a guard that records on drop.
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> InstantGuard {
    let start_ns = clock::now_ns();
    let mut depth = 0;
    with_tl(&STACK, |s| {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "span nesting depth is bounded well below u32::MAX in practice"
        )]
        let d = s.len() as u32;
        depth = d;
        s.push(SpanRecord {
            name,
            tag,
            start_ns,
            end_ns: start_ns,
            depth: d,
        });
    });
    InstantGuard {
        depth: Some(depth),
        _not_send: PhantomData,
    }
}

/// Guard — records the end timestamp on drop, moves span to `FINISHED`.
///
/// Depth-tagged: the guard remembers its span's stack index at push time.
/// Drop finalizes every span above that index first (forgotten nested
/// guards — e.g. via [`std::mem::forget`] — get the same end timestamp,
/// best-effort), then its own span. Out-of-order or duplicate drops whose
/// span is already gone are no-ops, so a misbehaving guard can never steal
/// another scope's span. Guards from [`dummy()`] (`depth: None`) never pop.
pub struct InstantGuard {
    depth: Option<u32>,
    // Thread-bound marker: `depth` indexes the CREATING thread's span
    // stack — a cross-thread drop would corrupt the receiving thread's
    // stack, so the guard must never be `Send`/`Sync`.
    _not_send: PhantomData<*const ()>,
}

/// Construct a no-op `InstantGuard` (does nothing on drop).
#[must_use]
pub fn dummy() -> InstantGuard {
    InstantGuard {
        depth: None,
        _not_send: PhantomData,
    }
}

impl Drop for InstantGuard {
    fn drop(&mut self) {
        let Some(depth) = self.depth else {
            return; // dummy guard — no-op.
        };
        let end_ns = clock::now_ns();
        let mut done: Vec<SpanRecord> = Vec::new();
        with_tl(&STACK, |s| {
            // Finalize forgotten nested spans above ours, then ours. If our
            // span is already gone (len <= depth) nothing happens.
            while s.len() > depth as usize {
                let Some(mut span) = s.pop() else { break };
                span.end_ns = end_ns;
                done.push(span);
            }
        });
        with_tl(&FINISHED, |f| f.append(&mut done));
    }
}

/// Number of frame boundaries retained by [`finish_frame`] before the
/// oldest frame (and the spans it covers) is evicted — caps thread-local
/// memory growth.
const RETAINED_FRAMES: usize = 60;

/// Mark a frame/tick boundary. Groups spans for per-tick breakdowns.
/// Automatically drains spans older than the current frame to prevent
/// unbounded memory growth in thread-local storage.
pub fn finish_frame() {
    let base = DRAIN_BASE.with(Cell::get);
    let mut count = 0;
    with_tl(&FINISHED, |v| count = v.len());
    let abs = base + count;

    with_tl(&FRAME_BOUNDARIES, |boundaries| {
        boundaries.push(abs);
        if boundaries.len() > RETAINED_FRAMES {
            let keep = boundaries.len() - RETAINED_FRAMES;
            let cutoff = boundaries[keep];
            // Drain spans older than the oldest retained frame. Boundaries
            // are absolute — no per-element index math, just bump the base.
            let n = cutoff.saturating_sub(base);
            if n > 0 {
                with_tl(&FINISHED, |v| {
                    v.drain(0..n.min(v.len()));
                });
            }
            DRAIN_BASE.with(|b| b.set(b.get().max(cutoff)));
            boundaries.drain(0..keep);
        }
    });
}

/// Drain all finished spans. Bumps `DRAIN_BASE` so absolute frame
/// boundaries stay interpretable (stale ones are filtered on rebase).
pub fn drain() -> Vec<SpanRecord> {
    let v = FINISHED.with(Cell::take);
    DRAIN_BASE.with(|b| b.set(b.get() + v.len()));
    v
}

/// The most recent `n` finished spans WITHOUT draining (oldest→newest).
///
/// Snapshot for error-time breadcrumbs; the accumulator is undisturbed
/// (unlike [`drain`], which also bumps `DRAIN_BASE`). Note the
/// `DRAIN_BASE`/eviction interplay: `FINISHED` holds only the retained
/// window — [`finish_frame`] drains spans older than the oldest retained
/// frame — so a peek can only ever read what is currently buffered, never
/// already-evicted spans. Order is finish order (innermost scopes first),
/// i.e. the vec tail IS the most recent span.
#[must_use]
pub fn peek_recent(n: usize) -> Vec<SpanRecord> {
    let mut out = Vec::new();
    with_tl(&FINISHED, |v| {
        let start = v.len().saturating_sub(n);
        out.extend_from_slice(&v[start..]);
    });
    out
}

/// Drain frame boundaries (for per-tick grouping), rebased relative to the
/// current `FINISHED` buffer; stale boundaries (< `DRAIN_BASE`) dropped.
pub fn drain_frames() -> Vec<usize> {
    let base = DRAIN_BASE.with(Cell::get);
    let boundaries = FRAME_BOUNDARIES.with(Cell::take);
    boundaries
        .into_iter()
        .filter(|b| *b >= base)
        .map(|b| b - base)
        .collect()
}

/// Group spans by name, preserving each span's duration per name.
///
/// Shared by [`crate::breakdown::print_breakdown`] and
/// [`crate::bench::aggregate`](crate::bench) — the one canonical
/// group-by-name pass, so the two consumers cannot drift (they format the
/// same groups differently: breakdown colors per-call averages, bench sums
/// to totals).
pub(crate) fn group_by_name(
    spans: &[SpanRecord],
) -> std::collections::BTreeMap<&'static str, Vec<Duration>> {
    let mut groups: std::collections::BTreeMap<&'static str, Vec<Duration>> =
        std::collections::BTreeMap::new();
    for s in spans {
        groups.entry(s.name).or_default().push(s.duration());
    }
    groups
}

/// Clear everything, including the drain base.
pub fn clear() {
    FINISHED.with(|c| {
        c.take();
    });
    FRAME_BOUNDARIES.with(|c| {
        c.take();
    });
    DRAIN_BASE.with(|b| b.set(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_recorded_on_guard_drop() {
        clear();
        {
            let _g = enter("outer", None);
            {
                let _g2 = enter("inner", Some("tag"));
            } // inner finishes first
        }
        let spans = drain();
        assert_eq!(spans.len(), 2);
        // Innermost scope finishes first (stack order).
        assert_eq!(spans[0].name, "inner");
        assert_eq!(spans[0].tag, Some("tag"));
        assert_eq!(spans[0].depth, 1);
        assert_eq!(spans[1].name, "outer");
        assert_eq!(spans[1].depth, 0);
        // end >= start for every recorded span.
        for s in &spans {
            assert!(s.end_ns >= s.start_ns, "end must not precede start");
        }
    }

    #[test]
    fn drain_is_destructive_roundtrip() {
        clear();
        drop(enter("a", None));
        assert_eq!(drain().len(), 1);
        // Second drain — accumulator empty, no double-count.
        assert!(drain().is_empty());
    }

    #[test]
    fn finish_frame_marks_boundaries() {
        clear();
        drop(enter("a", None));
        drop(enter("b", None));
        finish_frame();
        drop(enter("c", None));
        finish_frame();
        let boundaries = drain_frames();
        assert_eq!(boundaries, vec![2, 3]);
        let _ = drain();
    }

    #[test]
    fn dummy_on_empty_stack_records_nothing() {
        clear();
        drop(dummy());
        assert!(drain().is_empty());
    }

    #[test]
    fn dummy_guard_never_pops() {
        clear();
        let real = enter("real", None);
        drop(dummy()); // must not pop the real span
        drop(real);
        let spans = drain();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "real");
    }

    #[test]
    fn forgotten_child_is_finalized_not_corrupting() {
        clear();
        let a = enter("a", None);
        let b = enter("b", None);
        std::mem::forget(b); // leaked guard — b never dropped
        drop(a); // must finalize b (best-effort) then a
        let spans = drain();
        let names: Vec<_> = spans.iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["b", "a"]);
        for s in &spans {
            assert!(s.end_ns >= s.start_ns, "end must not precede start");
        }
        // Stack is clean — a later scope records at depth 0.
        drop(enter("c", None));
        let spans = drain();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "c");
        assert_eq!(spans[0].depth, 0);
    }

    #[test]
    fn boundaries_eviction_caps_at_60() {
        clear();
        for _ in 0..70 {
            drop(enter("x", None));
            finish_frame();
        }
        assert_eq!(drain_frames().len(), RETAINED_FRAMES);
        // Invariant implemented: eviction keeps the spans covered by the
        // RETAINED_FRAMES retained boundaries — only spans older than the
        // oldest retained boundary are drained. Each frame here produced
        // exactly 1 span, so 70 frames - (RETAINED_FRAMES + 1) retained
        // positions = 59 spans remain. FINISHED is NOT empty; it holds the
        // spans of the retained window.
        assert_eq!(drain().len(), 59);
    }

    #[test]
    fn duration_reflects_elapsed() {
        clear();
        {
            let _g = enter("slow", None);
            std::thread::sleep(Duration::from_millis(2));
        }
        let spans = drain();
        assert_eq!(spans.len(), 1);
        let d = spans[0].duration();
        assert!(
            d >= Duration::from_millis(1),
            "2ms span should measure ≥1ms, got {d:?}"
        );
    }
}
