# fast-observe

Errors, logs, traces, and profiling spans that actually know about each
other.

fast-observe pulls four things into one crate and wires them up: an
exn-style error type with causal trees, the logforth logging pipeline,
fastrace traces, and the `profiling` crate's span facade. An error knows
its code, the scope path it happened in, its trace id, and where it was
constructed — and it counts and logs itself at construction, before you've
set anything up.

It's heavily type-driven. Error categories determine retry/abort policy,
codes are shape-checked by the macro at compile time, the report's
one-fact-per-line contract is enforced by a type that escapes newlines,
and scope guards are `!Send` so a span can't silently record against the
wrong thread.

```rust,no_run
#![feature(error_generic_member_access)] // error! emits Error::provide
use fast_observe::exn::Result;
use fast_observe::{error, init, lookup_error, scope};

error! {
    pub enum AppError {
        /// Disk read failed.
        #[error("disk read failed: {path}")]
        #[code = "E100", category = Transient, advice = "check the path exists"]
        Disk { path: String },
    }
}

fn load(path: &str) -> Result<(), AppError> {
    let _span = scope!("load"); // feeds the active profiler set; also error context
    log::info!(path = path; "loading"); // plain log macros; kv becomes fields
    if path.is_empty() {
        // construct + count + span event + log, in one verb
        fast_observe::bail!(AppError::Disk(Disk { path: path.to_string() }));
    }
    Ok(())
}

fn main() {
    init(); // logs + traces with defaults; observe().init()? for toggles

    if let Err(f) = load("") {
        eprintln!("{f:?}"); // causal tree: codes, locations, scope path
    }

    let entry = lookup_error("E100").unwrap(); // works across every linked crate
    assert_eq!(entry.advice, Some("check the path exists"));
}
```

A runnable version is in [`examples/demo.rs`](examples/demo.rs).

## What you get for free

No `init()` call, no setup, and none of this needs thinking about:

- Every constructed `Fault` is counted per type (`error_counts()`), logged
  with its type, location, and scope, and emitted as a span event in the
  current trace. Hooks fan out and are panic-contained.
- `?` just works on your error types; `wrap`/`change_context`/`wrap_msg`
  nest the cause chain instead of flattening it, so the tree `Debug`
  renders is the tree `source()` walkers see.
- Panics go through the same pipeline as returned errors (counted, hooked,
  rendered) instead of a separate stderr dump.
- `#[fast_observe::main]` turns `fn main() -> Result<()>`'s error exit into
  a full report on stderr plus a sysexits-style exit code from the error's
  category.

## The report

`render_report(&fault)` — or `OBSERVE_REPORT=text|json` to have the error
hook emit it — gives you a deterministic, greppable block:

```text
report: fast-observe/1
error: [E100] [my_crate::repo::NotFound] entity not found: 17
category: Content (policy: fix the input; retrying unchanged input will fail)
location: src/repo.rs:42:10
scope: request → load_entity (elapsed 3ms)
attachment: attempt=3
cause 0: [E100] [my_crate::repo::NotFound] entity not found: 17, at src/repo.rs:42:10
cause 1: No such file or directory (os error 2), at src/repo.rs:42:10
trace_id: 4f3c9a2b…
fingerprint: 9f86d081
advice: check the entity table
action: fix the input; retrying unchanged input will fail
hint: run `doctor E100`
```

Cause lines are labeled by how the frame got there: `cause` (source
chain), `original` (wrapped), `attempt` (retry), `failure` (batch merge).
The fingerprint is a stable hash of the failure site, so "have we seen
this one before" is a string match. The text is snapshot-testable: no
ANSI, no timestamps, and values are newline-escaped so data can't inject
fake lines.

## Why not thiserror + log + tracing

Each of those does its slice well; the gaps are in the wiring you'd write
yourself.

- **thiserror** derives `Display`/`Error` and stops. No codes, no
  registry, no retry policy, no report — and `source()` wiring is on you,
  which is where cause chains quietly break. `error!` is
  thiserror-compatible attribute syntax plus `#[code]`/`#[category]`/
  `#[advice]`, and it wires the chain for you.
- **log** is a facade. You still choose and assemble a backend, and it
  knows nothing about your errors. Here the deployment is one `init()`
  (or the `observe()` builder), and error events land in it with
  structured fields.
- **tracing** instruments functions, but spans and errors never meet: an
  error deep in a handler doesn't know which span it was in, and the span
  tree can't tell you what ran right before the failure. And the
  `profiling` crate compiles exactly one backend in — fast-observe
  compiles several and picks at runtime (`OBSERVE_PROFILE`), with
  self-teaching warnings when you select one you didn't compile.

The same `scope!` calls also double as benchmark instrumentation: with
feature `bench`, `bencher.bench_profiled(..)` (divan) or
`bench::measure_breakdown(n, f)` hand you a per-phase span breakdown plus
the error-count delta, from the instrumentation you already wrote.

## Nightly required

Feature gates, declared at the crate root with tracking issues:
`error_generic_member_access` (`Error::provide`/`request_ref`, so codes
and categories are readable through `&dyn Error`), `error_iter`
(`Error::sources`), `backtrace_frames` (feature `backtrace`), and
`proc_macro_diagnostic` (macro crate). The devenv pins a known-good
toolchain.

## Configuration

| Env var | Effect |
|---|---|
| `OBSERVE_PROFILE` | Active profiling backends: comma-separated `off\|instant\|fastrace\|web\|puffin\|tracy\|superluminal\|tracing` (default `fastrace`). `off` alone. |
| `OBSERVE_LOG` | Max log level (falls back to `RUST_LOG`, then `info`). |
| `OBSERVE_LOG_DIR` | With feature `file`: also log to `<dir>/app.log`. |
| `OBSERVE_ERROR_THROTTLE` | Cap sink-hook invocations per error type per second (default 0 = unlimited). |
| `OBSERVE_REPORT` | Error hook emits the full report block: `text` or `json` (default `off`). |
| `OBSERVE_REPORT_SOURCE` | `1`: reports include the source line at the error location. |
| `OBSERVE_COLOR` | Diagnostic colors: `auto` (default), `always`, `never`. |
| `OBSERVE_BACKTRACE` | Feature `backtrace`: overrides `RUST_BACKTRACE` in both directions. |

Compiled-in ≠ active: cargo features compile a backend in, the `Backends`
mask (`config().set_backends(...)` or `OBSERVE_PROFILE`) selects which run.
Selecting one you didn't compile logs a warning naming the feature to add.

## Features

Default: `fastrace` + `bridge-log`. Nothing else compiles in unless named.
Weight: what it costs your build.

| Feature | Weight | What it wires |
|---|---|---|
| `instant` | tiny | thread-local span accumulator + per-phase `breakdown` (wasm-safe) |
| `web` | tiny | browser console logs + devtools timeline marks (wasm32-unknown-unknown) |
| `json`, `layout-logfmt`, `layout-gcl` | tiny | stdout layouts (JSON / logfmt / Google Cloud Logging) |
| `file` | tiny | rolling file appender via `OBSERVE_LOG_DIR` |
| `log-syslog`, `log-journald` | light | unix syslog / systemd journald appenders |
| `log-async` | light | background-thread stdout/file appenders |
| `filter-rustlog` | light | `RUST_LOG`-style per-module filter |
| `diag-task-local` | tiny | task-local diagnostic context |
| `otel` | heavy | `fastrace-opentelemetry` + OTel log appender |
| `bridge-tracing` | light | tracing spans → fastrace |
| `http` | light | `fastrace-reqwest` trace-context propagation |
| `int-axum`, `int-poem`, `int-tonic`, `int-tower`, `int-futures` | light | framework/stream middleware re-exports |
| `reporter-datadog`, `reporter-jaeger` | heavy | vendor reporters (prefer `otel` for new setups) |
| `metrics-facade` | tiny | `error_counts` mirrored into the `metrics` facade |
| `profile-with-puffin`, `profile-with-tracing` | medium | runtime-selectable profiler backends |
| `profile-with-tracy` | heavy | tracy backend |
| `profile-with-superluminal` | tiny | superluminal backend (windows) |
| `backtrace` | tiny | backtrace capture hook |
| `flush-on-exit` | tiny | fastrace flush on atexit/SIGTERM/SIGHUP |
| `bench` | light | divan re-export + `bench_profiled`/`measure_breakdown` |
| `serde` | tiny | serde derives for `Diagnostic` etc.; enables `render_report_json` |
| `anyhow-boundary`, `compat-eyre`, `compat-error-stack` | tiny | explicit boundary conversions |
| `int-tokio` | tiny | `JoinError` → `Fault`, cancelled vs panicked |

Libraries: depend with `default-features = false`; `fastrace` forwards
`fastrace/enable`, and that's the binary's call to make.

## Platform notes

- wasm32-wasip3 and wasm32-unknown-unknown are compile-verified in CI
  (`just check-wasip3` / `just check-wasm`). On wasip3, `web` degrades to
  `instant` spans (there's no browser console on WASI).
- The error registry is link-time (`linkme`), which doesn't exist on wasm.
  Call `register_statics(&[MyError::ENTRIES, ..])` once at startup there;
  `code()`/`category()`/`Display`/`From` work regardless.
- Hook panic containment uses `catch_unwind`; under wasm's default
  `panic = "abort"` it can't contain anything, so don't panic in hooks.

## Docs

- [OBSERVE.md](OBSERVE.md) — the agent/user guide: the vocabulary, the
  error rules, the debugging workflow.
- [DESIGN.md](DESIGN.md) / [SURFACE.md](SURFACE.md) — design rationale and
  the user-surface contract.
- [CONTRIBUTING.md](CONTRIBUTING.md) — dev environment and verification.

## License

MIT OR Apache-2.0
