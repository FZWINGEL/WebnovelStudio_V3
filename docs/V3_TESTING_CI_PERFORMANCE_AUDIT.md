# WebnovelStudio V3 — Testing and CI performance audit

**Date:** 7 September 2026  
**Repository:** `FZWINGEL/WebnovelStudio_V3`  
**Branch inspected:** `codex/v3-persistence`  
**Source snapshot:** `c83a127423512b0963eefebb992fefe719280cb2`  
**Timing baseline:** successful push run `34067931031`, source `63770b9122f598b3e32ea1b0f5f4020c4325115f`  
**Status:** Read-only audit and proposed changes. No repository changes or GitHub Actions runs were made for this audit. The in-progress, unpushed Story Workshop implementation is outside this snapshot.

## Recommendation

Fix avoidable test-harness work before weakening test gates or replacing the runner. The strongest immediate candidates are repeated process cleanup in the interruption harness and autocommit-heavy construction of legacy-import fixtures. Then improve frontend feedback scheduling, separate pure tests from DOM tests, and evaluate build-once native fan-out across isolated Windows runners.

The existing setup is already substantially optimized. Do not reintroduce separate core integration executables, unbounded SQLite test concurrency, or a second Windows core-contract job. Do not turn off SQLite durability or remove crash/recovery checks to obtain a better timing number.

## 1. Evidence and baseline

The latest successful push run returned by the connector was run `34067931031`. Its Windows job ran from 6 September 23:48:57 UTC to 7 September 00:04:27 UTC: **15m30s**. Its Ubuntu job took **3m24s**. This is one observed run, not a median or a guaranteed steady-state duration. [R1, R2]

| Windows component | Observed duration |
|---|---:|
| Rust dependency-cache restoration | 38 s |
| Workspace Clippy | 71 s |
| Workspace Rust tests, including compilation | 289 s |
| Tauri debug application build | 68 s |
| Main native suite | 182 s |
| Native HTTP suite | 14 s |
| Native normal-close suite | 15 s |
| Native interruption suite | 94 s |
| Native recovery suite | 17 s |
| Native memory-lookup suite | 8 s |
| Frontend tests | 46 s |
| npm installation | 12 s |
| Evidence upload | 8 s |
| Post-job npm cache creation | 28 s |
| Other setup, cleanup, and inter-step overhead | 40 s |
| **Total** | **930 s** |

The six native suites account for **330 seconds**, or **5m30s**, of that job. The npm cache was missing in this sample; its 28-second save must not be presented as a recurring cost on every run. The Rust dependency cache was a hit. [R2, R3]

The Windows job log provides a further decomposition of the 289-second Rust step:

| Rust phase | Log timing |
|---|---:|
| Compilation of test targets | Approximately 98 s |
| Core unit tests | 1.77 s |
| Core integration tests | 165.95 s |
| Desktop tests | 20.15 s |

The remaining difference is command and harness overhead. A faster compiler cache cannot remove the 166 seconds spent actually executing core integration tests. Conversely, faster fixture setup does not remove compilation and linking. [R3]

The documented Windows inventory at this checkpoint is 721 passing Rust tests, with the intentional child fixture ignored in the ordinary run, 434 frontend tests, 11 release/tooling tests, and 52 checks in the main native script, plus the other named native suites. Inventory is a baseline for comparison, not a permanently hard-coded count: Workshop implementation will legitimately add tests. [R4]

### Existing optimizations to preserve

`docs/TESTING.md`, `.cargo/config.toml`, and the workflows already establish:

- One registered core integration-test executable, with automatic test discovery disabled and a registration guard.
- Four Rust test threads by default to limit contention between durable database writers; explicit overrides remain possible.
- Two CI Vitest workers and fresh isolated jsdom environments.
- Rust and npm caching, Windows dev/test debug information reduced to level 1, documentation-only workflow exclusions, cancellation of superseded runs, and no duplicate Windows core-contract job.
- Separate manual installer qualification, including guarded reuse of an existing installer for eligible harness-only changes.

Repository measurements report warm local workspace testing reduced from 92.58 s to 31.01 s. A later full local check measured 38.61 s serial versus 51.32 s with Rust and frontend tasks running concurrently; that parallelization was reverted. The Windows Rust cache shrank from about 1.068 GB to 0.658 GB. These are existing repository-reported results, not new audit measurements. [R4–R7]

## 2. First fix: make native process cleanup idempotent

**Location:** `apps/desktop/scripts/native-interruption.mjs`, `stopIfAlive`, scenario-level cleanup, and the final `ownedApps` cleanup loop. [R8]

The helper currently treats `app.exitCode === null` as evidence that the process is still running. It sends a termination signal and waits for an `exit` event or a five-second timeout.

That predicate is incomplete. A process terminated by a signal can have a null `exitCode` and a non-null `signalCode`. Cleanup called after that termination can therefore wait for an event that has already occurred. Node documents both the distinction and why `killed` is not proof of termination. [E1]

The interruption scenarios launch six processes in total. They stop the processes during scenario cleanup, then the outer `finally` calls the same helper on all retained process objects. The run log shows a **30.145-second gap** between the last scenario summary (`00:02:48.362Z`) and the final success output (`00:03:18.507Z`). Six repeated five-second waits fit that gap. This is a strong causal hypothesis, not yet an A/B-tested Windows result. [R3, R8]

An isolated reproduction performed during this audit on **Linux, Node v22.16.0** terminated an owned child with SIGTERM and called the original helper again. It observed:

```text
exitCode: null
signalCode: SIGTERM
original repeated cleanup: 5005.58 ms
already-exited guard: below the displayed measurement resolution
```

This verifies the generic lifecycle problem. It is not a Windows/Tauri benchmark; the project pins Node 24.20.0. The reproduction source and result are included in the evidence archive.

### Implementation contract

Create a process-lifecycle record when spawning a child and attach settlement listeners immediately. Reuse its completion promise during repeated cleanup. Recognize ordinary exit, signal exit, failed spawn, and still-running states separately. An already-exited child satisfies `exitCode !== null || signalCode !== null`; this check alone is not a complete process-tree shutdown implementation.

Keep bounded shutdown and ownership checks. If actual termination or required stream closure cannot be confirmed, record an explicit cleanup failure instead of silently declaring success. Clear timers and listeners on settlement. Keep normal-close qualification distinct from forced process termination.

Add synthetic tests for already-exited normal and signaled children, repeated cleanup, a live child, failed spawn, and timeout/unconfirmed cleanup. These belong in the existing tooling-test registration, not only in the slow native journey.

**Expected impact:** approximately 30 seconds is a plausible recoverable component of the observed interruption run. Measure the actual Windows before/after result; do not shorten the timeout as a substitute for fixing state tracking.

## 3. Second fix: transact legacy fixture construction

**Locations:** `crates/core/tests/v2_import.rs::TempSource::new` and `tests/fixtures/v2-import/schema8.sql`. [R9, R10]

Each legacy-import test creates a fresh on-disk SQLite database and executes a fixture containing many CREATE and INSERT statements. Neither the loader nor the fixture puts that construction into one explicit transaction. SQLite's autocommit behavior can therefore impose transaction overhead repeatedly. SQLite specifically documents batching operations into a transaction as a way to amortize that overhead. [E2]

The V2-import test completions occupy a conspicuous portion of the Windows integration suite's tail, with the last finishing around 42 seconds after the first V2-import result. Because tests overlap and the log reports completions rather than individual start times, this is not a measurement of total V2 fixture cost. Instrument fixture setup separately from importer execution. [R3]

### Proposed change

Keep the two initial PRAGMAs outside the transaction, then wrap only the fixture's schema and data construction:

```sql
PRAGMA foreign_keys = ON;
PRAGMA user_version = 8;

BEGIN;
-- Existing fixture CREATE TABLE and INSERT statements, unchanged.
COMMIT;
```

The `foreign_keys` ordering matters: changing it inside a transaction is a no-op. Verify enforcement is enabled on the setup connection rather than assuming the pragma worked. [E3]

Commit and close the fixture connection before recording the source bytes/hash or running the operation under test. Preserve the fresh unique file per test. Check the schema version, logical fixture contents, foreign-key consistency, and the existing assertions that actual import/preview does not modify the source.

Do **not** wrap production operations or assertions in a transaction just for speed. Do not bypass production migrations, substitute in-memory databases, share writable fixtures, weaken synchronous settings, or change fault-injection boundaries. This change targets the construction of an old source fixture, not the durability behavior being tested.

**Expected impact:** unmeasured. Likely worth testing because it removes repeated setup work at a prominent point in the slow suite; no speedup multiplier is justified yet.

## 4. Improve frontend feedback and avoid unnecessary DOM setup

**Locations:** `apps/desktop/vitest.config.ts`, pure test files, and `.github/workflows/ci.yml`. [R5, R11, R12]

The config puts every test file in jsdom and caps CI workers at two. The Windows frontend run took 44.64 seconds within its 46-second npm step. Its Vitest breakdown attributed 67% of tracked phase time to environment setup and 26% to tests. Those percentages aggregate worker time; they are not percentages of wall-clock duration. Vitest's current performance guide makes this distinction explicit. [R3, E4]

Use the Node environment for genuinely pure tests. `src/shell/projectTabs.test.ts` is a concrete candidate: its runtime inputs are plain records and an injected Map-backed storage implementation. Keep browser-default/localStorage integration coverage separate where needed. [R12]

Vitest supports an explicit per-file environment comment:

```ts
// @vitest-environment node
```

A small reviewed allowlist or clearly separated projects is preferable to blindly assigning every `.test.ts` file to Node. Tiptap and ProseMirror tests may need DOM even when their extension is `.ts`. React, editor selection, focus, and browser-global behavior should remain in jsdom or real WebView2. Preserve isolation and the current worker limit initially. [E5]

For the new Workshop, make preference resolution, constraint conflicts, alternative selection, lock-preservation rules, and request-payload construction testable without rendering the entire application. Test Rust enforcement independently as well. Keep representative native journeys for actual persistence, IPC, proposal adoption, restart, and recovery.

### Fast feedback scheduling

The Ubuntu job currently finishes its Rust work before installing and testing the frontend. Extract a small independent frontend/tooling job, or first move the frontend checks earlier as a low-complexity experiment. Retain the existing Windows frontend execution unless a separately reviewed coverage decision removes it.

An independent job improves time to a useful frontend result. It does not eliminate the Windows gate or automatically shorten time to a fully green run. Do not multiply a 46-second frontend suite into many hosted shards unless setup-adjusted measurements justify it.

## 5. Build once, run native suites on two isolated Windows runners

**Locations:** `.github/workflows/ci.yml`, native launcher scripts, new artifact manifest and result aggregation. [R5, R8, R13]

The current native suites execute sequentially after the debug application has been built. Start with two consumers of the exact same artifact:

| Native consumer | Suites | Baseline suite time |
|---|---|---:|
| Main UI journey | `test:native` | 182 s |
| Lifecycle and transport | HTTP, normal close, interruption, recovery, memory lookup, sequentially | 148 s |

The arithmetic upper bound before new overhead is a reduction from 330 seconds to `max(182,148)=182` seconds: **148 seconds**. This is not a measured end-to-end saving. New VM startup, checkout, Node/npm setup, artifact upload/download, result aggregation, and any changed cache behavior subtract from it. The cleanup fix overlaps the second consumer's time, so do not add all projected savings mechanically.

GitHub workflow artifacts support transferring build output between jobs. The native harness already accepts `WNS_V3_NATIVE_EXE` in the inspected main and interruption scripts. Audit that interface and runtime dependencies across every consumer before enabling fan-out. The log's approximately 41 MB application is a more suitable transfer unit than the entire 658 MB Rust cache. [E6, R3, R8, R13]

### Required boundaries

The producer must build the existing debug Tauri application from the exact checked-out revision. Include a manifest recording the commit, executable SHA-256, build mode, toolchain identity, and required runtime/asset files. Consumers use that executable and matching checkout, do not compile another copy, and verify identity before starting. Include diagnostic symbols when needed, not automatically the whole target tree.

Use a separate Windows VM per native consumer. Separate profiles and ports alone do not isolate shared OS focus, keyboard input, clipboard, and native dialogs on one desktop. Retain those tests on real WebView2 rather than replacing them with a browser-only approximation.

Upload uniquely named evidence from every consumer, including failure paths. A final gate must require all scheduled consumers and reconcile their check identities against the expected suite manifest. Missing results, malformed reports, incomplete cleanup, and an unexpectedly skipped consumer must not produce a green result. Apply the repository's explicit documented-skip policy separately.

Start by keeping Clippy, Rust tests, and app compilation together in one producer job to preserve build reuse. Shipping Rust test binaries/nextest archives for a more aggressive build/test split is a separate, higher-complexity experiment, not a prerequisite.

## 6. Compilation, caches, and test runners: targeted experiments

### Cargo timings before build-system changes

Capture stable HTML timing reports for the existing Cargo invocations, for example:

```text
cargo test --workspace --locked --timings
```

Upload `target/cargo-timings/` as diagnostic evidence. Cargo's report exposes compilation units, dependency critical paths, features, build scripts, and concurrency; binary units do not always provide a separate code-generation breakdown. Supplement it with step timestamps and explicit test-runtime measurements. [E7]

Do not assume Clippy, Rust test binaries, and the embedded-asset Tauri executable are identical build products whose work can simply be deleted. Test changes and frontend asset changes have different invalidation paths.

### Keep the existing dependency cache first

The pinned rust-cache action already restores dependencies. Its documented default does not cache workspace crates and disables incremental compilation. This explains why a cache hit can coexist with compilation of project targets. [R3, E8]

Do not make caching the whole workspace target tree the first intervention. Test its restoration, extraction, upload, and correctness costs against compilation savings. Any app-artifact key must cover frontend assets, Rust sources, manifests, build scripts, features, compiler flags, and toolchain/runtime-relevant configuration. A Cargo.lock-only app cache would be unsound.

The 38-second Rust restore is much smaller than the combined compile-and-execute work. Avoid introducing another large cache without measuring total end-to-end cost.

### Check registry access separately

The Clippy log has about 33 seconds between `Updating crates.io index` and the first project-crate line. That is an observed gap, not a precise network-only measurement. Investigate whether warm cache runs can avoid repeated registry refreshes.

A dependency-preparation phase can validate availability and explicitly fetch missing locked dependencies; later Cargo commands can use `--frozen`, which combines `--locked` and `--offline`. Keep a working cold-cache path. Simply adding an unconditional `cargo fetch` may move the same work into another step instead of saving it. Do not retry a failing compiler/test invocation online indiscriminately. [E9]

### Nextest: benchmark, do not assume

Nextest offers global scheduling, per-test processes, constrained test groups, per-test reports, and build archives. Those can be useful, especially for distinguishing heavy durable-SQLite tests from lightweight contracts. However, the repository has already consolidated integration tests into one binary. That removes a common source of nextest's scheduling advantage, while hundreds of separate Windows test processes add another cost. [R4, E10, E11]

Compare the existing runner against a pinned nextest version using the same tests, fixtures, and concurrency. Validate ignored child fixtures, process-spawning tests, environment isolation, and any in-process coordination assumptions. Disable automatic retries for the comparison. Preserve a separate `cargo test --doc` step: current nextest documentation says doctests are not supported. [E12]

### sccache: limited relevance to the measured path

The upstream Rust documentation says targets that invoke the system linker, including binaries, cannot be cached. sccache may help reusable library compilation; it does not eliminate test-binary or Tauri-executable linking. Treat it as a later measured experiment, not a guaranteed fix for the 98-second test compilation and 59-second application compilation/linking portions. [R3, E13]

### Runner capacity

This is a private repository using `windows-latest`. GitHub currently documents its standard private x64 Windows runner as 2 CPUs and 8 GB RAM. The local measurements in the repository used a machine with 24 logical CPUs. The difference is a reason not to compare warm local test runtime directly with hosted CI wall time; it does not prove CPU count explains all the difference. [R1, R4, E14]

After harness fixes, benchmark the same workflow on a supported 4- or 8-vCPU Windows runner where the account permits it. Compare cost per successful result as well as duration. A separate isolated self-hosted machine is another operational option, not a reason to run focus-stealing native UI tests beside the author's active writing/coding session. Keep Linux portability and actual Windows behavior checks.

## 7. Measurement and implementation order

### Change set A — Instrumentation and cleanup

Add per-native-scenario and setup/action/teardown durations, executable identity, runner identity, and a suite manifest. Add lifecycle-helper regression tests and fix repeated cleanup. Verify interruption recovery, real process termination, and evidence reporting still work. Do not add this audit's reproduction to production without adapting it to the pinned Node version and existing tooling conventions.

### Change set B — Fixture setup and pure tests

Transact only schema-8 fixture construction. Verify foreign keys, fixture contents, closed connection state, isolated paths, and unchanged-source assertions during the actual importer tests. Move reviewed pure frontend files to Node and leave editor/UI tests isolated. Measure both changes separately before combining their reported gains.

### Change set C — Earlier frontend feedback

Add the independent fast frontend/tooling job or reorder checks first. Preserve full OS coverage. Record time to first actionable feedback separately from time to full green.

### Change set D — Native fan-out

Build one app, publish the manifest and necessary files, and run the two native consumers on separate Windows machines. Preserve the complete expected check set. Keep the change only if artifact/setup overhead is lower than the measured serial time removed.

### Evaluation protocol

Use at least five comparable successful runs per candidate as an initial engineering sample, alternating baseline and candidate where practical. Report the median and observed range; do not claim a robust tail percentile from five runs. Record failures and flakes as well, rather than timing only convenient successes.

Separate cold caches, dependency-cache hits with changed sources, and warm local reruns. Test frontend-only, Rust-only, and dependency-changing edits. Record compile time, execution time, fixture time, teardown, cache I/O, native startup, and artifact transfer separately. Verify exact test identities across the old/new schedules; total counts alone can conceal a missing suite plus a duplicate.

Keep the existing four Rust test threads initially. Any 2/4/8 comparison belongs on the actual target runner and must preserve intentional timeout/locking semantics. Compilation job count, Rust test threads, Vitest workers, and native VM concurrency are different controls; do not turn them all up together.

During Workshop development, use the repository's existing focused commands for inner-loop changes, then the full prescribed checks before handoff. Prefer table-driven pure tests for many preference/constraint combinations and a smaller set of full native author journeys, without deleting existing native regressions.

## 8. Non-goals and rejected shortcuts

Do not turn off WAL or `synchronous=FULL`, share mutable test databases, bypass migrations, run tests in memory instead of real files, shorten observation windows for no-replay checks, or replace normal-close testing with forced kill. Do not run multiple native UI suites on one desktop. Do not remove Windows coverage just because Ubuntu is faster. Do not treat provider test time as live LLM latency: the inspected native scenarios use synthetic/loopback fixtures.

Do not add a monorepo orchestration framework or switch package managers to solve a 12-second install step before addressing five minutes of native tests and nearly five minutes of Rust compilation/execution. Do not make UI-only changes reuse an executable with stale embedded frontend assets. Do not automatically publish, commit, or enable the proposed workflows based on this report.

## Sources

### Repository evidence

All source links below are to the inspected immutable snapshot except the explicit run/log endpoints.

[R1] Latest push run query: https://api.github.com/repos/FZWINGEL/WebnovelStudio_V3/actions/runs?branch=codex%2Fv3-persistence&event=push&per_page=1  
[R2] Timed jobs: https://api.github.com/repos/FZWINGEL/WebnovelStudio_V3/actions/runs/34067931031/jobs  
[R3] Windows job and log: https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067931031/job/101579887315  
[R4] Testing guide: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/docs/TESTING.md  
[R5] CI workflow: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/.github/workflows/ci.yml  
[R6] Cargo test-thread default: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/.cargo/config.toml  
[R7] Installer workflow: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/.github/workflows/windows-package-smoke.yml  
[R8] Interruption harness: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/apps/desktop/scripts/native-interruption.mjs  
[R9] V2 importer tests: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/crates/core/tests/v2_import.rs  
[R10] Schema-8 fixture: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/tests/fixtures/v2-import/schema8.sql  
[R11] Vitest config: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/apps/desktop/vitest.config.ts  
[R12] Pure-test candidate: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/apps/desktop/src/shell/projectTabs.test.ts  
[R13] Main native harness: https://github.com/FZWINGEL/WebnovelStudio_V3/blob/c83a127423512b0963eefebb992fefe719280cb2/apps/desktop/scripts/native-smoke.mjs

### Primary external documentation, checked 7 September 2026

[E1] Node ChildProcess lifecycle: https://nodejs.org/api/child_process.html  
[E2] SQLite FAQ, transaction batching: https://www.sqlite.org/faq.html#q19  
[E3] SQLite foreign_keys pragma: https://www.sqlite.org/pragma.html#pragma_foreign_keys  
[E4] Vitest current/main performance guide: https://main.vitest.dev/guide/improving-performance  
[E5] Vitest environments: https://vitest.dev/guide/environment.html  
[E6] GitHub artifacts between jobs: https://docs.github.com/en/actions/tutorials/store-and-share-data  
[E7] Cargo build timings: https://doc.rust-lang.org/cargo/reference/timings.html  
[E8] rust-cache upstream behavior: https://github.com/Swatinem/rust-cache  
[E9] Cargo fetch and frozen/offline modes: https://doc.rust-lang.org/cargo/commands/cargo-fetch.html  
[E10] Nextest execution model: https://nexte.st/docs/design/how-it-works/  
[E11] Nextest test groups: https://nexte.st/docs/configuration/test-groups/  
[E12] Nextest doctest limitation: https://nexte.st/  
[E13] sccache Rust limitations: https://github.com/mozilla/sccache/blob/main/docs/Rust.md  
[E14] GitHub hosted runner specifications: https://docs.github.com/en/actions/reference/runners/github-hosted-runners

External documentation informs the recommendations; newly added tool versions and options must still be validated against the repository's pinned toolchain. No end-to-end improvement claim in this report replaces a new Windows CI measurement.
