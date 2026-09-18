# Performance, Benchmarking, and Optimization Decisions

This document records the standard procedure for performance benchmarking in WebnovelStudio V3, documents accepted and rejected optimization decisions, and highlights common measurement traps.

---

## Benchmark Experiment Template

Every performance experiment must be documented using this standard template to ensure reproducibility and prevent re-litigating settled decisions.

```markdown
# Experiment: <Clear question being tested>
Status: proposed | measured | adopted | not adopted | superseded

## Source and Environment
- Baseline commit SHA:
- Candidate commit SHA (or patch identity):
- Toolchain: Node version, Rustc version, Cargo version, OS build
- Hardware: CPU model, logical cores, memory
- Environment variables: RUST_TEST_THREADS, etc.
- Workspace condition: Cold, dependency-hit, or warm cache

## Procedure
- Exact commands executed:
- Working directory:
- Warmup policy (e.g. 1 untimed warmup run):
- Sample order (e.g. alternating baseline/candidate A-B-A-B):
- Concurrency isolation: Confirmation that external heavy processes were excluded

## Results
- Full table of every attempt (including any failures or timeouts):
- Wall time (ms / s) per run:
- Reported test summary time (ms / s):
- Median and range:

## Interpretation
- What was directly measured:
- What is inferred:
- What remains unclassified residual:

## Decision
- Outcome: Adopted | Not Adopted | Retained Current
- Reopening condition: Explicit evidence or threshold required to reopen this decision

## Evidence
- Repository-relative paths to machine-readable logs or sanitized traces
```

---

## Optimization Decision Records

### 1. Retain 4 Rust Test Threads
- **Status**: `adopted`
- **Question**: Should the default Rust test thread concurrency (`.cargo/config.toml`) be changed from 4?
- **Context**: On high-core developer machines (e.g., 16 cores), setting test threads to physical core counts causes severe SQLite writer lock contention because WebnovelStudio uses real SQLite databases with Write-Ahead Logging (WAL) and disk persistence.
- **Results**: A controlled 3-round sweep across 1, 2, 4, 8, and 16 threads demonstrated that 4 threads provided the optimal balance between CPU utilization and database write serialization. Higher thread counts increased runtime variance due to lock retries.
- **Decision**: Retain 4 threads as the repository default in `.cargo/config.toml`. Contributors can override via `RUST_TEST_THREADS=N` or `-- --test-threads=N`.
- **Reopening Condition**: Reopening requires multi-run warm and cold benchmark data on Windows demonstrating lower median wall time across the full integration suite without increasing test failure or lock timeout rates.

---

### 2. Remove App-Server Fixture Mutex
- **Status**: `adopted` (Commit `5f0e38d`)
- **Question**: Can the global synchronization mutex in the `codex_app_server` test fixture be safely removed?
- **Context**: The test fixture previously acquired a global process-wide mutex across 14 separate test cases, serializing tests that otherwise operated on independent in-memory state.
- **Results**: Removing the mutex eliminated artificial serialization, reducing test suite execution time from 68.98s down to 54.34s (a 14.64s or 21.2% speedup) with zero test assertion modifications or timeout shortenings.
- **Decision**: Adopted. In-memory connection tests run concurrently.
- **Reopening Condition**: Reopening requires evidence of cross-test state leakage, data races, or unmanaged port/pipe collisions under concurrent execution.

---

### 3. Disable Empty Fixture Test Harness
- **Status**: `adopted` (Commit `6eb1a42`)
- **Question**: Does `windows-process-fixture` need an active libtest harness in default workspace test runs?
- **Context**: The `windows-process-fixture` binary is a synthetic child process helper used by integration tests. It contains no unit tests. By default, Cargo compiled and executed an empty test executable (`windows_process_fixture-*.exe`) during `cargo test --workspace`.
- **Results**: Adding `test = false` and `bench = false` to `[[bin]] name = "windows-process-fixture"` in `crates/core/Cargo.toml` eliminated the empty test target, reducing workspace targets from 41 to 40. Integration tests still compile and locate the binary via `CARGO_BIN_EXE_windows-process-fixture`. All 979 Rust tests pass.
- **Decision**: Adopted as a cleanup removing redundant target compilation/execution. Not claimed as a major end-to-end wall-time speedup (measured timing delta was within normal run-to-run noise).
- **Reopening Condition**: Reopening requires unit tests to be added directly to `windows-process-fixture.rs` or proof that integration tests cannot access the compiled binary.

---

### 4. Retain Existing Binary Selection in `quick` Check
- **Status**: `not adopted`
- **Question**: Should `desktop.cmd quick` replace `--bins` with `--bin webnovel-desktop` to avoid executing the empty fixture harness during quick checks?
- **Context**: While `test = false` omits the fixture from default test runs, passing `--bins` explicitly overrides default selection and executes all binaries.
- **Results**: Benchmarking `--bins` vs explicit `--bin webnovel-desktop` in workspace quick runs showed ~4.58s vs ~4.66s (within standard OS variance of ~80ms). In `webnovel-core`, running `--lib` alone vs `--lib --bins` showed ~848ms vs ~862ms (~14ms delta).
- **Decision**: Rejected. Pruning the empty binary target from `quick` yields no meaningful speedup while introducing package-specific target selection logic into launcher scripts.
- **Reopening Condition**: Reopening requires demonstrating a statistically significant (> 1 second) improvement without breaking binary tests in `webnovel-desktop`.

---

### 5. Non-Adoption of Nextest Runner
- **Status**: `not adopted`
- **Question**: Should `cargo-nextest` replace standard `cargo test` for workspace test execution?
- **Context**: Nextest executes test binaries in parallel processes with isolated process management.
- **Results**: In evaluation runs on Windows, nextest failed 1 out of 5 attempts due to process cleanup timing, signal handling differences, and lack of built-in support for Rust doc-tests (which required a separate runner invocation).
- **Decision**: Rejected for production CI and default development flows due to reliability requirements.
- **Reopening Condition**: Reopening requires 100% pass reliability across at least 20 consecutive runs on Windows, seamless doc-test integration, and demonstrated end-to-end wall-time savings.

### 6. CI Dependency `opt-level = 1` and Producer Cache Repair
- **Status**: `adopted` (CI only; local defaults unchanged)
- **Question**: Should hosted CI compile dependencies at the manifest's `opt-level = 2`, and could the truncated `rust-cache` entry be republished without changing verification?
- **Context**: The restored warm entry contained only check-level artifacts: `cargo clippy` finished in seconds while `cargo test` and `desktop:spike` recompiled every dependency (feature variant included) on each run. GitHub cache entries are immutable, so the partial entry exact-matched permanently.
- **Procedure**: Staged hosted-runner runs on the PR branch — cold `opt-level = 2` populate under a bumped `shared-key`, warm `opt-level = 2` with a source edit, cold `opt-level = 0` populate via a checkout-scoped `.cargo/config.toml` override, warm `opt-level = 0` with a source edit, then `opt-level = 1` after the consumer flake signal. Numbers are GitHub step timestamps, not `Compiling` line gaps.
- **Results**: `windows-native` producer — poisoned-warm opt2 `26m57s`, cold opt2 `34m55s`, warm opt2 `8m23s`, cold opt0 `16m29s`, warm opt0 `9m48s`, opt1 `TBD`. Warm opt0 step timings: dependency restore `68s`, clippy `34s`, `cargo test` `4m43s`, `desktop:spike` `1m17s`; fully green end-to-end `14m04s` vs `~34m` on the poisoned-cache baseline. Cold builds gain the most from lowered opt levels (dependency codegen dominates there); a `Cargo.lock` change always implies a cold run, so the cold saving is the operative one. **However**, across the three opt0 runs `native-consumer (main)` flaked twice (`memory_views` row count read before the write landed; draft textbox empty after navigate-back) versus once across the three opt2 runs — each at a different write-then-read assertion, consistent with unoptimized dependency code slowing the debug app's write paths. Weak but directional evidence that opt0 raised the consumer flake rate.
- **Decision**: Adopted the cache repair plus `opt-level = 1` (not `0`) as the dependency override: keeps most of the cold-compile saving while the app the consumers exercise runs its dependency code much closer to opt2 speed. `cache-on-failure: false` plus a `shared-key` generation bump restored a complete cache; the override appends to `.cargo/config.toml` before the cache step (self-invalidating through the lockfile hash, skipped in the index so the artifact gate still sees a clean checkout). sccache was evaluated and rejected: with a warm dependency cache the compile surface is already near zero, so a second cache layer adds only maintenance cost.
- **Reopening Condition**: Drop back to `opt-level = 2` if consumer flakes persist at `1`; revisit `opt-level = 0` only if evidence shows the flakes were ambient (e.g. the same assertions failing on opt2/opt1 builds) — that would also justify fixing the underlying test races directly.
- **Evidence**: workflow runs `35397231323` (cold opt2 populate), `35400445360` (warm opt2 + source edit), `35403190753` (cold opt0 populate), `35404930115` (warm opt0 + source edit, fully green), `35405936306` (warm opt0, consumer flake at `native-smoke.mjs:790`) on `FZWINGEL/WebnovelStudio_V3`; `cargo-timings-windows-*` artifacts on each run.

---

## Measurement Traps

Avoid these common benchmarking errors:

> [!WARNING]
> ### 1. Test Counts Are Not Coverage
> Increasing test counts does not necessarily mean higher code coverage. Conversely, eliminating redundant empty harnesses reduces target counts without losing a single logical test assertion.
>
> ### 2. Test-Summary Time $\neq$ Process Wall Time
> Summing the duration numbers printed in `test result: ok ... finished in X.XXs` does not equal Cargo wall time. Cargo wall time includes dependency freshness checking, compiler/linker invocations, target dispatch, rustdoc execution, and Windows process creation/teardown.
>
> ### 3. Worker Times Cannot Be Summed
> Summing elapsed times from parallel workers (e.g. summing Vitest worker threads or Cargo test threads) yields total CPU time, not elapsed wall time.
>
> ### 4. Warm vs Post-Install vs Cold Runs
> A run immediately after `npm ci` or `cargo build` is not comparable to a warm cache run. Always specify whether a measurement is cold, post-install, or warm.
>
> ### 5. Cargo Target Flag Semantics
> - `--tests`: Selects all integration tests, unit tests, and test-configured benchmarks. It does **not** mean "integration tests only".
> - `--no-run`: Compiles test binaries but executes zero tests. It produces no test results.
> - `--lib`: Restricts execution to library targets, omitting binary targets (`apps/desktop/src-tauri` tests will not run).
> - `--bins`: Overrides `test = false` in `Cargo.toml` and selects all binary targets.
