# Testing WebnovelStudio V3

Run commands below from the repository root. On Windows, prefer the desktop launcher (`desktop.cmd` or `desktop.ps1`) so its pinned Node runtime (24.20.0), Cargo environment, and command wrappers are consistently applied.

Use a focused check while developing. Use `check` for the complete local source-check pipeline. Native UI execution, installed-package qualification, live-provider trials, and human evaluation are separate qualification gates.

The change-aware planner recommends checks; it does not execute them or establish that they passed.

---

## Command Reference

| Task | Command from repository root | Important boundary |
|---|---|---|
| **Prepare dependencies** | `.\scripts\desktop.cmd ensure-deps` | Reuses valid installation signatures; not a test run. |
| **Inspect working-tree test plan** | `.\scripts\desktop.cmd plan` | Advisory recommendation based on dirty files; does not execute checks. |
| **Inspect revision-range test plan** | `.\scripts\desktop.cmd plan --base HEAD~1 --json` | Evaluates committed changes against base revision; does not inspect uncommitted working tree. |
| **Check one Rust crate** | `.\scripts\desktop.cmd quick wns-documents` | Focused Clippy and unit test check (~1–2s warm); does not run full integration tests. |
| **Run workspace quick check** | `.\scripts\desktop.cmd quick` | Workspace Clippy, library/binary unit tests, and frontend typecheck. Skips integration tests and Vitest. |
| **Run focused frontend tests** | `.\scripts\desktop.cmd test src/kernel/document.test.ts` | Runs only the matching Vitest suites. |
| **Interactive frontend watch mode** | `.\scripts\desktop.cmd test:watch` | Interactive Vitest watcher for frontend development. |
| **Run complete local check pipeline** | `.\scripts\desktop.cmd check` | Comprehensive local gate: tooling core, formatting, Clippy, all Rust tests, frontend build, and Vitest. Does not execute native WebView2 UI or packaging. |
| **Build native test application** | `.\scripts\desktop.cmd spike` | Builds debug WebView2 binary with remote debugging enabled. |
| **Run main native smoke** | `.\scripts\desktop.cmd native` | Runs the main WebView2 smoke automation against the built spike binary. |
| **Prune compilation cache** | `.\scripts\desktop.cmd prune` | Safely clears stale build artifacts without rebuilding external dependencies. |

> [!TIP]
> **PowerShell Syntax**:
> In PowerShell, execute via:
> ```powershell
> .\scripts\desktop.ps1 -Command <command> [args...]
> # Or directly:
> powershell.exe -ExecutionPolicy Bypass -File scripts/desktop.ps1 <command> [args...]
> ```

---

## Command Traps and Distinctions

### 1. `setup` vs `ensure-*`
- `desktop.cmd setup` forces package reinstallation regardless of current state.
- `desktop.cmd ensure-deps` (or `ensure-frontend`, `ensure-native`) validates SHA-256 signatures of `package.json` and lockfiles. If a verified installation already exists in `.cache`, it reuses it instantaneously without network activity.

### 2. Launcher Test Filters vs Cargo Filters
- When invoking `desktop.cmd test <filter>`, pass the test file or suite name directly:
  ```powershell
  .\scripts\desktop.cmd test src/kernel/document.test.ts
  ```
  Do **not** prefix the filter with `--`. PowerShell's parameter binder treats a lone `--` as an ambiguous parameter name.
- When calling raw `cargo test`, argument separator rules apply:
  ```powershell
  cargo test -p webnovel-core --test integration discussions::
  cargo test -p webnovel-desktop discussion_commands::
  ```

### 3. Working-Tree vs Revision-Range Planning
- `desktop.cmd plan` inspects only unstaged and uncommitted changes in your working tree (`git status`).
- If you have already committed your changes, running `desktop.cmd plan` on a clean checkout will report clean status and suggest zero checks.
- To inspect committed changes, pass an explicit base revision:
  ```powershell
  .\scripts\desktop.cmd plan --base HEAD~1
  ```

### 4. `quick` vs `check` Scope

| Phase | `desktop.cmd quick` | `desktop.cmd check` |
|---|:---:|:---:|
| Core Tooling Tests (`scripts/*.test.mjs`) | ❌ Skipped | ✅ Executed (`--profile=core`) |
| Rust Formatting Check (`cargo fmt`) | ✅ Executed | ✅ Executed |
| Rust Workspace Clippy (`-D warnings`) | ✅ Executed | ✅ Executed |
| Rust Library & Binary Unit Tests | ✅ Executed | ✅ Executed |
| Rust Integration Test Suites (`tests/integration.rs`) | ❌ Skipped | ✅ Executed |
| Rust Documentation Tests (`wns_documents`) | ❌ Skipped | ✅ Executed |
| Frontend TypeScript Check (`tsc --noEmit`) | ✅ Executed (when no crate passed) | ✅ Executed |
| Frontend Production Build (`npm run build`) | ❌ Skipped | ✅ Executed |
| Frontend Vitest Test Suites | ❌ Skipped | ✅ Executed |
| Native WebView2 UI Smoke Flow | ❌ Skipped | ❌ Separate (`spike` + `native`) |

### 5. Tooling Test Profiles
The script test runner (`scripts/run-tooling-tests.mjs`) supports three explicit profiles:
- `node scripts/run-tooling-tests.mjs --profile=core`: Runs version consistency, launcher logic, and test planner verification. Fast, pure-Node, zero native dependencies.
- `node scripts/run-tooling-tests.mjs --profile=native-preflight`: Validates native suite manifests, checkpoint ID registrations, and dependency trees.
- `node scripts/run-tooling-tests.mjs --profile=all`: Executes all tooling tests. (Default if no `--profile` argument is provided).

---

## Testing Handbook Navigation

- [Qualification Boundaries and CI Architecture](testing/QUALIFICATION.md): What each check establishes, CI topology, native obligations, and rerun/retest rules.
- [Writing and Maintaining Tests](testing/WRITING_TESTS.md): Test placement, registration macros, fixture hygiene, synchronization, and AST boundary rules.
- [Performance Benchmarking and Decisions](testing/PERFORMANCE.md): Standard benchmark template, accepted/rejected decision records, and measurement traps.
- [Native Windows Execution Runbook](../tests/native/README.md): Building, running, and debugging native WebView2 automation suites.
- [Historical Testing & CI Implementation Ledger](V3_TESTING_CI_PERFORMANCE_IMPLEMENTATION.md): Historical implementation record of the September 2026 testing audit and optimization experiments.
