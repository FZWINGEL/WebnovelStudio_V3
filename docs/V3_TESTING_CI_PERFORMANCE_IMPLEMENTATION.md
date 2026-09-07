# Testing and CI performance implementation ledger

The original audit is the scope. Implementation and qualification are separate;
no hosted saving or full completion is claimed without comparable executed runs.

| Audit requirement | Implementation | Required verification |
|---|---|---|
| Preserve one integration binary, four Rust threads, two isolated Vitest workers, durability and all native checks | Existing contracts retained | Full checks and exact native check manifest |
| A: immediate lifecycle settlement, idempotence, bounded failure, ownership, timer/listener cleanup | Shared lifecycle helper in all seven native launchers; synthetic tests pass | Real WebView2 interruption comparison remains |
| A: setup/action/teardown and per-scenario timing, identities, suite manifest | All suites emit executable/runner identity, checkpoint intervals, native-startup and teardown events | Real serial/fan-out failure and timing evidence remains |
| B: transaction only fixture construction, PRAGMAs outside, close before import | Implemented with assertions | rusqlite fixture benchmark, logical equivalence, full importer tests |
| B: pure Node tests and separate browser behavior | projectTabs, three Workshop files, pure request builder and preference matrix implemented | Current local checks pass; real native coverage retained |
| C: earlier frontend feedback, both OS coverage | Ubuntu reordered | Five comparable runs, first feedback separately from full green |
| D: build once, exact revision/hash/toolchain/assets manifest | Artifact producer/consumer verifier implemented; adversarial tests pass | Transferred app runtime-dependency smoke remains |
| D: two isolated Windows consumers and unique failure uploads | Default fan-out matrix and unique failure uploads implemented | Real hosted fan-out and serial comparison remains |
| D: fail-closed final reconciliation of every expected check | Strict final gate and exact manifest implemented; adversarial tests pass | Hosted consumer/gate execution remains |
| Cargo compile timing and execution/fixture separation | Clippy/test/app timing upload implemented; native and fixture timing hooks added | Hosted compile/runtime evidence remains |
| Registry availability and frozen cold/warm path experiment | Default prepared-frozen path implemented; actual missing-cache and warm-offline paths pass | Hosted cost comparison remains |
| Pinned nextest comparison at identical concurrency, no retries, doctests | Pinned comparator executed; exact identities matched; nextest rejected after 4/5 passes | Reliability investigation needed before another adoption study |
| sccache targeted library-only experiment | Five local library pairs pass with isolated target/cache/server | Hosted end-to-end and cache-transfer evidence remains |
| Supported 4/8 CPU runner/cost comparison where account permits | Pending external capacity | Account runner availability and real comparable run evidence |
| Five comparable successful samples per candidate, all failures retained | Pending | Cold/dependency-hit/warm; frontend/Rust/dependency edits; exact identity parity |
| No destructive shortcuts; publication explicitly authorized | Retained | Final source and workflow review |

Current hosted status was rechecked through `gh api`: latest run 34164413029
is terminal failure at source 074fc96. Fresh annotations confirm billing
admission failure; no hosted steps ran. Local ongoing Workshop edits are outside this work.

## Publication update (8 September 2026)

The user explicitly authorized commit/push and CI changes before hosted
qualification. Push/PR and manual defaults now use two isolated Windows native
consumers and prepared/frozen Cargo commands. Manual serial/locked options
remain available. Local validation passes; transferred WebView2 execution,
hosted timing/cost comparisons and full audit qualification remain pending.
Nextest and sccache remain experiments, not default runners or caches.

## Second local implementation pass

- Added an explicit 102-check manifest across main (52), Workshop (31), HTTP
  (6), close (2), interruption (4), recovery (4), and memory lookup (3).
  Checkpoint IDs are emitted independently of human-readable descriptions.
- All seven native app launchers register close tracking immediately. Main,
  Workshop, HTTP, close, recovery and memory now share confirmed cleanup;
  interruption retains its ownership map using the same lifecycle helper.
- Added producer executable manifest/verification, Windows consumer matrix,
  and strict final reconciliation. Manual `native_layout=fanout` selects the
  candidate in this historical pass; the publication update below supersedes
  that default.
  Consumers never compile. Producer and consumers must all succeed; identity,
  duplicate/missing checks, failed spawn, unclosed process, shared desktop and
  forced normal-close shutdown fail the gate. Artifact transfer uses pinned
  download-artifact 8.0.1, whose current upstream usage was checked.
- Added optional `cargo_network=prepared-frozen` with offline availability probe
  and fetch only on a miss, then frozen commands. Compiler/test failures are not
  retried online. Clippy, tests and app build now request Cargo timings.
- Added root experiment pins and a runner comparator retaining every attempt,
  checking complete/ignored identities and source consistency, and including
  doctests separately with nextest at four slots and zero retries.

Executed evidence: 20 tooling tests pass with pinned Node; actionlint 1.7.12
validates the workflow; native module syntax checks pass; all 15 importer tests
pass both locked and frozen. The rusqlite example compares every table's logical
rows and source-preserving preview over five alternating fresh-file pairs:
setup median 73.6748 ms baseline versus 4.7803 ms transaction; preview remains
separately measured. Raw evidence: `.local/v2-fixture-benchmark.json`.

Three reviewed Workshop pure files (branchEvidence, questionSuggestions, store)
retain the exact same ten tests in five jsdom and five Node runs with two
workers. Vitest elapsed median: 1058.24 ms baseline (1047.36-1106.26), 242.26 ms
Node (241.24-257.23). This measures those files locally, excluding npm startup,
not hosted job savings. Raw JSON is `.local/frontend-benchmark/`.

Registry fallback was exercised with a genuinely missing cached dependency:
offline probe failed in 179 ms, locked fetch succeeded in 17.11 s; the next
probe completed offline in 344 ms. Raw reports are
`.local/performance/dependencies-{cold,warm}.json`. This establishes the path,
not an end-to-end saving or a reason to enable it by default.

Nextest's first inventory build failed before running tests because concurrent
Workshop work temporarily omitted `story_possibilities` in two desktop test
initializers. This failure is retained in
`.local/performance/runners-1788816142115/`; no runner comparison is claimed.
Current hosted job annotations confirm recent payment failure/spending limit
prevented run 34160853258 from starting any steps. Hosted fan-out, all-suite
native qualification, nextest successful comparisons, sccache, larger runner
cost comparison, and the full cache/edit matrix remain open. No CI dispatch,
commit or push was made in this pass.

## Isolated runner comparison

Stable checkout: detached cee4bb4 at `.local/performance-checkout`, with only the
transactional fixture and fixture assertions copied for the experiment. The
comparator verified every binary's complete and ignored test lists against
nextest's JSON inventory. Both runners used four slots; nextest retries were 0.
All 771 ordinary tests plus the ignored child identity were retained. Five
alternating full-run pairs and separate doctests are recorded under
`.local/performance-checkout/.local/performance/runners-1788816361868/`.

- Libtest: 5/5 pass, median 35.842 s, observed range 34.413-39.039 s.
- Nextest: 4/5 pass. All-attempt median 23.254 s, range 19.644-27.892 s,
  excluding separate doctest commands. These timings do not establish adoption.
- Doctests: 5/5 pass. The first command compiled for 41.579 s; remaining runs
  took 1.754-1.877 s. Compilation is not hidden from the end-to-end comparison.
- Nextest attempt 4 failed `windows_process::stop_requested_before_wait_does_not_begin_packet_delivery`
  (unconfirmed job-process cleanup) and
  `windows_process::timeout_preserves_partial_output_and_kills_tree`
  (fixture roles not published within the existing bound). No timeouts were
  shortened or weakened, and no failed test was retried automatically.

Decision: do not adopt nextest. The experiment exposed a reliability difference
that needs investigation before another adoption study. The candidate does not
meet the five-success qualification sample; failures remain part of evidence.
This is a local stable-source experiment, not current Workshop or hosted proof.

## Library-cache experiment

Pinned sccache 0.17.0 used the same isolated cee4bb4 source, a separate target
directory, an isolated cache and server port, and `CARGO_INCREMENTAL=0`.
Only `webnovel-core --lib` was rebuilt; dependencies were prepared once. Each
measurement cleaned only that package from the study target before building.
All five baseline and five wrapped builds passed with unchanged source hashes.

Baseline median: 18.032 s (17.911-18.203). The first wrapped cache miss took
21.648 s; four subsequent hits took 1.806-1.859 s. Statistics confirm one Rust
miss, four Rust hits, no cache errors, and a 42,926,026-byte local cache. The
all-attempt wrapped median is 1.838 s, but cold and warm observations must remain
separate. Preparation took 41.069 s and clean-command costs are retained.
Evidence: `.local/performance/sccache-1788816787116/`.

Decision: keep sccache experimental. This establishes useful local library
reuse, not end-to-end hosted savings. It does not remove app/test linking,
measure hosted cache transfer, or provide five samples for each cache state.
The dedicated study server was stopped; the author's cache/server was untouched.

## Commands and remaining adoption gates

- `scripts/desktop.ps1 check` includes artifact, exact-check reconciliation and
  process-lifecycle regressions in the existing tooling registration.
- `cargo run -p webnovel-core --locked --example benchmark_v2_fixture` emits
  pinned-rusqlite fixture/preview samples. `WNS_V3_FIXTURE_TIMINGS=1` additionally
  emits setup durations keyed by test name from the importer fixture loader.
- Run `node scripts/benchmark-rust-runner.mjs <pinned-nextest-executable>` from a
  stable checkout using pinned Node. It saves full logs and refuses changing
  source. It never installs or changes the default test runner.
- Run `node scripts/benchmark-sccache.mjs <isolated-checkout> <pinned-sccache-executable>`
  using pinned Node for the bounded library study. Tool pins live in root
  `ci-performance.json`; temporary downloaded binaries and study output stay
  under `.local/`.
- Manual CI offers `native_layout=fanout` and
  `cargo_network=prepared-frozen`. These are now the defaults; manual serial/locked options retain comparison paths.
  Do not infer a saving from a successful candidate run: retain at least five
  comparable successful hosted samples, all failures, cache/edit classifications,
  first-feedback and full-green times, and cache/artifact setup costs.
- Fan-out must still pass real WebView2 startup using the transferred executable,
  every registered native checkpoint, and cleanup on separate VMs before adoption.
  Local PE import inspection found Windows/system CRT imports; WebView2 is an
  external runtime and the frontend is embedded. The real transferred-app
  consumer smoke remains the decisive runtime-dependency check.
- Larger-runner/cost and complete cold/cache-hit/frontend/Rust/dependency edit
  matrices remain blocked by hosted admission. No claimed hosted percentile or
  automatic workflow adoption replaces those measurements.

Workshop request construction is now a pure module with stale capture refusal,
exact version/generation binding, voice-sample isolation, note organization and
fixed-detail comparison regressions. A pure preference matrix covers hard
project conflicts and confirmed/scoped resolution without a rendered app.
Existing rendered Workshop coverage remains; 52 focused tests and TypeScript
passed after extraction. Rust protection enforcement remains independent.

## Latest local verification

The latest full `scripts/desktop.ps1 check` passed on the current worktree:
774 Rust tests (one intentional child ignored), 594 frontend tests, formatting,
strict Clippy and production build. Log:
`.local/testing-audit-full-check-latest.log`. The earlier compile failure was
resolved in the concurrent Workshop work; this task did not replace those
changes. Subsequent tooling-only edits pass all 22 tooling checks, every native
module syntax check, actionlint, and `git diff --check`.

`collect-ci-timings.mjs <run-id>` now preserves raw GitHub run/jobs and derives
first frontend feedback independently of full-green time. Cache state, edit
kind and candidate labels remain explicitly unclassified until evidence assigns
them. Baseline run 34067931031 records 131 s first frontend feedback, 933 s
full-green from run start (Windows job 930 s); run 34160853258 is ineligible with
null feedback/full-green measurements because no test steps ran. These are
single historical observations, not the missing five-run candidate comparison.
Raw records: `.local/performance/ci-34067931031.json` and
`.local/performance/ci-34160853258.json`.

Serial CI now enables the same identity and timing trace as fan-out, so the
comparison need not infer check identity from counts. Native startup, process
lifetime and checkpoint intervals can overlap and must not be added blindly.
Normal-close reconciliation explicitly refuses forced cleanup.

Final verification for this pass: all 24 tooling tests pass after adding exact
runner-execution identity checks. Those checks were also applied retrospectively
to all ten recorded libtest/nextest attempts and verified the exact 771 ordinary
identities, including the failed attempt. Current debug Tauri build with
`--frozen --timings` passed and emitted its HTML report; strict workspace Clippy
with the same flags also passed. Build log:
`.local/testing-audit-debug-build.log`. This builds the app but does not qualify
native UI behavior.

Latest hosted recheck: run 34164413029, source 074fc96d9fa37a5924ea36878346f82e733bcb0e,
is terminal failure. Jobs 101872503977 and 101872504099 contain no steps. The
Windows annotation again states recent account payments failed or a spending
limit must be increased. Thus the real hosted/native adoption gates remain
open; the goal is not complete. No task-owned process remains running. The
isolated source checkout and all raw benchmark evidence are retained under
`.local/` for review and follow-up. Changes were uncommitted at that checkpoint.

## Follow-up gate review

A fresh source review found that missing process-close exit fields could pass
an inequality-to-null check. The gate now requires typed exit/signal fields and
an explicit termination method, rejects abnormal natural exits, refuses
contradictory passed/failure records, and binds each suite's measured hostname
to its consumer. Adversarial regressions cover these malformed records.
All 25 tooling tests pass. This closes a local fail-closed verification gap;
it is not native execution evidence.

Run 34164413029 and its Windows annotation were rechecked: the same billing
admission failure remains, with no hosted steps. The preceding goal turn made
implementation and measurement progress; this turn also made concrete gate
hardening progress. The goal remains active and incomplete pending the stated
native/hosted adoption gates.

## Goal blocked on hosted execution

The same billing admission block has been verified across three consecutive
goal turns. The latest run remains terminal with no job steps; there is no live
job to wait for. Local implementation, adversarial checks and component studies
are retained, but the remaining real native/fan-out and hosted cache/runner/cost
matrix cannot be proved without restored GitHub Actions capacity. The goal is
blocked, not complete. Restore account billing/spending capacity before running
and measuring the candidate workflow. Changes have not been committed or pushed.
