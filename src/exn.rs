//! Exception type with causal tree + typed context.
//!
//! `Fault<E>` wraps error `E` + causal tree. Reflexive `From<E> for Fault<E>`.
//! Context attachment via `set_context`. Cross-type via `change_context`.
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
use std::io::Write as _;
use std::sync::{Arc, LazyLock};

// ── Error counter — every constructed error frame, keyed by type ───────────

static ERROR_COUNTS: LazyLock<Mutex<HashMap<&'static str, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Counter key for errors deliberately swallowed via [`ResultExt::report`] —
/// lives in the same namespace as type names but is a bare word, so it can
/// never collide with a `module::path::Type` key.
pub(crate) const REPORTED_KEY: &str = "reported";

/// Increment the counter for an error frame's type. Cold path — mutex is fine.
#[cold]
pub(crate) fn record_error(type_name: &'static str) {
    *ERROR_COUNTS.lock().entry(type_name).or_insert(0) += 1;
}

/// Feature `metrics-facade`: mirror the error-construction counter through
/// the `metrics` facade so exporters (Prometheus, `OTel`, …) see it too.
/// Called AFTER the counts lock is released — a slow exporter must never
/// stall the error hot path.
#[cfg(feature = "metrics-facade")]
#[cold]
pub(crate) fn record_error_metrics(type_name: &'static str) {
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

/// Snapshot of error counts grouped by category, sorted by count descending
/// (ties by category name, the uncategorized `None` bucket first on ties).
///
/// [`error_counts`] is keyed by type name — the category is NOT stored at
/// record time (the counter sees only a type name, never an error instance).
/// It is resolved HERE, at read time, from the registry: an error type counts
/// under category C when the registry holds an entry whose variant name
/// equals the type's trailing `::` path segment (e.g. `my_crate::errors::Boom`
/// matches entry `name: "Boom"`). Types with no matching entry — plain
/// errors, [`InternalError`], the `"reported"` key — land in the `None`
/// ("uncategorized") bucket.
///
/// Heuristic caveats: same-named types in different modules share a bucket,
/// and on wasm the registry is empty so EVERYTHING is uncategorized.
#[must_use]
pub fn error_counts_by_category() -> Vec<(Option<crate::ErrorCategory>, u64)> {
    let counts = ERROR_COUNTS.lock();
    let mut buckets: HashMap<Option<crate::ErrorCategory>, u64> = HashMap::new();
    for (type_name, count) in counts.iter() {
        *buckets
            .entry(category_for_type_name(type_name))
            .or_insert(0) += count;
    }
    let mut v: Vec<(Option<crate::ErrorCategory>, u64)> = buckets.into_iter().collect();
    v.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| category_key(a.0).cmp(category_key(b.0)))
    });
    v
}

/// Read-time category resolution for [`error_counts_by_category`]: match the
/// type's trailing path segment against registry entry names (see its docs
/// for the heuristic).
fn category_for_type_name(type_name: &str) -> Option<crate::ErrorCategory> {
    let leaf = type_name.rsplit("::").next().unwrap_or(type_name);
    crate::errors::error_registry()
        .find(|entry| entry.name == leaf)
        .map(|entry| entry.category)
}

/// Sort key for the category bucket: uncategorized (`None`) sorts first.
fn category_key(category: Option<crate::ErrorCategory>) -> &'static str {
    category.map_or("", Into::into)
}

/// A boxed error — the default `E` for `Fault` / `Result`.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

// ── Context — generic, reusable error context ──────────────────────────────

/// Typed context attached to an error frame.
#[derive(Debug, Clone, PartialEq, Eq, Default, derive_more::Display)]
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
    /// Named-scope context — a function, module, operation, or phase name
    /// (the [`Context::Scope`] variant).
    #[must_use]
    pub fn scope(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Scope(name.into())
    }
    /// Numeric tick/iteration context (the [`Context::Tick`] variant).
    #[must_use]
    pub const fn tick(s: u64) -> Self {
        Self::Tick(s)
    }
    /// Named-entity + tick context — e.g. object name + iteration (the
    /// [`Context::Entity`] variant).
    #[must_use]
    pub fn entity(name: impl Into<Cow<'static, str>>, tick: u64) -> Self {
        Self::Entity(name.into(), tick)
    }
    /// Free-form context for application-specific use (the
    /// [`Context::Custom`] variant).
    #[must_use]
    pub fn custom(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Custom(msg.into())
    }
}

// ── FrameKind — the causal relationship on a parent→child edge ─────────────

/// Why a frame sits under its parent. Stored on the EDGE (alongside each
/// child in [`Frame::child_edges`]), not on the frame itself: a frame's
/// meaning is fixed, but the same shared frame could be referenced under
/// different relationships.
///
/// The report renders these as distinct labels (`cause` / `original` /
/// `attempt` / `failure`) so a reader can tell "the underlying OS error"
/// from "retry attempt 2".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum FrameKind {
    /// A cause from the parent's `Error::source()` chain (stringified).
    Source,
    /// The original error a `wrap`/`context` overlay was raised over.
    Wrap,
    /// A failed attempt collected by [`retry_with_backoff`].
    Attempt,
    /// A failure merged in by [`FaultCollection`].
    Batch,
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

/// Built-in attachment keys — the single source of truth linking the
/// capture hooks in [`crate::hook`] (producers) to [`crate::report`]
/// (consumer). A typo in a bare `"scope_path"` literal on either side would
/// compile and silently drop a report section; the enum makes the linkage
/// compile-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinKey {
    /// Profiling scope path (`outer → … → leaf`) at fault time.
    ScopePath,
    /// Leaf scope's elapsed milliseconds at fault time.
    ScopeElapsedMs,
    /// Current fastrace trace id (feature `fastrace`).
    TraceId,
    /// Recent finished-span trail breadcrumb (feature `instant`).
    SpanTrail,
    /// Captured backtrace (feature `backtrace`, env-gated).
    Backtrace,
}

impl BuiltinKey {
    /// The wire key used in [`Attachment::key`] and report rendering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScopePath => "scope_path",
            Self::ScopeElapsedMs => "scope_elapsed_ms",
            Self::TraceId => "trace_id",
            Self::SpanTrail => "span_trail",
            Self::Backtrace => "backtrace",
        }
    }
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
    ///
    /// Keys must be non-empty and free of `\n`, `\r`, `=` — the report
    /// renders `attachment: key=value`, one fact per line, so those bytes
    /// are protocol-breaking (debug builds assert; release builds render
    /// the key as-is).
    #[must_use]
    pub fn with_key(key: &'static str, value: impl fmt::Display + Send + Sync + 'static) -> Self {
        debug_assert!(
            !key.is_empty() && !key.contains(['\n', '\r', '=']),
            "attachment key {key:?} breaks the report's one-fact-per-line contract"
        );
        Self {
            key: Some(key),
            ..Self::new(value)
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
    /// Children WITH their causal edge kind — see [`FrameKind`].
    pub(crate) children: Vec<(FrameKind, Arc<Frame>)>,
    pub(crate) type_name: &'static str,
    pub(crate) attachments: Vec<Attachment>,
}

impl Frame {
    /// Crate-private constructor — the only place a `Frame` is built. `context`
    /// and `children` are computed by the caller; attachments start empty (only
    /// capture hooks may push, and they run BEFORE the frame is shared).
    fn new(
        error: BoxError,
        type_name: &'static str,
        location: &'static Location<'static>,
        context: Context,
        children: Vec<(FrameKind, Arc<Frame>)>,
    ) -> Frame {
        Frame {
            error,
            location,
            context,
            children,
            type_name,
            attachments: Vec::new(),
        }
    }

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
        let mut frame = Frame::new(error, type_name, location, context, children);
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
    /// Child frames (the cause chain + raised contexts), without the edge
    /// kinds — see [`Frame::child_edges`].
    pub fn children(
        &self,
    ) -> impl ExactSizeIterator<Item = &Arc<Frame>> + DoubleEndedIterator + '_ {
        self.children.iter().map(|(_, frame)| frame)
    }

    /// Child frames WITH their causal edge kind — whether each child is a
    /// `source()` cause, a wrapped original, a retry attempt, or a batch
    /// merge. The report renders these as distinct labels.
    pub fn child_edges(
        &self,
    ) -> impl ExactSizeIterator<Item = (FrameKind, &Arc<Frame>)> + DoubleEndedIterator + '_ {
        self.children.iter().map(|(kind, frame)| (*kind, frame))
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

    /// The first attached value of type `T` in this frame's SUBTREE, in
    /// preorder (self first, then children depth-first).
    #[must_use]
    pub fn find_attachment_tree<T: 'static>(&self) -> Option<&T> {
        fn walk<T: 'static>(frame: &Frame) -> Option<&T> {
            frame.find_attachment::<T>().or_else(|| {
                frame
                    .children
                    .iter()
                    .find_map(|(_, child)| walk::<T>(child))
            })
        }
        walk::<T>(self)
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
    #[must_use]
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
        for (_, child) in frame.children.iter().rev() {
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
    Arc::new(Frame::new(
        error,
        type_name,
        location,
        Context::None,
        children,
    ))
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
        self.children.first().map(|(_, c)| c.as_ref() as &dyn Error)
    }

    /// Forward the wrapped error's provided data, and expose the frame's own
    /// structured metadata through the same channel (DESIGN.md §11c
    /// "capability ceiling") — generic middleware that knows nothing about
    /// fast-observe can `request_ref::<Context>` / `request_ref::<Frame>` /
    /// `request_ref::<&str>` (the type name) / `request_ref::<Location>` on
    /// any `&dyn Error` the tree surfaces.
    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        self.error.provide(request);
        request.provide_ref::<Context>(&self.context);
        request.provide_ref::<Location>(self.location);
        request.provide_ref::<&'static str>(&self.type_name);
        request.provide_ref::<Frame>(self);
    }
}

// ── Fault — the exception type ─────────────────────────────────────────────

/// An exception carrying a causal tree of [`Frame`]s.
#[must_use = "a Fault is an error — return it, handle it, or swallow it explicitly via `ResultExt::report`"]
pub struct Fault<E: Send + Sync + Sized + 'static = BoxError> {
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

    // Forward provide() so codes/categories/attachments the inner error
    // offers stay visible through the delegating wrapper.
    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        self.0.provide(request);
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

    fn provide<'a>(&'a self, request: &mut core::error::Request<'a>) {
        self.0.provide(request);
    }
}

impl<E: Error + Send + Sync + Sized + 'static> From<E> for Fault<E> {
    #[track_caller]
    fn from(error: E) -> Self {
        Fault::new(error)
    }
}

impl Fault<BoxError> {
    /// Create a `Fault<BoxError>` from a boxed error.
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

impl From<&str> for Fault<BoxError> {
    #[track_caller]
    fn from(msg: &str) -> Self {
        Self::from_boxed(internal_err(msg.to_string()))
    }
}

impl From<String> for Fault<BoxError> {
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
}

/// Frame accessors that need no `E: Error` bound — kept outside the
/// `E: Error` impl block so `Fault<BoxError>` (`Box<dyn Error>` does not
/// itself implement `Error`) and [`FaultCollection`] can use them.
impl<E: Send + Sync + Sized + 'static> Fault<E> {
    /// The root frame (shared via Arc).
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.root
    }

    /// Pre-order iterator over every frame in the causal tree, starting at the root.
    /// `&Fault` also implements `IntoIterator` (yielding the same frames).
    #[must_use]
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

    /// The fault's policy, from the root error's [`crate::errors::CategoryTag`]
    /// (provided through `Error::provide`) or a registry lookup by the
    /// provided [`crate::errors::ErrorCode`]. `None` when the error isn't coded.
    #[must_use]
    pub fn policy(&self) -> Option<crate::Policy> {
        frame_category(&self.root).map(crate::ErrorCategory::policy)
    }

    /// sysexits-style process exit code from the root error's category:
    /// Content → `EX_DATAERR` (65), Transient → `EX_TEMPFAIL` (75),
    /// Invariant/Fatal → `EX_SOFTWARE` (70), uncoded → 1.
    ///
    /// Category resolution is the same as [`Fault::policy`].
    #[must_use]
    pub fn exit_code(&self) -> std::process::ExitCode {
        std::process::ExitCode::from(self.exit_code_raw())
    }

    /// The sysexits code as a raw `u8` — shared by [`Fault::exit_code`]
    /// (the `Termination` value) and [`Fault::exit_with_report`] (the
    /// `process::exit` argument).
    fn exit_code_raw(&self) -> u8 {
        /// sysexits.h `EX_DATAERR` — the input was wrong (Content).
        const EX_DATAERR: u8 = 65;
        /// sysexits.h `EX_SOFTWARE` — internal software error (Invariant/Fatal).
        const EX_SOFTWARE: u8 = 70;
        /// sysexits.h `EX_TEMPFAIL` — temporary failure, retry later (Transient).
        const EX_TEMPFAIL: u8 = 75;
        /// No sysexits equivalent — generic failure for uncoded errors.
        const EX_GENERAL: u8 = 1;
        match frame_category(&self.root) {
            Some(crate::ErrorCategory::Content) => EX_DATAERR,
            Some(crate::ErrorCategory::Transient) => EX_TEMPFAIL,
            // Invariant/Fatal — plus any future category defaults to a
            // generic software error rather than "success-adjacent" codes.
            Some(_) => EX_SOFTWARE,
            None => EX_GENERAL,
        }
    }
    /// Attach typed [`Context`] to this fault's root frame.
    ///
    /// The root frame must be uniquely owned (the normal case: you just
    /// constructed the fault or received it by value). On a shared root the
    /// context would be silently dropped — that is a bug, and debug builds
    /// say so.
    pub fn set_context(mut self, ctx: Context) -> Self {
        if let Some(frame) = Arc::get_mut(&mut self.root) {
            frame.context = ctx;
        } else {
            debug_assert!(
                Arc::strong_count(&self.root) == 1,
                "set_context on a shared Fault root — context would be lost"
            );
        }
        self
    }

    /// Attach debugging data (rendered inline under the root frame).
    ///
    /// The display string is computed once, here; the typed value stays
    /// reachable via [`Fault::find_attachment`]. Same unique-root rule as
    /// [`Fault::set_context`].
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

    /// The single attachment mutation path — see [`Fault::set_context`]
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

    /// The first attached value of type `T` anywhere in the causal tree
    /// (preorder: root first, then children depth-first). Rootcause-style
    /// inspection: retry metadata, partial state, or a breadcrumb trail
    /// attached on a CHILD frame stays findable.
    #[must_use]
    pub fn find_attachment_tree<T: 'static>(&self) -> Option<&T> {
        self.root.find_attachment_tree::<T>()
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
        loop {
            // Clone the child Arc before reassigning — the borrow must end
            // first.
            let Some(first) = frame.children().next().cloned() else {
                break;
            };
            frame = first;
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
            root.children.push((FrameKind::Wrap, self.root));
        }
        fault
    }
}

/// `fn main() -> Fault<E>` — the exit path IS the report (SURFACE.md §6a):
/// the report renders to stderr and the process exits with the category's
/// sysexits code ([`Fault::exit_code`]) instead of the generic `Error: {:?}`
/// line `Result`'s `Termination` impl produces.
///
/// ```no_run
/// # use fast_observe::Fault;
/// # fn run() -> Fault { Fault::from("boom") }
/// fn main() -> Fault {
///     run()
/// }
/// ```
///
/// For the `fn main() -> Result<(), E>` form (the common case), use
/// `#[fast_observe::main]` — the attribute wraps the body and routes the
/// `Err` through this exact path.
impl<E: Send + Sync + Sized + 'static> std::process::Termination for Fault<E> {
    fn report(self) -> std::process::ExitCode {
        self.exit_with_report()
    }
}

impl<E: Send + Sync + Sized + 'static> Fault<E> {
    /// Print the full report to stderr, then exit with the fault's sysexits
    /// category code ([`Fault::exit_code`]). `-> !` so it can terminate a
    /// `fn main` body from any arm of a match.
    ///
    /// Shared by the `Termination for Fault<E>` impl (the `fn main() -> Fault`
    /// exit path) and `#[fast_observe::main]`'s `Err` arm retargeting — one
    /// render, one exit-code mapping, two entry points.
    pub fn exit_with_report(self) -> ! {
        // Infallible on stderr; ignore a closed stderr (e.g. daemonized).
        let _ = std::io::stderr().write_fmt(format_args!("{}", crate::report_display(&self)));
        // Flush before `process::exit` skips destructors: pending fastrace
        // spans and buffered log records are exactly what you need when the
        // process dies of this fault. Best-effort, no-op without the
        // features.
        crate::flush();
        log::logger().flush();
        std::process::exit(i32::from(self.exit_code_raw()))
    }
}

/// A frame's category: a provided [`crate::errors::CategoryTag`] first, then
/// a registry lookup by the provided [`crate::errors::ErrorCode`]. `None`
/// when neither source knows one. The single resolution through the
/// `Error::provide` channel — `report.rs` and `diagnostic.rs` call this.
pub(crate) fn frame_category(frame: &Frame) -> Option<crate::ErrorCategory> {
    if let Some(tag) = core::error::request_value::<crate::errors::CategoryTag>(frame.error()) {
        return Some(tag.0);
    }
    core::error::request_value::<crate::errors::ErrorCode>(frame.error())
        .and_then(|code| crate::errors::lookup_error(code.0))
        .map(|entry| entry.category)
}

/// Run `f`, retrying while failures have [`crate::Policy::Retry`] (see
/// [`Fault::policy`]), collecting attempts into one [`Fault`] on exhaustion.
/// Sync; sleeping between attempts is the caller's concern (wasm-safe by
/// construction).
///
/// `max_attempts` is the TOTAL attempt count (clamped to at least 1 — `f`
/// always runs once). Uncoded errors (policy `None`) and non-Retry policies
/// return immediately with the fault unchanged. On exhaustion the earlier
/// attempts are merged under the FINAL attempt's fault — the typed `E` is
/// preserved, so [`FaultCollection::into_fault_msg`] (which erases `E` to
/// [`BoxError`]) cannot be used; the exhaustion message
/// `"{label}: failed after {n} attempts"` rides in the final fault's
/// [`Context::Custom`] instead of a new root, and the collected attempts
/// become children of its root frame.
///
/// # Errors
///
/// Returns `Err(Fault<E>)` with the first failure when the policy is not
/// [`crate::Policy::Retry`], or one fault wrapping all attempts after
/// `max_attempts` Retry-policy failures.
/// The sleep policy between retry attempts ([`retry_with_backoff`]).
/// `sleep` is always injected by the caller (never called here) — wasm-safe
/// by construction, per DESIGN.md §11b's verdict-table rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backoff {
    /// No delay between attempts (`retry_with_policy` behavior).
    None,
    /// A fixed delay between attempts.
    Fixed(std::time::Duration),
    /// Exponential backoff: `base * factor^(attempt-1)`, capped at `max`.
    Exponential {
        /// First delay.
        base: std::time::Duration,
        /// Multiplier per attempt.
        factor: u32,
        /// Upper cap on any single delay.
        max: std::time::Duration,
    },
}

impl Backoff {
    /// Maximum exponent applied in [`Backoff::Exponential`] — caps the
    /// `factor^exp` product so the saturating arithmetic below can never
    /// overflow even with `factor = u32::MAX` and an unbounded attempt count.
    const MAX_EXPONENT: usize = 20;

    /// The delay sequence as a pure, side-effect-free iterator — attempt
    /// 1 first, one item per attempt, `None` meaning "no delay". Separating
    /// the POLICY (this) from the MECHANISM (sleep + retry loop in
    /// [`retry_with_backoff`]) makes the schedule unit-testable without
    /// sleeping.
    pub fn schedule(self) -> impl Iterator<Item = Option<std::time::Duration>> {
        (1..).map(move |attempt| self.delay(attempt))
    }

    #[must_use]
    fn delay(&self, attempt: usize) -> Option<std::time::Duration> {
        match self {
            Self::None => None,
            Self::Fixed(d) => Some(*d),
            Self::Exponential { base, factor, max } => {
                // factor < 2 would not back OFF — coerced to the minimum
                // sensible multiplier.
                let factor = u128::from((*factor).max(2));
                // attempt is 1-based; the first retry uses base. The exponent
                // is capped so the product cannot overflow u128.
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "the exponent is capped at MAX_EXPONENT, far below u32::MAX"
                )]
                let exp = (attempt - 1).min(Self::MAX_EXPONENT) as u32;
                let mult = factor.saturating_pow(exp);
                let delay_ns = base.as_nanos().saturating_mul(mult);
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "value is clamped to u64::MAX immediately before the cast"
                )]
                let delay =
                    std::time::Duration::from_nanos(delay_ns.min(u128::from(u64::MAX)) as u64);
                Some(delay.min(*max))
            }
        }
    }
}

/// Run `f`, retrying while failures have [`crate::Policy::Retry`] with a
/// caller-supplied `sleep` between attempts (see [`retry_with_policy`] for
/// the shared collection semantics; the `#[track_caller]` location is the
/// same for both).
///
/// `sleep` runs ONLY between retryable attempts — the final attempt (or the
/// first non-Retry failure) sleeps nothing. `sleep` is never called on wasm
/// by this crate; the app decides what a `Duration` means there
/// (gloo-timers, a no-op, …).
///
/// # Errors
///
/// Same contract as [`retry_with_policy`].
#[track_caller]
pub fn retry_with_backoff<T, E, F, S>(
    label: &'static str,
    max_attempts: usize,
    backoff: Backoff,
    mut sleep: S,
    mut f: F,
) -> Result<T, E>
where
    E: Error + Send + Sync + Sized + 'static,
    F: FnMut() -> core::result::Result<T, E>,
    S: FnMut(std::time::Duration),
{
    let max_attempts = max_attempts.max(1);
    let mut collection = FaultCollection::new();
    let mut schedule = backoff.schedule();
    let mut attempts = 0usize;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                let mut fault = Fault::new(e);
                attempts += 1;
                if fault.policy() == Some(crate::Policy::Retry) && attempts < max_attempts {
                    collection.push(fault);
                    // The schedule iterator is infinite — `None` here is
                    // the ITEM (no delay); the iterator itself never ends.
                    let delay = schedule.next().unwrap_or(None);
                    if let Some(delay) = delay {
                        sleep(delay);
                    }
                    continue;
                }
                if !collection.is_empty()
                    && let Some(root) = Arc::get_mut(&mut fault.root)
                {
                    root.context = Context::Custom(
                        format!("{label}: failed after {attempts} attempts").into(),
                    );
                    root.children.extend(
                        collection
                            .frames
                            .drain(..)
                            .map(|frame| (FrameKind::Attempt, frame)),
                    );
                }
                return Err(fault);
            }
        }
    }
}

/// See [`retry_with_backoff`] with [`Backoff::None`] — no sleeping.
///
/// # Errors
///
/// Same contract as [`retry_with_backoff`].
#[track_caller]
pub fn retry_with_policy<T, E, F>(label: &'static str, max_attempts: usize, f: F) -> Result<T, E>
where
    E: Error + Send + Sync + Sized + 'static,
    F: FnMut() -> core::result::Result<T, E>,
{
    retry_with_backoff(label, max_attempts, Backoff::None, |_| {}, f)
}

impl<E: Send + Sync + Sized + 'static> IntoIterator for &Fault<E> {
    type Item = Arc<Frame>;
    type IntoIter = FrameIter;

    fn into_iter(self) -> FrameIter {
        self.iter()
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
pub type Result<T, E = BoxError> = core::result::Result<T, Fault<E>>;

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
            root.children
                .extend(self.frames.into_iter().map(|f| (FrameKind::Batch, f)));
        }
        fault
    }

    /// Same as [`FaultCollection::into_fault`] but the root is a plain
    /// message (`Fault<BoxError>`).
    #[track_caller]
    pub fn into_fault_msg(self, msg: impl Into<Cow<'static, str>>) -> Fault {
        let mut fault = Fault::from_boxed(internal_err(msg));
        if let Some(root) = Arc::get_mut(&mut fault.root) {
            root.children
                .extend(self.frames.into_iter().map(|f| (FrameKind::Batch, f)));
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

/// Fluent raise for any error type: `err.raise()` wraps it in a [`Fault`].
///
/// Blanket-implemented for every error type satisfying the fault contract
/// (`Error + Send + Sync + Sized + 'static`); the trait is sealed, so
/// downstream crates cannot add impls.
#[diagnostic::on_unimplemented(
    message = "implement `ErrorExt` via `error!` or on your own error type",
    note = "ErrorExt is sealed; it is implemented for faults and error!-generated types"
)]
#[sealed]
pub trait ErrorExt: Error + Send + Sync + Sized + 'static {
    /// Wrap this error in a [`Fault`], capturing the caller's location.
    ///
    /// The single fluent-raise verb — [`Fault::new`] with `#[track_caller]`.
    #[track_caller]
    fn raise(self) -> Fault<Self> {
        Fault::new(self)
    }
}
impl<T: Error + Send + Sync + Sized + 'static> __seal_error_ext::Sealed for T {}
impl<T: Error + Send + Sync + Sized + 'static> ErrorExt for T {}

// ── ResultExt — context attachment + cross-type conversion ─────────────────

/// Context attachment + cross-type conversion for `Result<T, E>`.
///
/// Blanket-implemented for every `Result<T, E>` whose `E` satisfies the
/// fault contract (`Error + Send + Sync + Sized + 'static`); the trait is
/// sealed, so these verbs are the single idiom for moving a plain `Result`
/// into the fault contract.
#[diagnostic::on_unimplemented(
    message = "`ResultExt` methods are available on `fast_observe::Result` and `Result<T, E>` where E implements the fault contract",
    note = "if you are calling `.report()`/`.wrap_msg()` on a plain std Result, convert with `Fault::new`/`error!` first"
)]
#[sealed]
pub trait ResultExt {
    /// The `Ok` payload type — passed through untouched on success.
    type Success;
    /// The error type converted into [`Fault`] by these methods.
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

    /// Convert any error to `Fault<BoxError>` with a message (anyhow's verb).
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<BoxError>)` with `msg` when `self` is `Err`.
    #[track_caller]
    fn context(self, msg: impl Into<Cow<'static, str>>) -> Result<Self::Success>;

    /// Lazy form of [`ResultExt::context`] — the closure runs ONLY on Err
    /// (Ok pays nothing; anyhow's `with_context`).
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<BoxError>)` with `f()` when `self` is `Err`.
    #[track_caller]
    fn with_context(self, f: impl FnOnce() -> Cow<'static, str>) -> Result<Self::Success>;

    /// The anyhow-verb alias for [`ResultExt::context`]. Identical semantics:
    /// `Err` becomes `Fault<BoxError>` with `msg` as the root message.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<BoxError>)` with `msg` when `self` is `Err`.
    #[track_caller]
    fn wrap_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<Self::Success>;

    /// Attach a message as [`Context::Custom`](Context::Custom) while
    /// PRESERVING the typed error `Self::Error` — the flatland-observe
    /// `observed` verb. The message renders as `error (msg)` in Display/
    /// reports, but `Fault::deref` still reaches the original `E`.
    ///
    /// # Errors
    ///
    /// Returns `Err(Fault<Self::Error>)` carrying the message context on the
    /// root frame when `self` is `Err`.
    #[track_caller]
    fn observed(self, msg: impl Into<Cow<'static, str>>) -> Result<Self::Success, Self::Error>;

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
    fn context(self, msg: impl Into<Cow<'static, str>>) -> Result<T> {
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
                    root.children.push((FrameKind::Wrap, child));
                }
                Err(fault)
            }
        }
    }

    #[track_caller]
    #[cold]
    fn with_context(self, f: impl FnOnce() -> Cow<'static, str>) -> Result<T> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e).context(f()),
        }
    }

    #[track_caller]
    #[cold]
    fn wrap_msg(self, msg: impl Into<Cow<'static, str>>) -> Result<T> {
        self.context(msg)
    }

    #[track_caller]
    #[cold]
    fn observed(self, msg: impl Into<Cow<'static, str>>) -> Result<T, E> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(Fault::new(e).set_context(Context::Custom(msg.into()))),
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
                record_error(REPORTED_KEY);
                let location = Location::caller();
                log::warn!(
                    target: crate::log_targets::ERROR,
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

/// Raise on `None` for any [`Option`], producing a [`Fault`].
///
/// Blanket-implemented for every `Option<T>`; the trait is sealed, so no
/// downstream impls. [`OptionExt::ok_or_msg`] is the single verb.
#[diagnostic::on_unimplemented(
    message = "`OptionExt` is sealed and implemented for `Option<T>`",
    note = "use `Option::ok_or`/`ok_or_else` for a plain Option"
)]
#[sealed]
pub trait OptionExt {
    /// The `Some` payload type.
    type Some;

    /// # Errors
    ///
    /// Returns `Err(Fault<BoxError>)` with `msg` when `self` is `None`.
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

/// Extract a panic payload as a string slice (`&str`/`String` downcasts
/// only; anything else → `None`). Shared by the panic hook
/// ([`crate::deploy`]) and the tokio join boundary
/// ([`crate::tokio_ext`]) — one downcast chain, two `Any` containers.
pub(crate) fn payload_str<'a>(payload: &'a (dyn Any + Send + 'static)) -> Option<&'a str> {
    payload
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
}

/// A simple internal error for `ok_or_msg`, `bail!`-style message errors,
/// and stringified source-chain frames.
///
/// Public (and re-exported at the crate root) so callers can name it —
/// e.g. downcast a [`Fault`]'s root error or match the error type produced
/// by [`OptionExt::ok_or_msg`]. Construction stays crate-private via
/// `internal_err`; there are no public constructors.
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

/// Bail with an error. Three forms:
///
/// ```ignore
/// bail!("something: {x}");                    // → Fault<BoxError>, format!-interpolated
/// bail!("literal message");                   // → Fault<BoxError> (same arm, zero args)
/// bail!(Internal, "something: {x}");           // → Fault<Internal> with format! detail
/// bail!(Internal { detail: "...".into() });    // → Fault<Internal> from struct
/// ```
///
/// A literal first argument is always `format!`-interpolated (anyhow's
/// `bail!` semantics): `{x}` captures `x` from scope. A NON-literal
/// expression (`bail!(err)`, `bail!(MSG)`) goes through `Fault::from`
/// untouched.
#[macro_export]
macro_rules! bail {
    ($type:ident, $fmt:literal $(, $arg:expr)* $(,)?) => {{
        return ::core::result::Result::Err($crate::exn::Fault::from(
            $type { detail: format!($fmt $(, $arg)*) }
        ));
    }};
    ($fmt:literal $(, $arg:expr)* $(,)?) => {{
        return ::core::result::Result::Err($crate::exn::Fault::from(
            ::std::format!($fmt $(, $arg)*)
        ));
    }};
    ($err:expr) => {{ return ::core::result::Result::Err($crate::exn::Fault::from($err)); }};
}

/// Assert a condition, bailing with an error when false — the guard form
/// of [`bail!`]:
///
/// ```ignore
/// ensure!(x > 0, "x must be positive: {x}");      // → Fault<BoxError>, interpolated
/// ensure!(x > 0, AppError { detail: "x too small".into() });
/// ```
///
/// The literal form forwards to [`bail!`]'s `format!` arm, so `{x}`
/// captures from scope.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {{
        if !($cond) {
            $crate::bail!($fmt $(, $arg)*)
        }
    }};
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
fn walk_sources(
    error: &(dyn Error + 'static),
    location: &'static Location<'static>,
) -> Vec<(FrameKind, Arc<Frame>)> {
    // `Error::sources` is the std protocol the manual `error.source()`
    // while-loop this replaced walked: it yields `self` then the entire
    // cause chain, so `.skip(1)` drops the error itself and the
    // (type_name, message) pairs come out in the same order.
    let mut chain = Vec::new();
    for src in error.sources().skip(1) {
        chain.push((std::any::type_name_of_val(src), src.to_string()));
    }
    // Fold from the deepest cause outward into a nested chain. Every edge
    // here is a `source()` cause — [`FrameKind::Source`].
    let mut children = Vec::new();
    for (type_name, msg) in chain.into_iter().rev() {
        children = vec![(
            FrameKind::Source,
            Arc::new(Frame::new(
                Box::new(InternalError(msg.into())),
                type_name,
                location,
                Context::None,
                children,
            )),
        )];
    }
    children
}

impl<E: Send + Sync + Sized + 'static> fmt::Debug for Fault<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_fault(f, &self.root, "")
    }
}

/// Render the fault tree: one line per frame (`error, at file:line:col
/// [context]`), children nested under `|-- `/``-- `` connectors. When the
/// frame's error provides an [`crate::errors::ErrorCode`] (through
/// `Error::provide`), the line carries a `[CODE] ` prefix between the
/// connector and the message: `` `-- [E428] pipeline layout: io: closed ``.
/// Inline
/// attachments render as leading pseudo-children (`* {display}` lines)
/// BEFORE real children, sharing the sibling last-ness — the last line under
/// a frame (attachment or child) gets ``-- ``, the rest `|-- `. Non-inline
/// attachments are not rendered in-tree; the frame's own line gets a
/// ` (+N more attachments)` suffix when N > 0.
fn write_fault(f: &mut fmt::Formatter<'_>, root: &Frame, prefix: &str) -> fmt::Result {
    /// One frame's own line content (no connector): `[CODE] error, at
    /// file:line:col [context] (+N more attachments)`.
    fn write_frame_line(f: &mut fmt::Formatter<'_>, frame: &Frame) -> fmt::Result {
        if let Some(code) = core::error::request_value::<crate::errors::ErrorCode>(frame.error()) {
            write!(f, "[{}] ", code.0)?;
        }
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
        Ok(())
    }

    // Explicit stack, not recursion: Debug rendering runs in processes that
    // are already sick, and a degenerate deep tree must not overflow the
    // stack. Entries borrow the frames (the tree outlives the render).
    // `prefix` is the connector prefix inherited from the ancestors; `last`
    // picks ``-- `` over `|-- ` (the root has neither).
    struct Work<'a> {
        frame: &'a Frame,
        prefix: String,
        last: bool,
        is_root: bool,
    }
    let mut stack = vec![Work {
        frame: root,
        prefix: prefix.to_string(),
        last: true,
        is_root: true,
    }];
    while let Some(work) = stack.pop() {
        let Work {
            frame,
            prefix,
            last,
            is_root,
        } = work;
        // `prefix` is the PARENT's connector prefix (used for this frame's
        // own connector); this frame's subtree hangs off `own_prefix`.
        let own_prefix = if is_root {
            prefix.clone()
        } else {
            write!(f, "\n{prefix}{}", if last { "`-- " } else { "|-- " })?;
            format!("{prefix}{}", if last { "    " } else { "|   " })
        };
        write_frame_line(f, frame)?;
        // Inline attachments are leading pseudo-children: they share the
        // sibling last-ness with real children.
        let inline: Vec<&Attachment> = frame
            .attachments
            .iter()
            .filter(|a| a.placement == Placement::Inline)
            .collect();
        let total = inline.len() + frame.children.len();
        for (i, attachment) in inline.iter().enumerate() {
            let connector = if i + 1 == total { "`-- " } else { "|-- " };
            write!(f, "\n{own_prefix}{connector}* {attachment}")?;
        }
        // Push children in reverse so they pop left-to-right (preorder).
        for (i, (_, child)) in frame.children.iter().enumerate().rev() {
            let last = inline.len() + i + 1 == total;
            stack.push(Work {
                frame: child,
                prefix: own_prefix.clone(),
                last,
                is_root: false,
            });
        }
    }
    Ok(())
}
