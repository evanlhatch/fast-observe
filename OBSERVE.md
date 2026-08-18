# fast-observe — agent guide

Read this before writing or debugging code in a repo that uses `fast-observe`.
It is short on purpose: the surface is five verbs.

Status marker: **[live]** = in `src/` today. Everything in this guide is
live unless a cargo feature flag is named next to it.

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
use fast_observe::prelude::*;   // [live] one import: Fault/Frame/Result/ResultExt/
                                // OptionExt, bail!/ensure!/scope!/finish_frame!/error!,
                                // instrument/all_functions/skip, instrument_async, main,
                                // init/observe, doctor/lookup_error/error_registry/
                                // error_counts, render_report/report_display,
                                // add_error_hook/add_capture_hook, config/Backends,
                                // Category/Policy/Attachment/Placement/Coded.
                                // NOT in it: profiling!/func_path! (the `profiling`
                                // name would collide with the profiling module),
                                // root_span!/in_observed_span, render_report_json,
                                // Deployment/InitGuard/InitError, HookId —
                                // import those from their modules.

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

Plus `#[instrument]` on a fn (or `#[all_functions]` on an impl block
with `#[skip]` opt-outs — all [live], from `fast_observe_macros`,
re-exported at the crate root and in the prelude, expanding to
`::fast_observe` paths) to instrument without writing `scope!` by hand.
Defining typed errors with codes: the thiserror-style `error!` proc macro
is [live] and the only path.

That's it. If you know `log` and `anyhow`, you already know the verbs.

## Setup (binaries)

```rust
#[fast_observe::main]                   // [live] Err → full report to stderr +
fn main() -> fast_observe::Result<()> {   // category's sysexits code (or write
    fast_observe::init();                 // `fn main() -> fast_observe::Fault` directly)
    // ...
}
```

Configurable path [live]: `let _guard = fast_observe::observe().level(..).
file_from_env(true).init()?;` — the `Deployment` builder returns an
`InitGuard` whose drop flushes fastrace, and double-init is an
`Err(InitError)` instead of silently ignored. Two more builder defaults
[live]: `panic_hook(true)` routes panics through the SAME error pipeline —
each panic becomes a `Fault` (category Fatal, real panic location attached,
backtrace when enabled), so panics are counted, hooked, and rendered exactly
like returned errors — then chains the previously installed hook, never
stomped; `flush_on_exit(true)` (feature `flush-on-exit`, native only)
flushes fastrace on atexit/SIGTERM/SIGHUP.

Env knobs: `OBSERVE_PROFILE` [live] (comma list: `fastrace`, `instant`,
`puffin`, `tracy`, …; default `fastrace`), `OBSERVE_LOG` [live, honored by
the `observe()` builder; falls back to `RUST_LOG`, then `info`],
`OBSERVE_LOG_DIR` [live, with feature `file`], `RUST_BACKTRACE=1`/`full`
[live, with feature `backtrace` — the built-in capture hook attaches a
forced backtrace as an Appendix attachment; `OBSERVE_BACKTRACE` overrides
in both directions]. Runtime [live]:
`config().set_backends(Backends::FASTRACE | Backends::TRACY)` —
compiled-in ≠ active; features compile backends, the mask selects them.
Asking for an uncompiled backend logs a warning naming the cargo feature.

## Errors

Typed errors with codes, the [live] form — `error!` takes
thiserror-attribute syntax plus `#[code]`/`#[category]`/`#[advice]`:

```rust
fast_observe::error! {
    #[derive(Debug)]
    pub enum EngineError {
        /// check the entity table
        #[error("entity not found: {id}")]
        #[code = "E001", category = Content]
        EntityNotFound { id: u64 },

        #[error("io: {0}")]
        #[code = "E002", category = Transient, advice = "retry the io operation"]
        #[from]
        Io(std::io::Error),
    }
}
// Generates: the enum + one public struct per struct variant,
// Display/Error with auto-wired source(), From<Variant> for the enum AND
// for Fault<Enum>, registry entries with advice (default: first doc line),
// nightly Error::provide of the code/category, a 64-byte size assertion.

fn load(id: u64) -> fast_observe::Result<Entity, EngineError> {
    let e = repo.find(id).change_context(EngineError::from(EntityNotFound { id }))?; // [live]
    // or return the variant straight into a fault: `Err(EntityNotFound { id })?`
    // `#[from]` gives From<io::Error> for the enum but NOT for Fault<Enum>
    // (orphan rule) — write `err.map_err(EngineError::from)?`.
    // .wrap_msg("loading entity") for the anyhow-style message form [live];
    // .context(msg)/.with_context(|| ..) convert to Fault<BoxError> [live].
    Ok(e)
}
```

The report renderer is [live]: `render_report(&fault) -> String`,
`report_display(&fault)` (streaming `Display`, no report-string
allocation), `render_report_json(&fault)` (feature `serde`, versioned
`"schema": 2`). Codes/categories are read through `Error::provide` with a
registry fallback, so `error!` types report fully. One fact per line,
deterministic, no colors, no timestamps; every interpolated value is
newline-escaped, so data can never forge a report line. The first line
marks the format (`report: fast-observe/1`); cause lines carry the frame's
type (when not erased) + location, and the edge kind distinguishes
`cause` (source chain) / `original` (wrapped) / `attempt` (retry) /
`failure` (batch):

```
report: fast-observe/1
error: [E001] [my_crate::repo::NotFound] entity not found: 17
category: Content (policy: fix the input; retrying unchanged input will fail)
location: src/repo.rs:42:10
scope: request → load_entity (elapsed 3ms)
attachment: attempt=3
cause 0: [E001] [my_crate::repo::NotFound] entity not found: 17, at src/repo.rs:42:10
cause 1: No such file or directory (os error 2), at src/repo.rs:42:10
trace_id: 4f3c9a2b…          # same id on every log line and span
fingerprint: 9f86d081        # stable per failure site — dedupe across runs
advice: check the entity table
action: fix the input; retrying unchanged input will fail
hint: run `doctor E001`
```

Volatile process state (occurrence count of this error type, thread name,
uptime) rides as structured kv fields on the hook's log event — the report
body stays a pure function of the fault. `OBSERVE_REPORT_SOURCE=1` adds a
`source:` line with the code at the root location.

What feeds it [all live]: every `Fault` captures the caller location, the
fastrace `trace_id` (and emits an `error` span event, so the failure lands
in the trace timeline) and the scope path + leaf elapsed as attachments
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
`#[instrument]`/`#[all_functions]` [live] reject async fns at compile time
— deliberate: a guard held across `.await` records against whatever thread
polls next. Cross-await spans [live, feature `fastrace`]: bind
`fast_observe::root_span!("request")` once per task (continue an incoming
trace with `root_span!("request", ctx)`), wrap futures with
`.in_observed_span("load")` (`profiling::async_::ObservedFutureExt`) — the
span enters on every poll and follows the task across threads. The attribute
form is `#[fast_observe::instrument_async]` [live, feature `fastrace`] — an
async fn so marked gets a fastrace span entered on each poll, no direct
fastrace dependency needed. W3C trace-context helpers
(`extract_traceparent`/`inject_traceparent`, and the full
`extract_headers`/`inject_headers` incl. tracestate) live in
`profiling::async_` too.

Benchmarks [live, feature `bench` — implies `instant`]:
`fast_observe::bench` re-exports divan (write `#[divan::bench(crate =
fast_observe::bench::divan)]` — the attribute itself is not wrapped);
`bencher.bench_profiled(..)` returns a per-phase span breakdown plus the
error-count delta. Outside divan: `bench::measure_breakdown(n, f)`.

## Debugging a failure as an agent

1. Read the report block [live: `render_report(&fault)` / the `Debug`
   tree plus the `fast_observe.error` log line]. The `action:` line tells
   you the next step.
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
(browser console + instant spans). Also live: `backtrace` (capture hook),
`flush-on-exit`, `bench`, `int-futures` (fastrace-futures re-export),
`serde` (`render_report_json` + Diagnostic derives). `anyhow-boundary`
[live: `compat::anyhow_boundary::{from_anyhow, into_anyhow}` — explicit
`map_err` points, never implicit `From`]. `compat-eyre`
(`compat::eyre_boundary`), `compat-error-stack`
(`compat::error_stack_boundary` — typed context + frame stack survive),
`int-tokio` (`tokio_ext::ObserveJoinExt::observe_join` — cancelled vs
panic distinguished) [all live]. Caveat: `fastrace` forwards `fastrace/enable` — libraries must
depend with `default-features = false` and let the binary enable fastrace.
`optick` was dropped (unmaintained upstream, broken on modern toolchains).
Full table + weights + wasm notes: README.
