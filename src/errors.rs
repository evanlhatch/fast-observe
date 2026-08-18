//! Crate-agnostic error machinery: the registry types
//! (`ErrorRegistryEntry`, `ERROR_REGISTRY`), lookup (`error_registry`,
//! `lookup_error`, `doctor`), and the `Coded` trait.
//!
//! Error DEFINITION lives in the [`error!`](crate::error) proc macro
//! (thiserror-style attributes, `#[code]`/`#[category]`/`#[advice]`,
//! `provide()` tags, constructors). An `error!` invocation in ANY crate
//! registers its variants in the global `linkme` distributed slice at link
//! time (native) — your app's doctor/CLI (e.g. `myapp doctor <code>`) looks
//! up errors workspace-wide. This crate stays a clean leaf: no app-specific
//! types.

use std::fmt::Write as _;

use crate::ErrorCategory;

/// A stable error code provided through `Error::provide` (nightly
/// `error_generic_member_access`) — readable from any `&dyn Error` via
/// `core::error::request_value::<ErrorCode>(_)`. `error!`-generated types
/// provide this automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorCode(pub &'static str);

/// An error's category provided through `Error::provide` — same channel as
/// [`ErrorCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CategoryTag(pub ErrorCategory);

/// An error with a stable registry code — implemented by `error!` output.
/// Drives code-in-tree rendering, doctor, and policy-derived advice.
///
/// Deliberately NOT sealed — intended for user implementation: hand-implement
/// it for error types defined without the [`error!`](crate::error) macro
/// (implement manually only if not using `error!`, which generates this impl
/// plus registry entries for you).
#[diagnostic::on_unimplemented(
    message = "error types must implement `Coded` — define the type with `error!` to get it automatically",
    note = "`error!` generates the entry, code, and category accessors"
)]
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
    /// Prescriptive advice — populated by `error!` from `#[advice = "..."]`
    /// or the variant's first doc-comment line.
    pub advice: Option<&'static str>,
    /// Variant-specific action text (from `#[action = "..."]`) — overrides
    /// the generic category-policy line in the report's `action:` section.
    pub action: Option<&'static str>,
    /// Defining module path (`module_path!()` at the `error!` call site).
    pub module: &'static str,
}

/// Global error registry. One distributed slice spanning every linked
/// crate; `error!` emits one element per coded variant.
///
/// Duplicate codes CANNOT be rejected at registration (link-time slices
/// have no codegen hooks); [`lookup_error`] returns the first match.
/// Enforce uniqueness with a test in your workspace — link the crates that
/// define errors into one test binary and assert global uniqueness.
///
/// Platform note: `linkme` linker sections do not exist on wasm — on
/// `target_family = "wasm"` this is a statically EMPTY slice; the wasm
/// composition root is [`register_statics`], which populates a runtime
/// registry consulted alongside this slice. All other `error!`-generated
/// API (`code()`, `category()`, `Display`, `From`) works unchanged.
#[cfg(not(target_family = "wasm"))]
#[linkme::distributed_slice]
pub static ERROR_REGISTRY: [ErrorRegistryEntry];

/// wasm fallback — see the platform note on the non-wasm definition above.
#[cfg(target_family = "wasm")]
pub static ERROR_REGISTRY: [ErrorRegistryEntry; 0] = [];

/// wasm-only runtime registry backing [`register_statics`].
#[cfg(target_family = "wasm")]
static STATIC_REGISTRY: std::sync::LazyLock<
    parking_lot::RwLock<Vec<&'static [ErrorRegistryEntry]>>,
> = std::sync::LazyLock::new(|| parking_lot::RwLock::new(Vec::new()));

/// Register per-enum `ENTRIES` slices (emitted by `error!`). The wasm
/// composition root: call once at startup with each error enum's `ENTRIES`.
///
/// On native targets `ERROR_REGISTRY` is complete at link time (the
/// `linkme` distributed slice), so this is a no-op. On wasm — where linker
/// sections do not exist and `ERROR_REGISTRY` is a statically empty slice —
/// this populates the runtime registry; [`error_registry`] and
/// [`lookup_error`] consult both sources.
///
/// Duplicate codes across registered slices: first match wins (the same
/// rule as native link-time duplicates).
pub fn register_statics(entries: &'static [ErrorRegistryEntry]) {
    #[cfg(target_family = "wasm")]
    STATIC_REGISTRY.write().push(entries);
    #[cfg(not(target_family = "wasm"))]
    let _ = entries;
}

/// Iterate every registered error entry, across all linked crates.
///
/// On wasm this chains the (statically empty) [`ERROR_REGISTRY`] slice with
/// every slice passed to [`register_statics`], in registration order.
pub fn error_registry() -> impl Iterator<Item = &'static ErrorRegistryEntry> {
    #[cfg(not(target_family = "wasm"))]
    {
        ERROR_REGISTRY.iter()
    }
    #[cfg(target_family = "wasm")]
    {
        ERROR_REGISTRY
            .iter()
            .chain(STATIC_REGISTRY.read().clone().into_iter().flatten())
    }
}

/// Look up one error code — e.g. for a `doctor` CLI command. On duplicate
/// codes, returns the first match (native: slice order is link-time
/// deterministic but unspecified; wasm: `ERROR_REGISTRY` first, then
/// [`register_statics`] slices in registration order) — duplicates are a
/// bug; catch them with the workspace uniqueness test described on
/// [`ERROR_REGISTRY`].
#[must_use]
pub fn lookup_error(code: &str) -> Option<&'static ErrorRegistryEntry> {
    error_registry().find(|e| e.code == code)
}

/// Registry entries whose code starts with `query` (case-sensitive),
/// sorted by code. Pure lookup helper behind [`doctor`]'s prefix fallback.
fn similar_codes(query: &str) -> Vec<&'static ErrorRegistryEntry> {
    let mut matches: Vec<&'static ErrorRegistryEntry> = error_registry()
        .filter(|entry| entry.code.starts_with(query))
        .collect();
    matches.sort_by(|a, b| a.code.cmp(b.code));
    matches
}

/// Render a doctor report for an error code: code, name, category, policy
/// advice line, canonical display, advice, defining module. Deterministic
/// `key: value` lines, one fact per line.
///
/// On an exact miss, falls back to a case-sensitive prefix search: when any
/// registered code starts with `code`, returns a report whose first line is
/// `no exact match for {code:?} — similar codes:` followed by one
/// `code: name (category)` line per match, sorted by code. Returns `None`
/// only when neither exact nor prefix matches exist.
#[must_use]
pub fn doctor(code: &str) -> Option<String> {
    let Some(entry) = lookup_error(code) else {
        let similar = similar_codes(code);
        if similar.is_empty() {
            return None;
        }
        let mut out = format!("no exact match for {code:?} — similar codes:");
        for entry in similar {
            // Infallible on String.
            let _ = write!(out, "\n{}: {} ({})", entry.code, entry.name, entry.category);
        }
        return Some(out);
    };
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
    if let Some(action) = entry.action {
        // Infallible on String.
        let _ = write!(out, "\naction: {action}");
    }
    // Infallible on String.
    let _ = write!(out, "\nmodule: {}", entry.module);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registry is populated by `error!` macro invocations at link time;
    // these tests make no assumption about its contents.

    #[test]
    fn similar_codes_matches_manual_filter_and_is_sorted() {
        for query in ["E", "E1", "zz-no-such-prefix"] {
            let expected: Vec<&'static str> = error_registry()
                .filter(|entry| entry.code.starts_with(query))
                .map(|entry| entry.code)
                .collect();
            let actual: Vec<&'static str> = similar_codes(query)
                .iter()
                .map(|entry| entry.code)
                .collect();
            assert!(actual.windows(2).all(|w| w[0] <= w[1]));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn doctor_unknown_code_with_no_prefix_match_is_none() {
        // No registered code starts with a NUL byte.
        assert!(similar_codes("\0not-a-code").is_empty());
        assert!(doctor("\0not-a-code").is_none());
    }

    #[test]
    fn doctor_prefix_miss_lists_similar_codes_sorted() {
        let query = "E";
        if lookup_error(query).is_some() {
            // Exact hit — the prefix path is not exercised by this registry.
            return;
        }
        let similar = similar_codes(query);
        match doctor(query) {
            None => assert!(similar.is_empty()),
            Some(report) => {
                assert_eq!(
                    report.lines().next(),
                    Some("no exact match for \"E\" — similar codes:"),
                    "report:\n{report}"
                );
                let lines: Vec<&str> = report.lines().skip(1).collect();
                assert_eq!(lines.len(), similar.len(), "report:\n{report}");
                for (line, entry) in lines.iter().zip(&similar) {
                    assert_eq!(
                        *line,
                        format!("{}: {} ({})", entry.code, entry.name, entry.category)
                    );
                }
            }
        }
    }
}
