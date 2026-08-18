//! Explicit boundary conversions between `Fault` and foreign error types —
//! never implicit `From` (DESIGN.md §5.9). Each conversion is a deliberate
//! `map_err` point at the crate boundary; the causal tree is preserved in
//! both directions.

/// Generates one explicit `Fault` ↔ foreign-error boundary module
/// (DESIGN.md §5.9: explicit `map_err` points, never implicit `From`).
///
/// anyhow and eyre each erase their sources into their own representation
/// and expose no `&dyn Error` with `Send + Sync`, so their boundaries are
/// structurally identical: a `Debug`-deriving newtype wrapping the foreign
/// error, a delegating `Display`, an `Error` impl with `source()`
/// intentionally absent (the chain survives through formatting only), an
/// `inner()` accessor, a `#[track_caller]` `from_*` `map_err` point, and an
/// outbound `into_*` conversion.
///
/// Parameters (one boundary per instantiation):
/// - `$(#[$mod_attr])*` — attributes for the generated module (the
///   `#[cfg(feature = ...)]` gate plus the module doc comment).
/// - `module:` — the generated module name (e.g. `anyhow_boundary`).
/// - `newtype:` — the wrapper struct name (e.g. `AnyhowError`).
/// - `wrapped:` — the foreign error type being wrapped (`anyhow::Error`,
///   `eyre::Report`).
/// - `from:` — the inbound `map_err` fn name (`from_anyhow`).
/// - `into:` — the outbound conversion fn name (`into_anyhow`).
/// - `$(#[$into_attr])*` — attributes for the `into_*` fn (`#[must_use]`,
///   or none when the outbound type is already `#[must_use]`).
/// - `outbound:` — the `into_*` return type (`anyhow::Error`,
///   `eyre::Report`).
/// - `struct_doc:` — the newtype's doc comment (must document the wrapped
///   type and why `source()` is absent).
#[allow(
    unused_macros,
    reason = "every instantiation is feature-gated (anyhow-boundary, compat-eyre); the macro is unused when both are off"
)]
macro_rules! compat_wrapper {
    (
        $(#[$mod_attr:meta])*
        module: $module:ident,
        newtype: $newtype:ident,
        wrapped: $wrapped:path,
        from: $from:ident,
        into: $into:ident,
        $(#[$into_attr:meta])*
        outbound: $outbound:path,
        struct_doc: $struct_doc:literal,
    ) => {
        $(#[$mod_attr])*
        pub mod $module {
            use std::fmt;

            #[doc = $struct_doc]
            #[derive(Debug)]
            pub struct $newtype($wrapped);

            impl fmt::Display for $newtype {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    fmt::Display::fmt(&self.0, f)
                }
            }

            impl std::error::Error for $newtype {
                // source() intentionally absent: the wrapped error type
                // does not expose its inner error as a `&dyn Error` with
                // `Send + Sync`, so the chain cannot be lifted into
                // `Error::source()`. It survives through formatting only —
                // `Display` prints the top message, alternate `Display`
                // (`{:#}`) prints the cause chain, and `Debug` (`{:?}`)
                // prints the chain plus backtrace when captured. See the
                // struct docs for the per-crate specifics.
            }

            impl $newtype {
                /// Access the wrapped error type (e.g. for downcasting).
                #[must_use]
                pub fn inner(&self) -> &$wrapped {
                    &self.0
                }
            }

            /// The explicit `map_err` point (DESIGN.md §5.9). Wraps the
            /// error, preserving its message chain in the frame; the
            /// `Fault` hooks fire as for any other error construction.
            #[track_caller]
            pub fn $from(e: $wrapped) -> crate::Fault<$newtype> {
                crate::Fault::new($newtype(e))
            }

            /// The explicit outbound boundary. The causal tree survives:
            /// `Fault`'s `Error::source()` chains into the tree's
            /// first-branch frames, and its `Debug` renders the full tree.
            $(#[$into_attr])*
            pub fn $into<E: std::error::Error + Send + Sync + 'static>(
                f: crate::Fault<E>,
            ) -> $outbound {
                <$outbound>::new(f)
            }
        }
    };
}

#[cfg(feature = "anyhow-boundary")]
compat_wrapper! {
    /// anyhow ↔ `Fault` boundary (feature `anyhow-boundary`).
    ///
    /// Note: `Fault<BoxError>` (`BoxError` = `Box<dyn Error + Send +
    /// Sync>`) cannot go through [`into_anyhow`]: std's `impl<E: Error> Error
    /// for Box<E>` requires `E: Sized`, so boxed errors are not `Error` and
    /// `Fault<BoxError>` has no `Error` impl. Convert typed faults only.
    ///
    /// [`into_anyhow`]: anyhow_boundary::into_anyhow
    module: anyhow_boundary,
    newtype: AnyhowError,
    wrapped: anyhow::Error,
    from: from_anyhow,
    into: into_anyhow,
    #[must_use]
    outbound: anyhow::Error,
    struct_doc:
        "Wraps [`anyhow::Error`], preserving its message chain.\n\n\
         anyhow erases its sources into its own representation, so the \
         wrapped error exposes NO `Error::source()` — the chain survives \
         through formatting only: `Display` prints the top message, \
         alternate `Display` (`{:#}`) prints the cause chain, and `Debug` \
         (`{:?}`) prints the chain plus backtrace when captured.",
}

#[cfg(feature = "compat-eyre")]
compat_wrapper! {
    /// eyre ↔ `Fault` boundary (feature `compat-eyre`).
    ///
    /// Mirrors [`anyhow_boundary`]: eyre erases its sources into its own
    /// representation exactly like anyhow, so the same erased-chain caveat
    /// applies — and the same `Fault<BoxError>` restriction on
    /// [`into_eyre`] (typed faults only).
    ///
    /// [`into_eyre`]: eyre_boundary::into_eyre
    module: eyre_boundary,
    newtype: EyreError,
    wrapped: eyre::Report,
    from: from_eyre,
    into: into_eyre,
    outbound: eyre::Report,
    struct_doc:
        "Wraps [`eyre::Report`], preserving its message chain.\n\n\
         eyre erases its sources into its own representation, so the wrapped \
         error exposes NO `Error::source()` — the chain survives through \
         formatting only: `Display` prints the top message, alternate \
         `Display` (`{:#}`) prints the cause chain one per line, and `Debug` \
         (`{:?}`) prints the chain plus backtrace when captured. \
         [`eyre::Report::chain`] exists but yields `&dyn Error` WITHOUT \
         `Send + Sync`, so it cannot be lifted into `Error::source()` — \
         identical to the anyhow boundary.",
}

/// error-stack ↔ `Fault` boundary (feature `compat-error-stack`).
///
/// Unlike anyhow/eyre, `error_stack::Report<C>` does NOT implement
/// `std::error::Error` (0.5 offers only
/// `From<Report<C>> for Box<dyn Error + Send + Sync>`), so the inbound
/// direction wraps the report in [`ErrorStackReport`], a newtype with a
/// manual `Error` impl — the frames stay inspectable through
/// [`ErrorStackReport::inner`].
#[cfg(feature = "compat-error-stack")]
pub mod error_stack_boundary {
    use std::fmt;

    /// Wraps [`error_stack::Report<C>`], preserving the typed context `C`
    /// and the full frame stack (contexts + attachments).
    ///
    /// `Report` is not a std `Error`, so this newtype provides the `Error`
    /// impl by delegation. `source()` is intentionally absent: error-stack
    /// frames are not `&dyn Error` (contexts need only implement
    /// [`error_stack::Context`], and attachments are arbitrary values) —
    /// the frame stack is readable via [`ErrorStackReport::inner`]'s
    /// `frames()` iterator, or stringified through `Display` (every
    /// context frame, in order) / `Debug` (the full tree with
    /// attachments).
    #[derive(Debug)]
    pub struct ErrorStackReport<C: error_stack::Context>(error_stack::Report<C>);

    impl<C: error_stack::Context> fmt::Display for ErrorStackReport<C> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl<C: error_stack::Context> std::error::Error for ErrorStackReport<C> {
        // source() intentionally absent — see the struct docs.
    }

    impl<C: error_stack::Context> ErrorStackReport<C> {
        /// Access the wrapped `error_stack::Report<C>` — `frames()`,
        /// `current_context()`, `downcast_ref()`, `request_ref()`.
        #[must_use]
        pub fn inner(&self) -> &error_stack::Report<C> {
            &self.0
        }
    }

    /// `error_stack::Report<C>` → `Fault<ErrorStackReport<C>>` — the
    /// explicit `map_err` point. The context type `C` and the frame stack
    /// survive untouched inside the wrapper; the `Fault` hooks fire as for
    /// any other error construction.
    ///
    /// Note: `Report::new(context)` decomposes `context.source()` into
    /// frames, so a report built from a std error already stringified its
    /// chain; nothing further is lost here.
    #[track_caller]
    pub fn from_error_stack<C: error_stack::Context>(
        r: error_stack::Report<C>,
    ) -> crate::Fault<ErrorStackReport<C>> {
        crate::Fault::new(ErrorStackReport(r))
    }

    /// `Fault<E>` → `error_stack::Report<Fault<E>>` — the explicit
    /// outbound boundary.
    ///
    /// DEVIATION from `Report<E>`: a `Report`'s type parameter is the
    /// CURRENT context, and `Report::new` takes that context by value — a
    /// `Fault<E>` holds its `E` behind an `Arc`, so `E` cannot be
    /// extracted to become the context. The `Fault` itself becomes the
    /// context (`Fault<E>: Error + Send + Sync`, and error-stack blanket-
    /// implements `Context` for every std error). Typed access survives:
    /// `report.current_context()` is the `Fault<E>`, which `Deref`s to
    /// `E`. `Report::new` also walks `Fault`'s `Error::source()` into
    /// frames, so the first-branch causal chain lands in the report's
    /// frame stack; the full tree renders via `Debug`.
    // No #[must_use]: `error_stack::Report` is already #[must_use].
    pub fn into_error_stack<E: std::error::Error + Send + Sync + 'static>(
        f: crate::Fault<E>,
    ) -> error_stack::Report<crate::Fault<E>> {
        error_stack::Report::new(f)
    }
}
