# fast-observe — design

> Design rationale and invariants. The implemented user surface is documented in rustdoc and OBSERVE.md; where this doc and the code disagree, the code wins.

Goal: one crate that deploys errors-with-causal-trees, profiling, logs, and
traces sharing identifiers — fastrace + logforth + a multi-profiler facade
fused with an exn-grade typed error tree, producing causal, prescriptive,
LLM-agent-readable output.

Non-goals: reimplementing profiling backends, replacing tracing, being a
metrics system.

---

## 0. The thesis

Three facts drive the design:

1. **`profiling` crate's compile-time model is right for heavy profilers**
   (puffin/optick/tracy/superluminal/tracing): zero code when off, native
   instrumentation when on. Runtime selection is wrong for these — tracy
   either is linked or is not.
2. **Runtime selection is right for the cheap built-in backends**
   (Off/Instant/Fastrace/Web): they are always compiled in, cost ~2ns when
   off, and switching at runtime via env is a genuine dev workflow.
3. **The error path is the product.** Profiling and logging are sinks; the
   `Fault` causal tree is the data model. Everything else (hooks, scopes,
   trace ids, diagnostics, registry) exists to make the final error report
   more causal and more actionable.

So: keep runtime selection for Tier-1 backends, feature-forward Tier-2
backends to upstream `profiling`, and invest the design budget in the error
data model + the report.

---

## 1. Layering (hard dependency rule)

```
layer 0: core      Fault / Frame / Context / define_errors! / ERROR_REGISTRY
                   error_counts / hooks      — depends on: parking_lot, log (facade only)
layer 1: observe   profiling facade, config, diagnostics (ariadne), report
layer 2: deploy    init()/builder wiring logforth + fastrace + Tier-2 profilers
```

Rule: **lower layers never name logforth/fastrace types**. Layer 0 must work
pre-`init()`, post-`init()`, in tests, in wasm, with hooks cleared. The error
path never breaks because a sink is missing. This is already mostly true
(hook.rs is the only place logforth/fastrace are named); the Deployment
builder does not regress it.

Single crate, feature-gated. Not a workspace: the layers are small, and
feature flags give identical pay-for-what-you-use granularity without
version-skew pain.

---

## 2. Pillar: profiling facade (two tiers)

### Tier 1 — runtime-selected (existing, keep)

`ObserveConfig.profiling_backend: Off | Instant | Fastrace | Web`,
`OBSERVE_PROFILE` env. `scope!` = one relaxed atomic load + branch.

Fixes folded in:
- `scope!` currently checks `!= Off` in the macro and re-matches in
  `ScopeGuard::new_static` — fold to one load.
- `ScopeGuard::new` (dynamic names) `Box::leak`s — replace with a global
  intern table (`Mutex<HashSet<&'static str>>`), bounded by unique names.
- `CURRENT_SCOPE` stores `(Cow, Instant)`; the `Instant` is never read.
  Use it: `Fault::new` attaches `scope elapsed` to the frame context.
- **Clock: adopt fastant on native, keep web-time on wasm.**
  MIGRATING.md §7 dropped the old rdtsc clock for `web_time::Instant`
  (~tens of ns per read on native) purely for wasm portability. fastant
  (same `fast` org as fastrace/logforth; fastrace times its own spans with
  it) gives TSC reads (~2–5ns) on Linux x86_64 and falls back to
  `std::time` elsewhere — a superset of today's native behavior. Split by
  target, keep `clock.rs` `pub(crate)` so the swap is invisible:

  ```toml
  [target.'cfg(not(target_family = "wasm"))'.dependencies]
  fastant = "0.1"
  # web-time stays, wasm-only
  ```

  - native: `fastant::Instant` (TSC; auto-fallback if TSC unstable)
  - wasm: `web_time::Instant` (fastant would fall back to std, which
    panics on wasm32-unknown-unknown — the original reason for web-time)
  - feature-forward `clock-coarse` → `fastant/fallback-coarse` for
    speed-over-accuracy fallback platforms
  - bonus beyond speed: instant-backend spans and fastrace spans now share
    one clock source + calibration, so durations are directly comparable
    across the two Tier-1 backends (the isomorphism rule applied to time).

### Tier 2 — runtime-selectable backend SET (REVISED; no upstream `profiling` dep)

Original plan forwarded to the upstream `profiling` crate. REJECTED:
upstream's model is compile-time-only (feature on = always instrumenting),
and the design decision here is that **compiled-in ≠ active** — backends
are compiled in via features and SELECTED at runtime via flags, alongside
Tier-1. (User directive: "no profiles; just selecting what you want and
having flags/toggles. Default is fastrace; you turn instant on to look at
performance.")

Instead we own ~15-line glue modules per backend (upstream `profiling` is
the reference implementation — same APIs, same semantics), each behind
`profile-with-*` features named verbatim after upstream:

```toml
profile-with-puffin       = ["dep:puffin"]
profile-with-tracy        = ["dep:tracy-client"]
profile-with-optick       = ["dep:optick"]
profile-with-superluminal = ["dep:superluminal-perf"]  # windows-only dep
profile-with-tracing      = ["dep:tracing"]            # our scope! → tracing::span
```

Runtime selection replaces `ProfilingBackend` with a bitmask:

```rust
pub struct Backends(u16);   // INSTANT | FASTRACE | WEB | PUFFIN | TRACY | OPTICK | SUPERLUMINAL | TRACING
config().set_backends(Backends::FASTRACE | Backends::TRACY);
```

- `scope!` = ONE atomic load of the mask; each enabled+compiled backend
  enters its guard (ZST stubs when not compiled — the existing
  `profiling_backend!` wrap-module pattern extended per backend).
  Mask 0 → all-dummy guard, ~2ns.
- `OBSERVE_PROFILE` becomes a comma list (`fastrace,tracy`); single names
  (`off|instant|fastrace|web`) keep working.
- **Self-teaching config**: setting a bit whose feature is not compiled
  logs a one-time warning naming the exact cargo feature to enable
  (`profile-with-tracy`). LLM agents reading the log learn the flag.
- Backend enable hooks: puffin needs `set_scopes_on(true)` —
  `set_backends` calls per-backend `on_enable()` when a bit flips on.
- Dependency weight is the whole point of features (see §9d): tracy/optick
  compile C/C++, otel pulls the OTel SDK tree — none of it exists in the
  build unless its feature is named.

Attribute-macro re-export: superseded by §9c-ext — the macros are owned by
fast-observe-macros, not re-exported from `profiling-procmacros`.

### Async gap

`scope!` guards are thread-bound (`!Send`); instrumenting across `.await`
needs a dedicated surface. With feature `int-futures` (fastrace-futures):

```rust
future.in_observed_span("load")   // wraps fastrace's in_span + scope-name TLS
```

and document: sync scopes inside async fns are fine between awaits;
cross-await spans must use `in_observed_span` or a root span.

---

## 3. Pillar: log pipeline (full logforth surface + typestate builder)

### Feature map (all optional; `bridge-log`, `fastrace` stay default)

| feature | wires |
|---|---|
| `layout-json` (rename of `json`, alias kept) | JSON stdout |
| `layout-logfmt` / `layout-gcl` / `layout-text` | logfmt / Google Cloud / plain text layouts |
| `log-stderr` | Stderr appender (error split) |
| `log-file` (rename of `file`, alias) | rolling file via OBSERVE_LOG_DIR |
| `log-syslog` / `log-journald` | system log sinks |
| `log-async` | Async combiner appender (offload formatting/IO) |
| `log-testing` | capture appender for harnesses |
| `filter-rustlog` | `OBSERVE_LOG`/`RUST_LOG` directive filtering |
| `diag-task-local` | task-local MDC for async |
| (existing) `otel` | OTel appender + reporter |

### Builder (typestate where it matters)

```rust
let guard = fast_observe::builder()
    .logs(|l| l
        .env_filter()                    // OBSERVE_LOG, fallback RUST_LOG, default "info"
        .stdout()                        // text; colors iff tty && !NO_COLOR
        .stderr_from(Level::Error)       // optional split
        .file_from_env())                // OBSERVE_LOG_DIR when log-file
    .traces(|t| t.console())             // .reporter(r) / .otel(...) / .off()
    .errors(|e| e.throttle(100).backtrace(true))
    .init()?;                            // Err(InitError) on double-init
```

Type system doing real work, not ceremony:

- `init()` is the only terminal and returns
  `Result<InitGuard, InitError>`; `InitGuard: Drop` calls `fastrace::flush()`
  — kills the "forgot to flush, lost the last trace" class of bug.
- Reporter handling: `set_reporter` is global and unstoppable; the builder
  tracks `reporter: Option<...>` and refuses to stomp unless `.force()` —
  the Option in the type makes the conflict explicit.
- `init()` zero-arg keeps working: `builder().init()` with all defaults,
  so README's one-liner survives.
- `fastrace::flush` on guard drop is best-effort; documented.

### Env surface (single `EnvConfig`, parsed once in the LazyLock)

`OBSERVE_PROFILE` (have), `OBSERVE_LOG`, `OBSERVE_LOG_DIR` (have),
`OBSERVE_ERROR_THROTTLE`, `OBSERVE_BACKTRACE`, `OBSERVE_COLOR=always|never|auto`.
Unknown values warn-and-default (existing pattern). Every knob documented in
one table in README.

### Hooks

`add_error_hook` stays; `clear_error_hooks()`, `hooks_len()` (tests), and
`set_default_hook_enabled(bool)` round out hook management. Hook list clone-per-error is fine (cold
path); if a profile ever says otherwise, swap `Mutex<Vec<Hook>>` for
`arc_swap::ArcSwap<Vec<Hook>>` — internal change, no API impact.

---

## 4. Pillar: fastrace surface

- Reporter pluggability via builder (above); `init_otel` folds into
  `.traces(|t| t.otel(reporter))`.
- Re-export features (mirrors upstream integration crates):
  `int-futures`, `int-axum`, `int-poem`, `int-tonic`, `int-tower`;
  existing `http` (reqwest) and `bridge-tracing` unchanged.
- Root-span ergonomics: `root_span!("request")` macro →
  `Span::root(func_path!()-aware name, SpanContext::random())` +
  `set_local_parent` guard. Today every app hand-rolls this; it is the
  difference between "trace exists" and "trace correlates with logs/errors".
- `fast_observe::flush()` re-export (calls fastrace::flush when enabled,
  no-op otherwise).

---

## 5. Pillar: exn-core — the typed error data model

The attraction of exn is that context is *typed structure*, not strings.
Push that further than exn does:

### 5.1 Attachments — SUPERSEDED by §5.7 (typed + placement, one design)

Original two-channel sketch kept for reference:

```rust
pub struct Attachment { pub key: &'static str, pub value: Cow<'static, str> }   // rendered
struct TypedSlot(Box<dyn Any + Send + Sync>);                                     // programmatic

impl Fault<E> {
    fn attach(self, key: &'static str, value: impl Display) -> Self;   // render channel
    fn attach_typed<T: Send + Sync + 'static>(self, v: T) -> Self;     // type channel
    fn get<T: 'static>(&self) -> Option<&T>;
}
```

Render channel prints `[key=value ...]` in Display/Debug; type channel is
the error-stack-style grab bag for programmatic recovery (retry hints,
partial state). Both live on the root frame; `wrap` keeps the child's
attachments reachable via the tree.

### 5.2 Traversal + typing across the tree

- `Fault::iter()` — preorder `&Frame` iterator; `root_cause()`;
  `Frame::find_type(name)` for doctor tooling.
- `walk_sources` currently FLATTENS `A→B→C` into siblings of root and
  stringifies each into `InternalError`, and `wrap_msg` forgets to walk at
  all. Fix: nest properly (`B` child of `A`, `C` child of `B`), keep
  `type_name` per frame, and route ALL construction through one internal
  `Frame::capture(...)` so `new`/`wrap`/`wrap_msg`/`observed`/`from_boxed`
  cannot drift (they already have: `wrap_msg` drops the source chain).

### 5.3 Codes and categories as types

- `define_errors!` types implement a new `Coded { fn code(&self) -> &'static str }`
  trait; `write_fault` renders `[E100]` in the tree when the frame's error
  is coded (via a `&dyn Coded` downcast slot stored at capture time).
- `ErrorCategory::policy()` → `Policy::{Retry{after}, Poison, Abort,
  ContentFix}` — category stops being a label and becomes behavior; the
  report renderer prints the policy as the prescriptive line.
- `error_counts_by_category()` — registry lookup per type → grouped counts.

### 5.4 Boundaries

Feature `anyhow-boundary`: `from_anyhow(anyhow::Error) -> Fault<AnyhowError>`,
`into_anyhow(Fault<E>) -> anyhow::Error` (exn-anyhow pattern; explicit
`map_err` at the API boundary, never implicit).

### 5.5 Capture

Feature `backtrace` + `OBSERVE_BACKTRACE=1`:
`Backtrace::force_capture()` stored on the root frame, rendered in Debug
tree. Off by default (cost), on in dev via env. Location-per-frame stays
(always on, free).

### 5.6 Two hook families (rootcause's best idea, adapted)

Rootcause splits hooks into *creation* hooks (mutate the report at
capture time: attach backtrace, span fields, request ids) and *formatting*
hooks (control rendering). Our single `add_error_hook` is neither — it is a
read-only *sink* fan-out (log/metrics notification). All three jobs are
distinct; model them as three registries:

1. **Capture hooks** (new): `add_capture_hook(fn(&mut FrameCapture))` —
   run DURING `Frame::capture`, can attach data. Backtrace capture,
   trace-id, scope-elapsed, hostname/pid all become capture hooks instead
   of hardcoded fields — features register them, apps add their own
   (request id!). `OBSERVE_BACKTRACE` toggles the built-in one.
2. **Sink hooks** (existing `add_error_hook`): post-construction fan-out,
   throttled, panic-contained. Unchanged semantics.
3. **Formatter** (new, §7): the report renderer becomes a replaceable
   global formatter (`set_report_formatter`) — text default, `json` under
   serde, app-defined layouts. Rootcause splits formatting into four hook
   types (report/context/attachment + placement); we take ONE
   report-level formatter + per-attachment placement (below), not the
   full matrix.

### 5.7 Attachments upgraded: typed + inspectable + placement

Rootcause proves attachments should not be "glorified strings": their
`attachment.downcast_inner::<T>()` makes reports programmatically
inspectable (their example: extracting `RetryMetadata` from a retry tree).
Our planned two-channel attachment merges into ONE typed design:

```rust
pub struct Attachment {
    value: Arc<dyn Any + Send + Sync>,   // typed channel: get::<T>()
    display: AttachmentDisplay,          // render channel: cached Display string
    placement: Placement,                // Inline | Appendix | Opaque | Hidden
}
impl Frame {
    pub fn attach<A: Display + Send + Sync + 'static>(&mut self, a: A);   // typed+rendered
    pub fn attachments(&self) -> &[Attachment];
    pub fn find_attachment<T: 'static>(&self) -> Option<&T>;
}
```

`attach("key", value)` string form remains as sugar over this. Placement
controls the §7 report layout (Opaque = counted-not-shown, for secrets/
large payloads — rootcause's redaction answer). Keyed lookup stays via the
string form; typed lookup via `find_attachment`.

### 5.8 Multi-failure: `FaultCollection`

Rootcause's headline feature beyond anyhow: retry/batch failures collected
into one tree (`ReportCollection::new(); errors.push(e);
Err(errors.context("..."))`). Our Frame tree already supports n-ary
children; add the ergonomic surface:

```rust
let mut errs = FaultCollection::new();
for attempt in 1..=3 {
    match fetch().attach_with(|| format!("attempt #{attempt}")) {
        Ok(v) => return Ok(v),
        Err(e) => errs.push(e),
    }
}
Err(errs.into_fault(JournalError::Flaky))   // one Fault, three children
```

Retry/recovery loops and batch-compaction paths are the consumers.

### 5.9 Compat matrix (rootcause's full set, same pattern per crate)

Features `compat-anyhow`, `compat-eyre`, `compat-error-stack`, plus
always-on `Box<dyn Error>` conversion. Bidirectional, explicit, tree
preserved both directions. `into_fault()` / `from_fault()` extension
trait methods (their `IntoRootcause` pattern) so call sites read
`legacy_fn().into_fault()?`.

---

## 6. Pillar: diagnostics (ariadne, done properly)

1. **Multi-label**: `Diagnostic.labels: Vec<LabelSpan>` with
   `{ span, message, primary, color? }`; `with_source` stays as the
   one-label convenience; new `with_label(...)`.
2. **In-memory sources**: global `SourceStore`
   (`RwLock<HashMap<Utf8PathBuf, Arc<str>>>`); the ariadne cache checks the
   store, then disk. Diagnostics for generated/embedded/packed assets stop
   rendering `<unknown>`.
3. **Registry linkage**: `build_report` looks up `diag.code` in
   `ERROR_REGISTRY`; on hit, appends
   `note: [E100] disk read failed (category: Transient — safe to retry)`.
   The compiler-error path and the runtime-error path finally share the
   same code registry.
4. `Diagnostic::warning()/info()` constructors; `impl Error for Diagnostic`;
   `eprint_diagnostic` honors `NO_COLOR` + tty detection.
5. Rejected: miette interop. ariadne is already the renderer and miette's
   protocol would fork the data model. Revisit only if ecosystem pressure
   demands it.

---

## 7. Pillar: the Report — LLM-agent-first output

New module `report`: `render_report(&Fault<E>) -> String`
(+ `render_report_json` under `serde`).

Design principles (this is the "knowing LLM agents read it" part):

- **Stable sections, one fact per line, `key: value`.** No prose
  paragraphs. Diff-stable = snapshot-testable = reliably parseable by an
  agent with no special tooling.
- **No ANSI, no wall-clock timestamps** (elapsed/durations only), absolute
  paths — output identical across machines.
- **Deterministic order**: error → location → scope → attachments → cause
  chain → trace → action.
- **Prescriptive final line** derived from category policy — the agent's
  next action is in the output, not in tribal knowledge.

Format sketch:

```
error: [E100] disk read failed: /etc/app/spec.toml
category: Transient (policy: retry is safe)
location: src/config.rs:42:10
scope: load_config (elapsed 12.3ms)
attachment: attempt=3
attachment: path=/etc/app/spec.toml
cause 0: disk read failed: /etc/app/spec.toml
cause 1: No such file or directory (os error 2)
trace_id: 4f3c…9a2b   # grep this: logs + spans carry the same id
action: retry the operation; if persistent, check OBSERVE_LOG output for trace_id 4f3c…9a2b
```

The `trace_id` line is the keystone: the error hook reads the current
fastrace `SpanContext` at capture time; `FastraceDiagnostic` already stamps
logs with the same id; one grep across `app.log` reconstructs the entire
causal moment — error tree, log lines, span timings. That is "making them
work well together" made concrete.

Default hook gains an option to emit this block (structured, one event)
instead of today's single `log::error!` line.

---

## 8. Ergonomics: the whole API in one import

```rust
pub mod prelude {
    pub use crate::{Result, Fault, Context, ResultExt, OptionExt, ErrorExt};
    pub use crate::{bail, ensure, scope, profiling, finish_frame, root_span};
    pub use crate::{add_error_hook, error_counts, lookup_error};
    pub use profiling_procmacros::{function, all_functions, skip};
}
```

Dev experience target — three lines to full observability:

```rust
use fast_observe::prelude::*;

let _guard = fast_observe::builder().logs(|l| l.env_filter().stdout()).init()?;
work().await.change_context(Boot).report_or_die();
```

(Also add `report_or_die`/`Fatal` handling? Open — see §11; the
swallow/blessed-`report` story is unchanged.)

---

## 9. Performance budget (hard rules)

- `scope!` with everything off: ≤ ~2ns (one relaxed load + predicted
  branch). Tier-2 features add only their own upstream cost.
- Scope entry/exit: two `now_ns()` reads; with fastant on Linux x86_64
  that is ~4–10ns per scope instead of ~40–100ns under std/web-time —
  the instant backend stops being the expensive option.
- Ok path of every ResultExt method: zero allocation (make `msg` lazy —
  `impl FnOnce() -> Cow<'static, str>` — on `report`/`observed`/`wrap_msg`;
  breaking but worth it, pre-1.0).
- Error path is cold: `#[cold]` everywhere (mostly done), allocation
  acceptable, but no unbounded growth: intern table for dynamic scope
  names, finish_frame eviction (exists), hook list bounded by construction.
- `profiling!()` currently writes the logforth thread-local diagnostic on
  every call even when profiling is Off — needed for log correlation, so
  keep, but make the write cheap (insert only when a logger is installed).
- Backtrace capture strictly opt-in (feature + env).

---

## 9a. rootcause review — adopted vs rejected (evidence: rootcause @0.14)

Adopted (adapted to our leaner model): capture-hook family (5.6), typed
attachments w/ placement (5.7), pluggable report formatter (5.6),
FaultCollection (5.8), full compat matrix (5.9), `ReportRef`-style cheap
inspection (our `FrameRef` in the planned `iter()`), prelude (already
planned).

Rejected, with reasons:

- **`Report<C, O, T>` 3-parameter markers** (Mutable/Cloneable ×
  SendSync/Local × typed/Dynamic). Ownership invariants enforced at the
  type level are genuinely nice, but the cost is three type params on
  every error mention plus `into_cloneable`/`try_into_mutable` ceremony.
  Our `Arc<Frame>` + `Deref<Target = E>` covers the practical cases with
  `Fault<E>` alone. (Known wart: `with_context` silently no-ops on a
  shared Arc — fix by documenting + making attachment APIs take `&mut`
  during construction only, which capture hooks now own.)
- **`Local` (!Send/!Sync) reports.** Every global registry we have
  (counts, throttle, hooks, trace-id) assumes Send+Sync; no consumer has an
  Rc-error need. Skipping halves the marker matrix.
- **no_std.** parking_lot, TLS, logforth, fastrace are all std. Not a
  goal.
- **Handler auto-detection via `report!` macro + `new_custom<H>`.** Our
  frames always wrap `Error` types; the Error/Display/Debug/Any handler
  zoo exists to render non-Error contexts. Attachment placement (5.7)
  captures the 80% that matters.
- **Pointer-sized Report via triomphe.** Their Report is 8 bytes; our
  Fault is 16 (root Arc + typed-error Arc). The second Arc buys
  panic-free typed `Deref` — keep it.
- **Splitting into 4 crates** (internals/backtrace/tracing/preformat).
  Features give us the same modularity in one crate.

What their existence proves for this design: creation hooks carrying
backtrace+span capture as OPTIONAL sub-crates validates making our
capture hooks the extension point (5.6); their compat modules validate
the explicit-boundary philosophy (never implicit From).

## 9b. Ecosystem scan — further adoptions (source crate cited per idea)

### Features

1. **Scope STACK, not slot** (tracing-error's `SpanTrace`). `CURRENT_SCOPE`
   is one `(name, Instant)`; nested `scope!`/`profiling!` overwrite it.
   tracing-error captures the full span path with k=v fields into every
   error. Make it a TLS `Vec<(Cow, Option<Cow>, Instant)>`; report renders
   `scope path: request → load_config → parse_sql (3.1ms in leaf)`. Cheap
   (Vec push/pop), kills the "error says `parse_sql` but which phase
   called it?" ambiguity. THIS replaces the single-scope-elapsed idea.
2. **Breadcrumb trail attachment** (sentry). The instant backend already
   accumulates finished spans; on Fault capture, snapshot the last N span
   names+durations as an `Opaque`→`Appendix` attachment (§5.7). Every
   error report then carries "what was happening just before it broke"
   without any extra instrumentation. Gate: instant backend active.
3. **Panic unification** (color-eyre / human-panic).
   `observe().panic_hook(true)` (default on): panics render
   through the SAME report pipeline — category Fatal, location from
   PanicHookInfo, backtrace per capture config, scope path, trace_id,
   then chain to the previously installed hook (composability — never
   stomp). A panic and a returned error become indistinguishable in
   output, which is exactly right for an agent reading a crash log.
4. **Graceful shutdown flush** (tracing-appender non-blocking +
   atexit): InitGuard::drop flushes fastrace; additionally register
   `libc::atexit` / signal-hook (SIGTERM) best-effort flush behind
   `flush-on-exit`. Servers die by SIGTERM, not by Drop.
5. **tokio JoinError adapter** (feature `int-tokio`):
   `.observe_join()` converts JoinError → Fault (cancelled vs panic
   distinguished, panic payload stringified into the tree). Async error
   paths currently leak `Box<dyn Any>` strings.
6. **Retry helper driven by Policy** (category → behavior made real):
   `result.retry_policy(backoff)` retries iff `category() == Transient`,
   collecting attempts into a `FaultCollection` (§5.8) on exhaustion.
   This is where 5.8 + policy + scope trail converge. Keep it small;
   backoff params are the caller's.
7. **Diagnostic protocol trait** (miette's lesson): a `ToDiagnostic`
   trait (code/severity/labels/advice) that both `Diagnostic` and coded
   `Fault`s satisfy — one ariadne renderer for compile-time AND runtime
   errors (closes SURFACE.md §C5 with a trait instead of converters).

### Conventions to honor (familiarity = adoption)

- **`RUST_BACKTRACE` first** (error-stack, rootcause-backtrace both do):
  backtrace capture keys off `RUST_BACKTRACE=1|full`; `OBSERVE_BACKTRACE`
  only overrides. Agents and users already know this env var; inventing
  only our own would be a gratuitous fork.
- **Color discipline**: `NO_COLOR`, `CLICOLOR`/`CLICOLOR_FORCE`, `TERM=dumb`,
  tty detection via `std::io::IsTerminal` (std since 1.70 — no dep).
- **W3C `traceparent` naming**: trace_id/span_id fields named per OTel
  semantic conventions so log pipelines (GCL layout, otel appender)
  correlate without mapping config.
- **sysexits codes** for `main() -> Result` exit paths (EX_DATAERR for
  Content, EX_SOFTWARE for Invariant/Fatal, EX_TEMPFAIL for Transient) —
  category → exit code is free given the registry.

### Renderer notes

- **annotate-snippets** (rustc's own renderer) as an alternative ariadne
  backend behind a feature — some orgs standardize on rustc-style output;
  keep ariadne default (better label ergonomics). Low priority, the
  `ToDiagnostic` trait (7) makes it a small backend swap.
- Report formatter config surface: width, unicode/ascii connectors
  (rootcause supports both), color — all fields on the formatter hook,
  env-overridable.

### Testing/tooling best practices to adopt

- **insta** snapshot tests with duration/timestamp redactions for report
  + diagnostic output (the LLM-readable format must be diff-stable;
  snapshot tests are what keeps that promise from rotting).
- **trybuild** compile-fail tests for `error!` (missing `#[category]`
  with `#[code]`, bad field interpolation, size-assert violation).
- **cargo-hack --feature-powerset** (or `--each-feature`) in CI: 20+
  feature flags is exactly where feature-unification bugs breed.
- **cargo-semver-checks** pre-release; the Display-stability tests in
  tests/exn.rs are the right pattern, extend to report format.
- **deny(missing_docs)** once public API settles (rootcause enforces it;
  for a library whose docs ARE the interface, worth it).

### Forward-compat alignments

- **std `Error::provide`/`request_ref`** (error_generic_member_access,
  nightly): our typed attachments (§5.7) are the same idea std is
  stabilizing. Design `Attachment` so a future `provide()` impl can
  expose them; don't paint into a corner std will own.
- **fastrace upstream**: keep our surface additive-only over fastrace
  (never fork behaviors); when fastrace gains span-field capture, our
  scope stack (1) should delegate rather than duplicate.

### Rejected after consideration

- **sentry/crash-reporting backends**: a sink feature, not core — the
  hook system already lets an app forward reports anywhere. Document a
  recipe, don't ship an integration.
- **miette as the renderer**: would fork our data model (its Diagnostic
  protocol ≠ our Frame tree); we take its trait idea (7), not the dep.
- **color-eyre-style theme engine**: formatter config (width/ascii/color)
  covers the real demand; a theme DSL is scope creep.
- **loom** for the hook registry: three Mutex'd registries, cold paths;
  the deadlock risk was already eliminated by clone-before-invoke.

## 9c. Refactor pass — elegance / DRY / robustness / performance

(All verified against the source; not duplicating items already planned.)

### Correctness hazards

1. **`tests/hook.rs` global-config race (flaky test).** `throttle_caps_*`
   sets the GLOBAL throttle to 2 while `throttle_zero_is_unlimited` (same
   binary, parallel test threads) constructs 4 `FanoutError`s and asserts
   4 hook fires. Interleave → only 2 fire → spurious failure. Same for
   `multiple_hooks_all_fire`'s exact-count assertions. Fix: a test-only
   global mutex around throttle mutation + construction, or make throttle
   state a parameter of a `HookRegistry` struct (the global becomes one
   instance) — the latter also enables per-registry testing generally.
2. **`with_context` / attachments silently no-op on a shared `Arc`.**
   `Arc::get_mut` failure is invisible. Debug builds must be loud:
   `debug_assert!(Arc::strong_count == 1)` + a one-line log on release.
   Silent context loss is worse than a panic in dev.
3. **`instant` guard LIFO corruption.** `InstantGuard::drop` blindly pops;
   a `mem::forget`ed or out-of-order guard makes the next pop steal the
   wrong span, silently corrupting the stack for the rest of the thread's
   life. Tag each guard with the span's stack depth; on drop, truncate to
   that depth instead of a bare pop — wrong-order drops then degrade to a
   skipped span instead of corruption.
4. **`Frame` fields are all `pub`** — external code can hand-construct
   frames that never fired hooks/counters, breaking every invariant the
   report relies on. Privatize + getters; construction only via
   `Frame::capture`.
5. **`hook.rs` panic-containment gap on wasm** is documented in README —
   keep, but also gate: `catch_unwind` compiled out under
   `panic = "abort"` should fall back to direct invocation with a
   compile-time note, not silently differ.

### DRY

6. **Five `Fault` construction sites** → one `Frame::capture` (already
   planned — this is the big one; `wrap_msg`'s dropped source chain is
   the proof).
7. **`SharedError<E>` / `SharedBoxedError`** — near-identical delegation
   wrappers. Unify into one `Shared<T: ?Sized>` if `Box<dyn Error + Send
   + Sync>: Error` delegation can be expressed generically; verify at
   implementation, delete one type if so.
8. **`From<&str>` / `From<String>` for Fault** inline their own
   `Box::new(InternalError(..))` instead of the `internal_err` helper
   used elsewhere. One path.
9. **`profiling_backend!` macro** has two whole arms differing only in
   `finish_frame` — collapse with an optional `$(finish_frame)?` matcher.
10. **`instant.rs` take/set dance** (`let mut v = c.take(); …; c.set(v)`)
    repeated 8× — one `fn with_tl<T>(cell, f)` helper. The Cell-over-
    RefCell panic-safety rationale stays; only the boilerplate goes.
11. **`Context` × scope conversion** — `current_scope_name().map_or(
    Context::None, Context::Scope)` appears 4× in exn.rs; `Frame::capture`
    absorbs it.

### Elegance

12. **`ensure!` wraps the condition in `bool::from(...)`** — identity on
    `bool`; delete.
13. **`ProfilingBackend` discriminant decode** (magic 0/1/3 + `_ =>
    Fastrace`) → `strum::FromRepr` + `unwrap_or(Fastrace)`. strum is
    already a dep; this is what `FromRepr` exists for.
14. **`Context::scope`/`entity`/`custom` lack `#[must_use]`** while
    `tick` has it — pick one convention (add them).
15. **Default hook prints `— None`** for context-less errors
    (`Context::None` Displays as `"None"`, enshrined by test). Log line
    should omit the context field entirely when None; keep the Display
    string for the test only if something else depends on it.
16. **`breakdown.rs` sort-then-rev** — `sorted.sort_by_key(..)` then
    `.iter().rev()` → one descending `sort_by`.
17. **`profiling!` vs `function_scope!` tag handling diverges** (one
    takes `&str`, one takes `AsRef`) — one conversion convention.
18. **`finish_frame` eviction index math** (drain, shift every retained
    boundary by `drained`) — replace with a monotonic `base: usize`
    offset; boundaries stay absolute, subtraction happens once at drain
    time. Same behavior, no per-element mutation loop to get wrong.

### Performance

19. **Hook fan-out clones `Vec<Hook>` per error** (N Arc bumps). Store
    `Arc<Vec<Hook>>` and clone the snapshot Arc — one refcount bump,
    same reentrancy safety, no new dep (arc_swap only if profiles say so).
20. **`record_error` holds the counts mutex across the metrics-facade
    call** — move the `metrics::counter!` emit outside the lock.
21. **`derive_more = "full"`** for exactly two `Display` derives — narrow
    to `features = ["display"]`. Pure compile-time win.
22. **`documented` crate is dead weight** — `DocumentedVariants` is
    derived on `Context` and never read anywhere. Either use it (doctor
    text from doc comments — the `error!` macro can lift variant docs
    into registry `advice` defaults) or drop the dep. Recommendation:
    use it, it's the cheapest advice-source there is.
23. **`error_counts()` / `lookup_error`** are cold and fine; do NOT
    micro-optimize (avoid the trap).

### Lint/process tightening

24. Add `missing_docs = "warn"` to `[lints.rust]` now (deny at 1.0, per
    §9b).
25. `[package] exclude` / `include` for the crates.io tarball — README
    assets, DESIGN/SURFACE docs decisions, tests kept or dropped
    deliberately (cargo package output is user-facing).

## 9c-ext. Extension points + direct-vs-handroll (post-ecosystem scan)

Verified live bug first: `profiling-procmacros` expands to
`profiling::function_scope!()` / `profiling::tracing::span!(...)` — paths
into the `profiling` crate we do NOT depend on. Our re-exported
`all_functions`/`skip` are UNCOMPILABLE for consumers without a separate
`profiling` dep. Fix: own macros in fast-observe-macros
(expand to `$crate` paths, push our scope stack, delegate to
`#[fastrace::trace]` when enabled — async-correct via `enter_on_poll`).

Extension-point adoptions (no forks; all public APIs):

- fastrace `Event::add_to_local_parent` at Fault capture (errors as span
  events; OTel exception events on export); `Reporter` trait →
  `SamplingReporter` (traces with error events always kept, successes
  sampled) + `MultiReporter` (console+OTel fan-out); `LocalCollector` for
  test span assertions; builder takes `collector::Config`.
- logforth: `core::builder()` multi-dispatch for Deployment (starter_log
  stays for init()); `Append/Layout/Filter/Diagnostic` as typed escape
  hatches; `append::Testing` for tests. **log `kv` feature**: the default
  hook logs `error.code`/`error.category`/`error.location`/`trace.id` as
  structured kv so JsonLayout/OTel emit fields, not strings.
- exn convergence: `impl Error for Frame` with `source()` → first child;
  `Fault::source` likewise — the causal tree becomes traversable by
  std-protocol tooling (anyhow chain walker, sources(), error-stack).
- rootcause convergence: `IteratorExt`-style `.collect::<FaultCollection>()`.
- std/nightly: `Termination` (free), `backtrace_frames`, `error!`-generated
  `Error::provide` (code/category/location/backtrace through `&dyn Error`).
- divan: re-export via its `crate =` macro option + `bench_profiled` +
  versioned JSON baselines (agent-diffable benchmark records).

Direct vs handroll: REPLACE profiling-procmacros (above); keep our 6-line
`func_path!`; clock collapses to fastant/web-time cfg alias; throttle +
intern table stay hand-rolled (~60 lines total; governor et al. overkill);
instant accumulator stays (works fastrace-free); compat layers hand-rolled
small with deps only on their targets; `error!` via venial proc macro.

## 9d. Dependency weight budget + divan

### Weight audit (what a feature costs a consumer)

| feature | weight | why |
|---|---|---|
| `fastrace`, `bridge-log` (default) | light | fastrace (fastant + small vec types), logforth core |
| `instant`, layouts, `log-file`/`syslog`/`journald`/`async`, `filter-rustlog`, diagnostics | tiny | single-purpose crates |
| `profile-with-puffin` / `-tracing` | light-medium | pure Rust |
| `profile-with-tracy` / `-optick` | heavy BUILD | C/C++ compiled via build scripts |
| `otel` | heavy TREE | opentelemetry SDK + otlp dependency tree |
| `superluminal` | tiny, windows-only | FFI bindings |
| divan / bolero / insta / trybuild | dev-deps only | never in consumer builds |

Rule: every non-default capability is `dep:`-optional; README carries this
table; CI spot-checks `cargo tree --no-default-features` stays minimal.

### Divan (feature `bench`)

Divan's `Counter` set is closed (BytesCount/ItemsCount) and it has no
timing-source plugin API — spans cannot become divan columns. What folds
in sanely:

1. **Re-export** (`pub use divan;`): `#[fast_observe::bench]`, `Bencher`,
   `black_box` — benches need no separate dev-dep juggling.
2. **`BenchExt::bench_profiled(|| ...)`**: forces the instant backend on
   for the measured closure; after divan's stats, emits the per-phase
   span breakdown (print_breakdown data, per-iteration totals). divan
   answers "how fast", the breakdown answers "where". Overhead of the
   scope instrumentation is INCLUDED in profiled numbers — documented:
   plain `bench` (backend Off) for absolute numbers, `bench_profiled`
   for attribution.
3. **Error counters**: `error_counts` delta across the bench →
   `ItemsCount` ("errors/sec" on error-path benches).

## 10. Migration + rollout (jj stages, each independently green)

All stages below have landed; the list is kept as the historical record of
the change order.

1. `fix`: tree-prefix connectors, `wrap_msg` source walk, scope stack
   (9b.1), single `Frame::capture` constructor, hook-test race (9c.1),
   LIFO-hardened instant guards (9c.3), private Frame fields (9c.4),
   `— None` log polish (9c.15), DRY items 6–18, perf items 19–23.
2. `feat(exn)`: capture hooks (5.6), typed attachments w/ placement (5.7),
   `Fault::iter` + `FrameRef`, `Coded` + code-in-tree, `walk_sources`
   nesting, category policy, compat-anyhow, FaultCollection (5.8),
   scope stack + scope path in reports (9b.1), breadcrumb attachment (9b.2).
3. `feat(profiling)`: Tier-2 feature forwarding, `function` re-export,
   scope intern, single-load scope!, root_span!, int-* re-exports.
4. `feat(log)`: new appender/layout/filter/diagnostic features, EnvConfig,
   builder + InitGuard, hook management fns.
5. `feat(diag)`: multi-label, SourceStore, registry notes, NO_COLOR,
     severity constructors.
6. `feat(report)`: render_report (+json) as the default pluggable
   formatter (5.6.3), trace_id capture hook, default-hook report mode,
   backtrace capture hook (feature `backtrace`).
6b. `feat(compat)`: compat-eyre, compat-error-stack, Box<dyn Error>
    conversions (5.9).
6c. `feat(ops)`: panic-hook unification (9b.3), flush-on-exit (9b.4),
    int-tokio JoinError adapter (9b.5), retry_policy (9b.6), sysexits,
    RUST_BACKTRACE convention, insta snapshots + trybuild for error!.
7. docs: README feature table rewrite, MIGRATING entries, prelude, per-stage
   tests incl. report snapshot tests.

Stages 2–6 are independent given stage 1; order 2 → 7 by default.

---

## 11a. linkme: keep it — with two fixes

Verdict: **linkme stays** for the error registry. The alternatives are
strictly worse for this job:

inventory, honestly weighed — it DOES have real advantages:

- **wasm**: inventory works on wasm32 via wasm-bindgen's start section —
  but ONLY when wasm-bindgen is present. Our bare `instant` wasm build
  has no wasm-bindgen, so the registry would silently be empty there
  anyway. The advantage evaporates exactly where we'd need it.
- **Dynamic loading**: ctors run on `dlopen` → inventory registries work
  across cdylib/plugin boundaries; linkme slices are fixed at static
  link time. A real inventory win for plugin hosts — not our deployment
  model (static binaries), but noted in case a consumer builds cdylibs.
- **Linker portability**: inventory/ctor avoids linkme's reliance on
  GNU `__start_`/`__stop_` section symbols (the historical source of
  linkme's platform bugs — mostly fixed now, but inventory is the safer
  bet on exotic linkers).

linkme still wins for us because:

- **Entries are pure data in the binary image** — zero runtime, and a
  doctor-style tool can read error codes from a binary WITHOUT executing
  it (static introspection). inventory's registry only exists after
  ctors run.
- **No pre-main execution** — registration can't fail, reorder, or be
  skipped by exotic embedders; and no start-section requirement on bare
  wasm (our §11a.2 fallback needs no ctor machinery at all).
- **Explicit registration** (`register_errors::<MyError>()` at startup)
  kills the entire value proposition: zero-coordination cross-crate
  aggregation. The value is that a downstream crate's errors appear in the
  binary's doctor output without any crate knowing the others exist. Any
  explicit scheme reintroduces the registration call everyone forgets.
- **Per-crate statics + hand-listing** — same forgetfulness problem.

Two fixes to the current usage:

1. **Kill the "consumers must add linkme to their deps" wart.** The
   `define_errors!` expansion emits `#[linkme::distributed_slice(..)]`,
   resolving `linkme` through the CONSUMER's crate graph. Proc macros are
   re-exportable: `pub use linkme::distributed_slice;` in fast-observe,
   then the macro emits `#[ $crate::distributed_slice(..) ]`-style paths
   (`#[::fast_observe::__private::distributed_slice(..)]` convention).
   Consumers stop needing the dep — one less adoption papercut, and one
   less version-skew hazard (consumer's linkme vs ours).
2. **Wasm gets explicit composition instead of nothing.** `error!` emits
   `pub const ENTRIES: &[ErrorRegistryEntry]` per enum on ALL targets;
   on native it additionally registers into the linkme slice; on wasm
   `lookup_error`/`doctor` take a composition root:
   `fast_observe::register_statics(&[EngineError::ENTRIES, JournalError::ENTRIES])`
   called once by the app. Explicit, but only on wasm, and the SAME
   doctor/report code runs unchanged above it.

## 11b. wasm audit — design-wide pass

**Targets of record (revised): `wasm32-wasip3` (component model) is the
PRIMARY wasm target — wasi is what we deploy; `wasm32-unknown-unknown`
(browser) is secondary.** Both are `target_family = "wasm"` so cfgs
cover both, but the differences matter:

- wasip3: std `Instant` works (WASI monotonic clock) — web-time falls
  back to std there; stderr/stdout exist (wasi-cli) so logforth's
  Stdout/Stderr appenders work and the `web` browser-console appender is
  WRONG there (cfg'd to wasm32-unknown-unknown only... verify: currently
  `#[cfg(all(feature = "web", target_arch = "wasm32"))]` — needs
  tightening to exclude wasi); panic=abort typical for components;
  single-threaded; linkme registry empty → `register_statics`
  composition is THE registry path on wasip3.
- unknown-unknown: web-time via performance.now; no std env/fs; `web`
  feature's console appender; browser-only.

Consumers may use linkme freely if they already have it, but fast-observe
never REQUIRES it: `error!` registrations route
through `__private` via `#[linkme(crate = ...)]` (shipped).

### Original audit (target: wasm32-unknown-unknown) — still applies, with the wasip3 deltas above

Baseline today: `just check-wasm` builds `--no-default-features
--features instant|web` with `-Z build-std`; linkme registry empty;
web-time clock. Audit of everything the overhaul adds:

**Hard rules (encode in clippy.toml + CI):**

- **No `std::time::{Instant,SystemTime}::now` anywhere** — always
  `crate::clock`. Enforce with clippy `disallowed-methods`; the
  fastant/web-time split (§2) is then mechanically guaranteed.
- **Native-only deps are ALWAYS target-gated**
  (`[target.'cfg(not(target_family = "wasm"))'.dependencies]`): fastant,
  and every native-only extra below. Nothing native-only may reach a
  wasm compile through a feature alone.

**Per-item verdicts:**

| item | wasm verdict |
|---|---|
| fastant clock | native-only dep; wasm keeps web-time (fastant's fallback hits std Instant = panic) |
| linkme registry | empty slice today → per-enum `ENTRIES` + composition root (§11a.2) |
| `backtrace` capture | compiles (std::backtrace returns Disabled on wasm); capture hook no-ops. Fine as feature-gated no-op |
| panic-hook unification | `std::panic::set_hook` EXISTS on wasm — works; chain with console_error_panic_hook (document; don't stomp) |
| hook panic containment | `catch_unwind` never catches under panic=abort (wasm default). BUT check-wasm already requires nightly+build-std: investigate `-C panic=unwind -Z build-std` + `target-feature=+exception-handling` — containment on wasm becomes real. Keep abort assumption default |
| TLS (scope stack, instant spans, throttle) | fine — single-threaded, thread_local! works |
| parking_lot / hook registries / counts | fine (already proven by current wasm CI) |
| `log-file`, `log-syslog`, `log-journald`, `log-async` appenders | native-only (threads/fs/syslog sockets). Builder methods cfg'd out on wasm with docs pointing at WebConsoleAppend |
| `.console()` builder on wasm + `web` feature | maps to WebConsoleAppend instead of stdout (stdout writes vanish) |
| diagnostics | ariadne pure-render: fine. fs-err reads fail gracefully → ariadne error note; `SourceStore` (in-memory) is THE wasm path — the two compose, no wasm-specific code needed |
| Tier-2 profilers | puffin/tracing: cross-platform OK. tracy/optick/superluminal: native-only — document the consumer-side pattern: target-gated features in THEIR Cargo.toml. Our macro forwards `profiling::scope!` unconditionally; upstream no-ops where unsupported |
| `fastrace` on wasm | NOT wasm-verified (fastrace times via fastant). Current wasm recipe already excludes it (`--no-default-features`). Keep, document, CI-probe separately; do not claim support |
| `flush-on-exit` (atexit/SIGTERM) | native-only. wasm+web equivalent: optional `beforeunload`/`pagehide` flush via web-sys — nice-to-have, gated |
| `int-tokio` JoinError adapter | native-first (tokio rt on wasm is partial); gate and revisit |
| `retry_policy` | core decision logic is sync + portable; only the SLEEP adapters are native/tokio. Take `sleep: impl Fn(Duration)` so wasm callers pass gloo-timers/wasm-bindgen-futures |
| env vars | `env::var` returns Err on wasm → defaults apply (already the pattern). NEVER `set_var` outside native tests |
| sysexits / `main() -> Result` | compiles; exit codes are WASI-meaningful, browser-no-op. Fine |
| breadcrumbs / scope stack / FaultCollection / attachments | pure data structures — fine |

**W3C / WASI / WIT additions (cutting-edge wasm deployments):**

- `trace_context` helpers on top of fastrace's `W3CTraceContext`:
  `extract(&headers) -> SpanContext` / `inject(ctx) -> String`, and
  `root_span!` gains a continuation form `root_span!("name", ctx)`.
  Covers wasi-http handlers, axum, manual propagation.
- Component-model boundary rule (documented pattern): TLS scope stacks
  do not cross host-task lifts — extract ctx at the WIT boundary, root a
  new span on entry, inject on outbound. Context as VALUES at
  boundaries, TLS inside.
- `web` extension: instant-backend spans also emit
  `performance.mark()/measure()` via web-sys — `scope!` regions appear
  in the browser devtools timeline. Nothing else in Rust wasm does this.
- `wasi-logging` prototype feature: an `Append` impl over the
  `wasi:logging` WIT import (host-provided logging for components).
  `wasi-observe` proposal (traces/metrics imports) tracked; our
  Append/Reporter adapter layer makes each a thin feature on arrival.
  OTLP/HTTP over wasi:http for the OTel reporter: flagged, not built.

**CI matrix addition** (`just check-wasm`): no-default+instant,
no-default+web (have), PLUS probes: `+serde`, `+backtrace`
(expect no-op), and a non-CI `check-wasm-fastrace` experiment to learn
whether fastrace compiles/panics on wasm at all.

Net: the only DESIGN changes forced by wasm are the registry composition
root (§11a.2), the appender cfg-gating table, and retry_policy taking a
sleep fn. Everything else was already portable or is a documentation/CI
item — the layer-0 purity rule (no logforth/fastrace names below the
deploy layer) is what makes that true.

## 11c. Nightly-mandatory feature policy (1.99 nightly)

Premise: nightly is REQUIRED to consume the crate (accepted — dev tooling,
not a published library constraint). Rules:

1. Every gate is declared at lib.rs root with its tracking-issue link.
2. Gates may add impls and internals but must never be load-bearing for
   the public API SHAPE — if a feature is removed from nightly (never
   merges), refactor cost is internal-only.
3. `rust-version` in Cargo.toml becomes meaningless → drop it, README
   states the nightly requirement (devenv pins the toolchain).

### Adopt (direct value to this design)

| feature | what it buys |
|---|---|
| `error_generic_member_access` | `Error::provide` / `request_ref` / `request_value` — std's canonical typed-attachment channel. Our §5.7 attachments get exposed through it, and anyhow/error-stack interop flows through the same API (both read `provide` on nightly). This is the §9b forward-compat item, used NOW |
| `backtrace_frames` | programmatic `Backtrace::frames()` — the report renders structured, greppable frame lines (`frame 3: load_config at src/config.rs:42`) instead of dumping the raw backtrace text blob. LLM-readable backtraces |
| `gen_blocks` | `gen` blocks make tree iteration trivial: `Fault::iter()`, `iter_reports`, recursive report rendering — each is a 5-line generator instead of a hand-rolled explicit-stack iterator. The poster-child use case |
| `trait_alias` | `trait ErrorSendSync = Error + Send + Sync + 'static;` — that bound list appears ~20× in exn.rs alone; also the public-facing bound users write |
| `doc_cfg` (subsumes `doc_auto_cfg`) | automatic feature badges on every rustdoc item. For a crate with ~25 features this is the difference between usable and useless docs — users must see which feature gates each API |
| `proc_macro_diagnostic` | spanned compile errors from proc macros — REQUIRED for the next item's UX |
| `#[thread_local]` attribute | zero-flag-check TLS on the hottest path (scope entry touches clock + scope stack + span buffer). Verify wasm behavior; cfg-fallback to std `thread_local!` if it misbehaves |

### `error!` becomes a proc macro (consequence of the above)

`macro_rules` technically parses `#[error("...")]`-style attributes
(`$(#[$m:meta])*`), but thiserror-compatible enum syntax (generics, where
clauses, tuple/newtype/struct variants, per-variant optional attrs) in
macro_rules is a maintainability dead end, and its compile errors are
notorious. Decision: **`fast-observe-macros` companion crate** (the
serde_derive/thiserror-impl pattern — the only exception to the
single-crate rule), using `proc_macro_diagnostic` for spanned,
actionable errors ("variant `E100` has `#[code]` but no `#[category]` —
registry entries drive retry policy"). trybuild tests pin the messages.
`bail!`/`ensure!`/`scope!` stay macro_rules in the main crate (simple,
hygiene-critical).

Parser choice for `fast-observe-macros`: **venial**, not darling/syn.
The macro parses enum syntax + ~5 simple attributes; darling's value is
declarative attribute SCHEMAS (`FromDeriveInput`) and multi-error
accumulation over syn — we don't need schema machinery (`code`/`category`/
`advice` literals + `from`/`source` flags are trivially hand-interpreted),
and venial keeps the macro crate's own compile time off every consumer's
build path (proc-macro2 + quote + venial; no syn-full, no darling). Two
design points make venial sufficient:

- **No format-string parsing.** `#[error("...")]` templates forward ALL
  fields as named args to `write!` (`write!(f, tpl, field = self.field, ..)`
  — current `define_errors!` already does this), positional `{0}` for
  tuple variants likewise. thiserror's mini format-parser is avoided
  entirely.
- **Unknown-attribute forwarding is mandatory**: `#[cfg]`, `#[doc]`,
  `#[serde]` on variants/fields must re-emit onto generated structs,
  From impls, and registry entries. venial's token-level attributes make
  pass-through natural. Doc comments additionally feed the `advice`
  default (§9c.22).

Multi-error accumulation is hand-rolled on top of `proc_macro_diagnostic`
(collect `Vec<Diagnostic>`, emit all) — the one darling feature worth
replicating.

### Adopt (cheap, low risk)

- `try_blocks` (+ `yeet` if still gated): internal error paths — builder
  init, capture-hook application — read linearly instead of match
  pyramids. Internal only per rule 2.
- `error_iter` (if still unstable): `error.sources()` replaces the
  hand-written `walk_sources` loop.
- `non_exhaustive_omitted_patterns` lint: `Context` and `ErrorCategory`
  are `#[non_exhaustive]` — this lint catches silently-incomplete matches
  when variants are added.
- `rustdoc::missing_doc_code_examples` (rootcause enforces it): docs
  quality gate for a docs-as-interface crate.
- `-Znext-solver` in rustflags: better trait-error messages for users
  hitting the `Fault<E>` bounds; watch compile-time, drop if it regresses.
- `-Zbuild-std` uniformly (not just wasm): enables the §11b wasm
  panic=unwind + exception-handling experiment (real catch_unwind
  containment on wasm) and `panic_immediate_abort` size probes.

### What the gates unlock for USERS (capability ceiling, not convenience)

- **`error_generic_member_access`**: attachments become queryable through
  `&dyn Error` — a generic middleware or logger that knows NOTHING about
  fast-observe can extract `TraceId`, `Location`, `Backtrace`, `ErrorCode`
  from any Fault via std's protocol. Reverse direction too: `from_anyhow`
  extracts anyhow's provided Location/Backtrace into our frames. The
  trace/registry machinery becomes an ECOSYSTEM protocol, not a
  fast-observe silo.
- **`backtrace_frames`**: backtraces become DATA — filter out our own
  frames, dedup, "top 3 app frames" summary in the one-line log with the
  full frames as an appendix attachment. (color-backtrace does frame
  filtering by parsing text; we get it structurally.)
- **`fmt::from_fn` (`fmt_from_fn`)**: the report renders as a streaming
  `impl Display` into ANY writer — log record, socket, ring buffer —
  with zero intermediate `String`. `render_report() -> String` stays for
  snapshots/tests; the hot hook path streams.
- **`gen_blocks`**: lazy tree iteration composes —
  `fault.iter().filter_map(downcast::<NetworkError>)` — and rendering a
  10k-node `FaultCollection` never materializes intermediate Vecs.
- **`coverage_attribute`**: `#[coverage(off)]` on `#[cold]` error
  constructors — llvm-cov numbers reflect exercised code, not cold-path
  noise (cargo-llvm-cov is already in the devenv toolset).
- **`assert_matches`**: test ergonomics for pattern+payload assertions
  across the Fault tree.
- **`#[thread_local]`**: scope-entry TLS access loses the lazy-init flag
  check — the ON path moves toward the off path's ~2ns budget (wasm
  verification + cfg-fallback per §11b).
- **`-Znext-solver`**: users hitting `Fault<E>` bounds get the new
  solver's materially clearer where-clause errors at our generic
  boundaries.
- **`-Zub-checks` (already in rustflags) + miri (already a component)**:
  add a `just miri` recipe running the hook/TLS/instant-stack tests under
  miri — dev-mode UB detection across the dep tree.

Toolchain pinning: rolling nightly via devenv. Do NOT
add rust-toolchain.toml — it fights the devenv-provided toolchain;
revisit only if non-devenv contributors appear.

### Rejected even with nightly free rein

- `specialization` / `min_specialization` — soundness gray zone, and our
  own lints philosophy forbids it; the `&dyn Coded` slot at capture time
  solves the same problem safely.
- `adt_const_params` for `const CODE: &str` type-level codes — infects
  every error type signature for zero ergonomic gain; runtime `Coded`
  trait wins.
- `derive_smart_pointer` — saves 5 lines of `Deref` boilerplate, not
  worth a gate.
- `macro_metavar_expr` / `decl_macro` — moot once `error!` is a proc
  macro; `decl_macro` is too experimental even for this policy.

## 11. Open questions (decide at implementation time)

- `ProfilingBackend` stays a closed enum (simple) vs. extensible sink trait
  (pluggable custom runtime backends). Lean: closed enum + Tier-2 covers it.
- `report_or_die`/process-exit helpers: wait for user demand.
- Whether `define_errors!` grows `#[advice = "..."]` per variant feeding
  both Diagnostic notes and the report action line. Lean: yes.
