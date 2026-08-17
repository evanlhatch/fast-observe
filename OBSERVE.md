# fast-observe — agent guide

Read this before writing or debugging code in a repo that uses `fast-observe`.
It is short on purpose: the surface is five verbs.

Status markers: **[live]** = in `src/` today. **[landing]** = designed
(DESIGN.md/SURFACE.md) but not yet in the tree — do not write code
against it.

## Why

One crate deploys the whole observability stack — errors with causal trees
and stable codes, profiling spans, structured logs, distributed traces — and
makes them share identifiers. An error knows its code, its scope path, its
trace id, its caller location. One grep for a code or trace id reconstructs
the whole causal moment: error tree + log lines + span timings.

Observability is default-on. Errors count and log themselves at
construction. Profiling is free when off. You get all of it by writing
almost nothing.

## The vocabulary (this is the whole thing)

```rust
use fast_observe::prelude::*;   // [landing] one import. Until the prelude
                                // lands, import items directly:
                                // fast_observe::{bail, ensure, scope, ResultExt, OptionExt, Fault, Result}

log::info!(user_id = 42; "connected");   // [live] POINTS: plain log macros. No
                                          // fast-observe logging API exists.
                                          // kv pairs become structured fields.

let _span = scope!("phase.op");          // [live] INTERVALS: dotted names. Feeds the
                                          // selected profiler(s) and becomes
                                          // error context automatically.

bail!(MyError { detail });               // [live] VALUES: errors are returned, not
ensure!(x > 0, MyError { detail });       // emitted. bail! ALSO logs, counts,
                                          // and emits a span event — never
                                          // write log::error! + return Err.
```

Plus `#[instrument]` on a fn (or `#[all_functions]` on an impl block —
both [live], from `fast_observe::profiling`, expanding to `::fast_observe`
paths) to instrument without writing `scope!` by hand. Defining typed
errors with codes: `define_errors!` is [live]; the thiserror-style `error!`
proc macro is [landing] (SURFACE.md §5).

That's it. If you know `log` and `anyhow`, you already know the verbs.

## Setup (binaries)

```rust
fn main() -> fast_observe::Result<()> {   // Result works like anyhow's
    fast_observe::init();                 // [live] logs + traces with defaults
    // ...
}
```

Configurable path [live]: `let _guard = fast_observe::observe().level(..).
file_from_env(true).init()?;` — the `Deployment` builder returns an
`InitGuard` whose drop flushes fastrace, and double-init is an
`Err(InitError)` instead of silently ignored.

Env knobs: `OBSERVE_PROFILE` [live] (comma list: `fastrace`, `instant`,
`puffin`, `tracy`, …; default `fastrace`), `OBSERVE_LOG` [live, honored by
the `observe()` builder; falls back to `RUST_LOG`, then `info`],
`OBSERVE_LOG_DIR` [live, with feature `file`], `RUST_BACKTRACE=1`
[landing]. Runtime [live]:
`config().set_backends(Backends::FASTRACE | Backends::TRACY)` —
compiled-in ≠ active; features compile backends, the mask selects them.
Asking for an uncompiled backend logs a warning naming the cargo feature.

## Errors

Typed errors with codes, the [live] form:

```rust
fast_observe::define_errors! {
    enum EngineError {
        (EntityNotFound, "E001", Content, "entity not found",
            "entity not found: {id}", { id: u64 });
    }
}
// you hand-write: `enum EngineError { EntityNotFound(EntityNotFound) }`,
// `impl Error`, and (for foreign sources) the source() wiring.

fn load(id: u64) -> Result<Entity, EngineError> {
    let e = repo.find(id).change_context(EngineError::from(EntityNotFound { id }))?; // [live]
    // .wrap_msg("loading entity") for the anyhow-style message form [live];
    // .context(...) aliases are [landing] (SURFACE.md §6a)
    Ok(e)
}
```

The [landing] `error!` macro upgrades this to thiserror-attribute syntax
(`#[error]`, `#[from]`, `#[source]` + `#[code]`/`#[category]`/`#[advice]`)
and generates the enum, `Error` impl with auto-wired `source()`, registry
entries with advice, and size assertions (SURFACE.md §5).

The report renderer is [landing] (DESIGN.md §7; no `report.rs` yet) — the
shape of its output, one fact per line, deterministic, no colors, no
timestamps:

```
error: [E001] entity not found: 17
category: Content (policy: fix input; retrying unchanged input will fail)
location: src/repo.rs:42:10
scope: request → load_entity (elapsed 3.1ms)
cause 0: [E001] entity not found: 17
trace_id: 4f3c9a2b…          # same id on every log line and span
advice: check the entity table
action: fix the input and re-run; see `<bin> doctor E001`
```

What feeds it is already [live]: every `Fault` captures the caller location,
the fastrace `trace_id` and the scope path + leaf elapsed as attachments
(built-in capture hooks), nests the `source()` chain into the tree, and
renders the tree via `Debug`. `doctor(code)` renders the registry entry +
policy line as `key: value` text.

Rules:
- **Swallow deliberately** [live]: `result.report("why we're ignoring this")`
  → `None`, counted under `"reported"`. Never `let _ = result;`.
- **Attach data, don't format it away** [live]: `.attach_with(||
  expensive_debug())` runs only on the error path; `.attach_key("attempt", n)`
  keeps it typed and machine-readable (`find_attachment::<T>()` downcasts).
- **Codes are forever**: never renumber; new failure mode = new code.
- `lookup_error("E001")` / `error_registry()` [live] power `doctor` commands.
- Multi-failure (retry/batch) [live]: collect into a `FaultCollection`,
  then `into_fault(err)` — one fault, N children.

## Profiling

`scope!("name")` [live] is free when the backend mask is empty (~2ns). Turn
on `instant` to see a per-phase breakdown (`print_breakdown()`), `fastrace`
for trace export (default), `tracy`/`puffin`/`superluminal`/`tracing` for
live profilers ([live] `profile-with-*` features, selected at runtime via
the `Backends` mask — compiled-in ≠ active). `finish_frame!` marks tick
boundaries for the instant/puffin backends.

Async: `scope!` guards are thread-bound — do NOT hold them across `.await`.
`#[instrument]`/`#[all_functions]` [live] reject async fns at compile time;
cross-await spans use `fastrace::trace(enter_on_poll = true)` [live, via the
fastrace dep] or `in_observed_span` / `root_span!` [landing].

Benchmarks: `#[fast_observe::bench]` (divan) and
`bencher.bench_profiled(..)` for per-phase span breakdown are [landing]
(DESIGN.md §9d).

## Debugging a failure as an agent

1. Read the report block ([landing] renderer; today: the `Debug` tree plus
   the `fast_observe.error` log line). The `action:` line will tell you the
   next step.
2. `grep` the trace_id in the logs for the full moment [live with
   `fastrace`: capture hook attaches it, `FastraceDiagnostic` stamps logs].
3. `<bin> doctor <code>` [live: `fast_observe::doctor(code)`] for the
   error's canonical meaning + category policy line.
4. `error_counts()` [live] shows what's failing often, sorted.

## Feature flags (nothing compiles unless named)

Default: `fastrace` + `bridge-log` [live]. Heavy deps are opt-in: `otel`
(heavy tree), `profile-with-tracy` (heavy build, C/C++);
`profile-with-{puffin,tracing}` and `profile-with-superluminal`
(windows-only) are light. `instant`, `json`, `file`, `serde`,
`metrics-facade`, `bridge-tracing` (tracing spans → fastrace), `http`
(reqwest trace context) are tiny/light [all live]. `web` for wasm32
(browser console + instant spans). `compat-anyhow`/`anyhow-boundary` at API
boundaries: [landing]. `optick` was dropped (unmaintained upstream, broken
on modern toolchains). Full table + weights + wasm notes: README.
