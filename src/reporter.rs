//! Reporter composition for fastrace: sampling + fan-out.

use std::sync::atomic::{AtomicU64, Ordering};

use fastrace::collector::{Reporter, SpanRecord};

/// True when any span in the trace carries an `error` span event — the
/// event name the error-capture hook emits.
fn has_error_event(spans: &[SpanRecord]) -> bool {
    spans
        .iter()
        .any(|span| span.events.iter().any(|event| event.name == "error"))
}

/// Keeps 100% of traces containing an `error` span event; samples other
/// traces 1-in-`sample_rate` (deterministic counter). Wraps any Reporter.
pub struct SamplingReporter<R: Reporter> {
    inner: R,
    sample_rate: u64,
    counter: AtomicU64,
}

impl<R: Reporter> SamplingReporter<R> {
    /// Wrap `inner`. `sample_rate` of 0 or 1 keeps everything.
    #[must_use]
    pub fn new(inner: R, sample_rate: u64) -> Self {
        Self {
            inner,
            sample_rate,
            counter: AtomicU64::new(0),
        }
    }
}

impl<R: Reporter> Reporter for SamplingReporter<R> {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        // Error traces never touch the counter, so a burst of errors cannot
        // shift the sampling phase of clean traces.
        let keep = self.sample_rate <= 1
            || has_error_event(&spans)
            || self.counter.fetch_add(1, Ordering::Relaxed) % self.sample_rate == 0;
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
        for reporter in &mut self.reporters {
            reporter.report(spans.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use fastrace::collector::EventRecord;

    use super::*;

    /// Captures every reported trace for inspection.
    #[derive(Clone, Default)]
    struct MockReporter {
        traces: Arc<Mutex<Vec<Vec<SpanRecord>>>>,
    }

    impl MockReporter {
        fn count(&self) -> usize {
            self.traces
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len()
        }
    }

    impl Reporter for MockReporter {
        fn report(&mut self, spans: Vec<SpanRecord>) {
            self.traces
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(spans);
        }
    }

    fn clean_trace() -> Vec<SpanRecord> {
        vec![SpanRecord::default()]
    }

    fn error_trace() -> Vec<SpanRecord> {
        vec![SpanRecord {
            events: vec![EventRecord {
                name: "error".into(),
                ..EventRecord::default()
            }],
            ..SpanRecord::default()
        }]
    }

    #[test]
    fn error_traces_always_kept() {
        let mock = MockReporter::default();
        let mut reporter = SamplingReporter::new(mock.clone(), 3);
        // Misalign the counter, then prove error traces still pass.
        reporter.report(clean_trace());
        reporter.report(error_trace());
        reporter.report(error_trace());
        assert_eq!(mock.count(), 3);
    }

    #[test]
    fn clean_traces_sampled_one_in_three() {
        let mock = MockReporter::default();
        let mut reporter = SamplingReporter::new(mock.clone(), 3);
        for _ in 0..9 {
            reporter.report(clean_trace());
        }
        assert_eq!(mock.count(), 3);
    }

    #[test]
    fn sample_rate_zero_or_one_keeps_everything() {
        for rate in [0, 1] {
            let mock = MockReporter::default();
            let mut reporter = SamplingReporter::new(mock.clone(), rate);
            for _ in 0..5 {
                reporter.report(clean_trace());
            }
            assert_eq!(mock.count(), 5);
        }
    }

    #[test]
    fn multi_reporter_fans_out_to_all() {
        let a = MockReporter::default();
        let b = MockReporter::default();
        let mut multi = MultiReporter::new();
        multi.push(a.clone());
        multi.push(b.clone());
        multi.report(error_trace());
        assert_eq!(a.count(), 1);
        assert_eq!(b.count(), 1);
    }
}
