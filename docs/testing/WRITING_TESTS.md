# Writing and Maintaining Tests

This guide outlines test placement, registration rules, synchronization standards, and fixture hygiene for WebnovelStudio V3.

---

## 1. Test Placement and Registration

| Test Kind | Location | Registration & Enforcement | Runner Command |
|---|---|---|---|
| **Rust Unit Tests** | `src/**/*.rs` across workspace crates | Placed alongside code in `tests` submodules with `#[test]`. | `cargo test -p <crate> --lib` |
| **Core Integration Suites** | `crates/core/tests/<name>.rs` | **Must be registered** in `crates/core/tests/integration.rs` under the `suites!` macro. An automated harness test validates directory completeness and fails if any file is missing. Shared helpers live in `crates/core/tests/support/`. | `cargo test -p webnovel-core --test integration <module>::` |
| **Desktop Commands** | `apps/desktop/src-tauri/src/commands/` | Declared in Tauri command handlers and tested within `src-tauri/src/`. | `cargo test -p webnovel-desktop <filter>` |
| **Frontend Tests** | `apps/desktop/src/**/*.test.ts(x)` | Discovered by Vitest. Configured in `apps/desktop/vitest.config.ts`. | `desktop.cmd test <path>` |
| **Tooling Tests** | `scripts/*.test.mjs` | Discovered by Node native runner. Profile membership mapped in `scripts/run-tooling-tests.mjs`. | `node --test scripts/*.test.mjs` |
| **Native Scenarios** | `tests/native/*.mjs` | **Must be registered** in `scripts/native-suites.json` with explicit checkpoint IDs. | `desktop.cmd spike` then `desktop.cmd native` |

---

## 2. Fixture and Synchronization Rules

Follow these established repository patterns when writing tests:

### A. Private Temporary Paths and Owned Child Processes
- Never create unmanaged files in the repository root or shared system temporary directories.
- Always use RAII guards:
  - In Rust: use `tempfile::TempDir` for directories and wrap subprocesses in ownership guards that terminate processes on `Drop`.
  - In Node: use `mkdtempSync` inside `os.tmpdir()` and clean up in `try...finally` or test teardown hooks.

### B. Observable Readiness Over Arbitrary Sleeps
- **Never rely on fixed sleep intervals** to assume an asynchronous or background task has reached a specific state. Fixed sleeps cause flaky failures under CPU load and needlessly slow down test suites.
- Instead, poll for observable state transitions (e.g., waiting for a log message, file creation, or IPC event) with a bounded deadline:
  ```rust
  // Good: Wait for explicit observable event with bounded timeout
  let deadline = Instant::now() + Duration::from_secs(5);
  while Instant::now() < deadline {
      if client.is_ready().await { break; }
      tokio::time::sleep(Duration::from_millis(20)).await;
  }
  ```

### C. Explicit Gates for Concurrency Overlap
- When testing concurrency isolation (such as multi-connection handoffs or request cancellation), use explicit rendezvous primitives (`tokio::sync::Notify`, channels, or barriers) to guarantee that operation B is verifiably in flight before operation A is aborted.

### D. Actionable Diagnostics on Bounded Waits
- Timeout assertions must print the last observed state, received packet count, or subprocess exit status. A silent timeout forces developers to re-run the suite with ad-hoc debugging print statements.

### E. Preserving Durability and Real Timeouts
- Do not bypass SQLite write-ahead logging (WAL), set `synchronous=OFF`, or shorten intentional resilience timeouts just to make a test finish slightly faster. Durability recovery, transaction rollback, and timeout behaviors must be verified against their production configurations.

---

## 3. Test-Only Changes and the AST Boundary

Frontend changes that touch *only* tests (`*.test.ts(x)`) may bypass expensive native qualification runs. However, to ensure test code cannot contaminate production bundles or compromise runtime security, this policy is strictly enforced by an AST scanner:

- **Enforcing Suite**: `apps/desktop/src/featureBoundary.test.ts`
- **Mechanism**: Parses every TypeScript file in `apps/desktop/src/` using `@typescript-eslint/parser`.
- **Rule**: Any production file that imports from a test file, mock fixture, or test utility causes an immediate test failure.

---

## 4. Intentional Ignored Helpers

When running the full Rust workspace test suite, the output will consistently report:
```text
test result: ok. ... passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

- **Ignored Test**: `crates/core/tests/projects.rs` $\to$ `projects::tests::crash_child`
- **Purpose**: This function is marked `#[ignore]` because it is an intentional crash-inducing child process helper. It is not meant to be run directly by Cargo's test runner; instead, it is invoked explicitly as a subprocess by the parent test `projects::tests::recovers_from_unclean_process_crash`.
- **Rule**: **Do NOT remove `#[ignore]` from this helper, do NOT attempt to "fix" it, and do NOT claim zero ignored tests in qualification reports.** Exactly 1 ignored test is the expected, correct state.
