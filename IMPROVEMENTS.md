# fast-observe — improvement backlog

Review of the full codebase (src, macros, tests, benches), four rounds.
Items marked **[verified]** were reproduced empirically (compile probes /
runtime probes / fuzz findings). Ordered within sections by value.

Design principles reaffirmed during review (keep these):

- `render_report` is a **pure function of the fault** — no wall-clock, no
  process state. Volatile runtime context belongs in the **envelope**
  (hook log-event kv fields / JSON meta), never the body.
- Compiled-in ≠ active; self-teaching warnings name the exact cargo feature.
- Error path is `#[cold]`; hot path (`scope!`, `profiling!`) stays
  allocation-free.

---

## 1. Verified bugs

1. **[verified] `#[fast_observe::main]` rejects the crate's own `Result` alias.**
   `function_returns_result` (macros/lib.rs) string-matches `starts_with("Result<")`;
   `-> fast_observe::Result<()>` and `-> std::result::Result<(), E>` are rejected
   despite the doc comment claiming the latter works. Fix: match the last path
   segment == `Result` instead of prefix text.
2. **[verified] `bail!` doc promises interpolation it doesn't do.**
   `bail!("something: {x}")` hits the `$err:expr` arm → literal `{x}` stored.
   Fix: add a `($fmt:literal $(, $arg:expr)*)` arm that `format!`s, or fix the doc.
3. **[verified] Thread-bound guards are `Send`.** `InstantGuard`,
   `FunctionScopeGuard`, `WebMarkGuard` are auto-`Send`; dropping one on another
   thread pops THAT thread's TLS stack at a stored depth → silent corruption.
   `ScopeGuard` is `!Send` only by accident (fastrace's `Rc`). Fix:
   `PhantomData<*const ()>` markers + `try_with` in Drop (TLS-teardown panic
   inside Drop = abort risk during unwind) + `staticassertions` compile tests.
4. **[verified] Newline injection forges report lines.** A message containing
   `\naction: forged` renders a fake `action:` line in the text report — agents
   grepping `^action:` read data-controlled text. (JSON escapes correctly.)
   Fix: sanitize `\n`/`\r` in `write_report`; ideally a `Line<'a>(&'a str)`
   newtype only constructible via the sanitizer, so "line-safe" is a type.
5. **[verified — found by own fuzz suite, unfixed] Reversed `SourceSpan`
   panics inside `render_diagnostic`** (ariadne asserts `start <= end`).
   `fuzz_diagnostic` gates around it with `catch_unwind`. Fix: clamp
   `(min, max)` in `build_report` (fields must stay pub for serde), then
   remove the gate so any other panic fails hard.
6. **[verified] `uncoded_code`'s `dyn` branch is reachable** (`type_name_of_val`
   on `&dyn Error` yields the trait-object name, not the concrete one) — but its
   `Debug`-prefix heuristic is fragile (hand-written `Debug` → falls back to
   `"Error"`). Better: carry the concrete name at construction (a `TypeName`
   newtype wrapper in `internal_err`/`from_boxed`) instead of scraping `Debug`.

## 2. Soundness / robustness

7. **Panics bypass the error pipeline.** `install_panic_hook` logs but never
   `record_error("panic")` / fires hooks — counters and custom hooks see returned
   errors only. One line unifies them (DESIGN.md's own "indistinguishable" goal).
8. **`Fault::exit_with_report` loses buffered traces/logs.** Without feature
   `flush-on-exit`, `process::exit` drops pending fastrace spans and async
   appender records at the worst moment. Fix: `crate::flush()` +
   `log::logger().flush()` before exit.
9. **`SamplingReporter` phase-aligns the fleet.** Every process's counter starts
   at 0 → correlated sampling; decision not stable per trace. Fix: trace-id-modulo
   sampling (deterministic, fleet-decorrelated, consistent across services,
   drops the `AtomicU64`); keep counter mode opt-in.
10. **`intern()` leaks per unique string, tags included.** `ScopeGuard::new`
    interns the tag too; high-cardinality dynamic tags grow unbounded. Document
    the bound covers tags, or don't intern tags.
11. **`OBSERVE_LOG` read in 3 places** (`resolve_level`, `build_rust_log_filter`,
    `DeploymentConfig::from_env`) with subtly different fallbacks;
    **`OBSERVE_PROFILE` parsed twice** (config LazyLock + `Deployment::from_env`)
    → invalid values warn twice. Realize DESIGN.md's "single EnvConfig parse-once".
12. **`ReportMode::Json` silently degrades to Text** without feature `serde` —
    every other missing-feature path in the crate warns once; this one should too.
13. **`write_fault` (Debug renderer) recurses** — degenerate deep trees could
    overflow the stack in a process that is already sick. The crate has
    explicit-stack traversal elsewhere; use it.
14. **Hot-path allocation in `enter_function_scope`:** logforth's
    `ThreadLocalDiagnostic::insert` is `Into<String>` over a
    `BTreeMap<String,String>` (read its source) → ~2 String allocs per
    `profiling!()` call. Fix: cache the last name per thread, skip insert when
    unchanged. (EcoString cannot help — the boundary is logforth's API.)

## 3. DRY refactors

15. **`report.rs`: one traversal, two renderers.** Text and JSON each re-walk the
    tree and re-derive code/category/scope/attachments/action. Build a
    `ReportData` snapshot once; renderers become pure formatting. Fold in a
    single `fn preorder(&Frame) -> impl Iterator<Item = &Frame>` used by text,
    JSON, and `find_attachment_tree` (which currently recurses).
16. **`hook.rs`: generic `HookRegistry<F>`** — `HOOKS` and `CAPTURE_HOOKS` are the
    same OnceLock<Mutex<Arc<Vec<F>>>> snapshot pattern twice, including the
    "snapshot before invoke" reentrancy rule. Make it structural, ~60 lines saved.
17. **`deploy.rs`: the 16-field three-list problem.** Every `DeploymentConfig`
    field is named in the struct, `from_env`, `apply`'s destructure, AND the
    overlay — adding a toggle compiles fine if you forget one. A declarative
    field macro (needs `paste`) generates struct + from_env + apply from one list.
18. **`config.rs`: single backend table.** Bits, `from_env_value`'s match,
    `warn_unavailable`'s table, and `profiling.rs`'s `ScopeGuard` list all
    enumerate the backends. One `const BACKENDS: &[(Backends, name, feature,
    available)]` drives parse + warn. Also: `REPORT_MODE`/`COLOR_MODE` share one
    `env_enum(var, parse, default)` helper.
19. **`error_macro.rs`: parse helpers.** ~5 copies of "match
    `AttributeValue::Equals`, expect Literal/Ident, else push same error twice"
    → `expect_eq_literal`/`expect_eq_ident` + one generic `set_once` (replaces
    `set_tpl/set_code/set_category/set_advice/set_action`). `instrument` and
    `instrument_async` duplicate the `name = "..."` parse → shared
    `parse_name_attr`.
20. **Small:** `bench.rs` — factor `run_profiled(run: impl FnOnce())` shared by
    `measure_breakdown`/`bench_profiled`; `hook.rs::default_hook` — build message
    first, log once (kv arms dedup); `Attachment::with_key` —
    `Self { key: Some(key), ..Self::new(value) }`; shared `payload_str` for
    `install_panic_hook` + `tokio_ext::panic_payload`.

## 4. Type-system hardening (the "no strings, no magic numbers" batch)

21. **Strings used as protocol → types/consts:**
    - `BuiltinKey` enum (`ScopePath | ScopeElapsedMs | TraceId | SpanTrail |
      Backtrace`) with `as_str()` — keys are written in `hook.rs` and matched in
      `report.rs`; a typo compiles and silently drops a report section.
    - `pub(crate) const ERROR_EVENT` shared by `hook.rs` (producer) and
      `reporter.rs::has_error_event` (consumer) — a typo silently samples away
      error traces.
    - `mod targets` consts for the five `"fast_observe.*"` log targets.
    - Env-var name consts (backs #11).
    - `"reported"` counter key → const or `ErrorKey::Type(..) | Reported` enum.
22. **Magic numbers → named consts:** sysexits `EX_DATAERR=65 / EX_TEMPFAIL=75 /
    EX_SOFTWARE=70 / EX_GENERAL=1` in `exit_code_raw`; `RETAINED_FRAMES=60` in
    `instant.rs`; `NS_PER_SEC` in the throttle; the `exp.min(20)` overflow cap in
    `Backoff::delay`; `REPORT_SCHEMA_VERSION=1`. `SPAN_TRAIL_LEN` is the existing
    precedent. Comment or renumber the `Backends` bit-5 gap.
23. **Units in types, not names:** `Nanos(u64)` (and `Millis`) — `now_ns()`,
    `current_scope_elapsed_ms()`, `SpanRecord.start_ns/end_ns` carry units in
    identifiers. `now_ns` is public API → add typed accessor alongside, deprecate
    the raw one.
24. **`RateLimiter` with injected clock.** Evidence: `tests/hook.rs` needs a
    global serialization lock to test the throttle (globals for config, map,
    clock). `check(&self, key, now: Nanos) -> bool` — unit tests inject time.
25. **`Backoff::schedule() -> impl Iterator<Item = Option<Duration>>`** — pure,
    fuzzable delay policy separated from the sleep/retry mechanism; makes the
    `factor.max(2)` coercion and exponent cap named, tested behavior.
26. **Boundary validation (nutype the pattern, not the crate)** — registry
    entries are const, so `ErrorCode` shape (`^[A-Z]+\d+$`) is validated by
    `error!` at expansion. Runtime boundaries: `Attachment::with_key` rejects
    empty/newline/`=` keys; `Context::custom` rejects newlines (report protocol).
27. **API versioning consistency:** `#[non_exhaustive]` on `Policy`, `Severity`,
    `ReportMode`, `ColorMode` (`ErrorCategory` already has it; downstream
    `match` on `Policy` breaks on a new variant today).
28. **Hook API symmetry:** `add_error_hook` returns a `HookId` for surgical
    removal (tests want this); add `capture_hooks_len()`.
29. **`ScopeName(&'static str)` interned-marker newtype.**

## 5. Dependencies

- **Add:** `staticassertions` (dev — !Send contracts, `Fault` size pinning: 2
  words is a feature, a third field should fail CI); `paste` (only if #17/#18
  land); optional `int-http` feature (extract/inject_headers adapters for
  `http::HeaderMap` — axum/tonic users currently hand-convert).
- **Remove:** `documented` (dead derive on `Context`); `strum::EnumIter` derives
  on `Context`/`Severity` (no consumers).
- **Evaluated, rejected:** `ecow` (Cow::Borrowed already optimal on the hot path;
  EcoString would REGRESS static scope names >15B to heap allocs; only win is ~1
  cold-path alloc per Attachment), `arcstr`/`compact_str` (same), `bitflags`
  (hand-rolled `Backends` is const-friendly and fine), `dashmap`/`thread_local`
  (`intern()` is cold), serde_json for the report (hand-rolled is deliberate).
- **Underused:** `bolero` (see §6), `derive_more` (only Display — either use more
  or hand-roll and drop).

## 6. Fuzz coverage (bolero already a dev-dep)

30. **Report renderer fuzz** — hostile strings (newlines, quotes, control chars,
    unicode, `}`) in messages/contexts/attachment keys+values. Oracles: never
    panics; JSON parses + `schema == 1`; every text line matches
    `^(error|category|location|scope|attachment|cause \d+|trace_id|advice|action): `
    or is a tree-connector line. **Would have caught #4.**
31. **Instant span-stack model fuzz** — random enter/drop/`mem::forget`/dummy/
    finish_frame/drain sequences against a shadow model; assert no panic + drained
    counts/depths match. Exercises forgotten-guard finalization.
32. **Parser fuzz** — `Backends::from_env_value`, `DeploymentConfig::apply`:
    never panic, `off`-alone rule holds.
33. **traceparent/W3C round-trip** — arbitrary strings never panic;
    `decode(encode(ctx)) == ctx`.
34. **`Backoff::delay` fuzz** (after #25) — delay ≤ max, no overflow panics at
    `attempt = usize::MAX`, `factor = u32::MAX`, `base = Duration::MAX`.

## 7. AI-consumer error quality (report body — pure; envelope — volatile)

35. **Schema marker line 1:** `report: fast-observe/1` — self-describing format
    for agents that have never seen it.
36. **Per-cause type + location** — frames store both, report renders neither:
    `cause 1: [std::io::Error] No such file or directory, at src/repo.rs:42:10`.
    Type name also = `error_counts` key → free correlation with the metrics stream.
37. **`FrameKind { Source, Wrap, Attempt, Batch }`** set at the four construction
    sites, rendered as `cause` / `wrapped in` / `attempt 2` / `also failed` —
    kills the cause/attempt/context ambiguity.
38. **Render Appendix attachments** — `span_trail` ("what happened in the ~8 spans
    before it broke") is the most AI-valuable attachment and is currently
    invisible without programmatic access. Trailing `appendix:` section; **cap the
    backtrace** (32 frames + `… (N more)`) — uncapped backtraces are a
    token-budget attack on the LLM context this is designed for.
39. **`fingerprint: <hash>`** — hash of (code|type, root location, root-cause
    type). Pure → body-legal. Dedupe "same error, new log line" across runs.
40. **Source snippet at root location** (opt-in `OBSERVE_REPORT_SOURCE=1`) —
    ariadne + the source cache are already linked; emit
    `source: 42 | let cfg = load(path)?;`. Env-gated so snapshots stay stable.
41. **JSON v2** — location as `{file, line, column}` with real numbers;
    `scope_elapsed_ms` as number; causes as `{code, type, message, location,
    kind}` objects. Bump `REPORT_SCHEMA_VERSION`.
42. **Envelope: occurrence count** (`occurrence = 41` from `ERROR_COUNTS`,
    post-increment) — novel-vs-chronic is the first thing an agent should know.
    Must NOT enter the body: the counter is process-global and would make
    snapshots order-dependent.
43. **Envelope: thread name + `uptime_ms`** (monotonic clock already exists).
44. **Panic → report unification** — under `OBSERVE_REPORT=text|json` the panic
    hook renders the same report block (location from `PanicHookInfo`, backtrace
    appendix). Every process-level failure speaks one format.
45. **`doctor` prefix search** (`doctor E4` → matching codes — agents mistype) and
    a **`hint:` field** carrying a runnable command (`#[hint = "..."]` with
    `{code}` interpolation, defaults to the doctor invocation) — prose for humans
    in `action:`, command for agents in `hint:`.
- **Deliberately excluded:** wall-clock timestamps, env dumps, hostname/PID in the
  body, stack-variable capture (color-eyre trap — slow, leaks locals).

---

## Suggested implementation order

1. **Bug batch:** #1, #2, #4, #5 (+ drop `documented`, dead derives — #5's
   catch_unwind gate removal included).
2. **Hardening batch:** #3, #7, #8, #9 (+ staticassertions tests).
3. **Type batch:** #21, #22, #27 (mechanical, test-covered).
4. **Refactors:** #15, #17, #18, #24, #25 (each gated by the full suite).
5. **AI surface:** #35–#38, #45 (body), then #42–#44 (envelope), #41 (JSON v2 —
   schema bump gets its own commit).
6. **Fuzz targets** land alongside the code they guard (#30 with #4/#15, #31 with
   #3, #34 with #25).
