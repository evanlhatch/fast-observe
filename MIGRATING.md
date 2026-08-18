# Migrating from flatland-observe

`fast-observe` is a generalization of `flatland-observe` (flatland). APIs
whose names were not flatland-specific are unchanged. This file lists every
intentional difference.

## Crate / module rename

| flatland-observe | fast-observe |
|---|---|
| `flatland_observe` | `fast_observe` |
| log target `flatland.error` | `fast_observe.error` |
| log target `flatland_observe.diagnostic` | `fast_observe.diagnostic` |

## Renames

| flatland-observe | fast-observe | Notes |
|---|---|---|
| `config::FlatlandConfig` | `config::ObserveConfig` | same API plus throttle |
| `hook::set_error_hook` | `hook::add_error_hook` | **semantics changed**: replace → append |
| feature `profile-with-fastrace` | feature `fastrace` | now on by default |
| feature `profile-with-instant` | feature `instant` | instant backend was always compiled before; now opt-in |

## Behavior changes (precise)

1. **Hooks fan out instead of replace.** `add_error_hook` appends to a sink
   list; the lazily installed default hook stays unless you never need it.
   Each hook invocation is wrapped in `catch_unwind(AssertUnwindSafe(..))` —
   a panicking hook no longer propagates into the error path.
2. **New global throttle.** `ObserveConfig::set_error_hook_throttle(n)` caps
   hook invocations at `n` per error *type* per second. Default `0` =
   unlimited (matches old behavior). `error_counts()` is NOT throttled —
   counting always happens.
3. **`render_diagnostic` returns `String`.** It rendered to stderr before
   (returned `()`); the old stderr behavior is now `eprint_diagnostic`.
   String rendering disables ANSI colors. New `serde` feature derives
   `Serialize`/`Deserialize` for `Diagnostic`/`Severity`/`SourceSpan`.
4. **`Diagnostic.code: String`** (was `&'static str`) — required for serde
   `Deserialize` (`&'static str` cannot be deserialized from non-`'static`
   input). `Diagnostic::error()` still takes `&str`. `SourceSpan.file`
   stays `camino::Utf8PathBuf` (serde via camino's `serde1` feature, pulled
   in by the `serde` feature).
5. **`Context` lost two variants**: `Node(Cow, u64)` and
   `Actor(u32, Cow)` (plus `Context::node()` / `Context::actor()`). Use
   `Entity` or `Custom`. `None`/`Scope`/`Tick`/`Entity`/`Custom` and their
   `Display` strings are unchanged.
6. **`lib::metrics()` removed** — the vortex-metrics registry is decoupled.
   Feature `metrics-facade` instead mirrors `error_counts` through the
   `metrics` crate facade (`counter!("fast_observe.errors", "type" => ..)`
   on every error construction).
7. **Clock switched to `web-time`.** The rdtsc/`CNTVCT_EL0` architectural
   counter (with per-process calibration) is gone; span timing uses
   `web_time::Instant`, which is wasm-safe. On native this delegates to
   `std::time::Instant` — reads cost ~tens of ns instead of ~2ns under WSL2;
   the instant backend is still correct, just slightly more expensive per
   scope. Trade-off accepted for wasm portability.
8. **`init()` never panics on double init** — it uses logforth
   `try_apply()` and ignores the already-set error. It composes only
   feature-enabled appenders/diagnostics (stdout always; fastrace event +
   diagnostic with `fastrace`; JSON layout with `json`; rolling file with
   `file` + `OBSERVE_LOG_DIR`; browser console with `web` on wasm32).
9. **`ProfilingBackend::Web` added** (`OBSERVE_PROFILE=web`). Behaves like
   `Instant` for span timing; the browser-console half is a log appender,
   active only on wasm32 with feature `web`.
10. **Error registry on wasm.** `linkme` has no linker-section support
    for wasm — on wasm, `error!` emits per-enum `ENTRIES` slices and the
    app registers them once via `fast_observe::register_statics(&[...])`.
    Consumers need NO `linkme` dependency on any target: registrations use
    linkme's `#[linkme(crate = ::fast_observe::__private::linkme)]`
    override so every `::linkme::` path in the expansion resolves through
    fast-observe's re-export. Without `register_statics`, `lookup_error`
    returns `None` on wasm. All other `error!` output (`code()`,
    `category()`, `Display`, `From`, registry `ENTRY` consts) is unchanged
    across targets.
11. **Default features**: `fastrace` + `bridge-log` (flatland-observe had
    none). `--no-default-features` gives a minimal build; `bridge-log`
    separately re-exports `logforth::bridge::log` for custom pipelines.

## Unchanged

`Fault` / `Frame` / `bail!` / `ensure!` / `ResultExt` (`change_context`,
`wrap_msg`, `observed`, `report`) / `OptionExt` / `error_counts` /
`define_errors!` syntax / `scope!` / `profiling!` / `finish_frame!` /
`all_functions` / `skip` / `breakdown::{drain_spans, print_breakdown}` /
lazy default-on error hook / `#[track_caller]` locations.

## 0.1.0 → next (the overhaul)

The DESIGN.md/SURFACE.md overhaul lands in stages; this section tracks what
has landed. Items marked *(landing)* are in DESIGN.md but not yet in `src/`.

### Renames / removals

| 0.1.0 | next | Notes |
|---|---|---|
| `ProfilingBackend` enum | `config::Backends` bitmask | runtime-selectable SET, not one backend |
| `config().set_profiling_backend(..)` | `config().set_backends(Backends::FASTRACE \| Backends::TRACY)` | mask 0 = off, ~2ns `scope!` |
| `OBSERVE_PROFILE=off\|instant\|fastrace\|web` | comma list, e.g. `OBSERVE_PROFILE=fastrace,tracy` | single names keep working; `off` must appear alone; case-insensitive |
| `Frame` public fields | **private fields + getters** | breaking: `frame.error` → `frame.error()`, likewise `location()`, `context()`, `children()`, `type_name()`, `attachments()`. Construction only via the internal `Frame::capture` — hand-built frames can no longer bypass hooks/counters |
| `profiling-procmacros` re-exports (`all_functions`/`skip`; upstream `function`) | own macros in `fast-observe-macros`: `#[instrument]` (upstream `function` **renamed**), `#[all_functions]`, `#[skip]` | the old re-exports expanded to `profiling::` paths and were uncompilable without a direct `profiling` dep; the new ones expand to `::fast_observe::` paths. Async fns are rejected (scope guards are thread-bound) |
| feature `profile-with-optick` | **dropped** | optick is unmaintained upstream and broken on modern toolchains; use `profile-with-tracy` or `profile-with-puffin` |

### Additions

1. **Tier-2 profiling features**: `profile-with-{puffin,tracy,superluminal,tracing}`
   compile backend glue in; the `Backends` mask selects live backends at
   runtime. Compiled-in ≠ active. Requesting an uncompiled backend logs a
   one-time warning naming the exact cargo feature (self-teaching).
2. **`observe()` builder** (`deploy` module): `Deployment` (bon builder) →
   `.init()? -> InitGuard`. Unlike `hook::init()` (which stays the
   zero-config path and still swallows double-init), the builder reports
   `InitError::AlreadyInitialized`, and dropping `InitGuard` flushes
   fastrace. `hook::init()` now delegates to the builder
   (`observe().init()` with the result ignored). Builder-only toggles:
   `panic_hook` (default `true` — panics logged as structured error events
   on target `fast_observe.panic`, then the previous hook runs) and
   `flush_on_exit` (default `true`; feature `flush-on-exit`, native only —
   best-effort fastrace flush on atexit/SIGTERM/SIGHUP).
3. **Capture hooks** (new hook family): `add_capture_hook(fn(&mut Frame))`
   runs DURING frame construction and may attach data. Built-ins attach the
   fastrace `trace_id` (feature `fastrace`, plus an `error` span event) and
   the scope path / leaf elapsed ms. Capture hooks are not throttled; sink
   hooks (`add_error_hook`) are unchanged. New management fns:
   `clear_error_hooks()`, `hooks_len()`, `set_default_hook_enabled(bool)`.
4. **Typed attachments**: `Attachment` (cached display + typed value +
   `Placement::{Inline, Appendix, Opaque, Hidden}`), `Fault::attach` /
   `attach_key` / `attach_placed`, `ResultExt::attach` / `attach_with`
   (lazy, error-path only), `Frame::find_attachment::<T>()`.
5. **`FaultCollection`** — multi-failure aggregation (retry/batch): collect
   faults of any `E` (`FromIterator` included), then
   `into_fault(err)`/`into_fault_msg(msg)` wraps them under one root.
6. **Tree traversal + inspection**: `Fault::iter()` / `Frame::iter()`
   (preorder `Arc<Frame>`), `Fault::root_cause()`, `Fault::into_frame()`.
7. **`doctor(code) -> Option<String>`** — key:value report from the error
   registry; `ErrorCategory::policy() -> Policy` and
   `Policy::advice_line()` make category behavioral;
   `ErrorRegistryEntry` gained an `advice` field (struct literals in
   consumer code break; `define_errors!` output is regenerated).
8. **`SamplingReporter` / `MultiReporter`** (feature `fastrace`): traces
   with an `error` span event are always kept, clean traces sampled 1-in-N;
   fan-out to several reporters (console + OTel).
9. **`error!` proc macro** — thiserror-attribute syntax (`#[error]`,
   `#[from]`, `#[source]`) + `#[code]`/`#[category]`/`#[advice]` in
   `fast-observe-macros`, re-exported as `fast_observe::error!`. Generates
   the enum, one public struct per struct variant, `Display`/`Error` with
   auto-wired `source()`, `From<Variant>` for the enum AND for
   `Fault<Enum>`, registry entries with advice (default: first doc line),
   nightly `Error::provide` of `ErrorCode`/`CategoryTag`, and a 64-byte
   size assertion. v1 limits: no generics; `#[from]` needs a single-field
   tuple variant; no `From<InnerType> for Fault<Enum>` (orphan rule) —
   write `err.map_err(Enum::from)?`. `define_errors!` is now the
   deprecated path, supported through 0.x.
10. **Report module** — the prescriptive renderer (DESIGN.md §7):
    `render_report(&fault) -> String`, `report_display(&fault)` (streaming
    `Display`), `render_report_json(&fault)` (feature `serde`, versioned
    `"schema": 1`). Fixed section order, one fact per line, no colors or
    timestamps. Codes/categories are read through `Error::provide` with a
    registry fallback, so `error!` types report fully.
11. **`Coded` trait + `ErrorCode`/`CategoryTag`** — codes/categories
    through `Error::provide` (nightly `error_generic_member_access`).
    `error!`-generated types provide them automatically; `define_errors!`
    does NOT emit `Coded` impls *(landing)*.
12. **anyhow boundary** (feature `anyhow-boundary`) —
    `compat::anyhow_boundary::{from_anyhow, into_anyhow}`: explicit
    `map_err` points, never implicit `From`. anyhow erases its sources, so
    the wrapped chain survives via `{:#}`/`{:?}` formatting only; typed
    `Fault` → anyhow keeps the causal tree through `Fault`'s `source()`/
    `Debug`. `Fault<SimpleError>` cannot go into anyhow (a boxed dyn error
    is not `Error`). `compat-eyre` / `compat-error-stack` / `int-tokio` /
    `retry_with_policy` are now LANDED — see below.
13. **eyre / error-stack / tokio boundaries + policy helpers** (features
    `compat-eyre`, `compat-error-stack`, `int-tokio`) —
    `eyre_boundary::{from_eyre, into_eyre}`;
    `error_stack_boundary::{from_error_stack, into_error_stack}`
    preserving the typed context + frame stack (note: `into_error_stack`
    returns `Report<Fault<E>>` — the Fault becomes the context, `Deref`
    reaches E); `tokio_ext::ObserveJoinExt::observe_join` distinguishing
    cancelled vs panicked tasks. Also landed: `retry_with_policy`
    (policy-driven retries, attempts collected into one fault),
    `Fault::policy()` / `exit_code()` (sysexits),
    `error_counts_by_category()` (registry-suffix heuristic), codes in the
    Debug tree via `Error::provide`.

### Behavior changes (precise)

1. **`Frame`/`Fault` implement `Error::source()` → first child.** Generic
   source walkers (anyhow chain walkers, `error.sources()`) now traverse
   the causal tree — the same structure `Debug` renders. Code that walks
   `source()` sees frames it did not see before.
2. **`walk_sources` nests instead of flattening.** `A→B→C` used to become
   siblings of the root; now `B` is a child of `A`, `C` a child of `B` —
   the tree mirrors causality. `type_name` is preserved per frame.
3. **`wrap_msg` preserves the source chain.** It previously dropped it;
   the wrapped error enters as a child frame WITH its nested chain.
4. **`wrap` fires the hook for the new root.** The wrapper type is counted
   in `error_counts()` (the original fault was counted at its own
   construction, not again).
5. **Scope stack, not slot.** Nested `scope!`/`profiling!` used to
   overwrite `CURRENT_SCOPE`; now they push/pop. `current_scope_name()`
   reads the LEAF (unchanged signature, changed result under nesting);
   new `scope_path()` (outermost → innermost) and
   `current_scope_elapsed_ms()`. Faults capture the scope path + leaf
   elapsed as attachments via the built-in capture hook.
6. **Dynamic `scope!` names are interned** — one bounded leak per unique
   name instead of `Box::leak` per call.
7. **`scope!` loads the backend mask once** and enters one guard per
   selected backend (ZST stubs for the rest); mask 0 = all-dummy, ~2ns.
8. **`with_context` / attachments on a shared root** now `debug_assert!`
   instead of silently no-oping.
9. **`fastrace` feature forwards `fastrace/enable`.** fast-observe is the
   app-facing deployment crate — without `enable` every fastrace span
   silently no-ops. Libraries depending on fast-observe must use
   `default-features = false` and let the binary enable fastrace
   (fastrace's library-level-tracing rule).
10. **Panic hook installed by default.** The `observe()` builder defaults
    `panic_hook(true)`: panics are logged as structured error events
    (target `fast_observe.panic`, kv `panic_file`/`panic_line`), then the
    previously installed hook runs — chaining, never stomping. Opt out
    with `.panic_hook(false)`. With feature `flush-on-exit` (native),
    `flush_on_exit(true)` additionally flushes fastrace on
    atexit/SIGTERM/SIGHUP.
