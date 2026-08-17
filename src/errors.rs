//! Crate-agnostic error machinery: the `define_errors!` macro,
//! `ErrorRegistryEntry`, and the global `ERROR_REGISTRY`.
//!
//! The registry is a `linkme` distributed slice — a `define_errors!`
//! invocation in ANY crate registers its variants here at link time, so your
//! app's doctor/CLI (e.g. `myapp doctor <code>`) looks up errors
//! workspace-wide. This crate stays a clean leaf: no app-specific types.

use std::fmt::Write as _;

use crate::ErrorCategory;

/// An error with a stable registry code — implemented by `define_errors!`
/// (and later `error!`) output. Drives code-in-tree rendering, doctor, and
/// policy-derived advice.
pub trait Coded {
    /// Stable registry code, e.g. `"E100"`.
    fn code(&self) -> &'static str;
    /// Error category — drives [`crate::ErrorCategory::policy`].
    fn category(&self) -> ErrorCategory;
    /// Prescriptive advice for the report/doctor output.
    fn advice(&self) -> Option<&'static str> {
        None
    }
}

/// One entry in the error registry — maps a code string to metadata.
#[derive(Debug, Clone)]
pub struct ErrorRegistryEntry {
    /// Stable code, e.g. `"E100"` — the doctor/CLI lookup key.
    pub code: &'static str,
    /// Variant struct name, e.g. `"CompileError"`.
    pub name: &'static str,
    /// Error category — drives [`ErrorCategory::policy`](crate::ErrorCategory::policy).
    pub category: ErrorCategory,
    /// Canonical one-line display string.
    pub display: &'static str,
    /// Prescriptive advice — `None` for legacy `define_errors!` entries;
    /// populated by the future `error!` macro.
    pub advice: Option<&'static str>,
    /// Defining module path (`module_path!()` at the `define_errors!` call site).
    pub module: &'static str,
}

/// Global error registry. One distributed slice spanning every linked
/// crate; `define_errors!` emits one element per variant.
///
/// Duplicate codes CANNOT be rejected at registration (link-time slices
/// have no codegen hooks); [`lookup_error`] returns the first match.
/// Enforce uniqueness with a test in your workspace — link the crates that
/// define errors into one test binary and assert global uniqueness.
///
/// Platform note: `linkme` linker sections do not exist on wasm — on
/// `target_family = "wasm"` the registry is a statically EMPTY slice
/// (`lookup_error` returns `None`); all other `define_errors!`-generated
/// API (`code()`, `category()`, `Display`, `From`) works unchanged.
#[cfg(not(target_family = "wasm"))]
#[linkme::distributed_slice]
pub static ERROR_REGISTRY: [ErrorRegistryEntry];

/// wasm fallback — see the platform note on the non-wasm definition above.
#[cfg(target_family = "wasm")]
pub static ERROR_REGISTRY: [ErrorRegistryEntry; 0] = [];

/// Iterate every registered error entry, across all linked crates.
pub fn error_registry() -> impl Iterator<Item = &'static ErrorRegistryEntry> {
    ERROR_REGISTRY.iter()
}

/// Look up one error code — e.g. for a `doctor` CLI command. On duplicate
/// codes, returns the first registration (slice order is link-time
/// deterministic but unspecified) — duplicates are a bug; catch them with
/// the workspace uniqueness test described on [`ERROR_REGISTRY`].
#[must_use]
pub fn lookup_error(code: &str) -> Option<&'static ErrorRegistryEntry> {
    ERROR_REGISTRY.iter().find(|e| e.code == code)
}

/// Render a doctor report for an error code: code, name, category, policy
/// advice line, canonical display, advice, defining module. Deterministic
/// `key: value` lines, one fact per line.
#[must_use]
pub fn doctor(code: &str) -> Option<String> {
    let entry = lookup_error(code)?;
    let mut out = format!(
        "code: {}\nname: {}\ncategory: {}\npolicy: {}\ndisplay: {}",
        entry.code,
        entry.name,
        entry.category,
        entry.category.policy().advice_line(),
        entry.display,
    );
    if let Some(advice) = entry.advice {
        // Infallible on String.
        let _ = write!(out, "\nadvice: {advice}");
    }
    // Infallible on String.
    let _ = write!(out, "\nmodule: {}", entry.module);
    Some(out)
}

/// Generates error variant structs + per-struct Display/Error + ENTRY
/// consts + `From<Variant> for Enum` + link-time `ERROR_REGISTRY`
/// registration + the enum's `code()` / `category()` / `Display` from a
/// compact declaration. Crate-agnostic: the enum name is the first item;
/// the enum itself (and its `Error` impl) stays hand-written in the owning
/// crate, so variants the macro cannot express (e.g. newtype wrappers over
/// foreign errors) are declared alongside the macro and covered by a
/// trailing `extra` block.
///
/// ```ignore
/// fast_observe::define_errors! {
///     enum JournalError {
///         (WorkerClosed, "E206", Transient, "journal worker closed",
///             "journal worker channel closed", {});
///     }
/// }
/// ```
///
/// With non-generated variants (one `(code_pattern, display_pattern => "code",
/// Category, display_expr)` per variant — two patterns so the display match
/// can bind fields the code/category matches don't use):
///
/// ```ignore
/// fast_observe::define_errors! {
///     enum EngineError {
///         (EntityNotFound, "E001", Content, "entity not found",
///             "entity not found: {id}", { id: u32 });
///     }
///     extra {
///         (Self::Inner(_), Self::Inner(e) => "E500", Invariant, format_args!("inner: {e}"));
///     }
/// }
/// ```
///
/// Registrations need `linkme` in the consuming crate's dependencies (the
/// generated `#[linkme::distributed_slice]` expansion paths resolve through
/// it) — non-wasm targets only; on wasm the registration is cfg'd out.
#[macro_export]
macro_rules! define_errors {
    (
        enum $enum:ident {
            $( $variant:tt );+ $(;)?
        }
        extra {
            $( $extra:tt );+ $(;)?
        }
    ) => {
        $crate::define_errors!(@expand $enum { $($variant)+ } { $($extra)+ });
    };
    (
        enum $enum:ident {
            $( $variant:tt );+ $(;)?
        }
    ) => {
        $crate::define_errors!(@expand $enum { $($variant)+ } { });
    };
    (
        @expand $enum:ident
        { $( ($name:ident, $code:literal, $cat:ident, $display:literal, $fmt:literal, {
            $($field:ident : $fty:ty),* $(,)?
        }) )+ }
        { $( ($extra_pat:pat, $extra_disp_pat:pat => $extra_code:literal, $extra_cat:ident, $extra_disp:expr) )* }
    ) => {
        $(
            #[derive(Debug)]
            pub struct $name {
                $(pub $field: $fty,)*
            }

            impl $name {
                pub const CODE: &'static str = $code;
                pub const ENTRY: $crate::ErrorRegistryEntry = $crate::ErrorRegistryEntry {
                    code: Self::CODE,
                    name: stringify!($name),
                    category: $crate::ErrorCategory::$cat,
                    display: $display,
                    advice: None,
                    module: module_path!(),
                };
            }

            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, $fmt $(, $field = self.$field)*)
                }
            }

            impl std::error::Error for $name {}

            impl From<$name> for $enum {
                fn from(e: $name) -> Self { Self::$name(e) }
            }

            const _: () = {
                // No linker sections on wasm — registration is skipped there
                // (see the platform note on `ERROR_REGISTRY`).
                #[cfg(not(target_family = "wasm"))]
                #[linkme::distributed_slice($crate::ERROR_REGISTRY)]
                static ENTRY_ELEMENT: $crate::ErrorRegistryEntry = $name::ENTRY;
            };
        )+

        impl $enum {
            /// Stable error code for doctor/CLI lookup.
            #[must_use]
            pub fn code(&self) -> &'static str {
                match self {
                    $( Self::$name(_) => $name::CODE, )+
                    $( $extra_pat => $extra_code, )*
                }
            }

            /// Error category — drives poison/retry policy.
            #[must_use]
            pub fn category(&self) -> $crate::ErrorCategory {
                match self {
                    $( Self::$name(_) => $crate::ErrorCategory::$cat, )+
                    $( $extra_pat => $crate::ErrorCategory::$extra_cat, )*
                }
            }
        }

        impl std::fmt::Display for $enum {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( Self::$name(e) => write!(f, "{e}"), )+
                    $( $extra_disp_pat => write!(f, "{}", $extra_disp), )*
                }
            }
        }
    };
}
