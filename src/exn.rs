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
    pub fn scope(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Scope(name.into())
    }
    #[must_use]
    pub const fn tick(s: u64) -> Self {
        Self::Tick(s)
    }
    pub fn entity(name: impl Into<Cow<'static, str>>, tick: u64) -> Self {
        Self::Entity(name.into(), tick)
    }
    pub fn custom(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Custom(msg.into())
    }
}

// ── Frame — one node in the causal tree ────────────────────────────────────

/// A frame in the fault causal tree.
/// Frame tree for crash dumps and doctor-style tooling. Full detail belongs
/// in the app's journal/log.
#[derive(Debug)]
pub struct Frame {
    /// The error at this frame.
    pub error: BoxError,
    /// Where this frame was created (`#[track_caller]`).
    pub location: &'static Location<'static>,
    /// Typed context — what was happening when the error occurred.
    pub context: Context,
    /// Child frames (the cause chain + raised contexts).
    pub children: Vec<Arc<Frame>>,
    /// The type name of the error (for doctor-style debugging).
    pub type_name: &'static str,
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
        let location = Location::caller();
        let context = crate::profiling::current_scope_name().map_or(Context::None, Context::Scope);
        let type_name = std::any::type_name_of_val(&*error);
        let error = Arc::new(error);
        let frame = Arc::new(Frame {
            error: Box::new(SharedBoxedError(Arc::clone(&error))),
            location,
            context,
            children: Vec::new(),
            type_name,
        });
        crate::hook::invoke(&frame);
        Self { root: frame, error }
    }
}

impl From<&str> for Fault<SimpleError> {
    #[track_caller]
    fn from(msg: &str) -> Self {
        Self::from_boxed(Box::new(InternalError(msg.to_string().into())))
    }
}

impl From<String> for Fault<SimpleError> {
    #[track_caller]
    fn from(msg: String) -> Self {
        Self::from_boxed(Box::new(InternalError(msg.into())))
    }
}

impl<E: Error + Send + Sync + Sized + 'static> Fault<E> {
    /// Create a new fault wrapping `error`, capturing the caller's location.
    #[track_caller]
    #[cold]
    pub fn new(error: E) -> Self {
        let location = Location::caller();
        let type_name = std::any::type_name::<E>();
        let children = walk_sources(&error, location);
        let context = crate::profiling::current_scope_name().map_or(Context::None, Context::Scope);
        let error = Arc::new(error);
        let frame = Arc::new(Frame {
            error: Box::new(SharedError(Arc::clone(&error))),
            location,
            context,
            children,
            type_name,
        });
        crate::hook::invoke(&frame);
        Self { root: frame, error }
    }

    /// Attach typed [`Context`] to this fault's root frame.
    pub fn with_context(mut self, ctx: Context) -> Self {
        if let Some(frame) = Arc::get_mut(&mut self.root) {
            frame.context = ctx;
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
    #[track_caller]
    #[cold]
    pub fn wrap<T: Error + Send + Sync + Sized + 'static>(self, err: T) -> Fault<T> {
        let location = Location::caller();
        let type_name = std::any::type_name::<T>();
        let children = walk_sources(&err, location);
        let context = crate::profiling::current_scope_name().map_or(Context::None, Context::Scope);
        let error = Arc::new(err);
        let mut frame = Arc::new(Frame {
            error: Box::new(SharedError(Arc::clone(&error))),
            location,
            context,
            children,
            type_name,
        });
        // Attach the original fault as a child — no hook invocation here,
        // the hook was already fired when the original Fault was created.
        if let Some(f) = Arc::get_mut(&mut frame) {
            f.children.push(self.root);
        }
        Fault { root: frame, error }
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
                let msg = msg.into();
                let context =
                    crate::profiling::current_scope_name().map_or(Context::None, Context::Scope);
                let error: Arc<BoxError> = Arc::new(internal_err(msg));
                let frame = Arc::new(Frame {
                    error: Box::new(SharedBoxedError(Arc::clone(&error))),
                    location: Location::caller(),
                    context,
                    children: vec![Arc::new(Frame {
                        error: Box::new(e),
                        location: Location::caller(),
                        context: Context::None,
                        children: Vec::new(),
                        type_name: std::any::type_name::<E>(),
                    })],
                    type_name: "InternalError",
                });
                crate::hook::invoke(&frame);
                Err(Fault { root: frame, error })
            }
        }
    }

    #[track_caller]
    fn observed(self, msg: impl Into<Cow<'static, str>>) -> core::result::Result<T, Fault<E>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = msg.into();
                let location = Location::caller();
                let children = walk_sources(&e, location);
                let scope = crate::profiling::current_scope_name().unwrap_or(Cow::Borrowed(""));
                let context = if scope.is_empty() {
                    Context::Custom(msg)
                } else {
                    Context::Custom(format!("{scope}: {msg}").into())
                };
                // Store the original error as the root — the message rides in
                // the context, and the source chain stays as children.
                let error = Arc::new(e);
                let frame = Arc::new(Frame {
                    error: Box::new(SharedError(Arc::clone(&error))),
                    location,
                    context,
                    children,
                    type_name: std::any::type_name::<E>(),
                });
                crate::hook::invoke(&frame);
                Err(Fault { root: frame, error })
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
        if !bool::from($cond) {
            $crate::bail!($err)
        }
    }};
}

// ── Source chain walking ───────────────────────────────────────────────────

#[cold]
fn walk_sources(error: &dyn Error, location: &'static Location<'static>) -> Vec<Arc<Frame>> {
    let mut children = Vec::new();
    let mut source = error.source();
    while let Some(src) = source {
        let type_name = std::any::type_name_of_val(src);
        children.push(Arc::new(Frame {
            error: Box::new(InternalError(src.to_string().into())),
            location,
            context: Context::None,
            children: Vec::new(),
            type_name,
        }));
        source = src.source();
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
        if last {
            write!(f, "\n{prefix}`-- ")?;
        } else {
            write!(f, "\n{prefix}|-- ")?;
        }
        write_fault(f, child, &format!("{prefix}    "))?;
    }
    Ok(())
}
