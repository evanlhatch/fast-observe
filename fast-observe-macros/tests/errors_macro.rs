//! Integration tests for `fast_observe_macros::error!` — thiserror-compatible
//! attributes generating Display/Error/From impls, registry entries, and
//! nightly `Error::provide` code/category tags.
//!
//! No UI tests for the compile-error paths (trybuild is not a dependency,
//! same as the instrument tests).

#![feature(error_generic_member_access)]

use std::error::Error as _;
use std::io;

use fast_observe::errors::{CategoryTag, ErrorCode};
use fast_observe::{ErrorCategory, Fault};

fast_observe_macros::error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum EngineError {
        /// check the entity table for the missing id
        #[error("entity not found: {id}")]
        #[code = "E501", category = Content]
        EntityNotFound {
            /// The missing entity's id.
            id: u64,
        },

        /// an io failure bubbled up
        #[error("io: {0}")]
        #[code = "E502", category = Transient, advice = "retry the io operation"]
        #[from]
        Io(io::Error),

        /// pipeline layout failure wrapping another engine error
        #[error("pipeline layout: {source}")]
        #[code = "E503", category = Transient]
        PipelineLayout {
            /// The inner engine error.
            #[source]
            source: Box<EngineError>,
        },
    }
}

#[test]
fn display_for_variant_structs_and_enum() {
    assert_eq!(
        EntityNotFound { id: 42 }.to_string(),
        "entity not found: 42"
    );
    let e = EngineError::from(EntityNotFound { id: 42 });
    assert_eq!(e.to_string(), "entity not found: 42");

    let e = EngineError::from(io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed"));
    assert_eq!(e.to_string(), "io: pipe closed");
}

#[test]
fn coded_methods_and_trait() {
    let e = EngineError::from(EntityNotFound { id: 1 });
    assert_eq!(e.code(), "E501");
    assert_eq!(e.category(), ErrorCategory::Content);
    // Advice defaults to the first doc-comment line.
    assert_eq!(
        e.advice(),
        Some("check the entity table for the missing id")
    );
    assert_eq!(fast_observe::Coded::code(&e), "E501");

    let io = EngineError::from(io::Error::new(io::ErrorKind::BrokenPipe, "x"));
    assert_eq!(io.code(), "E502");
    assert_eq!(io.category(), ErrorCategory::Transient);
    // Explicit #[advice] wins over the doc line.
    assert_eq!(io.advice(), Some("retry the io operation"));
}

#[test]
fn from_variant_struct_into_enum_and_fault() {
    let e: EngineError = EntityNotFound { id: 9 }.into();
    assert!(matches!(e, EngineError::EntityNotFound(_)));

    let fault: Fault<EngineError> = EntityNotFound { id: 9 }.into();
    // Fault derefs to the typed enum.
    assert_eq!(fault.code(), "E501");

    // #[from] tuple variant: From<io::Error> for Enum. (From<io::Error> for
    // Fault<Enum> is NOT generated — orphan rule: Fault is not fundamental.)
    let e: EngineError = io::Error::new(io::ErrorKind::BrokenPipe, "pipe").into();
    assert!(matches!(e, EngineError::Io(_)));
    let fault: Fault<EngineError> =
        EngineError::from(io::Error::new(io::ErrorKind::BrokenPipe, "pipe")).into();
    assert_eq!(fault.code(), "E502");
}

#[test]
fn registry_lookup_hit() {
    match fast_observe::lookup_error("E501") {
        Some(entry) => {
            assert_eq!(entry.name, "EntityNotFound");
            assert_eq!(entry.category, ErrorCategory::Content);
            assert_eq!(entry.display, "entity not found: {id}");
            assert_eq!(
                entry.advice,
                Some("check the entity table for the missing id")
            );
            assert!(
                entry.module.ends_with("errors_macro"),
                "unexpected module: {}",
                entry.module
            );
        }
        None => unreachable!("E501 not registered in ERROR_REGISTRY"),
    }
}

#[test]
fn provide_roundtrip_through_fault_frame_error() {
    let fault: Fault<EngineError> = EntityNotFound { id: 3 }.into();
    let err = fault.frame().error();
    assert_eq!(
        core::error::request_value::<ErrorCode>(err),
        Some(ErrorCode("E501"))
    );
    assert_eq!(
        core::error::request_value::<CategoryTag>(err),
        Some(CategoryTag(ErrorCategory::Content))
    );
}

#[test]
fn from_tuple_variant_wires_source_chain() {
    let leaf = io::Error::new(io::ErrorKind::NotFound, "leaf gone");
    let mid = io::Error::new(io::ErrorKind::PermissionDenied, leaf);
    let e = EngineError::from(mid);
    match e.source() {
        // The enum's source() is the wrapped io error (which displays its
        // inner message). Deeper walking is std io::Error behavior
        // (its source() delegates past the payload).
        Some(src) => assert_eq!(src.to_string(), "leaf gone"),
        None => unreachable!("#[from] must wire source() to the inner error"),
    }
}

/// `Err(VariantStruct { .. })?` in a fn returning `fast_observe::Result`
/// relies on the generated `From<Variant> for Fault<Enum>`.
fn propagate_variant(id: u64) -> fast_observe::Result<(), EngineError> {
    if id == 0 {
        let err: Result<(), EntityNotFound> = Err(EntityNotFound { id });
        err?;
    }
    if id == 1 {
        // #[from] variant through `?`: io::Error → EngineError → Fault<EngineError>.
        let err: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::BrokenPipe, "pipe"));
        err.map_err(EngineError::from)?;
    }
    Ok(())
}

#[test]
fn variant_struct_question_mark_makes_fault() {
    match propagate_variant(0) {
        Ok(()) => unreachable!("expected Err"),
        Err(fault) => assert_eq!(fault.code(), "E501"),
    }
    match propagate_variant(1) {
        Ok(()) => unreachable!("expected Err"),
        Err(fault) => assert_eq!(fault.code(), "E502"),
    }
    assert!(propagate_variant(2).is_ok());
}

#[test]
fn recursive_source_variant() {
    let leaf = EngineError::from(EntityNotFound { id: 7 });
    let mid = EngineError::from(PipelineLayout {
        source: Box::new(leaf),
    });
    let e = EngineError::from(PipelineLayout {
        source: Box::new(mid),
    });
    assert_eq!(
        e.to_string(),
        "pipeline layout: pipeline layout: entity not found: 7"
    );
    // Two-level chaining through the recursive #[source] field.
    match e.source() {
        Some(src) => {
            assert_eq!(src.to_string(), "pipeline layout: entity not found: 7");
            match src.source() {
                Some(leaf) => {
                    assert_eq!(leaf.to_string(), "entity not found: 7");
                    // The inner enum's provide() is visible through the chain.
                    assert_eq!(
                        core::error::request_value::<ErrorCode>(leaf),
                        Some(ErrorCode("E501"))
                    );
                }
                None => unreachable!("expected the chain to continue"),
            }
        }
        None => unreachable!("#[source] field must wire source()"),
    }
}

#[test]
fn entries_list_all_coded_variants() {
    assert_eq!(EngineError::ENTRIES.len(), 3);
    assert!(EngineError::ENTRIES.iter().any(|e| e.code == "E503"));
}

// ── Debug on the generated enum ─────────────────────────────────────────
//
// `impl Error` requires `Debug + Display`, and `Result::unwrap_err`
// requires `E: Debug` — the macro must guarantee the enum is `Debug`
// whether the user derives nothing, `Debug` themselves, or something else.

// No derive on the enum — the macro must add `#[derive(Debug)]`.
fast_observe_macros::error! {
    /// Derive-less enum — the macro adds `Debug`.
    pub enum PlainError {
        /// a plain failure
        #[error("plain failure: {id}")]
        Plain {
            /// The failing id.
            id: u64,
        },
    }
}

// A user derive that is NOT `Debug` — must still end up `Debug`-able.
// (Unit variant: the generated variant structs do not inherit enum-level
// derives, so a payload-carrying variant would not be `Clone`.)
fast_observe_macros::error! {
    /// Enum with a user `#[derive(Clone)]` — `Debug` merged alongside.
    #[derive(Clone)]
    pub enum CloneError {
        /// a cloneable failure
        #[error("clone failure")]
        Cloned,
    }
}

fn needs_debug<T: std::fmt::Debug>(_: &T) {}

#[test]
// `unwrap_err` on a literal `Err` is deliberate: that call carries the
// `E: Debug` bound being regression-tested (the README failure mode).
#[allow(clippy::unnecessary_literal_unwrap, clippy::unwrap_used)]
fn generated_enum_is_debug_without_user_derive() {
    let e = PlainError::from(Plain { id: 1 });
    needs_debug(&e);
    assert!(format!("{e:?}").contains("Plain"));

    // The exact README failure mode: `unwrap_err` needs `E: Debug`.
    // (Type annotation: the generated `From<Variant> for Fault<Enum>` and
    // the blanket `From<E> for Fault<E>` otherwise make E ambiguous.)
    let fault: Fault<PlainError> = Err::<(), _>(Fault::from(Plain { id: 2 })).unwrap_err();
    needs_debug(&fault);
    needs_debug(&*fault);
}

#[test]
#[allow(clippy::unnecessary_literal_unwrap, clippy::unwrap_used)]
fn user_debug_derive_not_duplicated() {
    // EngineError above carries an explicit `#[derive(Debug)]` — no
    // conflicting-derive error, and the enum is usable as `Debug`.
    let e = EngineError::from(EntityNotFound { id: 1 });
    needs_debug(&e);
    let fault: Fault<EngineError> =
        Err::<(), _>(Fault::from(EntityNotFound { id: 1 })).unwrap_err();
    needs_debug(&*fault);
}

#[test]
fn other_user_derives_preserved_and_debug_still_added() {
    let e = CloneError::Cloned;
    needs_debug(&e);
    // The user's `Clone` still applies.
    let cloned = e.clone();
    assert_eq!(cloned.to_string(), "clone failure");
}
