//! Compile-fail UI tests for `error!` (trybuild). Each `ui/<name>.rs` case
//! asserts the exact `compile_error!` text emitted by
//! `src/error_macro.rs`; the `.stderr` files are regenerated with
//! `TRYBUILD=overwrite cargo test -p fast-observe-macros`.
//!
//! Nightly: the generated `Error::provide` impls need
//! `error_generic_member_access` in the CONSUMER crate — each ui case
//! enables the feature itself.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/code_without_category.rs");
    t.compile_fail("tests/ui/category_without_code.rs");
    t.compile_fail("tests/ui/missing_error_attr.rs");
    t.compile_fail("tests/ui/duplicate_codes.rs");
    t.compile_fail("tests/ui/invalid_code_format.rs");
    t.compile_fail("tests/ui/from_on_struct_variant.rs");
    t.pass("tests/ui/main_ok.rs");
    t.pass("tests/ui/main_alias.rs");
    t.pass("tests/ui/main_std_path.rs");
}
