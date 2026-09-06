# Development checks

Run the complete local check from the repository root:

```powershell
.\scripts\desktop.ps1 -Command check
```

It runs Rust formatting, strict workspace Clippy, every Rust test, TypeScript,
the frontend production build, and the full isolated frontend suite. Native
qualification remains separate; see [native checks](../tests/native/README.md).

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

## CI coverage

The Ubuntu job runs core Rust checks and frontend checks. The Windows native
job runs the complete Rust workspace and frontend suite, then the existing
native editor, HTTP, close, interruption, recovery and memory-lookup flows.
This removes the former duplicate Windows core job while retaining both
operating systems' coverage. Native safety checks are not skipped for speed.

Documentation-only changes skip the build workflow. Code, test, manifest,
script and workflow changes retain both jobs; manual dispatch remains available.
A newer push on the same branch cancels an obsolete run. Product-version checks
run before expensive build setup and during the local check.

The 3.0.0 candidate tests Windows CI with limited Rust debug information
(`CARGO_PROFILE_DEV_DEBUG=1`, `CARGO_PROFILE_TEST_DEBUG=1`) to reduce dependency
artifacts and cache work. Assertions and native development features remain
enabled. Local debugging and release profiles are unchanged; hosted qualification
must confirm the candidate before this experiment is counted as an improvement.

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
