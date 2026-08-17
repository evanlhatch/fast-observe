//! Exception type with causal tree + typed context.
//!
//! `Fault<E>` wraps error `E` + causal tree. Reflexive `From<E> for Fault<E>`.
//! Context attachment via `with_context`. Cross-type via `change_context`.
//!
//! How `?` works: `From<E>` covers `Result<T, E>` → `Result<T, Fault<E>>`.
//! Cross-type needs `.change_context(EngineError::from)?`.

use core::error::Error;
use core::fmt;
use core::ops::Deref;
use core::panic::Location;
use parking_lot::Mutex;
use sealed::sealed;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

// ── Error counter — every constructed error frame, keyed by type ───────────

static ERROR_COUNTS: LazyLock<Mutex<HashMap<&'static str, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Increment the counter for an error frame's type. Cold path — mutex is fine.
#[cold]
pub(crate) fn record_error(type_name: &'static str) {
    *ERROR_COUNTS.lock().entry(type_name).or_insert(0) += 1;
    // Feature `metrics-facade`: mirror the counter through the `metrics`
    // facade so exporters (Prometheus, OTel, …) see error construction too.
    #[cfg(feature = "metrics-facade")]
    metrics::counter!("fast_observe.errors", "type" => type_name).increment(1);
}

/// Snapshot of per-error-type construction counts, sorted by count descending.
/// Includes the `"reported"` key — errors explicitly swallowed via
/// [`ResultExt::report`].
#[must_use]
pub fn error_counts() -> Vec<(&'static str, u64)> {
    let mut v: Vec<(&'static str, u64)> =
        ERROR_COUNTS.lock().iter().map(|(k, c)| (*k, *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    v
}

/// A boxed error — the default `E` for `Fault` / `Result`.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;
pub type SimpleError = BoxError;

// ── Context — generic, reusable error context ──────────────────────────────

/// Typed context attached to an error frame.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    derive_more::Display,
    documented::DocumentedVariants,
    strum::EnumIter,
)]
#[non_exhaustive]
pub enum Context {
    /// No context attached (default).
    #[default]
    None,
    /// A named scope — function, module, operation, or phase name.
    #[display("{_0}")]
    Scope(Cow<'static, str>),
    /// A numeric tick/iteration.
    #[display("tick {_0}")]
    Tick(u64),
    /// A named entity + tick (e.g. object name + iteration).
    #[display("{_0} at tick {_1}")]
    Entity(Cow<'static, str>, u64),
    /// Free-form context for application-specific use.
    #[display("{_0}")]
    Custom(Cow<'static, str>),
}

impl Context {
    #[must_use]
    pub fn scope(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Scope(name.into())
    }
    #[must_use]
    pub const fn tick(s: u64) -> Self {
        Self::Tick(s)
    }
    #[must_use]
    pub fn entity(name: impl Into<Cow<'static, str>>, tick: u64) -> Self {
        Self::Entity(name.into(), tick)
    }
    #[must_use]
    pub fn custom(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Custom(msg.into())
    }
}

// ── Frame — one node in the causal tree ────────────────────────────────────

/// A frame in the fault causal tree.
/// Frame tree for crash dumps and doctor-style tooling. Full detail belongs
/// in the app's journal/log.
///
/// Fields are crate-private: every frame is born in [`Frame::capture`] (root
/// frames — hook + counter fan-out) or [`frame_from_error`] (child frames).
/// Hand-constructed frames would bypass the invariants the report relies on.
#[derive(Debug)]
pub struct Frame {
    pub(crate) error: BoxError,
    pub(crate) location: &'static Location<'static>,
    pub(crate) context: Context,
    pub(crate) children: Vec<Arc<Frame>>,
    pub(crate) type_name: &'static str,
}

impl Frame {
    /// The single root-frame construction path: auto scope context, nested
    /// source chain, hook + counter fan-out. Every `Fault` root is born here
    /// — exactly one hook firing per constructed frame.
    #[cold]
    fn capture(
        error: BoxError,
        type_name: &'static str,
        location: &'static Location<'static>,
    ) -> Arc<Frame> {
        let context = crate::profiling::current_scope_name().map_or(Context::None, Context::Scope);
        let children = walk_sources(&*error, location);
        let frame = Arc::new(Frame {
            error,
            location,
            context,
            children,
            type_name,
        });
        crate::hook::invoke(&frame);
        frame
    }

    /// The error at this frame.
    #[must_use]
    pub fn error(&self) -> &(dyn Error + Send + Sync + 'static) {
        &*self.error
    }
    /// Where this frame was created (`#[track_caller]`).
    #[must_use]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
    /// Typed context — what was happening when the error occurred.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }
    /// Child frames (the cause chain + raised contexts).
    #[must_use]
    pub fn children(&self) -> &[Arc<Frame>] {
        &self.children
    }
    /// The type name of the error (for doctor-style debugging).
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

/// A child frame for an error entering the tree via wrap/context APIs —
/// nested source chain, NO hook: the hook fires at the root's capture
/// (children are causes, not new constructions).
#[cold]
fn frame_from_error(
    error: BoxError,
    type_name: &'static str,
    location: &'static Location<'static>,
) -> Arc<Frame> {
    let children = walk_sources(&*error, location);
    Arc::new(Frame {
        error,
        location,
        context: Context::None,
        children,
        type_name,
    })
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;
        if !matches!(self.context, Context::None) {
            write!(f, " ({})", self.context)?;
        }
        Ok(())
    }
}

// ── Fault — the exception type ─────────────────────────────────────────────

/// An exception carrying a causal tree of [`Frame`]s.
#[must_use = "a Fault is an error — return it, handle it, or swallow it explicitly via `ResultExt::report`"]
pub struct Fault<E: Send + Sync + Sized + 'static = SimpleError> {
    root: Arc<Frame>,
    /// The typed error — the `Deref` target. Shared with the root frame via a
    /// delegating wrapper, so deref never downcasts (and can never panic).
    error: Arc<E>,
}

/// Boxed wrapper delegating to a shared typed error. Lets `Frame.error`
/// (a `BoxError`) and `Fault::deref` (the typed `E`) share one value.
#[derive(Debug)]
struct SharedError<E: Error + Send + Sync + 'static>(Arc<E>);

impl<E: Error + Send + Sync + 'static> fmt::Display for SharedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<E: Error + Send + Sync + 'static> Error for SharedError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

/// Same as [`SharedError`] but for `BoxError`, which does not itself
/// implement `Error` (std's blanket `impl<E: Error> Error for Box<E>`
/// requires `E: Sized`).
#[derive(Debug)]
struct SharedBoxedError(Arc<BoxError>);

impl fmt::Display for SharedBoxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Error for SharedBoxedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl<E: Error + Send + Sync + Sized + 'static> From<E> for Fault<E> {
    #[track_caller]
    fn from(error: E) -> Self {
        Fault::new(error)
    }
}

impl Fault<SimpleError> {
    /// Create a `Fault<SimpleError>` from a boxed error.
    #[track_caller]
    #[cold]
    pub fn from_boxed(error: BoxError) -> Self {
        Self::capture_boxed(error, Location::caller())
    }

    /// The single boxed construction path — see [`Frame::capture`].
    #[cold]
    fn capture_boxed(error: BoxError, location: &'static Location<'static>) -> Self {
        let type_name = std::any::type_name_of_val(&*error);
        let error = Arc::new(error);
        let root = Frame::capture(
            Box::new(SharedBoxedError(Arc::clone(&error))),
            type_name,
            location,
        );
        Self { root, error }
    }
}

impl From<&str> for Fault<SimpleError> {
    #[track_caller]
    fn from(msg: &str) -> Self {
        Self::from_boxed(internal_err(msg.to_string()))
    }
}

impl From<String> for Fault<SimpleError> {
    #[track_caller]
    fn from(msg: String) -> Self {
        Self::from_boxed(internal_err(msg))
    }
}

impl<E: Error + Send + Sync + Sized + 'static> Fault<E> {
    /// Create a new fault wrapping `error`, capturing the caller's location.
    #[track_caller]
    #[cold]
    pub fn new(error: E) -> Self {
        Self::capture_typed(error, Location::caller())
    }

    /// The single typed construction path — see [`Frame::capture`].
    #[cold]
    fn capture_typed(error: E, location: &'static Location<'static>) -> Self {
        let error = Arc::new(error);
        let root = Frame::capture(
            Box::new(SharedError(Arc::clone(&error))),
            std::any::type_name::<E>(),
            location,
        );
        Self { root, error }
    }

    /// Attach typed [`Context`] to this fault's root frame.
    ///
    /// The root frame must be uniquely owned (the normal case: you just
    /// constructed the fault or received it by value). On a shared root the
    /// context would be silently dropped — that is a bug, and debug builds
    /// say so.
    pub fn with_context(mut self, ctx: Context) -> Self {
        if let Some(frame) = Arc::get_mut(&mut self.root) {
            frame.context = ctx;
        } else {
            debug_assert!(
                Arc::strong_count(&self.root) == 1,
                "with_context on a shared Fault root — context would be lost"
            );
        }
        self
    }

    /// The current context on the root frame.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.root.context
    }

    /// The root frame (shared via Arc).
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.root
    }

    /// Consume the fault and return the shared root frame.
    #[must_use]
    pub fn into_frame(self) -> Arc<Frame> {
        self.root
    }

    /// Wrap this fault in another error, preserving the root cause chain.
    ///
    /// The hook fires for the NEW root frame (a newly constructed frame —
    /// its type is counted), but not again for the original fault: it was
    /// counted at its own construction.
    #[track_caller]
    #[cold]
    pub fn wrap<T: Error + Send + Sync + Sized + 'static>(self, err: T) -> Fault<T> {
        let mut fault = Fault::capture_typed(err, Location::caller());
        if let Some(root) = Arc::get_mut(&mut fault.root) {
            root.children.push(self.root);
        }
        fault
    }
}

impl<E: Send + Sync + Sized + 'static> Deref for Fault<E> {
    type Target = E;
    fn deref(&self) -> &E {
        // No downcast: the typed error is stored alongside the root frame.
        &self.error
    }
}

impl<E: Error + Send + Sync + Sized + 'static> Error for Fault<E> {}

impl<E: Send + Sync + Sized + 'static> fmt::Display for Fault<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root.error)?;
        if !matches!(self.root.context, Context::None) {
            write!(f, " ({})", self.root.context)?;
        }
        Ok(())
    }
}

// ── Result type alias ──────────────────────────────────────────────────────

/// `Result<T, Fault<E>>` — the standard return type.
pub type Result<T, E = SimpleError> = core::result::Result<T, Fault<E>>;

// ── ErrorExt — fluent raise on error types ─────────────────────────────────

#[sealed]
pub trait ErrorExt: Error + Send + Sync + Sized + 'static {
    #[track_caller]
    fn raise(self) -> Fault<Self> {
        Fault::new(self)
    }
}
impl<T: Error + Send + Sync + Sized + 'static> __seal_error_ext::Sealed for T {}
impl<T: Error + Send + Sync + Sized + 'static> ErrorExt for T {}

// ── ResultExt — context attachment + cross-type conversion ─────────────────

#[sealed]
pub trait ResultExt {
    type Success;
    type Error: Error + Send + Sync + Sized + 'static;

    /// Change the error context: convert `Err(E)` to `Err(Fault<A>)`.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<A>)` wrapping the original error when `self` is `Err`.
    #[track_caller]
    fn change_context<A>(self, new_err: A) -> Result<Self::Success, A>
    where
        A: Error + Send + Sync + Sized + 'static;

    /// Convert any error to `Fault<SimpleError>` with a message.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<SimpleError>)` with `msg` when `self` is `Err`.
    #[track_caller]
    fn wrap_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<Self::Success>;

    /// Observe this error through the full observability stack (profiling span
    /// + structured log), then return `Result<T, Fault<E>>` — full type preservation.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<E>)` carrying the observation message as context
    /// when `self` is `Err`.
    #[track_caller]
    fn observed(
        self,
        msg: impl Into<Cow<'static, str>>,
    ) -> core::result::Result<Self::Success, Fault<Self::Error>>;

    /// The one blessed swallow: report the error and continue with `None`.
    ///
    /// The error hook already fired at construction (the error is logged once,
    /// at its origin); `report` bumps the `"reported"` counter in
    /// [`error_counts`] and logs a warning with the caller's location marking
    /// where the error was deliberately dropped. Use instead of `let _ =
    /// result;` or `.ok()?` — swallowed errors stay greppable and counted.
    #[track_caller]
    fn report(self, msg: impl Into<Cow<'static, str>>) -> Option<Self::Success>;
}

impl<T, E: Error + Send + Sync + Sized + 'static> __seal_result_ext::Sealed
    for core::result::Result<T, E>
{
}
impl<T, E: Error + Send + Sync + Sized + 'static> ResultExt for core::result::Result<T, E> {
    type Success = T;
    type Error = E;

    #[track_caller]
    #[cold]
    fn change_context<A>(self, new_err: A) -> Result<T, A>
    where
        A: Error + Send + Sync + Sized + 'static,
    {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Fault::new(e).wrap(new_err)),
        }
    }

    #[track_caller]
    #[cold]
    fn wrap_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<T> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                let location = Location::caller();
                // The original error becomes a child frame — WITH its nested
                // source chain (previously dropped). No hook for the child:
                // it fires at the root's capture below.
                let child = frame_from_error(Box::new(e), std::any::type_name::<E>(), location);
                let mut fault = Fault::capture_boxed(internal_err(msg), location);
                if let Some(root) = Arc::get_mut(&mut fault.root) {
                    root.children.push(child);
                }
                Err(fault)
            }
        }
    }

    #[track_caller]
    fn observed(self, msg: impl Into<Cow<'static, str>>) -> core::result::Result<T, Fault<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = msg.into();
                // The original error stays the ROOT — the message rides in
                // the context, and the nested source chain stays as children
                // (both via the single capture path).
                let mut fault = Fault::capture_typed(e, Location::caller());
                let scope = crate::profiling::current_scope_name().unwrap_or(Cow::Borrowed(""));
                let context = if scope.is_empty() {
                    Context::Custom(msg)
                } else {
                    Context::Custom(format!("{scope}: {msg}").into())
                };
                if let Some(root) = Arc::get_mut(&mut fault.root) {
                    root.context = context;
                }
                Err(fault)
            }
        }
    }

    #[track_caller]
    fn report(self, msg: impl Into<Cow<'static, str>>) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                record_error("reported");
                let location = Location::caller();
                log::warn!(
                    target: "fast_observe.error",
                    "{}: {} (reported at {}:{})",
                    msg.into(),
                    e,
                    location.file(),
                    location.line(),
                );
                None
            }
        }
    }
}

// ── OptionExt — raise on None ──────────────────────────────────────────────

#[sealed]
pub trait OptionExt {
    type Some;

    /// # Errors
    ///
    /// Returns `Err(Fault<SimpleError>)` with `msg` when `self` is `None`.
    #[track_caller]
    fn ok_or_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<Self::Some>;
}

impl<T> __seal_option_ext::Sealed for Option<T> {}
impl<T> OptionExt for Option<T> {
    type Some = T;

    #[track_caller]
    fn ok_or_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<T> {
        match self {
            Some(v) => Ok(v),
            None => Err(Fault::from_boxed(internal_err(msg))),
        }
    }
}

/// A simple internal error for `ok_or_msg` and manual Frame construction.
#[derive(Debug)]
pub struct InternalError(Cow<'static, str>);

fn internal_err(msg: impl Into<Cow<'static, str>>) -> BoxError {
    Box::new(InternalError(msg.into()))
}
impl fmt::Display for InternalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for InternalError {}

// ── Macros ────────────────────────────────────────────────────────────────

/// Bail with an error. Two forms:
///
/// ```ignore
/// bail!("something: {x}");                    // → Fault<SimpleError>
/// bail!(Internal, "something: {x}");           // → Fault<Internal> with format! detail
/// bail!(Internal { detail: "...".into() });    // → Fault<Internal> from struct
/// ```
#[macro_export]
macro_rules! bail {
    ($type:ident, $fmt:literal $(, $arg:expr)* $(,)?) => {{
        return ::core::result::Result::Err($crate::exn::Fault::from(
            $type { detail: format!($fmt $(, $arg)*) }
        ));
    }};
    ($err:expr) => {{ return ::core::result::Result::Err($crate::exn::Fault::from($err)); }};
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $err:expr $(,)?) => {{
        if !($cond) {
            $crate::bail!($err)
        }
    }};
}

// ── Source chain walking ───────────────────────────────────────────────────

/// Walk `error.source()` into a NESTED child chain: the direct source is
/// the single child, its source is that child's child, and so on — the tree
/// mirrors causality instead of flattening it into siblings of the root.
///
/// Source errors are stringified into [`InternalError`] (std only lends
/// `&dyn Error`); the original `type_name` is preserved per frame. No hook
/// fires for these frames — they are causes, not new constructions.
#[cold]
fn walk_sources(error: &dyn Error, location: &'static Location<'static>) -> Vec<Arc<Frame>> {
    let mut chain = Vec::new();
    let mut source = error.source();
    while let Some(src) = source {
        chain.push((std::any::type_name_of_val(src), src.to_string()));
        source = src.source();
    }
    // Fold from the deepest cause outward into a nested chain.
    let mut children = Vec::new();
    for (type_name, msg) in chain.into_iter().rev() {
        children = vec![Arc::new(Frame {
            error: Box::new(InternalError(msg.into())),
            location,
            context: Context::None,
            children,
            type_name,
        })];
    }
    children
}

impl<E: Send + Sync + Sized + 'static> fmt::Debug for Fault<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_fault(f, &self.root, "")
    }
}

fn write_fault(f: &mut fmt::Formatter<'_>, frame: &Frame, prefix: &str) -> fmt::Result {
    write!(f, "{}", frame.error)?;
    let loc = frame.location;
    write!(f, ", at {}:{}:{}", loc.file(), loc.line(), loc.column())?;
    if !matches!(frame.context, Context::None) {
        write!(f, " [{}]", frame.context)?;
    }
    for (i, child) in frame.children.iter().enumerate() {
        let last = i == frame.children.len() - 1;
        let next_prefix = if last {
            write!(f, "\n{prefix}`-- ")?;
            format!("{prefix}    ")
        } else {
            write!(f, "\n{prefix}|-- ")?;
            format!("{prefix}|   ")
        };
        write_fault(f, child, &next_prefix)?;
    }
    Ok(())
}
