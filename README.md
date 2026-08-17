# fast-observe

One crate deploys an app's whole observability stack — **errors with stable
codes and causal trees, profiling spans, structured logs, distributed
traces** — and makes them share identifiers (error code, scope path, trace
id, caller location). Wasm-safe, including `wasm32-unknown-unknown`.

- **`Fault<E>`** — exceptions with a causal tree, caller location, typed
  context, and typed attachments. `From<E>` makes `?` just work; `wrap` /
  `change_context` / `wrap_msg` preserve the cause chain (nested, not
  flattened). `impl Error for Frame` means generic `source()` walkers see
  the same tree `Debug` renders.
- **Error hooks, two families** — *capture* hooks mutate a frame during
  construction (built-ins attach the trace id and scope path); *sink* hooks
  fan out post-construction to every registered sink. The default sink
  (profiling scope + structured log) self-installs: default-on, no
  `init()` required. Sink hooks are panic-contained and throttlable per
  error type per second.
- **`scope!`** — one profiling macro feeding the runtime-selected backend
  **set** (`Backends` mask): fastrace, instant, web — plus Tier-2
  profilers (puffin, tracy, superluminal, tracing) compiled in via
  `profile-with-*` features. ~2ns when the mask is off.
- **`error!`** — declarative typed errors: thiserror-compatible
  attributes (`#[error]`/`#[from]`/`#[source]`) plus
  `#[code]`/`#[category]`/`#[advice]`; generates the enum, structs,
  Display/Error (with `source()` wiring + `Error::provide` of code and
  category), `From` conversions, and link-time registration in the
  global error registry so `lookup_error` / `doctor` work workspace-wide.
- **`Diagnostic`** — ariadne-rendered compile-time-style diagnostics with
  source spans, multi-label, and in-memory sources; `render_diagnostic`
  returns a `String`.
- **Deployment** — `init()` for zero-config, or the `observe()` builder
  (`Deployment` → `InitGuard`, drop flushes fastrace) for toggles.

## Nightly required

fast-observe requires a **nightly toolchain (1.99+)**. Feature gates are
declared at the crate root (`src/lib.rs`) with their tracking issues;
currently:

- `error_generic_member_access`
  ([#99301](https://github.com/rust-lang/rust/issues/99301)) —
  `Error::provide` / `request_ref`: codes, categories, and attachments
  readable through `&dyn Error`.

The repo's devenv pins the toolchain.

## Quickstart

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
    let _span = scope!("load"); // interval: feeds the selected profiler set
    log::info!(path = path; "loading"); // point: plain log macros, kv becomes fields
    if path.is_empty() {
        // value: construct + count + span event + log in one verb —
        // never log::error! + return Err
        fast_observe::bail!(AppError::Disk(Disk { path: path.to_string() }));
    }
    Ok(())
}

fn main() {
    init(); // logforth (stdout/fastrace/json/file/web per features) + fastrace reporter

    if let Err(f) = load("") {
        eprintln!("{f:?}"); // causal tree: [code], locations, scope path, trace id
    }

    // Doctor-style lookup across every linked crate's error definitions:
    let entry = lookup_error("E100").unwrap();
    assert_eq!(entry.name, "Disk");
    assert_eq!(entry.advice, Some("check the path exists"));
}
```

`#[instrument]` on a fn (or `#[all_functions]` on an impl block)
instruments without writing `scope!` by hand. `.attach_*()` puts typed
data on the error path; `.report("why")` is the one blessed swallow.

## Configuration

| Env var | Effect |
|---|---|
| `OBSERVE_PROFILE` | Active profiling backend set: a comma-separated, case-insensitive list of `off\|instant\|fastrace\|web\|puffin\|tracy\|superluminal\|tracing` (default `fastrace`). `off` must appear alone. |
| `OBSERVE_LOG` | Max log level for the `observe()` builder (falls back to `RUST_LOG`, then `info`). |
| `OBSERVE_LOG_DIR` | With feature `file`: also log to `<dir>/app.log` (rolling). |

Runtime knobs: `config().set_backends(...)`,
`config().set_error_hook_throttle(n)` (cap sink-hook invocations at `n`
per error type per second; `0` = unlimited). Landing env forms
(DESIGN.md §3): `OBSERVE_ERROR_THROTTLE`; backtrace capture will honor
`RUST_BACKTRACE` first.

## Runtime backend selection

**Compiled-in ≠ active.** Cargo features compile backend glue in; the
`Backends` mask selects which compiled backends are live at runtime:

```rust,ignore
use fast_observe::config::{Backends, config};
config().set_backends(Backends::FASTRACE | Backends::TRACY);
// or from the environment: OBSERVE_PROFILE=fastrace,tracy
```

Requesting a backend whose feature is not compiled in logs a one-time
warning **naming the exact cargo feature to enable**
(`profile-with-tracy`, …) — the config is self-teaching. `web` rides on
the `instant` feature for spans (the browser-console half is a log
appender, wasm32 only).

## Features

Weight = what the feature costs a consumer's build (DESIGN.md §9d):
**tiny** (single-purpose crate), **light** (small pure-Rust),
**light-medium** (pure Rust, more of it), **heavy-build** (compiles C/C++
via build scripts), **heavy-tree** (large dependency tree).

| Feature | Default | Weight | What it wires |
|---|---|---|---|
| `fastrace` | yes | light | fastrace spans + `FastraceEvent` appender + `FastraceDiagnostic` + `ConsoleReporter`; `SamplingReporter`/`MultiReporter` |
| `bridge-log` | yes | light | re-exports the log ↔ logforth bridge (`logforth::bridge::log`) for custom pipelines |
| `instant` | no | tiny | thread-local span accumulator + `breakdown` (wasm-safe via `web-time`) |
| `web` | no | tiny | `instant` + browser-console log appender (wasm32 only; no-op on native) |
| `json` | no | tiny | JSON layout on the stdout appender |
| `file` | no | tiny | rolling file appender (active when `OBSERVE_LOG_DIR` is set) |
| `otel` | no | heavy-tree | re-exports `fastrace-opentelemetry` + `logforth-append-opentelemetry`; `hook::init_otel(reporter)` |
| `bridge-tracing` | no | light | re-exports `fastrace-tracing` (tracing spans → fastrace) |
| `http` | no | light | re-exports `fastrace-reqwest` (trace context over HTTP) |
| `metrics-facade` | no | tiny | mirrors `error_counts` through the `metrics` facade (`fast_observe.errors` counter) |
| `profile-with-puffin` | no | light-medium | puffin backend glue; runtime-selected via `Backends::PUFFIN` |
| `profile-with-tracing` | no | light-medium | `scope!` emits `tracing::span!`; runtime-selected via `Backends::TRACING` |
| `profile-with-tracy` | no | heavy-build | tracy-client backend glue; runtime-selected via `Backends::TRACY` |
| `profile-with-superluminal` | no | tiny | superluminal-perf (windows only); runtime-selected via `Backends::SUPERLUMINAL` |
| `serde` | no | tiny | `Serialize`/`Deserialize` for `Diagnostic`, `Severity`, `SourceSpan` |

**Dropped:** `profile-with-optick` — optick is unmaintained upstream and
broken on modern toolchains. Use `profile-with-tracy` or
`profile-with-puffin` instead.

## Platform notes

- **wasm32-unknown-unknown**: compiles with the documented recipe
  (`--no-default-features --features instant|web`, nightly `-Z build-std`).
  The link-time error registry is empty on wasm (`linkme` has no linker
  sections there); `define_errors!` metadata (`code()`, `category()`,
  `Display`) still works. Panicking-hook containment relies on
  `catch_unwind`, which requires unwindable panics (default for wasm is
  abort — hooks should not panic there). `fastrace` is not wasm-verified.
- Native-only features (`file`, tracy, superluminal): gate them in your
  own `Cargo.toml` via target-specific dependencies.

## Documentation map

- **[OBSERVE.md](OBSERVE.md)** — the agent/user guide: the five verbs,
  error rules, debugging workflow (with live/landing status markers).
- **[MIGRATING.md](MIGRATING.md)** — from `flatland-observe`, plus the
  0.1.0 → next overhaul changes.
- **[DESIGN.md](DESIGN.md)** / **[SURFACE.md](SURFACE.md)** — the overhaul
  design and the user-surface contract.

## License

MIT OR Apache-2.0
