//! Implementation of the [`error!`](crate::error) macro.
//!
//! Pipeline: venial-parse the input enum → strip + interpret the
//! macro-owned attributes (`error` / `code` / `category` / `advice` / `from`
//! / `source` / `max_size`) → validate (collecting ALL errors) → codegen.

use std::collections::HashMap;

use proc_macro2::{Ident, Literal, Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, quote};
use venial::{Attribute, AttributeValue, Enum, EnumVariant, Fields, Item, NamedField, TupleField};

use crate::{error_at, error_at_tokens, venial_error};

/// Default byte budget for the generated enum (`#[max_size = N]` overrides).
const DEFAULT_MAX_SIZE: usize = 64;

/// Macro-owned attributes collected off one variant.
#[derive(Default)]
struct VariantMeta {
    tpl: Option<Literal>,
    code: Option<Literal>,
    category: Option<Ident>,
    advice: Option<Literal>,
    action: Option<Literal>,
    from: bool,
}

/// A validated variant, ready for codegen.
struct Variant {
    name: Ident,
    /// Attributes forwarded to the emitted items (docs, `#[cfg]`, derives…).
    forward_attrs: Vec<Attribute>,
    tpl: Option<Literal>,
    code: Option<Literal>,
    category: Option<Ident>,
    /// Explicit `#[advice]`, else the first doc-comment line.
    advice: Option<Literal>,
    /// Explicit `#[action = "..."]` — overrides the policy line in the
    /// report's `action:` section.
    action: Option<Literal>,
    from: bool,
    kind: Kind,
    /// `#[source]`-marked or named-`source` field of a struct variant.
    source_field: Option<Ident>,
}

enum Kind {
    Unit,
    Tuple(Vec<TupleField>),
    Struct(Vec<NamedField>),
}

/// True for single-segment attributes named `name` (`#[name ...]`).
fn attr_is(attr: &Attribute, name: &str) -> bool {
    matches!(attr.path.as_slice(), [TokenTree::Ident(ident)] if ident == name)
}

/// The value tokens of an `= ...` attribute, or one diagnostic.
fn expect_eq_value<'a>(
    attr: &'a Attribute,
    expected: &str,
    errors: &mut Vec<TokenStream2>,
) -> Option<&'a [TokenTree]> {
    match &attr.value {
        AttributeValue::Equals(_, tokens) => Some(tokens.as_slice()),
        _ => {
            errors.push(error_at_tokens(&attr.to_token_stream(), expected));
            None
        }
    }
}

/// Token types usable as a `set_once` value: span-carrying (Literal, Ident).
trait ValueSpan {
    fn value_span(&self) -> Span;
}

impl ValueSpan for Literal {
    fn value_span(&self) -> Span {
        self.span()
    }
}

impl ValueSpan for Ident {
    fn value_span(&self) -> Span {
        self.span()
    }
}

/// One generic duplicate-checked slot setter (replaces the per-field
/// set_tpl/set_code/set_category/set_advice/set_action bodies). `name`
/// completes the `duplicate \`{name}\`` diagnostic, spanned to the new value.
fn set_once<T: Clone + ValueSpan>(
    slot: &mut Option<T>,
    value: T,
    name: &str,
    errors: &mut Vec<TokenStream2>,
) {
    if slot.replace(value.clone()).is_some() {
        errors.push(error_at(value.value_span(), &format!("duplicate `{name}`")));
    }
}

/// The `#[cfg]` / `#[cfg_attr]` subset of `attrs` — forwarded onto generated
/// impls, match arms, and registrations so cfg'd-out variants stay coherent.
fn cfg_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs
        .iter()
        .filter(|attr| attr_is(attr, "cfg") || attr_is(attr, "cfg_attr"))
        .collect()
}

/// True if `attr` is a `#[derive(...)]` listing `Debug`.
fn derive_has_debug(attr: &Attribute) -> bool {
    if !attr_is(attr, "derive") {
        return false;
    }
    matches!(&attr.value, AttributeValue::Group(_, tokens)
        if tokens
            .iter()
            .any(|tt| matches!(tt, TokenTree::Ident(ident) if ident == "Debug")))
}

/// Strip quotes off a plain `"..."` literal (raw strings / escapes: None).
fn unquote(lit: &Literal) -> Option<String> {
    lit.to_string()
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .map(str::to_owned)
}

/// True when the template is thiserror's `#[error(transparent)]` — Display
/// and `source()` both delegate to the single inner field.
fn is_transparent_tpl(tpl: &Literal) -> bool {
    unquote(tpl).as_deref() == Some("transparent")
}

/// The first non-empty doc-comment line, trimmed — the default advice text.
fn first_doc_line(attrs: &[Attribute]) -> Option<Literal> {
    for attr in attrs {
        if !attr_is(attr, "doc") {
            continue;
        }
        if let AttributeValue::Equals(_, tokens) = &attr.value
            && let Some(TokenTree::Literal(lit)) = tokens.first()
            && let Some(text) = unquote(lit)
            && let trimmed = text.trim()
            && !trimmed.is_empty()
        {
            return Some(Literal::string(trimmed));
        }
    }
    None
}

fn set_tpl(meta: &mut VariantMeta, lit: Literal, errors: &mut Vec<TokenStream2>) {
    set_once(&mut meta.tpl, lit, "#[error]", errors);
}

fn set_code(meta: &mut VariantMeta, lit: Literal, errors: &mut Vec<TokenStream2>) {
    set_once(&mut meta.code, lit, "code", errors);
}

fn set_category(meta: &mut VariantMeta, cat: Ident, errors: &mut Vec<TokenStream2>) {
    set_once(&mut meta.category, cat, "category", errors);
}

fn set_advice(meta: &mut VariantMeta, lit: Literal, errors: &mut Vec<TokenStream2>) {
    set_once(&mut meta.advice, lit, "advice", errors);
}

fn set_action(meta: &mut VariantMeta, lit: Literal, errors: &mut Vec<TokenStream2>) {
    set_once(&mut meta.action, lit, "action", errors);
}

/// Parse the `= "E001", category = Content, advice = "..."` tail of `#[code]`.
fn parse_code_tail(tokens: &[TokenTree], meta: &mut VariantMeta, errors: &mut Vec<TokenStream2>) {
    let mut iter = tokens.iter();
    match iter.next() {
        Some(TokenTree::Literal(lit)) => set_code(meta, lit.clone(), errors),
        Some(tt) => {
            errors.push(error_at(
                tt.span(),
                "expected the code as a string literal: `#[code = \"E123\", category = ...]`",
            ));
            return;
        }
        None => {
            errors.push(error_at_tokens(
                &quote!(code),
                "expected `#[code = \"E123\", category = ...]`",
            ));
            return;
        }
    }
    while let Some(sep) = iter.next() {
        match sep {
            TokenTree::Punct(p) if p.as_char() == ',' => {}
            _ => {
                errors.push(error_at(sep.span(), "expected `,`"));
                return;
            }
        }
        // Trailing comma ends the list.
        let Some(TokenTree::Ident(key)) = iter.next() else {
            return;
        };
        match iter.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == '=' => {}
            other => {
                errors.push(error_at(
                    other.map_or_else(|| key.span(), TokenTree::span),
                    "expected `= <value>`",
                ));
                return;
            }
        }
        match (key.to_string().as_str(), iter.next()) {
            ("category", Some(TokenTree::Ident(cat))) => set_category(meta, cat.clone(), errors),
            ("advice", Some(TokenTree::Literal(lit))) => set_advice(meta, lit.clone(), errors),
            ("action", Some(TokenTree::Literal(lit))) => set_action(meta, lit.clone(), errors),
            (_, Some(tt)) => errors.push(error_at(
                tt.span(),
                "expected `category = <Ident>` | `advice = \"...\"` | `action = \"...\"`",
            )),
            (_, None) => errors.push(error_at(key.span(), "missing value after `=`")),
        }
    }
}

/// Parse `#[max_size = 128]` on the enum.
fn parse_max_size(attr: &Attribute) -> Result<usize, TokenStream2> {
    if let AttributeValue::Equals(_, tokens) = &attr.value
        && let Some(TokenTree::Literal(lit)) = tokens.first()
        && let digits = lit
            .to_string()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
        && let Ok(n) = digits.parse::<usize>()
    {
        return Ok(n);
    }
    Err(error_at_tokens(
        &attr.to_token_stream(),
        "expected `#[max_size = 128]` — an integer literal",
    ))
}

/// Strip + interpret the macro attributes of one variant and validate it.
fn parse_variant(
    v: &EnumVariant,
    errors: &mut Vec<TokenStream2>,
    default_category: Option<&Ident>,
) -> Variant {
    let mut meta = VariantMeta::default();
    let mut forward_attrs = Vec::new();

    for attr in &v.attributes {
        if attr_is(attr, "error") {
            match &attr.value {
                AttributeValue::Group(_, tokens) => {
                    let lit = tokens.iter().find_map(|tt| match tt {
                        TokenTree::Literal(lit) => Some(lit.clone()),
                        _ => None,
                    });
                    match lit {
                        Some(lit) => set_tpl(&mut meta, lit, errors),
                        None => errors.push(error_at_tokens(
                            &attr.to_token_stream(),
                            "expected `#[error(\"...\")]`",
                        )),
                    }
                }
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "expected `#[error(\"...\")]`",
                )),
            }
        } else if attr_is(attr, "code") {
            let Some(tokens) = expect_eq_value(
                attr,
                "expected `#[code = \"E123\", category = ...]`",
                errors,
            ) else {
                continue;
            };
            parse_code_tail(tokens, &mut meta, errors);
        } else if attr_is(attr, "advice") {
            let Some(tokens) = expect_eq_value(attr, "expected `#[advice = \"...\"]`", errors)
            else {
                continue;
            };
            match tokens.first() {
                Some(TokenTree::Literal(lit)) => set_advice(&mut meta, lit.clone(), errors),
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "expected `#[advice = \"...\"]`",
                )),
            }
        } else if attr_is(attr, "category") {
            let Some(tokens) = expect_eq_value(attr, "expected `#[category = <Category>]`", errors)
            else {
                continue;
            };
            match tokens.first() {
                Some(TokenTree::Ident(cat)) => set_category(&mut meta, cat.clone(), errors),
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "expected `#[category = <Category>]`",
                )),
            }
        } else if attr_is(attr, "from") {
            match &attr.value {
                AttributeValue::Empty => meta.from = true,
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "`#[from]` takes no arguments",
                )),
            }
        } else if attr_is(attr, "action") {
            let Some(tokens) = expect_eq_value(attr, "expected `#[action = \"...\"]`", errors)
            else {
                continue;
            };
            match tokens.first() {
                Some(TokenTree::Literal(lit)) => set_action(&mut meta, lit.clone(), errors),
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "expected `#[action = \"...\"]`",
                )),
            }
        } else {
            forward_attrs.push(attr.clone());
        }
    }

    // ── Per-variant validation ────────────────────────────────────────────
    if meta.tpl.is_none() {
        errors.push(error_at(
            v.name.span(),
            &format!(
                "variant `{}` is missing `#[error(\"...\")]` — the Display template is required \
                 on every variant",
                v.name
            ),
        ));
    }
    // Registry lookup-key shape: `^[A-Z]+[0-9]+$` (e.g. "E100"). Skip
    // silently when the literal isn't a plain quoted string (unquote: None).
    if let Some(code) = &meta.code
        && let Some(text) = unquote(code)
    {
        let split = text
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(text.len());
        let (letters, digits) = text.split_at(split);
        let valid = !letters.is_empty()
            && letters.chars().all(|c| c.is_ascii_uppercase())
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit());
        if !valid {
            errors.push(error_at(
                code.span(),
                &format!("error code must match `PREFIX+digits` (e.g. \"E100\") — got \"{text}\""),
            ));
        }
    }
    match (&meta.code, &meta.category) {
        (Some(_), None) => {
            // The enum-level `#[category]` default fills the gap.
            if let Some(default) = default_category {
                meta.category = Some(default.clone());
            } else {
                errors.push(error_at(
                    v.name.span(),
                    "`#[code]` requires `category = <Content|Invariant|Transient|Fatal>` \
                     (or an enum-level `#[category]` default)",
                ));
            }
        }
        (None, Some(cat)) => errors.push(error_at(
            cat.span(),
            "`category` requires `#[code]` — uncoded variants take neither",
        )),
        _ => {}
    }

    let kind = match &v.fields {
        Fields::Unit => Kind::Unit,
        Fields::Tuple(fields) => Kind::Tuple(fields.fields.items().cloned().collect()),
        Fields::Named(fields) => Kind::Struct(fields.fields.items().cloned().collect()),
    };
    match (&kind, meta.from) {
        (Kind::Struct(_), true) => errors.push(error_at(
            v.name.span(),
            "`#[from]` on a struct variant is unsupported — mark a field with `#[source]` instead",
        )),
        (Kind::Unit, true) => errors.push(error_at(
            v.name.span(),
            "`#[from]` requires a single-field tuple variant",
        )),
        (Kind::Tuple(fields), true) if fields.len() != 1 => errors.push(error_at(
            v.name.span(),
            "`#[from]` requires a single-field tuple variant (v1)",
        )),
        _ => {}
    }

    let mut source_field = None;
    if let Kind::Struct(fields) = &kind {
        let marked: Vec<&Ident> = fields
            .iter()
            .filter(|f| f.attributes.iter().any(|a| attr_is(a, "source")) || f.name == "source")
            .map(|f| &f.name)
            .collect();
        if marked.len() > 1 {
            errors.push(error_at(
                v.name.span(),
                "multiple source fields — mark at most one field with `#[source]`",
            ));
        }
        source_field = marked.first().map(|ident| (*ident).clone());
    }

    // `#[error(transparent)]` shape check: exactly one delegated field.
    // (A lone struct field becomes the source even without the marker.)
    if let Some(tpl) = &meta.tpl
        && is_transparent_tpl(tpl)
    {
        match &kind {
            Kind::Tuple(fields) if fields.len() == 1 => {}
            Kind::Struct(fields) if fields.len() == 1 => {
                source_field = source_field.or_else(|| fields.first().map(|f| f.name.clone()));
            }
            _ => errors.push(error_at(
                tpl.span(),
                "`#[error(transparent)]` requires a single-field tuple or a single-field struct variant",
            )),
        }
    }

    let advice = meta.advice.or_else(|| first_doc_line(&v.attributes));

    Variant {
        name: v.name.clone(),
        forward_attrs,
        tpl: meta.tpl,
        code: meta.code,
        category: meta.category,
        advice,
        action: meta.action,
        from: meta.from,
        kind,
        source_field,
    }
}

/// The `impl Error::provide` body for a coded variant (own values only —
/// no recursion into `source()`, matching the std pattern).
fn provide_body(v: &Variant) -> TokenStream2 {
    match (&v.code, &v.category) {
        (Some(code), Some(cat)) => quote! {
            request.provide_value(::fast_observe::errors::ErrorCode(#code));
            request.provide_value(::fast_observe::errors::CategoryTag(
                ::fast_observe::ErrorCategory::#cat
            ));
        },
        _ => TokenStream2::new(),
    }
}

/// The `ErrorRegistryEntry` literal for a coded variant (None when uncoded).
fn entry_expr(v: &Variant) -> Option<TokenStream2> {
    let (code, cat, tpl) = (v.code.as_ref()?, v.category.as_ref()?, v.tpl.as_ref()?);
    let vname = &v.name;
    let advice = match &v.advice {
        Some(lit) => quote!(::core::option::Option::Some(#lit)),
        None => quote!(::core::option::Option::None),
    };
    let action = match &v.action {
        Some(lit) => quote!(::core::option::Option::Some(#lit)),
        None => quote!(::core::option::Option::None),
    };
    Some(quote! {
        ::fast_observe::ErrorRegistryEntry {
            code: #code,
            name: ::core::stringify!(#vname),
            category: ::fast_observe::ErrorCategory::#cat,
            display: #tpl,
            advice: #advice,
            action: #action,
            module: ::core::module_path!(),
        }
    })
}

/// Re-emit the enum: macro attributes stripped, struct variants rewritten to
/// newtype variants wrapping their generated struct.
fn emit_enum(en: &Enum, enum_attrs: &[Attribute], variants: &[Variant]) -> TokenStream2 {
    let vis = &en.vis_marker;
    let name = &en.name;
    let defs = variants.iter().zip(en.variants.items()).map(|(pv, orig)| {
        let attrs = &pv.forward_attrs;
        let vname = &pv.name;
        match &pv.kind {
            Kind::Struct(_) => quote!( #(#attrs)* #vname(#vname) ),
            Kind::Tuple(fields) => quote!( #(#attrs)* #vname( #(#fields),* ) ),
            Kind::Unit => {
                let value = orig.value.as_ref().map(ToTokens::to_token_stream);
                quote!( #(#attrs)* #vname #value )
            }
        }
    });
    // `impl Error` requires `Debug`: add the derive unless the user's own
    // `#[derive(...)]` already lists `Debug` (merging avoided — a second
    // derive attribute composes fine, a duplicate `Debug` does not).
    let debug_derive = (!enum_attrs.iter().any(derive_has_debug)).then(|| quote!(#[derive(Debug)]));
    quote! {
        #debug_derive
        #(#enum_attrs)*
        #vis enum #name {
            #( #defs, )*
        }
    }
}

/// Emit everything. `errors` must be empty — validation ran beforehand.
fn codegen(
    en: &Enum,
    enum_attrs: &[Attribute],
    max_size: usize,
    variants: &[Variant],
) -> TokenStream2 {
    let enum_name = &en.name;
    let vis = &en.vis_marker;

    let mut out = emit_enum(en, enum_attrs, variants);

    let any_coded = variants.iter().any(|v| v.code.is_some());
    let all_coded = variants.iter().all(|v| v.code.is_some());
    let has_source = variants.iter().any(|v| v.source_field.is_some() || v.from);

    let mut entry_refs: Vec<TokenStream2> = Vec::new();

    for v in variants {
        let Some(tpl) = &v.tpl else {
            continue;
        };
        let vname = &v.name;
        let attrs = &v.forward_attrs;
        let cfgs = cfg_attrs(attrs);

        match &v.kind {
            Kind::Struct(fields) => {
                let field_defs: Vec<TokenStream2> = fields
                    .iter()
                    .map(|f| {
                        let fattrs: Vec<&Attribute> = f
                            .attributes
                            .iter()
                            .filter(|a| !attr_is(a, "source"))
                            .collect();
                        let fname = &f.name;
                        let fty = &f.ty;
                        quote!( #(#fattrs)* pub #fname: #fty )
                    })
                    .collect();
                let fnames: Vec<&Ident> = fields.iter().map(|f| &f.name).collect();
                let source_fn = v.source_field.as_ref().map(|sf| {
                    quote! {
                        fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                            ::core::option::Option::Some(&self.#sf)
                        }
                    }
                });
                let provide_fn = if v.code.is_some() {
                    let body = provide_body(v);
                    Some(quote! {
                        fn provide<'a>(&'a self, request: &mut ::core::error::Request<'a>) {
                            #body
                        }
                    })
                } else {
                    None
                };
                // Same double-derive guard as the enum: a user `#[derive]`
                // on the variant is forwarded here, so skip ours if it
                // already lists `Debug`.
                let debug_derive =
                    (!attrs.iter().any(derive_has_debug)).then(|| quote!(#[derive(Debug)]));
                // `#[error(transparent)]`: Display delegates to the inner
                // field instead of formatting a template.
                let display_body = if is_transparent_tpl(tpl) {
                    let sf = v
                        .source_field
                        .as_ref()
                        .expect("transparent struct variant validated to have a source field");
                    quote!(::core::fmt::Display::fmt(&self.#sf, f))
                } else {
                    quote!(::core::write!(f, #tpl #(, #fnames = self.#fnames)*))
                };
                out.extend(quote! {
                    #(#attrs)*
                    #debug_derive
                    #vis struct #vname {
                        #( #field_defs, )*
                    }

                    #(#cfgs)*
                    impl ::core::fmt::Display for #vname {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                            #display_body
                        }
                    }

                    #(#cfgs)*
                    impl ::core::error::Error for #vname {
                        #source_fn
                        #provide_fn
                    }

                    #(#cfgs)*
                    impl ::core::convert::From<#vname> for #enum_name {
                        fn from(v: #vname) -> Self {
                            Self::#vname(v)
                        }
                    }

                    #(#cfgs)*
                    impl ::core::convert::From<#vname> for ::fast_observe::Fault<#enum_name> {
                        // track_caller is load-bearing: without it fault
                        // locations stamp this macro site, not the caller.
                        #[track_caller]
                        fn from(v: #vname) -> Self {
                            ::fast_observe::Fault::new(#enum_name::#vname(v))
                        }
                    }
                });
            }
            Kind::Tuple(fields) => {
                if v.from && fields.len() == 1 {
                    let inner = &fields[0].ty;
                    // NOTE: `From<Inner> for Fault<Enum>` is NOT generated —
                    // with a foreign Inner it violates the orphan rule
                    // (`Fault` is not #[fundamental]). Propagate via
                    // `Err(inner).map_err(Enum::from)?` instead.
                    out.extend(quote! {
                        #(#cfgs)*
                        impl ::core::convert::From<#inner> for #enum_name {
                            fn from(e: #inner) -> Self {
                                Self::#vname(e)
                            }
                        }
                    });
                }
            }
            Kind::Unit => {}
        }

        // Registry entry + link-time registration for coded variants.
        if let Some(entry) = entry_expr(v) {
            let entry_ref = if matches!(v.kind, Kind::Struct(_)) {
                out.extend(quote! {
                    #(#cfgs)*
                    impl #vname {
                        #[doc = "Registry entry: code, name, category, display template, advice, module."]
                        pub const ENTRY: ::fast_observe::ErrorRegistryEntry = #entry;
                    }
                });
                quote!(#vname::ENTRY)
            } else {
                entry
            };
            out.extend(quote! {
                #(#cfgs)*
                const _: () = {
                    // No linker sections on wasm — registration is skipped
                    // there (see the platform note on `ERROR_REGISTRY`).
                    #[cfg(not(target_family = "wasm"))]
                    #[::fast_observe::__private::distributed_slice(::fast_observe::ERROR_REGISTRY)]
                    #[linkme(crate = ::fast_observe::__private::linkme)]
                    static ENTRY_ELEMENT: ::fast_observe::ErrorRegistryEntry = #entry_ref;
                };
            });
            entry_refs.push(quote!( #(#cfgs)* #entry_ref ));
        }
    }

    // ── Enum-level Display ────────────────────────────────────────────────
    let display_arms: Vec<TokenStream2> = variants
        .iter()
        .filter_map(|v| {
            let tpl = v.tpl.as_ref()?;
            let vname = &v.name;
            let cfgs = cfg_attrs(&v.forward_attrs);
            let arm = match &v.kind {
                Kind::Struct(_) => quote!(Self::#vname(v) => ::core::fmt::Display::fmt(v, f)),
                Kind::Tuple(fields) => {
                    let binds: Vec<Ident> = (0..fields.len())
                        .map(|i| Ident::new(&format!("__fo_{i}"), Span::call_site()))
                        .collect();
                    if is_transparent_tpl(tpl) {
                        // Transparent is validated single-field: bind one.
                        quote!(Self::#vname(b0) => ::core::fmt::Display::fmt(&b0, f))
                    } else {
                        quote!(Self::#vname(#(#binds),*) => ::core::write!(f, #tpl #(, #binds)*))
                    }
                }
                Kind::Unit => quote!(Self::#vname => ::core::write!(f, #tpl)),
            };
            Some(quote!( #(#cfgs)* #arm ))
        })
        .collect();
    out.extend(quote! {
        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #( #display_arms, )*
                }
            }
        }
    });

    // ── Enum-level Error ──────────────────────────────────────────────────
    let source_fn = has_source.then(|| {
        let source_arms: Vec<TokenStream2> = variants
            .iter()
            .map(|v| {
                let vname = &v.name;
                let cfgs = cfg_attrs(&v.forward_attrs);
                let arm = match &v.kind {
                    Kind::Struct(_) => {
                        quote!(Self::#vname(v) => ::core::error::Error::source(v))
                    }
                    Kind::Tuple(fields) if v.from && fields.len() == 1 => {
                        quote!(Self::#vname(e) => ::core::option::Option::Some(e))
                    }
                    Kind::Tuple(_) if v.tpl.as_ref().is_some_and(is_transparent_tpl) => {
                        // Transparent is validated single-field.
                        quote!(Self::#vname(e) => ::core::option::Option::Some(e))
                    }
                    Kind::Tuple(_) => {
                        quote!(Self::#vname(..) => ::core::option::Option::None)
                    }
                    Kind::Unit => quote!(Self::#vname => ::core::option::Option::None),
                };
                quote!( #(#cfgs)* #arm )
            })
            .collect();
        quote! {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                match self {
                    #( #source_arms, )*
                }
            }
        }
    });
    let provide_fn = any_coded.then(|| {
        let provide_arms: Vec<TokenStream2> = variants
            .iter()
            .map(|v| {
                let vname = &v.name;
                let cfgs = cfg_attrs(&v.forward_attrs);
                let arm = match &v.kind {
                    Kind::Struct(_) if v.code.is_some() => {
                        quote!(Self::#vname(v) => ::core::error::Error::provide(v, request))
                    }
                    Kind::Struct(_) => quote!(Self::#vname(_) => {}),
                    Kind::Tuple(_) if v.code.is_some() => {
                        let body = provide_body(v);
                        quote!(Self::#vname(..) => { #body })
                    }
                    Kind::Tuple(_) => quote!(Self::#vname(..) => {}),
                    Kind::Unit if v.code.is_some() => {
                        let body = provide_body(v);
                        quote!(Self::#vname => { #body })
                    }
                    Kind::Unit => quote!(Self::#vname => {}),
                };
                quote!( #(#cfgs)* #arm )
            })
            .collect();
        quote! {
            fn provide<'a>(&'a self, request: &mut ::core::error::Request<'a>) {
                match self {
                    #( #provide_arms, )*
                }
            }
        }
    });
    out.extend(quote! {
        impl ::core::error::Error for #enum_name {
            #source_fn
            #provide_fn
        }
    });

    // ── code()/category()/advice() + Coded — only when EVERY variant is
    // coded, so the methods are total without placeholder values. ──────────
    if all_coded {
        let mut code_arms = Vec::new();
        let mut category_arms = Vec::new();
        let mut advice_arms = Vec::new();
        for v in variants {
            let vname = &v.name;
            let cfgs = cfg_attrs(&v.forward_attrs);
            let pat = match &v.kind {
                Kind::Struct(_) => quote!(Self::#vname(_)),
                Kind::Tuple(_) => quote!(Self::#vname(..)),
                Kind::Unit => quote!(Self::#vname),
            };
            if let (Some(code), Some(cat)) = (&v.code, &v.category) {
                code_arms.push(quote!( #(#cfgs)* #pat => #code ));
                category_arms.push(quote!( #(#cfgs)* #pat => ::fast_observe::ErrorCategory::#cat ));
            }
            let advice = match &v.advice {
                Some(lit) => quote!(::core::option::Option::Some(#lit)),
                None => quote!(::core::option::Option::None),
            };
            advice_arms.push(quote!( #(#cfgs)* #pat => #advice ));
        }
        out.extend(quote! {
            impl #enum_name {
                /// Stable registry code, e.g. `"E100"` — the doctor/CLI lookup key.
                #[must_use]
                pub fn code(&self) -> &'static str {
                    match self {
                        #( #code_arms, )*
                    }
                }

                /// Error category — drives retry/poison policy.
                #[must_use]
                pub fn category(&self) -> ::fast_observe::ErrorCategory {
                    match self {
                        #( #category_arms, )*
                    }
                }

                /// Prescriptive advice (explicit `#[advice]`, else the first
                /// doc-comment line, else `None`).
                #[must_use]
                pub fn advice(&self) -> ::core::option::Option<&'static str> {
                    match self {
                        #( #advice_arms, )*
                    }
                }
            }

            impl ::fast_observe::errors::Coded for #enum_name {
                fn code(&self) -> &'static str {
                    self.code()
                }
                fn category(&self) -> ::fast_observe::ErrorCategory {
                    self.category()
                }
                fn advice(&self) -> ::core::option::Option<&'static str> {
                    self.advice()
                }
            }
        });
    }

    // ── ENTRIES — the wasm composition path (coded variants only). ────────
    if any_coded {
        out.extend(quote! {
            impl #enum_name {
                #[doc = "Registry entries of all coded variants — the wasm composition path \
                         (link-time `ERROR_REGISTRY` registration is cfg'd out on wasm)."]
                pub const ENTRIES: &[::fast_observe::ErrorRegistryEntry] = &[
                    #( #entry_refs, )*
                ];
            }
        });
    }

    // ── Size budget ───────────────────────────────────────────────────────
    let size_msg = Literal::string(&format!(
        "{enum_name} exceeds the {max_size}-byte error size budget \
         (override with `#[max_size = N]` on the enum)"
    ));
    out.extend(quote! {
        const _: () = ::core::assert!(
            ::core::mem::size_of::<#enum_name>() <= #max_size,
            #size_msg
        );
    });

    out
}

/// Append collected errors after `item` tokens.
fn append_errors(mut item: TokenStream2, errors: Vec<TokenStream2>) -> TokenStream2 {
    for err in errors {
        item.extend(err);
    }
    item
}

/// The `error!` entry point — see the macro's doc comment for the contract.
pub(crate) fn expand(input: TokenStream2) -> TokenStream2 {
    let mut errors: Vec<TokenStream2> = Vec::new();

    let parsed = match venial::parse_item(input.clone()) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.push(venial_error(err));
            return append_errors(input, errors);
        }
    };
    let Item::Enum(en) = parsed else {
        errors.push(error_at_tokens(
            &input,
            "`error!` expects a single enum item",
        ));
        return append_errors(input, errors);
    };

    if let Some(generics) = &en.generic_params {
        let tokens = generics.to_token_stream();
        if !tokens.is_empty() {
            errors.push(error_at_tokens(
                &tokens,
                "`error!` does not support generic enums (v1)",
            ));
        }
    }
    if let Some(where_clause) = &en.where_clause {
        errors.push(error_at_tokens(
            &where_clause.to_token_stream(),
            "`error!` does not support where clauses (v1)",
        ));
    }

    // Enum-level attributes: `#[max_size = N]` and `#[category = ...]`
    // consumed, the rest forwarded.
    let mut max_size = DEFAULT_MAX_SIZE;
    let mut enum_category: Option<Ident> = None;
    let mut enum_attrs = Vec::new();
    for attr in &en.attributes {
        if attr_is(attr, "max_size") {
            match parse_max_size(attr) {
                Ok(n) => max_size = n,
                Err(err) => errors.push(err),
            }
        } else if attr_is(attr, "category") {
            let Some(tokens) = expect_eq_value(
                attr,
                "expected `#[category = <Category>]` on the enum",
                &mut errors,
            ) else {
                continue;
            };
            match tokens.first() {
                Some(TokenTree::Ident(cat)) => {
                    if enum_category.replace(cat.clone()).is_some() {
                        errors.push(error_at(cat.span(), "duplicate enum-level `#[category]`"));
                    }
                }
                _ => errors.push(error_at_tokens(
                    &attr.to_token_stream(),
                    "expected `#[category = <Category>]` on the enum",
                )),
            }
        } else {
            enum_attrs.push(attr.clone());
        }
    }

    let variants: Vec<Variant> = en
        .variants
        .items()
        .map(|v| parse_variant(v, &mut errors, enum_category.as_ref()))
        .collect();

    // ── Cross-variant validation ──────────────────────────────────────────
    let mut seen_codes: HashMap<String, ()> = HashMap::new();
    for v in &variants {
        if let Some(code) = &v.code
            && seen_codes.insert(code.to_string(), ()).is_some()
        {
            errors.push(error_at(
                code.span(),
                &format!("duplicate error code {code} within this enum"),
            ));
        }
    }
    let mut from_types: HashMap<String, Ident> = HashMap::new();
    for v in &variants {
        if !v.from {
            continue;
        }
        if let Kind::Tuple(fields) = &v.kind
            && let Some(field) = fields.first()
            && let key = field.ty.to_token_stream().to_string()
            && let Some(first) = from_types.insert(key, v.name.clone())
        {
            errors.push(error_at(
                v.name.span(),
                &format!(
                    "multiple `#[from]` variants with the same inner type (first: \
                     `{first}`) — rustc would reject the overlapping `From` impls"
                ),
            ));
        }
    }

    let enum_def = emit_enum(&en, &enum_attrs, &variants);
    if errors.is_empty() {
        codegen(&en, &enum_attrs, max_size, &variants)
    } else {
        // Error recovery: emit the cleaned enum (macro attrs stripped) so
        // downstream code still type-checks, then every collected error.
        append_errors(enum_def, errors)
    }
}
