//! Explicit boundary conversions between `Fault` and foreign error types —
//! never implicit `From` (DESIGN.md §5.9). Each conversion is a deliberate
//! `map_err` point at the crate boundary; the causal tree is preserved in
//! both directions.

/// anyhow ↔ `Fault` boundary (feature `anyhow-boundary`).
///
/// Note: `Fault<SimpleError>` (`SimpleError` = `Box<dyn Error + Send +
/// Sync>`) cannot go through [`into_anyhow`]: std's `impl<E: Error> Error
/// for Box<E>` requires `E: Sized`, so boxed errors are not `Error` and
/// `Fault<SimpleError>` has no `Error` impl. Convert typed faults only.
///
/// [`into_anyhow`]: anyhow_boundary::into_anyhow
#[cfg(feature = "anyhow-boundary")]
pub mod anyhow_boundary {
    use std::fmt;

    /// Wraps [`anyhow::Error`], preserving its message chain.
    ///
    /// anyhow erases its sources into its own representation, so the
    /// wrapped error exposes NO `Error::source()` — the chain survives
    /// through formatting only: `Display` prints the top message,
    /// alternate `Display` (`{:#}`) prints the cause chain, and `Debug`
    /// (`{:?}`) prints the chain plus backtrace when captured.
    #[derive(Debug)]
    pub struct AnyhowError(anyhow::Error);

    impl fmt::Display for AnyhowError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl std::error::Error for AnyhowError {
        // source() intentionally absent: anyhow::Error does not expose its
        // inner error as &dyn Error. The chain is readable via {:#} / {:?}.
    }

    impl AnyhowError {
        /// Access the wrapped `anyhow::Error` (e.g. for downcasting).
        #[must_use]
        pub fn inner(&self) -> &anyhow::Error {
            &self.0
        }
    }

    /// `anyhow::Error` → `Fault<AnyhowError>` — the explicit `map_err`
    /// point. Wraps the error, preserving its message chain in the frame;
    /// the `Fault` hooks fire as for any other error construction.
    #[track_caller]
    pub fn from_anyhow(e: anyhow::Error) -> crate::Fault<AnyhowError> {
        crate::Fault::new(AnyhowError(e))
    }

    /// `Fault<E>` → `anyhow::Error`. The causal tree survives: `Fault`'s
    /// `Error::source()` chains into the tree's first-branch frames, and
    /// its `Debug` renders the full tree.
    #[must_use]
    pub fn into_anyhow<E: std::error::Error + Send + Sync + 'static>(
        f: crate::Fault<E>,
    ) -> anyhow::Error {
        anyhow::Error::new(f)
    }
}

/// eyre ↔ `Fault` boundary (feature `compat-eyre`).
///
/// Mirrors [`anyhow_boundary`]: eyre erases its sources into its own
/// representation exactly like anyhow, so the same erased-chain caveat
/// applies — and the same `Fault<SimpleError>` restriction on
/// [`into_eyre`] (typed faults only).
///
/// [`into_eyre`]: eyre_boundary::into_eyre
#[cfg(feature = "compat-eyre")]
pub mod eyre_boundary {
    use std::fmt;

    /// Wraps [`eyre::Report`], preserving its message chain.
    ///
    /// eyre erases its sources into its own representation, so the wrapped
    /// error exposes NO `Error::source()` — the chain survives through
    /// formatting only: `Display` prints the top message, alternate
    /// `Display` (`{:#}`) prints the cause chain one per line, and `Debug`
    /// (`{:?}`) prints the chain plus backtrace when captured.
    /// [`eyre::Report::chain`] exists but yields `&dyn Error` WITHOUT
    /// `Send + Sync`, so it cannot be lifted into `Error::source()` —
    /// identical to the anyhow boundary.
    #[derive(Debug)]
    pub struct EyreError(eyre::Report);

    impl fmt::Display for EyreError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl std::error::Error for EyreError {
        // source() intentionally absent: eyre::Report does not expose its
        // inner error as &dyn Error + Send + Sync. The chain is readable
        // via {:#} / {:?}.
    }

    impl EyreError {
        /// Access the wrapped `eyre::Report` (e.g. for downcasting).
        #[must_use]
        pub fn inner(&self) -> &eyre::Report {
            &self.0
        }
    }

    /// `eyre::Report` → `Fault<EyreError>` — the explicit `map_err`
    /// point. Wraps the report, preserving its message chain in the frame;
    /// the `Fault` hooks fire as for any other error construction.
    #[track_caller]
    pub fn from_eyre(e: eyre::Report) -> crate::Fault<EyreError> {
        crate::Fault::new(EyreError(e))
    }

    /// `Fault<E>` → `eyre::Report`. The causal tree survives: `Fault`'s
    /// `Error::source()` chains into the tree's first-branch frames, and
    /// its `Debug` renders the full tree.
    // No #[must_use]: `eyre::Report` is already #[must_use].
    pub fn into_eyre<E: std::error::Error + Send + Sync + 'static>(
        f: crate::Fault<E>,
    ) -> eyre::Report {
        eyre::Report::new(f)
    }
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
