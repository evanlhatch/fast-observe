//! Proc macros for fast-observe: `#[instrument]`, `#[all_functions]`, `#[skip]`.
//!
//! Replaces the `profiling-procmacros` re-exports, whose expansion referenced
//! the `profiling` crate (absent from `fast-observe` consumers). These macros
//! expand to absolute `::fast_observe::...` paths instead.
//!
//! Expansion for a sync `fn foo(args) -> R { body }`:
//!
//! ```ignore
//! fn foo(args) -> R {
//!     let _guard = ::fast_observe::scope!(
//!         ::core::concat!(::core::module_path!(), "::", "foo")
//!     );
//!     body
//! }
//! ```
//!
//! `module_path!()` is resolved by rustc at the *expansion* site, i.e. the
//! caller's crate — for methods it names the module containing the `impl`
//! block (impl blocks live at module level), which is the intended behavior.

// `proc_macro_diagnostic` (tracking #54140): native spanned diagnostics
// (`proc_macro::Diagnostic`) on top of the `compile_error!` tokens — one of
// the crate's design-mandated nightly gates (see-DESIGN.md §11c; REQUIRED for
// the error! macro UX). Declared at the root per §11c rule 1. See
// `span_error` in this file.
#![feature(proc_macro_diagnostic)]

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Group, Ident, Literal, Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, quote};
use venial::{Attribute, Function, ImplMember, Item};

mod error_macro;

/// Rejection text for async fns (shared by `#[instrument]` / `#[all_functions]`).
/// Our scope guards are thread-bound: holding one across `.await` would make
/// futures `!Send` and misattribute time under task interleaving.
fn async_rejection(attr: &str) -> String {
    format!(
        "{attr} does not support async fn yet — use \
         fastrace::trace(enter_on_poll = true) or fast_observe::in_observed_span \
         (see OBSERVE.md)"
    )
}

/// Emit a native rustc diagnostic (file/line/column) for `span`, beside the
/// `compile_error!` tokens that carry the same message into the expanded item
/// (the textual contract asserted by the trybuild UI tests).
///
/// `proc_macro2::Span::unwrap()` is only available inside a proc-macro crate
/// (the `wrap_proc_macro` cfg) and never panics there: every span wraps a live
/// `proc_macro::Span`, including `Span::call_site()`.
fn span_error(span: Span, msg: &str) {
    // This nightly replaced the old `Diagnostic::spanned_error(span, msg)`
    // constructor with `Diagnostic::spanned(spans, level, msg)` (Level here).
    let span = span.unwrap();
    proc_macro::Diagnostic::spanned(span, proc_macro::Level::Error, msg).emit();
}

/// Emit + render a venial parse error (kept only for the compile_error!
/// tokens; the native diagnostic is emitted alongside).
pub(crate) fn venial_error(err: venial::Error) -> TokenStream2 {
    span_error(err.span(), &err.to_string());
    err.to_compile_error()
}

fn error_at(span: Span, message: &str) -> TokenStream2 {
    span_error(span, message);
    venial::Error::new_at_span(span, message).to_compile_error()
}

fn error_at_tokens(tokens: &TokenStream2, message: &str) -> TokenStream2 {
    // Same primary span venial picks for `new_at_tokens`: first token, or
    // `call_site()` for an empty stream.
    span_error(
        tokens
            .clone()
            .into_iter()
            .next()
            .map_or_else(Span::call_site, |tt| tt.span()),
        message,
    );
    venial::Error::new_at_tokens(tokens.clone(), message).to_compile_error()
}

/// Build the scope-name expression for the default (`module::path::fn_name`)
/// and custom (`name = "..."`) cases.
fn name_expr(fn_name: &Ident, custom: Option<&Literal>) -> TokenStream2 {
    match custom {
        Some(lit) => quote!(#lit),
        None => {
            let name = Literal::string(&fn_name.to_string());
            quote!(::core::concat!(::core::module_path!(), "::", #name))
        }
    }
}

/// Parse the shared `#[instrument]` / `#[instrument_async]` attribute
/// arguments: empty, or `name = "..."` (a string literal). `macro_name` is
/// the bare macro name (`instrument` / `instrument_async`) for the
/// diagnostic text.
fn parse_name_attr(
    attr2: &TokenStream2,
    macro_name: &str,
    errors: &mut Vec<TokenStream2>,
) -> Option<Literal> {
    let mut args = attr2.clone().into_iter();
    match (args.next(), args.next(), args.next(), args.next()) {
        (None, _, _, _) => None,
        (
            Some(TokenTree::Ident(key)),
            Some(TokenTree::Punct(eq)),
            Some(TokenTree::Literal(lit)),
            None,
        ) if key == "name" && eq.as_char() == '=' && lit.to_string().starts_with('"') => Some(lit),
        _ => {
            errors.push(error_at_tokens(
                attr2,
                &format!("expected `#[{macro_name}]` or `#[{macro_name}(name = \"...\")]`"),
            ));
            None
        }
    }
}

/// Wrap `func`'s body in an observability scope guard named by `name`.
fn wrap_body(func: &mut Function, name: TokenStream2) {
    let Some(body) = &func.body else {
        return;
    };
    let inner = body.stream();
    let mut group = Group::new(
        Delimiter::Brace,
        quote! {
            let _guard = ::fast_observe::scope!(#name);
            #inner
        },
    );
    group.set_span(body.span());
    func.body = Some(group);
}

/// True for `#[skip]`, `#[fast_observe::skip]`, `#[fast_observe_macros::skip]`.
fn is_skip_attr(attr: &Attribute) -> bool {
    let segments: Vec<String> = attr
        .path
        .iter()
        .filter_map(|tt| match tt {
            TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
        .collect();
    match segments.as_slice() {
        [single] => single == "skip",
        [krate, last] => {
            last == "skip" && (krate == "fast_observe" || krate == "fast_observe_macros")
        }
        _ => false,
    }
}

/// Append collected errors after the (possibly rewritten) item tokens.
fn finish(mut item: TokenStream2, errors: Vec<TokenStream2>) -> TokenStream {
    for err in errors {
        item.extend(err);
    }
    item.into()
}

/// Instrument a sync function: run its body inside a `fast_observe::scope!`
/// named `module::path::fn_name` (or `name = "custom.name"`).
///
/// Async fns are rejected with `compile_error!` — our scope guards are
/// thread-bound and must not be held across `.await` (see crate docs).
#[proc_macro_attribute]
pub fn instrument(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors: Vec<TokenStream2> = Vec::new();

    // ── Attribute arguments: empty or `name = "..."` ──────────────────────
    let attr2 = TokenStream2::from(attr);
    let custom_name = parse_name_attr(&attr2, "instrument", &mut errors);

    // ── Item: must be a sync function ──────────────────────────────────────
    let item2 = TokenStream2::from(item);
    let Ok(parsed) = venial::parse_item(item2.clone()).map_err(venial_error) else {
        // The parse error was already pushed (and emitted) by `map_err`.
        let _ = errors;
        return finish(item2, errors);
    };
    let Item::Function(mut func) = parsed else {
        errors.push(error_at_tokens(
            &item2,
            "#[instrument] is only supported on functions and methods",
        ));
        return finish(item2, errors);
    };

    if let Some(tk_async) = &func.qualifiers.tk_async {
        errors.push(error_at(tk_async.span(), &async_rejection("#[instrument]")));
        // Re-emit the original function so other errors still surface.
        return finish(item2, errors);
    }

    let name = name_expr(&func.name, custom_name.as_ref());
    wrap_body(&mut func, name);
    finish(func.into_token_stream(), errors)
}

/// Async counterpart of [`instrument`]: wraps an `async fn` body so a
/// Async counterpart of [`instrument`]: wraps an `async fn` so a fastrace
/// span is entered on each poll — the missing half of OBSERVE.md's sync-only
/// story (the DESIGN.md §2 async gap). Expands the fn to
/// `#[fastrace::trace(enter_on_poll = true)]` with the crate path baked to
/// `::fast_observe::__private::fastrace`, so consumers need no direct
/// fastrace dependency (only the `fastrace` feature of fast-observe).
///
/// Accepts an optional `name = "..."` override (default: `func_path!()` -
/// full module path + fn name). `properties` are NOT supported: fastrace
/// forbids `enter_on_poll` together with `properties`.
///
/// Sync fns are rejected with `compile_error!` — use [`instrument`] there.
/// Requires feature `fastrace`; without it the expansion fails to resolve
/// `::fast_observe::__private::fastrace` (enable the feature to fix).
///
/// ```
/// # #[cfg(feature = "fastrace")]
/// # mod m {
/// use fast_observe::prelude::*;
///
/// #[fast_observe::instrument_async]
/// async fn load(id: u64) -> u64 {
///     id
/// }
/// # }
/// ```
#[proc_macro_attribute]
pub fn instrument_async(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors: Vec<TokenStream2> = Vec::new();

    // ── Attribute arguments: empty or `name = "..."` ──────────────────────
    let attr2 = TokenStream2::from(attr);
    let custom_name = parse_name_attr(&attr2, "instrument_async", &mut errors);

    // ── Item: must be an async function ────────────────────────────────────
    let item2 = TokenStream2::from(item);
    let Ok(parsed) = venial::parse_item(item2.clone()).map_err(|e| {
        errors.push(venial_error(e));
    }) else {
        return finish(item2, errors);
    };
    let Item::Function(func) = parsed else {
        errors.push(error_at_tokens(
            &item2,
            "#[instrument_async] is only supported on functions and methods",
        ));
        return finish(item2, errors);
    };

    if func.qualifiers.tk_async.is_none() {
        errors.push(error_at_tokens(
            &item2,
            "#[instrument_async] is only supported on async fn — use #[instrument] for sync fn",
        ));
        return finish(item2, errors);
    }

    let full = func.into_token_stream();
    let trace_attr: TokenStream2 = match custom_name {
        Some(lit) => quote! {
            #[::fast_observe::__private::fastrace::trace(
                crate = ::fast_observe::__private::fastrace,
                enter_on_poll = true,
                name = #lit
            )]
        },
        None => quote! {
            #[::fast_observe::__private::fastrace::trace(
                crate = ::fast_observe::__private::fastrace,
                enter_on_poll = true
            )]
        },
    };
    finish(quote!(#trace_attr #full), errors)
}

/// Exit-path-as-report for the common `fn main() -> Result<(), E>` form
/// (SURFACE.md §6a, DESIGN.md §9c-ext).
///
/// `fn main() -> Fault<E>` already gets the report-and-exit behavior for
/// free via `Termination`; this attribute gives the `Result` form the same
/// exit path without changing the return type:
///
/// ```ignore
/// #[fast_observe::main]
/// fn main() -> Result<(), MyError> {
///     run()?;
///     Ok(())
/// }
/// ```
///
/// expands the body to
///
/// ```ignore
/// match { body } {
///     Ok(ok) => Ok(ok),
///     Err(err) => ::fast_observe::Fault::from(err).exit_with_report(),
/// }
/// ```
///
/// `exit_with_report` renders the full fault report to stderr and exits
/// with the error category's sysexits code ([`Fault::exit_code`]) — the
/// same path as `fn main() -> Fault`. Requires the error type to satisfy
/// `Error + Send + Sync + 'static` (the `Fault::from` bound). Sync only:
/// for async entry use `#[fast_observe::main]` above `#[tokio::main]` (the
/// attribute wraps the sync `main` the runtime calls).
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors: Vec<TokenStream2> = Vec::new();

    // ── Attribute arguments: none. ────────────────────────────────────────
    let attr2 = TokenStream2::from(attr);
    if !attr2.is_empty() {
        errors.push(error_at_tokens(
            &attr2,
            "expected `#[fast_observe::main]` with no arguments",
        ));
    }

    // ── Item: must be a sync function returning `Result<_, E>` ────────────
    let item2 = TokenStream2::from(item);
    let Ok(parsed) = venial::parse_item(item2.clone()).map_err(|e| {
        errors.push(venial_error(e));
    }) else {
        return finish(item2, errors);
    };
    let Item::Function(mut func) = parsed else {
        errors.push(error_at_tokens(
            &item2,
            "#[fast_observe::main] is only supported on functions",
        ));
        return finish(item2, errors);
    };

    if func.qualifiers.tk_async.is_some() {
        errors.push(error_at_tokens(
            &item2,
            "#[fast_observe::main] does not support async fn — wrap #[tokio::main] \
             (or async fn mains) and let this attribute handle the sync main",
        ));
        return finish(item2, errors);
    }

    // Find the `-> Result<_, E>` return: walk `ret` tokens for a `Result`
    // path (last segment `Result`), then keep the body rewrite regardless
    // (the match arms are type-inferred from the body).
    let Ok(is_result_ret) = function_returns_result(&func) else {
        errors.push(error_at_tokens(
            &item2,
            "#[fast_observe::main] expects `fn main() -> Result<_, E>` (the Err arm \
             routes through Fault::exit_with_report)",
        ));
        return finish(item2, errors);
    };
    if !is_result_ret {
        errors.push(error_at_tokens(
            &item2,
            "#[fast_observe::main] expects the fn to return Result — use \
             `fn main() -> Fault` directly for the no-Result form",
        ));
        return finish(item2, errors);
    }

    let Some(body) = &func.body else {
        errors.push(error_at_tokens(
            &item2,
            "#[fast_observe::main] missing function body",
        ));
        return finish(item2, errors);
    };
    // The return type tokens re-annotate the scrutinee closure so the
    // error type E is fixed ([`FnOnce`] closure with an explicit return
    // type; a bare `match { .. }` on the statement-list block cannot carry
    // the `Result<_, E>` context through — E would stay unconstrained).
    let ret_ty: TokenStream2 = func
        .return_ty
        .as_ref()
        .map(|ty| ty.tokens.iter().cloned().collect())
        .unwrap_or_default();
    let inner = body.stream();
    let mut group = Group::new(
        Delimiter::Brace,
        quote! {
            match (|| -> #ret_ty { #inner })() {
                ::core::result::Result::Ok(ok) => ::core::result::Result::Ok(ok),
                ::core::result::Result::Err(err) => {
                    ::fast_observe::Fault::from(err).exit_with_report()
                }
            }
        },
    );
    group.set_span(body.span());
    func.body = Some(group);

    finish(func.into_token_stream(), errors)
}

/// Is the function's return type a `Result<_, _>`? Token-walks the `->`
/// type: accepts when the ident immediately preceding the top-level `<` of
/// the generic-argument list is `Result` — handles `Result<T, E>`,
/// `std::result::Result<...>`, `fast_observe::Result<T>` (a single generic
/// arg uses the default error type, which is still a Result). A missing
/// return type is not a Result.
fn function_returns_result(func: &Function) -> Result<bool, ()> {
    let Some(ret) = &func.return_ty else {
        return Ok(false);
    };
    let mut last_ident: Option<&Ident> = None;
    for tt in &ret.tokens {
        match tt {
            TokenTree::Ident(ident) => last_ident = Some(ident),
            TokenTree::Punct(p) if p.as_char() == '<' => {
                return Ok(last_ident.is_some_and(|ident| ident == "Result"));
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Instrument every method of an `impl` block (inherent or trait impl).
///
/// Methods marked `#[skip]`, `#[fast_observe::skip]`, or
/// `#[fast_observe_macros::skip]` are left untouched; the marker attribute is
/// consumed here so it never reaches rustc. Associated non-fn items
/// (consts, types, macros) are passed through unchanged. Async methods are
/// rejected with `compile_error!` and left unmodified.
#[proc_macro_attribute]
pub fn all_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors: Vec<TokenStream2> = Vec::new();

    let attr2 = TokenStream2::from(attr);
    if !attr2.is_empty() {
        errors.push(error_at_tokens(
            &attr2,
            "#[all_functions] takes no arguments",
        ));
    }

    let item2 = TokenStream2::from(item);
    let Ok(parsed) = venial::parse_item(item2.clone()).map_err(|e| {
        errors.push(venial_error(e));
    }) else {
        return finish(item2, errors);
    };
    let Item::Impl(mut imp) = parsed else {
        errors.push(error_at_tokens(
            &item2,
            "#[all_functions] is only supported on impl blocks",
        ));
        return finish(item2, errors);
    };

    for member in &mut imp.body_items {
        let ImplMember::AssocFunction(func) = member else {
            continue;
        };
        // Consume skip markers so they never reach rustc.
        let mut skipped = false;
        func.attributes.retain(|attr| {
            if is_skip_attr(attr) {
                skipped = true;
                return false;
            }
            true
        });
        if skipped {
            continue;
        }
        if let Some(tk_async) = &func.qualifiers.tk_async {
            errors.push(error_at(
                tk_async.span(),
                &async_rejection("#[all_functions]"),
            ));
            continue;
        }
        if func.body.is_none() {
            continue;
        }
        let name = name_expr(&func.name, None);
        wrap_body(func, name);
    }

    finish(imp.into_token_stream(), errors)
}

/// Define a typed error enum with thiserror-compatible attributes plus
/// fast-observe codes, categories, advice, and registry registration.
///
/// ```ignore
/// fast_observe::error! {
///     /// Doc comments pass through to the enum.
///     pub enum EngineError {
///         /// Variant docs pass through to the generated struct; the first
///         /// doc line is the default `advice`.
///         #[error("entity not found: {id}")]
///         #[code = "E001", category = Content, advice = "check the entity table"]
///         EntityNotFound { id: u64 },
///
///         #[error("io: {0}")]
///         #[from]
///         Io(std::io::Error),
///
///         #[error("pipeline layout: {source}")]
///         #[code = "E428", category = Transient]
///         PipelineLayout { #[source] source: Box<EngineError> },
///     }
/// }
/// ```
///
/// Attributes (thiserror's subset works verbatim):
/// - `#[error("...")]` — Display template, REQUIRED on every variant.
///   Forwarded to `write!` unparsed: struct variants interpolate
///   `{field}`/`{field:?}`; tuple variants interpolate `{0}`..`{N}`.
/// - `#[code = "E123", category = <Category>, advice = "..."]` — opts the
///   variant into the `ERROR_REGISTRY` / doctor / report codes. `category`
///   is REQUIRED with `code` (and vice versa); `advice` may also be a
///   standalone `#[advice = "..."]`; its default is the first doc line.
/// - `#[from]` — single-field tuple variants only: generates
///   `From<InnerType>` for the enum and wires `source()` to the inner
///   error. (`From<InnerType> for Fault<Enum>` is NOT generated — with a
///   foreign inner type it would violate the orphan rule, since `Fault` is
///   not `#[fundamental]`; use `Err(inner).map_err(Enum::from)?`.)
/// - `#[source]` — marks a struct-variant field as the `Error::source()`;
///   a field NAMED `source` is wired without the attribute.
/// - `#[max_size = N]` on the enum — overrides the 64-byte size budget
///   enforced by a generated `const _` assertion.
///
/// Generated: the enum (docs + unknown attributes like `#[cfg]` forwarded;
/// `#[cfg]`/`#[cfg_attr]` also propagate onto the generated impls and match
/// arms), one public struct per struct variant, per-variant + enum
/// `Display`/`Error` impls (including nightly `Error::provide` of
/// `ErrorCode`/`CategoryTag` for coded variants), `From<Variant> for Enum`
/// and `From<Variant> for Fault<Enum>`, a per-variant `ENTRY` const plus
/// link-time registration (non-wasm; needs `linkme` in the consuming
/// crate's dependencies, same as `define_errors!`), and `Enum::ENTRIES`
/// (coded variants only) for the wasm composition path.
///
/// `code()` / `category()` / `advice()` and the
/// `::fast_observe::errors::Coded` impl are generated ONLY when EVERY
/// variant is coded — a mixed enum has no total `code()`, so the methods
/// (and the trait) are omitted rather than returning placeholders.
///
/// v1 limits: no generics/where clauses; `#[from]` requires a single-field
/// tuple variant; uncoded variants behave exactly like thiserror output.
///
/// The input enum must implement `Debug` (e.g. via `#[derive(Debug)]`), as
/// the generated `Error` impls require it.
#[proc_macro]
pub fn error(item: TokenStream) -> TokenStream {
    error_macro::expand(TokenStream2::from(item)).into()
}

/// Identity attribute: strips itself, leaves the item unchanged.
///
/// Its real consumer is `#[all_functions]`, which pattern-matches and removes
/// `#[skip]` markers before rustc sees them; this standalone macro exists so
/// a stray `#[skip]` (e.g. after `#[all_functions]` was removed) still
/// compiles instead of erroring with "cannot find attribute".
#[proc_macro_attribute]
pub fn skip(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
