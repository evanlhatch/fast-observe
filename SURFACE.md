# fast-observe — user surface design

Internals are in DESIGN.md. This doc is the contract with the user. Grounded
in observed usage in ~/flatland (the only real consumer):

- Verbs actually used: `bail!` ×57, `.report()` ×32, `scope!` ×15.
  `change_context`/`wrap_msg`/`observed` ≈ 0. Constructor + swallow are the
  vocabulary; design for that.
- Every crate hand-writes 4 things around `define_errors!`: the enum, the
  `impl Error`, a constructor helper (`compile_err!`), a size assertion.
- flatland-renderer has 5 variants with `source: Box<RenderError>` fields
  and an `impl Error` with **no `source()`** — causal chain silently broken.
- `fast_observe::init()` is the only setup call. Doctor CLI hand-rolled.

---

## 1. The adoption funnel (the whole product in three tiers)

A project adopts fast-observe incrementally; each tier works alone:

```
Tier A — drop-in (own nothing):
    fast_observe::observe().dev()?;
    → existing log::*/tracing::* call sites flow into the pipeline.
    → anyhow errors convert at your API boundary.
    → panics/errors get codes, locations, prescriptive output.

Tier B — instrument (keep your error types):
    scope!("journal.compact") / #[profiling::function]
    → spans to fastrace AND any compiled Tier-2 profiler (tracy, puffin…).

Tier C — typed errors (the full causal model):
    fast_observe::error! { pub enum EngineError { ... } }
    → codes, registry, doctor, causal trees, prescriptive reports.
```

No tier requires the next. A serde-using library never hears about logforth;
a binary gets everything from one `observe()` call.

---

## 2. Deployment surface (Tier A) — bon + strum, one entry point

One function returns one builder; three presets cover 90%; every field
optional; env vars are overrides, not requirements.

```rust
use fast_observe::prelude::*;

// NO environment presets (dev/prod/test) — rejected design: they bundle
// opinionated choices and hide the toggles. Everything is granular:
// each capability is one builder toggle with a documented default, so the
// type surface IS the documentation and nothing is hardcoded.
// Zero-config `fast_observe::init()` stays for the default-on path.

// ── Full control (bon builder; .build() validates) ───────────────────
let deployment = fast_observe::observe()
    .level(log::LevelFilter::Info)              // default: env OBSERVE_LOG → RUST_LOG → "info"
    .console(Console::Stdout, Layout::Text)     // Layout::{Text,Json,Logfmt,Gcl}
    .errors_to_stderr(true)                     // error+ split, like starter-log demo
    .file_from_env()                            // rolling file when OBSERVE_LOG_DIR set
    .otel_logs(log_exporter)                    // caller builds exporter (SDK config is app's)
    .traces(Traces::Console)                    // Traces::{Console,Off} or .otel_reporter(r)
    .backends(Backends::FASTRACE | Backends::TRACY)  // runtime-selected set; compiled-in ≠ active
    .error_hooks(|h| h.throttle_per_type(100).backtrace(true))
    .appender(my_custom_append)                 // escape hatch: any logforth Append
    .build();                                   // type-checked, env applied, returns Deployment
let _guard = deployment.init()?;                // Err(InitError) on double-init;
                                                // guard Drop → fastrace::flush()
```

### Why bon, why strum

- **bon**: `#[derive(Builder)]` gives named optional fields, `Into`
  conversions (`.console(Console::Stdout, "json".parse()?)`), `maybe_*`
  setters for config-file plumbing, and a compile error if a required piece
  (otel exporter) is missing — without hand-written typestate gymnastics.
- **strum**: every choice is an enum with `EnumString` + `Display` +
  `VariantNames`. Env parsing, CLI parsing (`value_enum`), and `--help`
  listing come free. `ProfilingBackend` already exists; `Layout`,
  `Console`, `Traces`, `Preset` follow the same pattern. No stringly-typed
  config anywhere.

### Output routing — modeled as types, not feature soup

| destination | type surface | feature |
|---|---|---|
| stdout/stderr (text) | `Console`, `Layout::Text` (color: tty && !NO_COLOR) | always |
| stdout json/logfmt/gcl | `Layout::{Json,Logfmt,Gcl}` | `layout-json` / `layout-logfmt` / `layout-gcl` |
| rolling file | `.file_from_env()` / `.file(dir, rotation)` | `log-file` |
| OpenTelemetry logs | `.otel_logs(exporter)` | `otel` |
| syslog / journald | `.appender(append::Syslog::…)` via escape hatch | `log-syslog` / `log-journald` |
| async offload | wraps any appender: `.console_async(..)` | `log-async` |
| fastrace events | automatic when traces on (correlation) | `fastrace` |
| test capture | `TestCapture` handle from `.test()` | `log-testing` |
| anything else | `.appender(impl Append)` / `.layout(impl Layout)` / `.filter(impl Filter)` | `log-*` deps |

Layout parameters exist only on methods whose appenders use layouts —
logforth documents that FastraceEvent/Async/OTel ignore layouts; our
builder makes that unrepresentable instead of surprising.

### Env surface (parsed once, one table in README)

`OBSERVE_LOG` (→ RUST_LOG fallback, needs `filter-rustlog`), `OBSERVE_PROFILE`,
`OBSERVE_LOG_DIR`, `OBSERVE_ERROR_THROTTLE`, `OBSERVE_BACKTRACE`,
`OBSERVE_COLOR`, `OTEL_EXPORTER_OTLP_ENDPOINT` (prod preset auto-wires otel
when set). `Deployment::from_env()` = zero-code deployment.

### Config files — figment-compatible, NOT figment-dependent

Decision: **do not depend on figment/config-rs.** We are a leaf crate with
~10 knobs; figment's job (merging TOML+env+CLI hierarchies) is the APP's
concern, and dragging figment+toml into every consumer of an observability
leaf is the opposite of lean.

What we do instead — the pattern good leaf crates use (serde_json doesn't
use figment; figment composes serde_json):

- `DeploymentConfig`: a plain-data mirror of the builder (all fields
  `Option`, strum enums for every choice) with
  `#[derive(Serialize, Deserialize)]` under the existing `serde` feature.
- `Deployment::from_config(cfg)` / `builder.config(cfg)` consumes it.
- Apps using figment embed `DeploymentConfig` in their own config struct;
  `observe.toml` / `[observe]` in app config just works.
- The TOML schema is documented in README — and a documented config-file
  schema is itself an LLM-adoption surface: agents that won't learn a
  builder API will happily write `[observe]\nlayout = "json"`.

---

## 3. Facades: log, tracing, anyhow — the plug-in story

The project being observed keeps its facades; fast-observe is the backend.

### log (already true, keep + document)

`log::info!` etc. flow through logforth's bridge (default feature
`bridge-log`). Every dependency's log output lands in the same pipeline
with the same layouts, filters, MDC, and trace-id stamping.

### tracing — two distinct bridges, both kept, names disambiguated

1. **`tracing-compat`** (have, rename from `bridge-tracing`):
   fastrace-tracing — `tracing::span!` in dependencies become fastrace
   spans. One trace tree across both ecosystems.
2. **`profile-with-tracing`** (new, Tier-2): upstream `profiling` crate's
   tracing backend — OUR `scope!` emits `tracing::span!`, so an app with
   an existing tracing-subscriber stack (fmt layer, otel layer) sees
   fast-observe scopes without fastrace. Direction is opposite to (1);
   the README needs one paragraph + a diagram for exactly this.

### anyhow — explicit boundary (exn-anyhow pattern, feature `anyhow-boundary`)

```rust
use fast_observe::anyhow_boundary::{from_anyhow, into_anyhow};
let ours  = their_result.map_err(from_anyhow);   // anyhow::Error → Fault<AnyhowError>
let theirs = our_result.map_err(into_anyhow);    // Fault<E> → anyhow::Error (tree → message chain)
```

Never implicit `From<anyhow::Error> for Fault` — boundary conversions are
deliberate. The whole `Fault` tree survives `into_anyhow` via a custom
Display/Debug; `from_anyhow` walks anyhow's chain into nested frames.

### profiling — fastrace-first, upstream as Tier-2

Upstream `profiling` supports puffin/optick/tracy/superluminal/tracing but
NOT fastrace — and fastrace is our Tier-1 default. Resolution (unchanged
from DESIGN.md §2, restated as surface):

- `scope!` expands to fastrace/instant (runtime-selected Tier-1) PLUS
  `profiling::scope!` (Tier-2, compiles to nothing without features).
- Features mirror upstream verbatim:
  `profile-with-{puffin,optick,tracy,superluminal,tracing}` →
  `profiling/{same}`. Users already knowing upstream `profiling` know our
  feature names.
- `#[profiling::function]`, `#[all_functions]`, `#[skip]`,
  `register_thread!`, `finish_frame!` all re-exported — upstream docs
  remain valid documentation for our macro surface.

LLM-surface rules (the `profiling` crate is in training data; exploit it):

1. Macro names AND call shapes stay byte-identical to upstream —
   `scope!("literal")`, `scope!("literal", "tag")` — so LLM-generated
   instrumentation compiles without correction. Literal-first, always.
2. Feature names verbatim `profile-with-*` — agents guess them
   correctly from upstream knowledge.
3. The ONE semantic difference must be loud in README/docs: upstream is
   compile-time one-backend-at-a-time; ours compiles any set in and
   SELECTS AT RUNTIME (`Backends` bitmask, `OBSERVE_PROFILE=fastrace,tracy`).
   Compiled-in ≠ active. Agents WILL assume upstream's rule — and setting
   an uncompiled backend logs a warning naming the feature to enable, so
   the correction is self-teaching.
4. All of it lands in `prelude` — one glob import = the whole familiar
   vocabulary.

---

## 4. Feature-forwarding: the whole ecosystem, pay-as-you-go

Rule: one feature per ecosystem crate, named after it, `dep:`-gated,
re-exported under one namespace. Nothing compiles unless named. This is
the entire mechanism — there is no wrapper code to maintain.

```rust
#[cfg(feature = "int-axum")] pub use fastrace_axum;      // under fast_observe::x
```

| feature | forwards |
|---|---|
| `profile-with-{puffin,optick,tracy,superluminal,tracing}` | profiling backends |
| `layout-{json,logfmt,gcl,text}` | logforth-layout-* |
| `log-{file,syslog,journald,async,testing}` | logforth-append-* |
| `otel` | logforth-append-opentelemetry + fastrace-opentelemetry |
| `filter-rustlog` | OBSERVE_LOG/RUST_LOG directives |
| `diag-task-local` | async MDC |
| `tracing-compat` | fastrace-tracing |
| `int-{futures,axum,poem,tonic,tower,reqwest}` | fastrace-* integrations |
| `anyhow-boundary`, `backtrace`, `serde`, `metrics-facade` | error model extras |

Default stays `["fastrace", "bridge-log"]`: the fastrace-first happy path
with log bridging, nothing else.

**wasm availability** (wasm32-unknown-unknown; full audit in DESIGN.md
§11b): `web`, `instant`, `serde`, `backtrace` (no-op), layouts, filters,
diagnostics — yes. `log-file`/`log-syslog`/`log-journald`/`log-async`,
`int-tokio`, native Tier-2 profilers (tracy/optick/superluminal) —
native-only, cfg'd out. `fastrace` is not wasm-verified: the documented
wasm recipe stays `--no-default-features --features web`. Consumers gate
native-only features in their own Cargo.toml via target-specific
dependencies — the standard pattern, documented in README.

---

## 5. Tier C: `error!` — declarative errors, thiserror-grade

Replaces `define_errors!` + the 4 hand-written artifacts around it. Real
before/after from flatlandc (58 lines → ~12):

```rust
// BEFORE (crates/flatlandc/src/errors.rs): hand enum + macro block +
// impl Error + compile_err! helper macro + CompileResult alias.

// AFTER:
fast_observe::error! {
    /// flatlandc's error type.
    pub enum FlatlandcError {
        /// Build-time compiler error — designer bug, not engine fault.
        #[code = "E100", category = Content]
        CompileError { detail: String }
            => "compile error: {detail}",

        /// SQL parse failure.
        #[code = "E101", category = Content,
          advice = "check SQL syntax near the reported span"]
        Parse { span: SourceSpan, detail: String }
            => "parse error: {detail}",
    }
}
```

Renderer errors with foreign + recursive sources:

```rust
fast_observe::error! {
    pub enum RenderError {
        #[code = "E428", category = Transient]
        PipelineLayout { #[source] source: Box<RenderError> }   // source() auto-wired
            => "pipeline layout: {source}",

        #[code = "E409", category = Invariant]
        UnsupportedTextureFormatForReadback { format: wgpu::TextureFormat }
            => "unsupported texture format for readback: {format:?}",

        #[code = "E500", category = Invariant, from]            // replaces extra{}
        Vortex(Box<vortex_error::VortexError>) => "vortex: {0}",
    }
}
```

Generated per invocation (nothing hand-written remains):

1. the enum (doc comments pass through) + variant structs
2. `impl Error` — **`source()` auto-wired from any `#[source]`/named-`source`
   field** (kills flatland-renderer's silently-broken chain class)
3. `Display` per variant + enum, from the `=> "…"` template (`{field}` and
   `{field:?}` interpolation, same as today)
4. `From<Variant> for Enum` **and `From<Variant> for Fault<Enum>`** —
   the second one is what kills `compile_err!`: `bail!(CompileError {
   detail })` now works in any fn returning `Result<T, FlatlandcError>`,
   and `Err(CompileError{..})?` propagates. (Orphan-legal: both types
   concrete, variant type local.)
5. `code()` / `category()` / `advice()` + `Coded` trait impl — attribute
   surface is thiserror-compatible (`#[error]`/`#[from]`/`#[source]`); see
   §6a for the full syntax mapping
6. registry entry incl. advice (doctor gets prescriptive text, not just a
   display string)
7. `const _` size assertion (default ≤ 64, overridable
   `#[max_size = 128]` on the enum) — flatland hand-writes this today
8. per-variant constructors `Enum::compile_error(detail)` → `Fault<Enum>`
   — one call from value to wired fault

`define_errors!` stays one release as a deprecated alias (positional form
maps onto the attribute form mechanically).

### bail!/ensure! upgrade (the ×57 verb)

```rust
bail!(CompileError { detail: format!("bad plan: {p}") });   // From<Variant> for Fault<Enum>
bail!(CompileError, "bad plan: {p}");                        // detail-field shorthand (today's form)
bail!(FlatlandcError::CompileError, "bad plan: {p}");        // enum-qualified shorthand
ensure!(tick < MAX, JournalError::OutOfRange, "past last tick {last}");
```

### Doctor, provided

`fast_observe::doctor(code) -> Option<String>` renders code, name,
category, policy, canonical display, advice, defining crate. flatlandc's
hand-rolled 35-line subcommand becomes `print!("{}", doctor(&code)?)`.

---

## 5b. The canonical surface — five items, three ontologies

The primitives are ontological, not historical: POINTS (log macros),
INTERVALS (`scope!`), VALUES (`bail!`/`ensure!` — errors are returned,
not emitted). Collapsing any pair is a category error (interval as two
points loses nesting; error as event loses the type). So the surface
condenses to exactly:

1. **`log` macros** — ALL point events. No fast-observe logging API
   exists. `log::info!(user_id = 42; "msg")` — kv flows through logforth
   into span events automatically (fastrace's design: the log crate is
   the facade, deliberately no logging macros in fastrace itself).
2. **`scope!("phase.op")`** — intervals. (profiling!/function_scope!
   FOLD INTO #[instrument] — the tracing reflex; ours delegates to
   #[fastrace::trace] for async-correctness + pushes the scope stack.)
3. **`bail!` / `ensure!`** — error values. bail! IS the log statement:
   construct + count + span event + log in one verb. Double-reporting
   (`log::error!` then `return Err`) is an anti-pattern the vendored
   AGENTS.md snippet calls out by name.
4. **`#[instrument]` / `#[all_functions]`** — zero-effort propagation
   (shift-left: one attribute instruments a whole impl block).
5. **`error!`** — typed error definitions (codes/categories enforced at
   authoring time = policy shift-left).

Everything else is methods (`.context()`, `.attach()`, `.report()`) and
configuration. A user who knows only `log` + `bail!` gets full value.

## 6. Isomorphism: four unifiers, one vocabulary

"Making them work together" = the same identifiers flowing through every
subsystem. These are the four:

1. **Scope name** — `scope!("journal.compact")` is simultaneously: tracy
   zone, fastrace span, instant span, `scope=` field on every log record
   (ThreadLocalDiagnostic), and the `Context::Scope` auto-attached to any
   Fault raised inside it. One name, five sinks. Convention: dotted
   `crate.phase.op` (flatland already does this organically).
2. **Error code** — `error!` → registry → fault tree display (`[E100]` in
   Debug) → default log line → Diagnostic → report → doctor →
   error_counts + metrics-facade label. Grep one string, get everything.
3. **Trace id** — captured from fastrace SpanContext at Fault construction
   (hook), stamped on every log record (FastraceDiagnostic), printed in
   the report. One id = the entire causal moment.
4. **Category** — drives policy (`Transient → retry is safe`), the
   prescriptive `action:` line in reports, metrics grouping, doctor
   output. Types, not vibes: `ErrorCategory::policy() -> Policy`.

And one verb vocabulary: `bail!` / `ensure!` / `report()` / `scope!` /
`profiling!`. The neglected trio (`change_context`, `wrap_msg`,
`observed`) gets folded: `change_context` resurfaces as
`.context(Enum::Variant)` (generated constructors make this cheap), and
`observed`'s job (message rides as context, type preserved) becomes the
default behavior of `.context_msg(...)`. Fewer verbs, same power.

---

## 6a. The familiar-pattern layer (LLM adoption, zero power cost)

LLM agents write from training distribution: `anyhow::{Result, bail!,
ensure!, Context}`, thiserror `#[derive(Error)] #[error] #[from]
#[source]`, `tracing::{instrument, info!}`, `tracing_subscriber::fmt::init()`,
`env_logger::init()`. They do not know `Fault`/`change_context`/`scope!`.
Strategy: **meet the distribution, keep the power.** Every familiar name
below is a thin alias or superset — nothing typed or explicit is removed.

### Vocabulary mapping (what an agent reaches for → what works)

| familiar (anyhow/thiserror/tracing) | fast-observe | note |
|---|---|---|
| `anyhow::Result<T>` | `fast_observe::Result<T>` (default `BoxError`) | already true |
| `bail!("msg {x}")` / `ensure!` | identical names, identical shape | already true |
| `.context("msg")` | **new primary name** for `wrap_msg` (lazy `impl FnOnce() -> Cow`) | anyhow's verb; keeps Fault typing |
| `.with_context(\|\| ...)` | lazy form of the same | matches anyhow |
| error-stack `.change_context(TypedErr)` | stays as-is (error-stack semantics) | typed context crossing |
| `#[derive(Error)] #[error("...")] #[from] #[source]` | `error!` macro **accepts thiserror's attrs verbatim** and adds `#[code]/#[category]/#[advice]` | see below |
| `#[tracing::instrument]` | `#[fast_observe::instrument]` alias over function-scope + span | one-line shim, huge familiarity |
| `tracing_subscriber::fmt::init()` / `env_logger::init()` | `fast_observe::init()` (zero-config) / `observe().dev()?` | same shape: builder → init |
| `fn main() -> anyhow::Result<()>` | `fn main() -> fast_observe::Result<()>` | works today via `Termination for Result<T,E: Debug>`; upgrade `Fault`'s Debug to the §7 report so the exit path IS the report |

### thiserror-compatible `error!` syntax

The macro accepts the exact thiserror attribute surface as a subset:

```rust
fast_observe::error! {
    pub enum EngineError {
        // an agent writing plain thiserror syntax gets working code:
        #[error("io error: {0}")] #[from]
        Io(std::io::Error),                       // no #[code] → unregistered (today's `extra` block)

        // and upgrades by ADDING attrs, never rewriting:
        #[error("entity not found: {id}")]
        #[code = "E001", category = Content, advice = "check the entity table"]
        EntityNotFound { id: RowIndex },
    }
}
```

`#[error]` = the Display template, `#[from]`/`#[source]` = thiserror
semantics (auto `source()` wiring kills the flatland-renderer broken-chain
class). `#[code]` is the only thing that opts a variant into the registry /
doctor / report codes. `#[category]` is required whenever `#[code]` is
present (registry entries drive policy; policy must be a decision). Uncoded
variants behave exactly like thiserror output — that is the ramp.

### Where we do NOT compromise

- `Fault<E>` stays typed; no anyhow-style boxed-only mode.
- anyhow boundary stays explicit (`map_err(from_anyhow)`), never implicit
  `From`. (Reverse direction is free already: `anyhow::Error::new(fault)`
  accepts any `Error`, so agent-written anyhow code over our errors works
  today.)
- Codes are never auto-generated — doctor stability depends on it.

### Output as the second adoption surface

Agents that never WRITE fast-observe code still READ its output. The report
is self-teaching: the `action:` line names concrete next steps
("run `flatlandc doctor E100`"), and swallowed-error paths name the API
("intentional swallow: use `.report(msg)` so this is counted").
Additionally the crate ships an `AGENTS.md`/`SKILL.md` snippet projects can
vendor — a one-page translation table (the one above) so in-repo agents
adopt the vocabulary on sight. This is the cheapest adoption lever that
exists.

## 7. The Report (output contract for humans AND LLM agents)

`report::render(&Fault) -> String` (+ `_json` under `serde`). Stable
`key: value` sections, deterministic order, no ANSI, no wall-clock,
absolute paths. Ends with the prescriptive line:

```
error: [E100] compile error: table 'units' has no column 'hp'
category: Content (policy: fix input; retrying unchanged input will fail)
location: crates/flatlandc/src/analyzer.rs:214:9
scope: flatlandc.analyze (elapsed 3.1ms)
cause 0: [E100] compile error: table 'units' has no column 'hp'
cause 1: no column named 'hp' in schema 'units'
trace_id: 4f3c9a2b…   # same id on every log line and span of this moment
advice: check SQL syntax near the reported span
action: fix the input and re-run; see `flatlandc doctor E100`
```

The default error hook gains a `report` mode emitting this block as one
structured event. `OBSERVE_REPORT=json` for machine pipelines.

---

## 8. What this means for the codebase (refactor map)

| module | change |
|---|---|
| `exn.rs` | single `Frame::capture` constructor (fixes wrap_msg/walk_sources drift); attachments; `iter()`; `Coded` slot; trace-id capture point; lazy `msg` params on report/observed |
| `errors.rs` | `error!` macro v2 alongside deprecated `define_errors!`; advice in `ErrorRegistryEntry`; `doctor()`; `Coded` trait |
| `config.rs` | strum for all enums; `EnvConfig` parse-all-once; `OBSERVE_*` additions |
| `hook.rs` | `init()` → thin wrapper over `observe()`; bon `Deployment` builder lives in new `deploy.rs`; hook management fns |
| `deploy.rs` (new) | `observe()` entry, presets, `Deployment`, `InitGuard` (Drop → fastrace::flush), appender/layout/fan-out wiring |
| `profiling.rs` | Tier-2 forwarding in `scope!`; `function` re-export; intern table; single atomic load |
| `report.rs` (new) | the §7 renderer, text + serde json |
| `diagnostic.rs` | multi-label; `SourceStore`; registry-linked notes; severity ctors; NO_COLOR |
| `lib.rs` | `prelude`; feature re-export namespace `x` |

Migration for flatland: mechanical — `define_errors!` → `error!` (sed-able),
delete `compile_err!` + size asserts + doctor printing, `init()` →
`observe().dev()?`.

---

## 9. Open decisions (pick at implementation)

1. `error!` attribute syntax vs. keeping positional tuples: attributes
   (readability, defaults, extensibility) — recommended above.
2. Preset names: `dev/prod/test/wasm` vs. `pretty/json/…` — environment
   names recommended (they bundle level+layout+sinks coherently).
3. `observe()` returning builder vs. `Deployment::builder()` — `observe()`
   (reads as the verb it is; `Deployment::builder()` remains available).
4. Whether `.report()` grows a report-mode flag or the default hook gets a
   config knob — hook knob recommended (one place).
5. `From<Variant> for Fault<Enum>` is orphan-legal only because both types
   are concrete and the variant is local to the consuming crate — macro
   generates it in the consumer's crate, so fine. Confirm no coherence
   clash with the reflexive `impl<E: Error> From<E> for Fault<E>`: none —
   `Variant ≠ Enum` as types, and `Fault<Enum>` vs `Fault<Variant>` are
   different Self types.
