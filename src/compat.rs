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
