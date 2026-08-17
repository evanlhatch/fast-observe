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
//! Timing uses [`web_time::Instant`] (see `clock.rs`) — wasm-safe. Storage
//! is `Cell<Vec>` (take/set) — no `RefCell` borrow states, so instrumentation
//! can never panic on reentrancy.

use super::clock;
use std::cell::Cell;
use std::time::Duration;

thread_local! {
    static FINISHED: Cell<Vec<SpanRecord>> = Cell::new(Vec::new());
    static STACK: Cell<Vec<SpanRecord>> = Cell::new(Vec::new());
    static FRAME_BOUNDARIES: Cell<Vec<usize>> = Cell::new(Vec::new());
}

/// One recorded span — name, optional tag, start/end (ns since a process
/// origin), depth.
#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub name: &'static str,
    pub tag: Option<&'static str>,
    pub start_ns: u64,
    pub end_ns: u64,
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
    STACK.with(|c| {
        let mut s = c.take();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "span nesting depth is bounded well below u32::MAX in practice"
        )]
        let depth = s.len() as u32;
        s.push(SpanRecord {
            name,
            tag,
            start_ns,
            end_ns: start_ns,
            depth,
        });
        c.set(s);
    });
    InstantGuard
}

/// Guard — records the end timestamp on drop, moves span to `FINISHED`.
pub struct InstantGuard;

/// Construct a no-op `InstantGuard` (does nothing on drop).
#[must_use]
pub fn dummy() -> InstantGuard {
    InstantGuard
}

impl Drop for InstantGuard {
    fn drop(&mut self) {
        let end_ns = clock::now_ns();
        STACK.with(|c| {
            let mut s = c.take();
            let span = s.pop();
            c.set(s);
            if let Some(mut span) = span {
                span.end_ns = end_ns;
                FINISHED.with(|f| {
                    let mut v = f.take();
                    v.push(span);
                    f.set(v);
                });
            }
        });
    }
}

/// Mark a frame/tick boundary. Groups spans for per-tick breakdowns.
/// Automatically drains spans older than the current frame to prevent
/// unbounded memory growth in thread-local storage.
pub fn finish_frame() {
    // Read the finished count without disturbing the buffer.
    let count = FINISHED.with(|c| {
        let v = c.take();
        let n = v.len();
        c.set(v);
        n
    });

    let mut boundaries = FRAME_BOUNDARIES.with(Cell::take);
    boundaries.push(count);

    if boundaries.len() > 60 {
        let keep = boundaries.len() - 60;
        let cutoff = boundaries[keep];
        // Drain spans older than the oldest retained frame. The retained
        // boundary values are absolute indices into FINISHED — after the
        // drain shrinks the buffer by `cutoff`, shift them down to match.
        let drained = FINISHED.with(|f| {
            let mut v = f.take();
            let drained = if cutoff < v.len() { cutoff } else { 0 };
            v.drain(0..drained);
            f.set(v);
            drained
        });
        let mut retained = boundaries[keep..].to_vec();
        if drained > 0 {
            for b in &mut retained {
                *b -= drained;
            }
        }
        FRAME_BOUNDARIES.with(|b| b.set(retained));
    } else {
        FRAME_BOUNDARIES.with(|b| b.set(boundaries));
    }
}

/// Drain all finished spans.
pub fn drain() -> Vec<SpanRecord> {
    FINISHED.with(Cell::take)
}

/// Drain frame boundaries (for per-tick grouping).
#[allow(
    dead_code,
    reason = "public API reserved for per-tick grouping; not used inside the crate"
)]
pub fn drain_frames() -> Vec<usize> {
    FRAME_BOUNDARIES.with(Cell::take)
}

/// Clear everything.
#[allow(
    dead_code,
    reason = "test helper + public reset API; not used inside the crate"
)]
pub fn clear() {
    FINISHED.with(|c| {
        c.take();
    });
    FRAME_BOUNDARIES.with(|c| {
        c.take();
    });
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
