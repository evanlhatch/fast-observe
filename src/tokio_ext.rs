//! tokio boundary: `JoinError` → `Fault` with cancelled/panic distinguished
//! (feature `int-tokio`).
//!
//! KNOWN FEATURE GAP: `tokio::task::JoinError` is gated behind tokio's `rt`
//! feature, and `int-tokio` only enables `dep:tokio` with
//! `default-features = false` (Cargo.toml is frozen for this change). This
//! module therefore compiles only when some other feature/dependency unifies
//! tokio's `rt` on (e.g. `http` via reqwest/hyper under `--all-features`).
//! `cargo check --features int-tokio` alone fails inside tokio, not here.

use std::fmt;

use sealed::sealed;
use tokio::task::JoinError;

/// Why a joined task failed — the typed distinction on [`JoinTaskError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinFailureKind {
    /// The task was cancelled (its `JoinHandle` was aborted or the runtime
    /// shut down).
    Cancelled,
    /// The task panicked.
    Panicked,
}

impl fmt::Display for JoinFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Panicked => f.write_str("panic"),
        }
    }
}

/// A failed task join: which task, how it failed, and the panic payload
/// (when recoverable as a string).
///
/// The `Display` message names the task and the failure kind: "task NAME
/// cancelled" or "task NAME panicked: PAYLOAD".
#[derive(Debug)]
pub struct JoinTaskError {
    task: &'static str,
    kind: JoinFailureKind,
    payload: Option<String>,
}

impl JoinTaskError {
    /// The task name passed to [`ObserveJoinExt::observe_join`].
    #[must_use]
    pub fn task(&self) -> &'static str {
        self.task
    }

    /// Cancelled vs panicked.
    #[must_use]
    pub const fn kind(&self) -> JoinFailureKind {
        self.kind
    }

    /// The panic payload stringified (`&str`/`String` downcasts only),
    /// `None` for cancellations and non-string payloads.
    #[must_use]
    pub fn payload(&self) -> Option<&str> {
        self.payload.as_deref()
    }
}

impl fmt::Display for JoinTaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            JoinFailureKind::Cancelled => write!(f, "task `{}` cancelled", self.task),
            JoinFailureKind::Panicked => {
                write!(f, "task `{}` panicked: ", self.task)?;
                f.write_str(self.payload.as_deref().unwrap_or("unknown panic payload"))
            }
        }
    }
}

impl std::error::Error for JoinTaskError {}

/// Extract the panic payload as a string (`&str`/`String` downcasts;
/// anything else → `None`, rendered as "unknown panic payload").
fn panic_payload(e: JoinError) -> Option<String> {
    let payload = e.into_panic();
    crate::exn::payload_str(&payload).map(str::to_owned)
}

/// Extension on the output of `JoinHandle::await`:
/// `join_handle.await.observe_join("task-name")?`.
///
/// Cancelled and panicked tasks become [`crate::Fault<JoinTaskError>`] with
/// the distinction carried twice: typed in [`JoinTaskError::kind`]
/// (reachable via `Fault`'s `Deref`) and as string attachments on the root frame
/// (`task: <name>`, `join: cancelled|panic`).
///
/// The error type is [`JoinTaskError`], not the `BoxError` default:
/// `Fault<BoxError>` has no `Error` impl (`Box<dyn Error>` is unsized),
/// which `Fault::attach_key` requires. Convert with
/// `ResultExt::context` when a `Fault<BoxError>` is needed.
///
/// ```no_run
/// use fast_observe::tokio_ext::ObserveJoinExt;
///
/// async fn drive(
///     handle: tokio::task::JoinHandle<u32>,
/// ) -> fast_observe::Result<u32, fast_observe::tokio_ext::JoinTaskError> {
///     handle.await.observe_join("worker")
/// }
/// ```
///
/// NOTE: the doctest is `no_run` and the crate has no runtime test for the
/// mapping — `int-tokio` does not enable tokio's `rt` feature (needed even
/// to CONSTRUCT a `JoinError`), and dev-dependencies cannot add features.
/// A real spawn/panic/abort test lands when a consumer enables `tokio/rt`.
///
/// Sealed: implemented only for `Result<T, JoinError>` (the output of
/// `JoinHandle::await`) — not intended for user implementation.
#[sealed]
pub trait ObserveJoinExt<T> {
    /// Map `Err(JoinError)` into a `Fault<JoinTaskError>` distinguishing
    /// cancellation from panic; `Ok` passes through untouched.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<JoinTaskError>)` when the task was cancelled or
    /// panicked.
    #[track_caller]
    fn observe_join(self, task: &'static str) -> crate::Result<T, JoinTaskError>;
}

impl<T> __seal_observe_join_ext::Sealed<T> for Result<T, JoinError> {}
impl<T> ObserveJoinExt<T> for Result<T, JoinError> {
    #[track_caller]
    #[cold]
    fn observe_join(self, task: &'static str) -> crate::Result<T, JoinTaskError> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                let (kind, payload) = if e.is_cancelled() {
                    (JoinFailureKind::Cancelled, None)
                } else if e.is_panic() {
                    (JoinFailureKind::Panicked, panic_payload(e))
                } else {
                    // JoinError is only ever cancelled or panicked today;
                    // keep the Display string as a forward-compat fallback.
                    (JoinFailureKind::Panicked, Some(e.to_string()))
                };
                let kind_str = kind.to_string();
                Err(crate::Fault::new(JoinTaskError {
                    task,
                    kind,
                    payload,
                })
                .attach_key("task", task)
                .attach_key("join", kind_str))
            }
        }
    }
}
