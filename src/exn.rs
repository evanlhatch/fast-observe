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
use std::any::Any;
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

// ── Attachments — typed, inspectable data on frames ───────────────────────

/// Where an attachment appears when a fault tree is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// In the tree, under the frame. Default.
    #[default]
    Inline,
    /// Deferred to an appendix section (large payloads).
    Appendix,
    /// Counted but not shown (secrets).
    Opaque,
    /// Not shown, not counted. Programmatic only.
    Hidden,
}

/// A typed attachment on a [`Frame`]: cached display string + typed value
/// for programmatic inspection (`downcast`).
pub struct Attachment {
    key: Option<&'static str>,
    display: String,
    value: Arc<dyn Any + Send + Sync>,
    placement: Placement,
}

impl Attachment {
    /// An attachment with just a value — rendered as its display string,
    /// [`Placement::Inline`]. Used by capture hooks; see also the
    /// consuming [`Fault::attach`] API.
    ///
    /// The display string is computed once, here; the typed value stays
    /// reachable via [`Attachment::downcast`].
    #[must_use]
    pub fn new(value: impl fmt::Display + Send + Sync + 'static) -> Self {
        Self {
            key: None,
            display: value.to_string(),
            value: Arc::new(value),
            placement: Placement::Inline,
        }
    }
    /// An attachment with a key — rendered `key: value`,
    /// [`Placement::Inline`].
    #[must_use]
    pub fn with_key(key: &'static str, value: impl fmt::Display + Send + Sync + 'static) -> Self {
        Self {
            key: Some(key),
            display: value.to_string(),
            value: Arc::new(value),
            placement: Placement::Inline,
        }
    }
    /// Builder-style [`Placement`] override.
    #[must_use]
    pub fn with_placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
    /// The attachment's key, when it was attached with one.
    #[must_use]
    pub fn key(&self) -> Option<&'static str> {
        self.key
    }
    /// The display string, computed once at attach time.
    #[must_use]
    pub fn display(&self) -> &str {
        &self.display
    }
    /// Where this attachment appears when the fault tree is rendered.
    #[must_use]
    pub fn placement(&self) -> Placement {
        self.placement
    }
    /// The typed value, if the caller knows what was attached.
    #[must_use]
    pub fn downcast<T: 'static>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }
}

impl fmt::Debug for Attachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attachment")
            .field("key", &self.key)
            .field("display", &self.display)
            .field("placement", &self.placement)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for Attachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.key {
            Some(key) => write!(f, "{key}: {}", self.display),
            None => f.write_str(&self.display),
        }
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
    pub(crate) attachments: Vec<Attachment>,
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
        let mut frame = Frame {
            error,
            location,
            context,
            children,
            type_name,
            attachments: Vec::new(),
        };
        // Capture hooks run on `&mut Frame` BEFORE sharing — they may push
        // attachments. Order: capture (mutate) → share → sink-notify.
        crate::hook::run_capture_hooks(&mut frame);
        let frame = Arc::new(frame);
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
    /// Typed attachments on this frame, in attach order.
    #[must_use]
    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }
    /// The first attached value of type `T` (linear scan + downcast).
    #[must_use]
    pub fn find_attachment<T: 'static>(&self) -> Option<&T> {
        self.attachments.iter().find_map(Attachment::downcast::<T>)
    }

    /// Push an attachment onto this frame. Used by capture hooks
    /// ([`crate::hook::run_capture_hooks`]), which receive `&mut Frame`
    /// BEFORE the frame is wrapped in `Arc` — on a shared frame this is
    /// unreachable because construction completes before sharing.
    pub fn push_attachment(&mut self, attachment: Attachment) {
        self.attachments.push(attachment);
    }
    /// Pre-order iterator over this frame and all descendants (self first,
    /// then children recursively). Explicit-stack implementation, no allocation
    /// beyond the stack Vec.
    pub fn iter(self: &Arc<Frame>) -> FrameIter {
        FrameIter {
            stack: vec![Arc::clone(self)],
        }
    }
}

/// Pre-order iterator over a [`Frame`] tree — see [`Frame::iter`].
///
/// Yields `Arc<Frame>` clones (cheap refcount bumps) so callers can hold
/// frames past the iterator's borrow.
pub struct FrameIter {
    stack: Vec<Arc<Frame>>,
}

impl Iterator for FrameIter {
    type Item = Arc<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = self.stack.pop()?;
        // Push children in reverse so they pop left-to-right.
        for child in frame.children.iter().rev() {
            self.stack.push(Arc::clone(child));
        }
        Some(frame)
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
        attachments: Vec::new(),
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

/// Makes the causal tree traversable via the standard `Error::source`
/// protocol (anyhow walkers, `error.sources()`, …): the source is the FIRST
/// child frame — the direct cause — so walking `source()` descends the
/// first-branch chain, mirroring the tree's causality.
impl Error for Frame {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.children.first().map(|c| c.as_ref() as &dyn Error)
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

    /// Attach debugging data (rendered inline under the root frame).
    ///
    /// The display string is computed once, here; the typed value stays
    /// reachable via [`Fault::find_attachment`]. Same unique-root rule as
    /// [`Fault::with_context`].
    pub fn attach<A: fmt::Display + Send + Sync + 'static>(self, value: A) -> Self {
        self.attach_inner(None, value, Placement::Inline)
    }

    /// Attach debugging data with a key (rendered `key: value`).
    pub fn attach_key<A: fmt::Display + Send + Sync + 'static>(
        self,
        key: &'static str,
        value: A,
    ) -> Self {
        self.attach_inner(Some(key), value, Placement::Inline)
    }

    /// Attach debugging data with explicit [`Placement`].
    pub fn attach_placed<A: fmt::Display + Send + Sync + 'static>(
        self,
        value: A,
        placement: Placement,
    ) -> Self {
        self.attach_inner(None, value, placement)
    }

    /// The single attachment mutation path — see [`Fault::with_context`]
    /// for the shared-root rule.
    fn attach_inner<A: fmt::Display + Send + Sync + 'static>(
        mut self,
        key: Option<&'static str>,
        value: A,
        placement: Placement,
    ) -> Self {
        let attachment = match key {
            Some(key) => Attachment::with_key(key, value).with_placement(placement),
            None => Attachment::new(value).with_placement(placement),
        };
        if let Some(frame) = Arc::get_mut(&mut self.root) {
            frame.attachments.push(attachment);
        } else {
            debug_assert!(
                Arc::strong_count(&self.root) == 1,
                "attach on a shared Fault root — attachment would be lost"
            );
        }
        self
    }

    /// The root frame's typed attachment lookup.
    #[must_use]
    pub fn find_attachment<T: 'static>(&self) -> Option<&T> {
        self.root.find_attachment::<T>()
    }

    /// The current context on the root frame.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.root.context
    }

    /// The deepest frame in the tree — the root cause. For a chain
    /// A→B→C this is C's frame. With branching children, the deepest
    /// FIRST-branch frame (depth-first, ties broken toward earlier children).
    #[must_use]
    pub fn root_cause(&self) -> Arc<Frame> {
        let mut frame = Arc::clone(&self.root);
        while let Some(first) = frame.children().first() {
            frame = Arc::clone(first);
        }
        frame
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

/// Frame accessors that need no `E: Error` bound — kept outside the
/// `E: Error` impl block so `Fault<SimpleError>` (`Box<dyn Error>` does not
/// itself implement `Error`) and [`FaultCollection`] can use them.
impl<E: Send + Sync + Sized + 'static> Fault<E> {
    /// The root frame (shared via Arc).
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.root
    }

    /// Pre-order iterator over every frame in the causal tree, starting at the root.
    pub fn iter(&self) -> FrameIter {
        self.root.iter()
    }

    /// Consume the fault and return the shared root frame.
    ///
    /// [`FaultCollection`] collects faults of any `E` — only the frames are kept.
    #[must_use]
    pub fn into_frame(self) -> Arc<Frame> {
        self.root
    }
}

impl<E: Send + Sync + Sized + 'static> Deref for Fault<E> {
    type Target = E;
    fn deref(&self) -> &E {
        // No downcast: the typed error is stored alongside the root frame.
        &self.error
    }
}

impl<E: Error + Send + Sync + Sized + 'static> Error for Fault<E> {
    /// Chains into the causal TREE, not into `E`'s own source: the root
    /// frame's children already encode `E`'s source chain (captured at
    /// construction) plus raised/wrapped contexts, and [`Fault::deref`]
    /// covers the typed-`E` view. `Error::source` walkers therefore see the
    /// same structure that `Debug` renders.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.root.source()
    }
}

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

// ── FaultCollection — multi-failure aggregation ────────────────────────────

/// Collects multiple faults (retry attempts, batch failures) into one tree.
/// Stores the collected faults' root frames — cheap `Arc` moves, no re-capture
/// and no extra hook firings.
#[derive(Default)]
pub struct FaultCollection {
    frames: Vec<Arc<Frame>>,
}

impl FaultCollection {
    /// An empty collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect one fault's root frame.
    pub fn push<E: Send + Sync + Sized + 'static>(&mut self, fault: Fault<E>) {
        self.frames.push(fault.into_frame());
    }

    /// Number of collected faults.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True when no faults were collected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Wrap all collected failures under a new error: one [`Fault`] whose
    /// root has every collected fault's root as a child (after `err`'s own
    /// captured source chain, if any).
    ///
    /// The fresh root is uniquely owned, so the merge always applies.
    #[track_caller]
    pub fn into_fault<T: Error + Send + Sync + Sized + 'static>(self, err: T) -> Fault<T> {
        let mut fault = Fault::new(err);
        if let Some(root) = Arc::get_mut(&mut fault.root) {
            root.children.extend(self.frames);
        }
        fault
    }

    /// Same as [`FaultCollection::into_fault`] but the root is a plain
    /// message (`Fault<SimpleError>`).
    #[track_caller]
    pub fn into_fault_msg(self, msg: impl Into<Cow<'static, str>>) -> Fault {
        let mut fault = Fault::from_boxed(internal_err(msg));
        if let Some(root) = Arc::get_mut(&mut fault.root) {
            root.children.extend(self.frames);
        }
        fault
    }
}

/// Collect `Err` values straight into a [`FaultCollection`] — any `E`, only
/// the frames are kept:
///
/// ```
/// # use core::fmt;
/// # use fast_observe::exn::{Fault, FaultCollection};
/// # #[derive(Debug)] struct BatchErr;
/// # impl fmt::Display for BatchErr {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("batch") }
/// # }
/// # impl std::error::Error for BatchErr {}
/// let results: Vec<Result<i32, Fault<BatchErr>>> = vec![Ok(1), Err(Fault::new(BatchErr))];
/// let failures = results
///     .into_iter()
///     .filter_map(Result::err)
///     .collect::<FaultCollection>();
/// assert_eq!(failures.len(), 1);
/// ```
impl<E: Send + Sync + Sized + 'static> FromIterator<Fault<E>> for FaultCollection {
    fn from_iter<I: IntoIterator<Item = Fault<E>>>(iter: I) -> Self {
        let mut collection = Self::new();
        collection
            .frames
            .extend(iter.into_iter().map(Fault::into_frame));
        collection
    }
}

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

    /// Attach debugging data when Err. On Ok the value passes through untouched.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<Self::Error>)` carrying the attachment on the root
    /// frame when `self` is `Err`.
    #[track_caller]
    fn attach<A: fmt::Display + Send + Sync + 'static>(
        self,
        value: A,
    ) -> Result<Self::Success, Self::Error>;

    /// Lazy form of [`ResultExt::attach`] — the closure runs ONLY on Err
    /// (Ok pays nothing).
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<Self::Error>)` carrying the attachment on the root
    /// frame when `self` is `Err`.
    #[track_caller]
    fn attach_with<A: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> A,
    ) -> Result<Self::Success, Self::Error>;

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
    #[cold]
    fn attach<A: fmt::Display + Send + Sync + 'static>(self, value: A) -> Result<T, E> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Fault::new(e).attach(value)),
        }
    }

    #[track_caller]
    #[cold]
    fn attach_with<A: fmt::Display + Send + Sync + 'static>(
        self,
        f: impl FnOnce() -> A,
    ) -> Result<T, E> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Fault::new(e).attach(f())),
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
            attachments: Vec::new(),
        })];
    }
    children
}

impl<E: Send + Sync + Sized + 'static> fmt::Debug for Fault<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_fault(f, &self.root, "")
    }
}

/// Render the fault tree: one line per frame (`error, at file:line:col
/// [context]`), children nested under `|-- `/``-- `` connectors. Inline
/// attachments render as leading pseudo-children (`* {display}` lines)
/// BEFORE real children, sharing the sibling last-ness — the last line under
/// a frame (attachment or child) gets ``-- ``, the rest `|-- `. Non-inline
/// attachments are not rendered in-tree; the frame's own line gets a
/// ` (+N more attachments)` suffix when N > 0.
fn write_fault(f: &mut fmt::Formatter<'_>, frame: &Frame, prefix: &str) -> fmt::Result {
    write!(f, "{}", frame.error)?;
    let loc = frame.location;
    write!(f, ", at {}:{}:{}", loc.file(), loc.line(), loc.column())?;
    if !matches!(frame.context, Context::None) {
        write!(f, " [{}]", frame.context)?;
    }
    let inline_count = frame
        .attachments
        .iter()
        .filter(|a| a.placement == Placement::Inline)
        .count();
    let deferred = frame.attachments.len() - inline_count;
    if deferred > 0 {
        write!(f, " (+{deferred} more attachments)")?;
    }
    // Inline attachments are leading pseudo-children: they share the
    // sibling last-ness with real children.
    let total = inline_count + frame.children.len();
    for (i, attachment) in frame
        .attachments
        .iter()
        .filter(|a| a.placement == Placement::Inline)
        .enumerate()
    {
        if i + 1 == total {
            write!(f, "\n{prefix}`-- ")?;
        } else {
            write!(f, "\n{prefix}|-- ")?;
        }
        write!(f, "* {attachment}")?;
    }
    for (i, child) in frame.children.iter().enumerate() {
        let last = inline_count + i + 1 == total;
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
