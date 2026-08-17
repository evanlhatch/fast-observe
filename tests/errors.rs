//! `define_errors!` + global `ERROR_REGISTRY`. This binary links ONLY
//! fast-observe, so the registry contents here are exactly what this test
//! file registers — cross-crate visibility is proven by the consuming
//! workspace's own registry tests.

use fast_observe::{ErrorCategory, define_errors, error_registry, lookup_error};

/// Test error enum — exercises the macro outside any app crate.
#[derive(Debug)]
pub enum TestError {
    Boom(Boom),
    Fizzle(Fizzle),
}

define_errors! {
    enum TestError {
        (Boom, "E991", Invariant, "boom", "boom: {detail}", {
            detail: String,
        });
        (Fizzle, "E992", Transient, "fizzle", "fizzle", {});
    }
}

impl std::error::Error for TestError {}

#[test]
fn lookup_finds_registered_variants() {
    let entry = lookup_error("E991").expect("E991 registered");
    assert_eq!(entry.name, "Boom");
    assert_eq!(entry.category, ErrorCategory::Invariant);
    assert_eq!(entry.display, "boom");
    assert!(lookup_error("E999").is_none());
}

#[test]
fn generated_api_code_category_display_from() {
    let err = TestError::from(Boom {
        detail: "x".to_string(),
    });
    assert_eq!(err.code(), "E991");
    assert_eq!(err.category(), ErrorCategory::Invariant);
    assert_eq!(err.to_string(), "boom: x");

    // Fieldless variant.
    let fizzle = TestError::from(Fizzle {});
    assert_eq!(fizzle.code(), "E992");
    assert_eq!(fizzle.category(), ErrorCategory::Transient);
    assert_eq!(fizzle.to_string(), "fizzle");
}

#[test]
fn registry_iterates_all_local_entries() {
    let codes: std::collections::HashSet<&'static str> = error_registry().map(|e| e.code).collect();
    assert!(codes.contains("E991"));
    assert!(codes.contains("E992"));
}
