//! Integration tests: our instrument macros expand against `::fast_observe`
//! paths and drive the `instant` span accumulator.
//!
//! The span accumulator is thread-local, and each `#[test]` runs on its own
//! thread, so `drain_spans()` sees only the calling test's spans. The backend
//! mask is process-global, but both tests set it to `INSTANT` — same value,
//! no cross-test interference.
//!
//! Note: `#[instrument]` on an async fn is a compile error by design
//! (thread-bound guard across `.await`). No UI test — trybuild is not a
//! dependency.

use fast_observe::config::{Backends, config};

#[fast_observe_macros::instrument]
fn the_answer() -> u32 {
    42
}

#[fast_observe_macros::instrument(name = "custom.name")]
fn custom_named() -> u32 {
    7
}

#[test]
fn instrument_records_default_module_path_name() {
    config().set_backends(Backends::INSTANT);
    assert_eq!(the_answer(), 42);
    let spans = fast_observe::drain_spans();
    assert!(
        spans.iter().any(|s| s.name.ends_with("::the_answer")),
        "expected a span ending in ::the_answer, got: {spans:?}"
    );
}

#[test]
fn instrument_records_custom_name_verbatim() {
    config().set_backends(Backends::INSTANT);
    assert_eq!(custom_named(), 7);
    let spans = fast_observe::drain_spans();
    assert!(
        spans.iter().any(|s| s.name == "custom.name"),
        "expected a span named custom.name, got: {spans:?}"
    );
}

struct Counter;

#[fast_observe_macros::all_functions]
impl Counter {
    fn a(&self) -> u8 {
        1
    }

    #[fast_observe_macros::skip]
    fn b(&self) -> u8 {
        2
    }

    #[skip]
    fn c(&self) -> u8 {
        3
    }
}

/// `#[skip]` standalone is an identity attribute (strips itself):
#[fast_observe_macros::skip]
fn standalone_skip_compiles() -> u8 {
    9
}

#[test]
fn all_functions_instruments_unskipped_methods_only() {
    config().set_backends(Backends::INSTANT);
    let counter = Counter;
    assert_eq!(counter.a(), 1);
    assert_eq!(counter.b(), 2);
    assert_eq!(counter.c(), 3);
    assert_eq!(standalone_skip_compiles(), 9);

    let spans = fast_observe::drain_spans();
    assert!(
        spans.iter().any(|s| s.name.ends_with("::a")),
        "expected a span ending in ::a, got: {spans:?}"
    );
    assert!(
        !spans.iter().any(|s| s.name.ends_with("::b")),
        "#[fast_observe_macros::skip] method b must not be instrumented: {spans:?}"
    );
    assert!(
        !spans.iter().any(|s| s.name.ends_with("::c")),
        "bare #[skip] method c must not be instrumented: {spans:?}"
    );
}
