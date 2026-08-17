# fast-observe

Error faults with causal trees + fastrace-first profiling/log orchestration.

One crate for an app's observability stack, wasm-safe (including
`wasm32-unknown-unknown`):

- **`Fault<E>`** — exceptions with a causal tree, caller location, and typed
  context. `From<E>` makes `?` just work; `wrap` / `change_context` preserve
  the cause chain.
- **Error hooks** — every constructed `Fault` fans out to all registered
  sinks. The default sink (profiling scope + structured log) self-installs:
  observability is default-on, no `init()` required. Hooks are
  panic-contained and can be throttled per error type per second.
- **`scope!`** — one profiling macro feeding the config-selected backend
  (instant accumulator, fastrace, or web) with ~2ns cost when off.
- **`define_errors!`** — generates error structs, `Display`/`Error`,
  `code()`/`category()`, and link-time registration in a global error
  registry so your doctor/CLI can look up any error code workspace-wide.
- **`Diagnostic`** — ariadne-rendered compile-time-style diagnostics with
  source spans; `render_diagnostic` returns a `String`.
- **logforth orchestration** — `init()` composes exactly the appenders,
  layouts, and diagnostics enabled by your cargo features.

## Example

```rust,no_run
use fast_observe::exn::Result;
use fast_observe::{add_error_hook, define_errors, init, lookup_error, scope};

#[derive(Debug)]
enum AppError {
    Disk(Disk),
}
impl std::error::Error for AppError {}

define_errors! {
    enum AppError {
        (Disk, "E100", Transient, "disk read failed", "disk read failed: {path}", {
            path: String,
        });
    }
}

fn load(path: &str) -> Result<(), Disk> {
    let _span = scope!("load"); // profiling scope on the selected backend
    if path.is_empty() {
        fast_observe::bail!(Disk { path: path.to_string() });
    }
    Ok(())
}

fn main() {
    init(); // logforth (stdout/fastrace/json/file/web per features) + fastrace reporter
    add_error_hook(|frame| eprintln!("extra sink: {frame}"));

    if let Err(f) = load("") {
        eprintln!("{f:?}"); // causal tree with caller locations
    }

    // Doctor-style lookup from any linked crate's error definitions:
    let entry = lookup_error("E100").unwrap();
    assert_eq!(entry.display, "disk read failed");
}
```

## Configuration

- `OBSERVE_PROFILE=off|instant|fastrace|web` — overrides the profiling
  backend at startup (default: `fastrace`). Runtime override:
  `config().set_profiling_backend(...)`.
- `config().set_error_hook_throttle(n)` — cap error-hook invocations at `n`
  per error type per second (`0` = unlimited, the default).
- `OBSERVE_LOG_DIR=<dir>` — with feature `file`, also log to
  `<dir>/app.log` (rolling).

## Features

| Feature | Default | What it wires |
|---|---|---|
| `fastrace` | yes | fastrace spans + `FastraceEvent` appender + `FastraceDiagnostic` + `ConsoleReporter` |
| `bridge-log` | yes | re-exports the log ↔ logforth bridge (`logforth::bridge::log`) for custom pipelines |
| `instant` | no | thread-local span accumulator + `breakdown` (wasm-safe via `web-time`) |
| `web` | no | `instant` + browser-console log appender (wasm32 only; no-op on native) |
| `otel` | no | re-exports `fastrace-opentelemetry` + `logforth-append-opentelemetry`; `hook::init_otel(reporter)` |
| `json` | no | JSON layout on the stdout appender |
| `file` | no | rolling file appender (active when `OBSERVE_LOG_DIR` is set) |
| `bridge-tracing` | no | re-exports `fastrace-tracing` (tracing spans → fastrace) |
| `http` | no | re-exports `fastrace-reqwest` (trace context over HTTP) |
| `metrics-facade` | no | mirrors `error_counts` through the `metrics` facade (`fast_observe.errors` counter) |
| `serde` | no | `Serialize`/`Deserialize` for `Diagnostic`, `Severity`, `SourceSpan` |

## Platform notes

- **wasm32-unknown-unknown**: compiles with any feature set. The link-time
  error registry is empty on wasm (`linkme` has no linker sections there);
  `define_errors!` metadata (`code()`, `category()`, `Display`) still works.
  Panicking-hook containment relies on `catch_unwind`, which requires
  unwindable panics (default for wasm is abort — hooks should not panic
  there).

## Migrating from flatland-observe

See [MIGRATING.md](MIGRATING.md).

## License

MIT OR Apache-2.0
