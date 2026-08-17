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
10. **Error registry empty on wasm.** `linkme` has no linker-section support
    for wasm, so on `target_family = "wasm"` `ERROR_REGISTRY` is a static
    empty slice: `lookup_error` returns `None`, `error_registry()` yields
    nothing. All other `define_errors!` output (`code()`, `category()`,
    `Display`, `From`, registry `ENTRY` consts) is unchanged. Consuming
    crates only need `linkme` as a dependency on non-wasm targets.
11. **Default features**: `fastrace` + `bridge-log` (flatland-observe had
    none). `--no-default-features` gives a minimal build; `bridge-log`
    separately re-exports `logforth::bridge::log` for custom pipelines.

## Unchanged

`Fault` / `Frame` / `bail!` / `ensure!` / `ResultExt` (`change_context`,
`wrap_msg`, `observed`, `report`) / `OptionExt` / `error_counts` /
`define_errors!` syntax / `scope!` / `profiling!` / `finish_frame!` /
`all_functions` / `skip` / `breakdown::{drain_spans, print_breakdown}` /
lazy default-on error hook / `#[track_caller]` locations.
