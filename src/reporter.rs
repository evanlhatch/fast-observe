//! Reporter composition for fastrace: sampling + fan-out.

use fastrace::collector::{Reporter, SpanRecord};

/// True when any span in the trace carries an `error` span event — the
/// event name the error-capture hook emits.
fn has_error_event(spans: &[SpanRecord]) -> bool {
    spans.iter().any(|span| {
        span.events
            .iter()
            .any(|event| event.name == crate::hook::ERROR_EVENT)
    })
}

/// Keeps 100% of traces containing an `error` span event; samples other
/// traces 1-in-`sample_rate` by trace id. Wraps any Reporter.
///
/// The sampling decision is derived from the trace id
/// (`trace_id % sample_rate == 0` keeps the trace), so it is deterministic
/// per trace, consistent across every service on the trace's path, and has
/// no process-local counter to phase-align across the fleet.
pub struct SamplingReporter<R: Reporter> {
    inner: R,
    sample_rate: u64,
}

impl<R: Reporter> SamplingReporter<R> {
    /// Wrap `inner`. `sample_rate` of 0 or 1 keeps everything.
    #[must_use]
    pub fn new(inner: R, sample_rate: u64) -> Self {
        Self { inner, sample_rate }
    }
}

impl<R: Reporter> Reporter for SamplingReporter<R> {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        // All spans of a trace share one trace id (a fastrace TraceId is a
        // u128 newtype; `.0` is the integer). The `sample_rate <= 1` and
        // error arms short-circuit before the modulo, so `sample_rate` is
        // never 0 at the division. An empty batch maps to id 0 (kept — the
        // inner reporter sees an empty vec).
        let trace_id = spans.first().map_or(0, |span| span.trace_id.0);
        let keep = self.sample_rate <= 1
            || has_error_event(&spans)
            || trace_id.is_multiple_of(u128::from(self.sample_rate));
        if keep {
            self.inner.report(spans);
        }
    }
}

/// Fan-out: report every trace to several reporters (e.g. console + `OTel`).
#[derive(Default)]
pub struct MultiReporter {
    reporters: Vec<Box<dyn Reporter>>,
}

impl MultiReporter {
    /// Empty fan-out; add targets with [`MultiReporter::push`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a reporter; every reported trace is cloned to each target.
    pub fn push(&mut self, reporter: impl Reporter + 'static) {
        self.reporters.push(Box::new(reporter));
    }
}

impl Reporter for MultiReporter {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        let n = self.reporters.len();
        // Single sink: move the vec straight through — no clone.
        if n <= 1 {
            if let Some(reporter) = self.reporters.first_mut() {
                reporter.report(spans);
            }
            return;
        }
        // Multi sink: clone for all but the last target; the last gets the
        // vec moved straight through (no clone).
        let Some((last, rest)) = self.reporters.split_last_mut() else {
            return;
        };
        for reporter in rest {
            reporter.report(spans.clone());
        }
        last.report(spans);
    }
}

#[cfg(test)]
mod tests {
    use fastrace::collector::{EventRecord, TestReporter, TraceId};

    use super::*;

    fn clean_trace(id: u128) -> Vec<SpanRecord> {
        vec![SpanRecord {
            trace_id: TraceId(id),
            ..SpanRecord::default()
        }]
    }

    fn error_trace(id: u128) -> Vec<SpanRecord> {
        vec![SpanRecord {
            trace_id: TraceId(id),
            events: vec![EventRecord {
                name: "error".into(),
                ..EventRecord::default()
            }],
            ..SpanRecord::default()
        }]
    }

    #[test]
    fn error_traces_always_kept() {
        let (mock, spans) = TestReporter::new();
        let mut reporter = SamplingReporter::new(mock, 3);
        // Ids 1 and 2 are not multiples of 3 — a clean trace with either id
        // is dropped — but the error event keeps them.
        reporter.report(error_trace(1));
        reporter.report(error_trace(2));
        reporter.report(clean_trace(1));
        assert_eq!(spans.lock().len(), 2);
    }

    #[test]
    fn clean_traces_sampled_by_trace_id() {
        let (mock, spans) = TestReporter::new();
        let mut reporter = SamplingReporter::new(mock, 3);
        // Trace ids 1..=9 at rate 3: exactly the multiples of 3 are kept.
        for id in 1..=9 {
            reporter.report(clean_trace(id));
        }
        let kept: Vec<u128> = spans.lock().iter().map(|span| span.trace_id.0).collect();
        assert_eq!(kept, [3, 6, 9]);
    }

    #[test]
    fn sampling_decision_is_stable_per_trace() {
        let (mock, spans) = TestReporter::new();
        let mut reporter = SamplingReporter::new(mock, 3);
        // Re-reporting a trace id repeats its decision: id 3 kept every
        // time, id 4 dropped every time — no counter phase involved.
        reporter.report(clean_trace(3));
        reporter.report(clean_trace(4));
        reporter.report(clean_trace(3));
        reporter.report(clean_trace(4));
        let kept: Vec<u128> = spans.lock().iter().map(|span| span.trace_id.0).collect();
        assert_eq!(kept, [3, 3]);
    }

    #[test]
    fn sample_rate_zero_or_one_keeps_everything() {
        for rate in [0, 1] {
            let (mock, spans) = TestReporter::new();
            let mut reporter = SamplingReporter::new(mock, rate);
            for id in 0..5 {
                reporter.report(clean_trace(id));
            }
            assert_eq!(spans.lock().len(), 5);
        }
    }

    #[test]
    fn multi_reporter_fans_out_to_all() {
        let (a, a_spans) = TestReporter::new();
        let (b, b_spans) = TestReporter::new();
        let mut multi = MultiReporter::new();
        multi.push(a);
        multi.push(b);
        multi.report(error_trace(1));
        assert_eq!(a_spans.lock().len(), 1);
        assert_eq!(b_spans.lock().len(), 1);
    }

    #[test]
    fn multi_reporter_single_sink_moves_without_clone() {
        let (a, a_spans) = TestReporter::new();
        let mut multi = MultiReporter::new();
        multi.push(a);
        multi.report(clean_trace(1));
        assert_eq!(a_spans.lock().len(), 1);
    }
}
