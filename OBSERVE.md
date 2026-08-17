# fast-observe — agent guide

Read this before writing or debugging code in a repo that uses `fast-observe`.
It is short on purpose: the surface is five verbs.

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
use fast_observe::prelude::*;   // one import

log::info!(user_id = 42; "connected");   // POINTS: plain log macros. No
                                          // fast-observe logging API exists.
                                          // kv pairs become structured fields.

let _span = scope!("phase.op");          // INTERVALS: dotted names. Feeds the
                                          // selected profiler(s) and becomes
                                          // error context automatically.

bail!(MyError { detail });               // VALUES: errors are returned, not
ensure!(x > 0, MyError { detail });       // emitted. bail! ALSO logs, counts,
                                          // and emits a span event — never
                                          // write log::error! + return Err.
```

Plus `#[instrument]` on a fn (or `#[all_functions]` on an impl block) to
instrument without writing `scope!` by hand, and `error!` to define typed
errors with codes.

That's it. If you know `log` and `anyhow`, you already know the verbs.

## Setup (binaries)

```rust
fn main() -> fast_observe::Result<()> {   // Result works like anyhow's
    fast_observe::init();                 // logs + traces with defaults
    // ...
}
```

Env knobs: `OBSERVE_LOG` (log directives, falls back to `RUST_LOG`),
`OBSERVE_PROFILE` (comma list: `fastrace`, `instant`, `puffin`, `tracy`, …;
default `fastrace`), `OBSERVE_LOG_DIR` (rolling file), `RUST_BACKTRACE=1`.
Runtime: `config().set_backends(Backends::FASTRACE | Backends::TRACY)` —
compiled-in ≠ active; features compile backends, the mask selects them.
Asking for an uncompiled backend logs a warning naming the cargo feature.

## Errors

```rust
// define typed errors once per crate — codes are stable and greppable:
fast_observe::error! {
    pub enum EngineError {
        #[error("entity not found: {id}")]
        #[code = "E001", category = Content, advice = "check the entity table"]
        EntityNotFound { id: u64 },
        #[error("io: {0}")] #[from]          // thiserror syntax works verbatim
        Io(std::io::Error),
    }
}

fn load(id: u64) -> Result<Entity, EngineError> {
    let e = repo.find(id).context("loading entity")?;    // anyhow-style
    Ok(e)
}
```

On `Err`, you get (deterministic, no colors, no timestamps — diffable):

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

Rules:
- **Swallow deliberately**: `result.report("why we're ignoring this")` →
  `None`, counted under `"reported"`. Never `let _ = result;`.
- **Attach data, don't format it away**: `.attach_with(|| expensive_debug())`
  runs only on the error path; `.attach_key("attempt", n)` keeps it typed
  and machine-readable in the report.
- **Codes are forever**: never renumber; new failure mode = new code.
- `lookup_error("E001")` / `error_registry()` power `doctor` commands.

## Profiling

`scope!("name")` is free when the backend mask is empty (~2ns). Turn on
`instant` to see a per-phase breakdown (`print_breakdown()`), `fastrace`
for trace export (default), `tracy`/`puffin` for live profilers
(`profile-with-*` features). Async: don't hold `scope!` guards across
`.await` — use `#[instrument]` (async-aware) or `root_span!` per task.

Benchmarks: `#[fast_observe::bench]` (divan) for numbers;
`bencher.bench_profiled(..)` to also get the per-phase span breakdown.

## Debugging a failure as an agent

1. Read the report block. The `action:` line tells you the next step.
2. `grep` the trace_id in the logs for the full moment.
3. `<bin> doctor <code>` for the error's canonical meaning + advice.
4. `error_counts()` shows what's failing often, sorted.

## Feature flags (nothing compiles unless named)

Default: `fastrace` + `bridge-log`. Heavy trees (`otel`, `tracy`) are
opt-in. `web` for wasm32 (browser console + `performance.mark` timeline).
`serde` for JSON reports/config. `compat-anyhow`/`compat-eyre` at API
boundaries. Full table + wasm notes: README.
