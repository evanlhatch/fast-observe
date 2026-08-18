# Contributing

## Toolchain

fast-observe requires **nightly Rust** (feature gates are listed in
`src/lib.rs` and README.md). The repo ships a [devenv](https://devenv.sh)
environment (`devenv.nix` / `devenv.yaml`) that pins a known-good
toolchain; a plain recent `rustup` nightly works too.

## Verify

Everything runs through the `justfile`:

```sh
just check        # test (default + all features) + clippy + fmt
just check-wasm   # wasm32-unknown-unknown compile checks (-Z build-std)
just check-wasip3 # wasm32-wasip3 compile checks (the primary wasm target)
```

`just check` must be green before sending a change. Clippy runs pedantic;
in library code `panic!`/`unwrap`/`expect` are denied (tests use `assert!`
with messages and `let … else { unreachable!(…) }`). No `#[allow]` without
a `reason = "…"`.

## Architecture in one paragraph

Three layers, lower layers never name logforth/fastrace types:
**core** (`Fault`/`Frame`/`Context`, registry, counters, hooks — depends
only on `parking_lot` + the `log` facade) → **observe** (profiling facade,
config, diagnostics, report) → **deploy** (`init()`/`observe()` wiring).
The error path must work pre-`init()`, in tests, and on wasm. The report
body is a *pure function of the fault* — volatile process state goes in the
hook's log-event kv fields, never in the report. See DESIGN.md for the full
rationale.

## Testing conventions

- Unit tests live in-module; integration tests in `tests/`.
- **trybuild** UI tests pin macro diagnostics
  (`fast-observe-macros/tests/ui/`); regenerate with
  `TRYBUILD=overwrite cargo test -p fast-observe-macros` and review the
  `.stderr` diff by hand.
- **insta** snapshots pin the report/Debug formats
  (`tests/snapshots/`); regenerate with `INSTA_UPDATE=always cargo test`
  and review the diff by hand — these formats are a public contract.
- **bolero** property tests in `tests/fuzz_*.rs` run under `cargo test`
  (and as real fuzz targets). Add one when you touch a parser, a renderer,
  or a state machine.
- Env-var-dependent behavior gets its own test binary (see
  `tests/env_override.rs`, `tests/report_source.rs`) — globals and parallel
  tests don't mix.

## Adding a cargo feature / backend

1. Optional dependency in `Cargo.toml` + feature that forwards it.
2. If it's a profiling backend: a `src/profiling/<name>.rs` module, a
   `profiling_backend!` wrap module (real/ZST-stub pair), a `Backends` bit,
   and a row in `config::BACKENDS_INFO` (that one table drives env parsing
   AND the missing-feature warnings).
3. Env/config knobs: read through `config::env_enum` /
   `DeploymentConfig::from_env`; names are consts in `crate::env_vars`.
4. Log targets are consts in `crate::log_targets`.
5. Update README's feature table + OBSERVE.md.

## Commits

The repo uses [Jujutsu](https://jj-vcs.github.io/) (`jj`): the working copy
is a commit, `jj describe` to describe, `jj new` to start the next change.
Small, atomic, described changes; nothing is destructive (`jj undo`).
