# Development checks

For the fast inner-loop development check (~8–10 seconds), run:

```powershell
.\scripts\desktop.ps1 -Command quick
# Or from Command Prompt / batch:
.\scripts\desktop.cmd quick
```

This checks Rust formatting, strict workspace Clippy, runs all workspace crate
unit tests, and validates TypeScript without running the full 75 integration test suites.

Run the complete local qualification check before submitting:

```powershell
.\scripts\desktop.ps1 -Command check
# Or from Command Prompt / batch:
.\scripts\desktop.cmd check
```

It runs Rust formatting, strict workspace Clippy, every Rust test, TypeScript,
the frontend production build, and the full isolated frontend suite. Native
qualification remains separate; see [native checks](../tests/native/README.md).

To safely prune orphaned compilation artifacts and recover tens of gigabytes of disk space without rebuilding dependencies:

```powershell
.\scripts\desktop.cmd prune
```

For a focused development loop, use the normal Cargo or Vitest filters:

```powershell
# A Rust integration suite; names are prefixed by the existing source filename.
cargo test -p webnovel-core --test integration discussions::
cargo test -p webnovel-core --test integration context_migration::

# Desktop command tests.
cargo test -p webnovel-desktop discussion_commands::

# One frontend file, with its normal isolated jsdom environment.
npm.cmd --prefix apps/desktop test -- src/providers/ModelSelector.test.tsx
```

Focused checks speed up iteration. Run the full check before submitting a
change; use the relevant native flow when editor, lifecycle, or IPC behavior
changes. These commands use mock providers and synthetic projects.

## Current chat-first qualification boundary — 9 September 2026

The project conversation is an **opt-in** native development surface. The
authoritative [chat-first implementation status](V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md)
maintains current Rust, frontend, native, live-provider, and build-identity
evidence. The functional surface includes direct source attachment, explicit
chapter handoff, isolated draft review, project-scoped resizing, and responsive
documents/review navigation.

A local native 200% zoom geometry check is verified. Human formative evaluation,
installed-package behavior, screen-reader behavior, and broader provider and
narrative qualification remain open. English-only authoring means IME
qualification is not a product requirement; chat remains opt-in. A bounded
Codex Exec trial now covers the handoff and chapter-range response contracts;
other adapters and narrative quality require separate qualification.

Run the rebuilt local smoke after `.\scripts\desktop.ps1 -Command spike` with:

```powershell
npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-smoke.mjs
```

The live check is separately opt-in and submits exactly two requests:

```powershell
$previous = $env:WNS_V3_ALLOW_LIVE_CHAT
try {
    $env:WNS_V3_ALLOW_LIVE_CHAT = '1'
    npm.cmd exec --yes --package=node@24.20.0 -- node apps/desktop/scripts/native-chat-live.mjs
} finally {
    $env:WNS_V3_ALLOW_LIVE_CHAT = $previous
}
```

These checks establish development behavior only. See [ADR
0034](ADR_0034_PROJECT_CONVERSATION.md) and the [chat-first implementation
status](V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md) for qualification boundaries.
The current project reader floor is schema 40.

To qualify the newer handoff and chapter-range response formats, set
`WNS_V3_CHAT_LIVE_MODE=contracts` as well as `WNS_V3_ALLOW_LIVE_CHAT=1` for the
same live script. This separate mode submits at most two requests: one
project-chat v2 handoff and one unselected chapter discussion proposing an
exact paragraph. Confirmation stages an unsent edit and must leave ordinary
heads unchanged. It retains both packets, outputs, and provider receipts under
`.local/native-results/chat-live-contracts`. It never retries or falls back;
a failed or uncertain attempt requires inspection before another trial.
Restore both environment variables after the run. This mode is not a narrative
quality benchmark or a replacement for the author/installed-package gates.

## Bounded live Workshop check

`crates/core/examples/qualify_live_workshop.rs` is a separate Windows-only,
explicitly opted-in check. With no flag it exits before provider discovery or
project creation. Normal tests and CI never enable the flag.

When deliberately qualifying a live connection, this command permits
at most one Codex invocation on a synthetic project, using the discovered
Luna/xhigh/priority author settings and the
normal core packet, output, and durable receipt path:

```powershell
$previousWorkshopOptIn = $env:WNS_V3_ALLOW_LIVE_WORKSHOP
try {
    $env:WNS_V3_ALLOW_LIVE_WORKSHOP = '1'
    cargo run --locked -p webnovel-core --example qualify_live_workshop
} finally {
    $env:WNS_V3_ALLOW_LIVE_WORKSHOP = $previousWorkshopOptIn
}
```

It never retries. A unique `.local/workshop-qualification-*.json` report retains
the exact packet, dispatch uncertainty, terminal receipt, parsed alternatives,
and synthetic project location. The project is retained for inspection; use
that evidence after a failure instead of blindly running another generation.
This checks the provider/core boundary, not native UI or narrative quality.
Append `-- --app-server` to the Cargo command to qualify the optional transport
instead of Exec. This still permits only one explicit synthetic request and
never falls back or resends after failure.

## Optional Codex app-server no-generation check

The optional persistent Codex route uses a Rust client over local stdio. Exec
remains the default, and bounded story lookup remains on Exec. The development
check validates app-owned isolation, the read-only file-auth handoff, the
restrictive multi-model catalog, fresh ephemeral threads, Astra low/priority
maintenance traits, pre-turn cancellation, warm reuse, and cleanup without
submitting a paid `turn/start`:

```powershell
cargo run --locked -p webnovel-core --example qualify_codex_app_server -- --check
```

The 8 September 2026 check passed on installed Codex 0.153.4 and retained its
report at `.local/app-server-qualification/final-check.json`. Four fresh-thread
checks covered Astra low with priority, Standard, priority again, and Luna;
Standard did not inherit priority. The run took 8.604 seconds, including
3.450 seconds discovery, 2.761 seconds startup, and 2.075 seconds shutdown.
This no-generation sample is not a latency benchmark.
It does not qualify live generation, native UI behavior, narrative quality,
hosted providers, or release packaging. The app-server route remains an opt-in
development transport and has no automatic fallback or request replay.

## Explicit transport comparison

`crates/core/examples/compare_codex_transport.rs` is inert by default. With
`WNS_V3_ALLOW_TRANSPORT_COMPARISON=1`, it authorizes three separate synthetic
Astra/low/priority requests: Exec, first app-server, and warm app-server. It
retains one durable `.local/codex-transport-comparison-*.json` report, never
retries a failed case, and measures server startup separately. Its Exec first
visible result is a completed message, not time to first token. Normal tests
and CI do not opt into this live check. See [the qualification record](APP_SERVER_QUALIFICATION.md)
for the executed sample and its limits.

## Rust test organization

`crates/core/tests/integration.rs` compiles the existing integration suites as
separate modules in one executable. Register new top-level test files in its
`suites!` list. Its registration test compares that list with the directory and
fails if a file is omitted. Keep shared helper files under `tests/support/`.
The legacy-schema helper is imported once by the harness and reused by suites.

The manifest disables automatic integration-target discovery and explicitly
registers the grouped target. Normal `cargo test --workspace --locked` still
runs every unit, binary, integration and documentation test. Cargo continues to
provide the Windows process-fixture environment; no custom runner, schema
cache, in-memory database substitution, or test retry was introduced.

`.cargo/config.toml` defaults to four test threads. This avoids excessive
contention between durable SQLite writers on machines with many cores. An
existing `RUST_TEST_THREADS` value or explicit `-- --test-threads=N` argument
can override it. WAL, `synchronous=FULL`, actual project creation/migration,
reopening, recovery and intentional timeout behavior remain unchanged.

## Audit experiments

See the [implementation and measurement ledger](V3_TESTING_CI_PERFORMANCE_IMPLEMENTATION.md)
for the complete audit scope, exact evidence and adoption gates. Manual CI can
select serial native scheduling or locked registry access for comparison.
Push, pull-request and manual defaults use build-once native fan-out and
prepared/frozen Cargo commands, as authorized on 8 September 2026. Hosted
qualification and performance comparisons remain pending. A fan-out run requires its final native gate;
the producer's successful compilation alone is not native qualification.

## CI coverage

The Ubuntu job runs frontend checks before Rust setup and core checks. The Windows native
job runs the complete Rust workspace and frontend suite, then the short
Workshop smoke followed by the existing native editor, HTTP, close,
interruption, recovery and memory-lookup flows. Running Workshop first surfaces
its failures before waiting for the broader editor flow; every gate is retained.
This removes the former duplicate Windows core job while retaining both
operating systems' coverage. Native safety checks are not skipped for speed.

Documentation-only changes skip the build workflow. Code, test, manifest,
script and workflow changes retain both jobs; manual dispatch remains available.
A newer push on the same branch cancels an obsolete run. Product-version checks
run before expensive build setup and during the local check.

The Windows job allows 35 minutes for a cold build and cache upload. The first
3.0.0 run passed all test and native steps but exceeded the previous 25-minute
job limit during cache saving. Individual test harness timeouts are unchanged.

The 3.0.0 candidate now uses limited Rust debug information
(`CARGO_PROFILE_DEV_DEBUG=1`, `CARGO_PROFILE_TEST_DEBUG=1`) in standard CI to
reduce dependency artifacts and cache work. Assertions and native development
features remain enabled. This is adopted for hosted CI; local debugging and
release profiles are unchanged.

The final standard CI qualification for source
`63770b9122f598b3e32ea1b0f5f4020c4325115f` is [run
34067931031](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067931031).
Both jobs passed 721 Rust tests, 434 frontend tests, and 11 tooling checks. The
Windows job also passed all 52 main native checks plus the HTTP, normal-close,
strict-interruption, recovered-project, and reviewed-memory lookup flows. Logs
are `.local/release-ci-qualified.log` and
`.local/release-ci-qualified-jobs.json`.

The adopted cache restored 657,680,598 bytes versus 1,067,977,623 bytes at the
earlier full-debug checkpoint, about 38% smaller. The qualified run recorded a
38-second restore, 71-second Clippy step, and 289-second Rust test step; its
post-cache step was already up to date, so no archive was created. Local
debugging and release profiles remain unchanged.

## Measured checkpoint — 7 September 2026

The Windows baseline on the same 24-logical-processor machine was 92.58 seconds
for `cargo test --workspace --locked`: 720 passing tests and one intentionally
ignored subprocess fixture. The first grouped run took 38.83 seconds including
8.40 seconds of compilation, with the same original tests plus the registration
guard. The full local check passed 721 Rust and 434 frontend tests in 52.04
seconds. These are local measurements; CI timing is recorded separately in
[implementation status](IMPLEMENTATION_STATUS.md).

A final warm run with the shared helper imported once took **31.01 seconds**
for all 721 Rust tests, with 0.30 seconds of compilation. The baseline also had
a warm build (0.34 seconds), making this about **66.5% less wall time** for the
same original tests plus the guard. Report:
`.local/test-performance-grouped-warm.{log,json}`.

An independent name-level comparison confirmed all 587 original integration
tests across 63 files are present in the grouped harness. The 62 passing core
unit tests, 71 desktop tests and one intentionally ignored unit fixture also
remain unchanged; the registration guard is the only added test.

Baseline and qualification logs are ignored under `.local/test-performance-*`.
A temporary unregistered test file was correctly refused by the guard and
removed afterward. Core and SQLite optimization-level experiments did not
improve the slow discussion suite enough to adopt. Frontend thread-pool changes
saved little, so its existing isolation and CI worker limit were retained.

A later warm serial full check took **38.61 seconds**. Running its Rust chain
alongside its frontend chain took **51.32 seconds** with the same passing tests,
so that experiment was reverted. Cargo checks and the frontend suite remain
sequential. Use the normal root `target/` cache for routine development; avoid
creating a separate Cargo target tree for each feature or test invocation.

## Installer harness iteration

Dispatch **Windows package smoke** without inputs for a fresh locked build.
The workflow retains its installer before exercising the installed lifecycle,
so a failed harness does not discard a successful build. It also checks that
the installed executable has the expected product version.

For a harness or documentation change, supply the original completed package
run ID in the optional `installer_run_id` field. The workflow downloads the
installer and original metadata, checks the source commit and complete changed
file list, hashes, clean build source, and version, then runs the current harness
without Rust setup, dependency installation, or a package rebuild. Changes to
application code, manifests, dependencies, or build scripts require a fresh
build. A failed or missing original build and ambiguous/mismatched artifacts
are refused.

Retest evidence keeps the original `installerBuild` identity separately from
the current `qualificationSource` commit and harness hash. It never describes
the current harness checkout as the installer build source. Both paths retain
the same synthetic install, write, reopen, normal close, and same-version
uninstall/reinstall checks. Their qualification remains separate from offline
installation, true upgrades, live providers, and an author trial.

The small Node guard suite runs in the normal local check and both CI jobs:

```powershell
node --test scripts/check-versions.test.mjs scripts/prepare-package-retest.test.mjs
```

The fresh package job [34067936098](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067936098)
passed in 16 minutes 20 seconds. [Retest 34068729080](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34068729080)
passed the same installed lifecycle in **1 minute 59 seconds**, using the exact
installer from `63770b9` and the documentation-only harness checkout `ded8f3d`.
All four dependency/build steps were skipped; original and qualification source
identities remained distinct in the downloaded evidence. This is a measured
improvement for harness iteration, not a substitute for a fresh application build.

## Testing audit optimization follow-up (7 September 2026)

Implemented interruption cleanup with immediately registered close tracking,
shared completion, explicit timeout failures, and five tooling regressions.
Interruption reports retain cleanup errors, scenario/setup/teardown timings,
runner identity and expected scenarios. Schema-8 fixture setup now transacts
only construction, checks foreign keys/schema/commit state and closes explicitly.
Project-tab pure tests use Node, with separate browser localStorage coverage.
Ubuntu frontend checks precede Rust setup; Windows coverage is retained. Cargo
test timing artifacts upload from both jobs. No durability gates were removed.

Validation: pinned-Node full local check passed 16 tooling checks, formatting,
strict Clippy, workspace Rust tests, production build and 569 frontend tests.
The subsequently added browser-storage test and existing pure file passed all
7 focused tests. Native script syntax passed. Other ongoing Workshop changes
were preserved; this evidence describes the checkout at execution time.

Five alternating local Python SQLite fixture pairs measured baseline setup
median 70.75 ms (69.57-80.86), transaction median 3.90 ms (3.69-4.38). This is
fixture SQL evidence using Python SQLite, not a rusqlite or hosted CI benchmark.

Pending: fresh native interruption recovery/termination qualification, timing
instrumentation across other suites, and five comparable hosted runs per
candidate. Build-once native fan-out remains deferred until its setup/artifact
cost and exact suite reconciliation can be qualified. The latest repository
record reports GitHub billing admission failure. No CI dispatch, push, or native
UI run was performed. Nextest/cache/runner experiments remain deferred.
