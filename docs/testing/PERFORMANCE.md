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
