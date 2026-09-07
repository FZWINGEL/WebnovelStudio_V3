# V3 implementation status

**Status date:** 7 September 2026

**Current branch:** `codex/v3-persistence`
**Overall:** in progress; the full V3 goal is not complete.

The current Workshop controls checkpoint preserves provisional consequence
comparisons, includes relationship endpoint preferences, keeps protection
independent of decision status, respects question dispositions, and guards
navigation while a request is being prepared or reconciled. The standard
wrapper passes 766 Rust / 544 frontend / 11 tooling checks; the final frontend
check also passes 544 tests after the last UI corrections. Exact local,
headless and pending native/package evidence is recorded in the
[Workshop ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md).

Product source is `fc3468829e037588458a05204c0ef92ddbea9cc2`. Its debug build
succeeded, and fresh [CI 34150860150](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150860150)
and [package run 34150884037](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150884037)
are pending on that exact source. The preceding package does not qualify these
product changes.

The preceding names/aliases implementation is pushed at
`b801ae9cfc00203a37c7de05da8de801a0214af1`. Fresh
[CI 34147701184](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147701184)
and [installer run 34147720364](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147720364)
ran on that exact source. CI passed the contract steps, then failed after 16
Workshop groups on a count-bearing `Characters 2` tab selector; the harness
correction is awaiting a fresh native run. The installer job passed its
synthetic installed write/reopen and same-version retention lifecycle.
No prior installer was reused for this product change. The current corrections
and exact qualification boundaries are recorded in the
[Workshop ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md).

### Story Workshop — implementation in progress

The new [Story Workshop specification](V3_STORY_WORKSHOP_UX_SPEC.md), fetched from
`codex/v3-persistence` at `c83a127`, is being implemented across all three slices.
The [requirement ledger](V3_STORY_WORKSHOP_IMPLEMENTATION.md) tracks the full
scope. [CI 34133645198](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34133645198)
passed all 15 Workshop native checks and the existing native suites. The fresh
[Workshop installer](WINDOWS_PACKAGE_QUALIFICATION.md) also passed its installed
write/reopen and same-version retention lifecycle. Full specification acceptance,
broader provider/native coverage, and author evaluation remain open.

The previous relationship slice added the schema-36 reader-floor migration for
typed relationship packet fields. It preserves existing Workshop state,
context, packet bytes, and hashes. World/People relationship exploration now
prepares an independent session without a model call, pins both endpoint heads,
shows named direction and uncertainty, and ignores or refuses stale/late reads;
RequestContext projects the immutable relationship envelope. Moment responses
require two or three treatments, while a one-treatment raw response remains
recoverable. That previous relationship slice's focused UI checks passed (32 tests in 3 files); headless checks at
1440 and 800 pixels covered keyboard use, two sources, an explicit destination,
and frozen context with no errors or overflow. Two visual rounds were inspected
under `.local/workshop-relationship-qa/`. After the UI wording correction, that
previous slice's standalone frontend build/check passed 514 tests in 46 files;
this was not a full workspace check.

The current W23 local slice adds optional Unicode names, aliases, and
transliterations to existing character/world documents without a schema change.
The World/People saved-material picker and Writer **Names & aliases** surface use
an explicit source-epoch CAS save, read-only reconciliation after an uncertain
write, and dirty navigation/close protection. Component focused checks pass 11,
Workshop checks pass 28, and Writer/session checks pass 42; they verify
unchanged title/body and restricted-context alias exclusion. The aliases full
wrapper passes 763 Rust tests (70 core unit, 620 grouped integration, 73
desktop; one intentional subprocess ignore), 534 frontend tests in 48 files,
11 tooling checks, formatting, strict Clippy, TypeScript, and production build;
the pre-existing large-chunk warning is the only noted warning. The pinned
frontend-only check at `.local/workshop-aliases-final-frontend.log` also passed
534 tests in 48 files after the accessibility markup correction. The source-final
headless fixture at `.local/workshop-aliases-qa/report.json` passed 1440 and 800
pixel checks for Unicode/transliterations, dirty navigation refusal, exactly one
lost-ack current-read confirmation, close/reopen, and no AI or manuscript calls,
errors, or overflow. The fresh native aliases CI run, full specification, and
author study remain pending.

The previous relationship-slice 18:51 Berlin standard wrapper passed 762 Rust tests (70 core unit,
619 grouped integration, 73 desktop; one intentional subprocess fixture ignore),
515 frontend tests in 46 files, 11 tooling checks, formatting, strict Clippy,
TypeScript, and production build. Log: `.local/workshop-relationship-final-check.log`.
The final review also covers relationship edits with unchanged participant documents:
results become stale when their saved relationship or target scope changes, while
historical output and selected details remain saveable. New generation and adoption
refuse mismatched candidate authority. Unrelated relationship edits leave independent
requests fresh; the UI refreshes completed-result status after a relationship save.

This slice is pushed as `e61640a4738128b9744919275e362e402c7ed0d8`.
[CI 34145255173](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145255173)
reached 21 groups, including relationship exploration and preset/lens/new-project
flows, then failed at W30 when the sidebar-close control was intercepted;
subsequent auxiliary suites were skipped. [Package run 34145254658](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145254658)
succeeded on that exact source with installer SHA-256
`59394f6903ff4cc633e47556cffc3e921a7784cfa32a3da4bd68fcfcaf3ea495`.
The current native harness includes names/aliases checks, but they are not yet
executed. No broad native pass is claimed.
The aliases debug build completed successfully with
`.\scripts\desktop.ps1 -Command spike` at 19:24 UTC (17:24:48 local). The
binary is `D:\WebnovelStudio_V3\target\debug\webnovel-desktop.exe`,
48,684,032 bytes, ProductVersion 3.0.0, SHA-256
`f171aa81b37e908b96b7d10d13b47d77054040fa35adad4dad67418a1022512e`.
The build log is `.local/workshop-aliases-debug-build.log` and its manifest is
`.local/workshop-aliases-debug-build.json`. A fresh process read found no prior
author application process; no application was closed and the rebuilt app was
not launched.

The preceding hosted what-if run [CI 34141998962](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34141998962)
on exact source `ff276eb5eecfec3b38da3af758ca1f6377add8a4` ended after 20
Workshop checks with no page errors when W30 could not reach its button behind
the open context overlay. The harness now closes that overlay through the real
button and has 22 groups including relationship coverage; this failure does not
establish a broad native pass. Package run [34142007124](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34142007124)
succeeded on that exact source with installer SHA-256
`954416c66c8982f03113b643550dda6d39539539cfaf3a608be4911f13d1cc01`.
The installer was not downloaded for this checkpoint; upgrade, live/provider,
and broader Workshop-native/quality gates remain open.

The subsequent local hardening checkpoint fixes saved preset reuse/name editing,
definition-only updates, and per-source Story Bible failure isolation with exact
revision verification. Rust now rejects competing chosen decisions and trims/
case-normalizes confirmed hard-project preference conflicts. New persisted-request
tests cover rejection/noncanon/chat exclusions and mandatory context-budget refusal
without losing author notes. The 17:19 Berlin standard check passed 749 Rust,
488 frontend (42 files), and 11 tooling checks; the later copy/spacing adjustment
passed 16 focused frontend tests and the 17:22 Berlin final standard check with
the same totals. Synthetic Chromium review/reuse and unavailable
Story Bible checks passed at 1440 and 800 pixels with no page errors or overflow.
The expanded native harness is ready for a new hosted run. The earlier installer
does not contain these changes; current qualification is tracked in the ledger.
The final reviewed-source check at 17:29 Berlin passes 750 Rust tests (608 grouped
integration), 488 frontend tests and 11 tooling checks. It includes the retained
trashed-source revision read with unchanged live-document/restore restrictions,
plus symmetric preference conflict handling. The expanded native harness has
20 intended check groups; execution is still required on the new source.
The first hosted attempt, [CI 34138626172](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34138626172)
on `0cd1c95`, passed Ubuntu and Windows Rust/frontend/build, then 19 Workshop
groups with zero page errors. It stopped on an ambiguous heading selector in the
final untitled-project group; the harness correction is locally reproduced.
The later native suites were skipped. Fresh package run
[34138625419](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34138625419)
was intentionally cancelled before lifecycle qualification finished; a product
notice fix and the what-if changes require a fresh build. The 17:46 Berlin full
check of the notice fix passed 750 Rust, 488 frontend, and 11 tooling checks,
including formatting, Clippy, TypeScript, and build. The notice now confirms
explicit preset adoption instead of still saying no preferences were added.
`target/debug/webnovel-desktop.exe`
has been rebuilt from `0cd1c95`; exact identity is recorded in the Workshop ledger.

The corrected [CI 34139777354](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34139777354)
passed both jobs at `27d1ef1a1b6571b84174e9210e0bc546fcbdaeda`. Downloaded evidence
confirms 20 Workshop groups, 52 main native checks, HTTP 6, normal-close 2,
interruption 4, project recovery 4, and memory lookup 3, with clean runtime
observations on WebView2 `151.0.4129.101`. It predates the preset-notice and
what-if changes and does not qualify a new installer.

The next what-if slice implements branch graph validation and immutable existing
ancestry, isolates draft protection, inherits chosen ancestor context, and adds
source-bound decision/impact comparison with explicit adoption. Inherited candidate
impacts appear in adoption review. The 18:09 Berlin standard check passes 755 Rust
tests (613 grouped integration), 499 frontend tests in 44 files, 11 tooling checks,
formatting, strict Clippy, TypeScript, and build. Synthetic headless keyboard/source
and visual checks pass at 1440 and 800 pixels. The hosted Workshop harness now has
21 groups; this source still needs its own native run and fresh installer. See the
[what-if contracts and evidence](V3_STORY_WORKSHOP_IMPLEMENTATION.md#what-if-comparison-and-isolation-implementation).

The implementation is pushed at `ff276eb5eecfec3b38da3af758ca1f6377add8a4`.
[CI 34141998962](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34141998962)
ended after 20 Workshop checks when W30 could not reach its button behind the
open context overlay; no page errors were recorded. The harness now closes that
overlay through its real button and has 22 groups including relationship
coverage, so this is not a broad native pass. Package run
[34142007124](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34142007124)
succeeded on the same source with installer SHA-256
`954416c66c8982f03113b643550dda6d39539539cfaf3a608be4911f13d1cc01`.
The installer was not downloaded; upgrade, live/provider, and broader
Workshop-native/quality gates remain open.

The Develop/Write shell and dedicated six-lens Workshop frontend are implemented
with editable brief/current direction/original notes, typed three-candidate
comparison, detail selection, working draft and local history, scoped
preferences, custom tags, preset review/import/export UI, relationships for
existing and new document endpoints in one adoption, noncanon moments/guidance, what-if and existing
parent comparison, and a source-based Story Bible that projects exact chosen
revisions. Explicit saved-packet reads are available in the Workshop context
view. The 14:45 checkpoint passed the full frontend build and 465 tests
in 39 files, including frozen selected ranges, stale scoped-edit refusal,
and offline reconciliation with immutable request identity.

Synthetic headless Chromium inspection found no page errors or horizontal
overflow at 600, 800, 1024, and 1440 pixels; it is browser-fixture evidence,
not native qualification. The local captures are under `.local/workshop-qa/` and
are ignored. The earlier 14:45 `desktop.ps1 -Command check` passed at that checkpoint:
formatting, strict workspace Clippy, all Rust workspace tests, production build,
465 frontend tests, and 11 tooling checks. Schema-36 integration includes exact
candidate provenance, frozen selection scope, supersession, before/after
revisions, protected additions and paragraph boundaries, immutable retries,
and readable recovered alternatives. Five independent database boundary tests
cover atomic stale refusal, exact history, chapter preservation, author-secret
exclusion, hard preference conflicts, and multiline protection. Broader native, live-provider, and requirement-specific quality checks remain pending.

The subsequent development changes add atomic relationship adoption with stable
new endpoints, candidate-derived impact flags with author classifications,
explicit rejection-to-preference promotion, reviewed voice-guidance generation,
an explicit subversion choice, and a recap based on actual saved decisions.
The 15:56:22 Berlin local `desktop.ps1 -Command check` passed 746 Rust tests (plus one
intentional subprocess entry-point ignore), 474 frontend tests in 41 files,
11 tooling checks, formatting, strict workspace Clippy, TypeScript, and the
production build. Candidate IDs and relationship IDs remain attached to impact
flags; changed relationship endpoints reference their exact adoption decisions.
Workshop UI/source and focused tests also cover question status cycling, the
saved-decision recap, offline/manual save and retry identity, late-result
protection, partial/stopped recovery, and relationship endpoint/impact
provenance. These are local development checks. A first bounded headless live
Workshop smoke on source `2a97a39` completed at 13:47 UTC with one
Luna/xhigh/priority request, 7,289 confirmed stdin bytes, 2,106 input tokens, 2,354
output tokens, 1,167 reasoning tokens, and three valid directions; the unchanged
anchor, manual path, and no-chapter boundaries held. The response named internal
anchors as affected material; source review showed that these could become
visible review flags. Current/historical flag projection and backend
anchor-override handling were then fixed while preserving raw responses and real
flags. A separate read-only database reopen verified the saved packet, input
hash, immutable receipt, response, and usage without another model call. This is
one smoke, not broad live-provider or requirement-specific quality qualification.
Earlier hosted qualification history is retained below; the latest bounded native checkpoint is recorded after it.

[CI 34124050562](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34124050562)
on `a023511` passed the existing Windows native suite. Ubuntu frontend checks
passed 464 of 465 tests; the chapter-review test read the screen before async
summary validation finished. Its local synchronization fix passes all 33 focused
ReviewPanel tests. The first new Workshop native suite stopped at its synthetic
path-containment guard with a Windows short-name temp directory. The harness now
canonicalizes its temp root before comparing paths; the guard remains intact.
The subsequent hosted run
([CI 34127175895](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34127175895))
passed Ubuntu, Windows Rust/frontend/build, and the existing 52-check native
suite, then passed six bounded Workshop native groups before timing out at the
adoption **Kind** selector. It recorded no page errors on WebView2
`151.0.4129.101`; later native suites were skipped. Failure artifacts are under
`.local/ci-workshop-34127175895/workshop/failure.*`. A headless reproduction
showed the harness's exact-label `getByLabel` lookup failing while the exact
role/combobox lookup resolves. The initial harness correction was then exercised
by the native-first rerun below; this is not a broad native pass. Live Workshop qualification, broader installer/quality coverage, and the author
study remain pending. The
native-first rerun [CI 34129236987](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34129236987)
on source `2a97a39` also failed in the six bounded Workshop groups at the
prefilled **Content** selector: the exact-label lookup found no element while
the exact textbox role resolved. Ubuntu and Windows checks/build passed; later
native gates were skipped. The harness correction was exercised by the later 341307 run. Earlier
package run `34129253871` was intentionally cancelled because the product
fix required a fresh installer; the later package result below qualifies the
current installed lifecycle.

The next hosted run [CI 34130744589](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34130744589)
on exact source `df747b6d4f8d9496cb00f1fc5f0b53285d4b5edf` completed as a harness
failure at 14:14:09 UTC. Ubuntu, Windows frontend/Clippy/Rust/build, and the
WebView2 `151.0.4129.101` environment passed; the Workshop path passed 10 of
15 intended groups with no page errors, including explicit mock generation,
detail selection/manual edit, adoption preview without writes, one world
adoption with zero chapters and an authorRoom decision, the Develop-to-Write
barrier, directional relationships with two exact heads, and history UI. After
reopen, line 344 waited on a hidden starting-idea textarea because
`details.workshop-brief` was collapsed. The harness now expands the summary
before querying the exact textbox role; the remaining five Workshop groups and
all later native suites were skipped and remain unqualified. This is a harness
failure, not a product failure or broad native pass. Artifacts are under
`.local/ci-workshop-34130744589/workshop/`. The next hosted rerun is recorded
below.
Package run [34130743953](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34130743953)
succeeded against the exact clean builder and qualification source
`df747b6d4f8d9496cb00f1fc5f0b53285d4b5edf`, completing at
`2026-09-07T14:20:03.2244053Z` on Windows Server 2025 `10.0.26100` with
WebView2 `151.0.4129.101`. ProductVersion was exactly `3.0.0`; installed-release
creation, synthetic project/chapter creation, English text save/reopen, normal
close, in-place uninstall without delete-data, and identical-version reinstall
with project/document/text retention passed with `errors=[]` and
`forcedProcessStop=false`. Metadata/results were read and
`.local/ci-package-34130743953/run-20260907-141852-540/result.json` plus
`.local/ci-package-34130743953/build-metadata.json` were verified;
`.local/builds/3.0.0-workshop-df747b6/WebnovelStudio V3_3.0.0_x64-setup.exe`
is 269,716,613 bytes with SHA-256
`8ea09a47e8e4a5408a46cf1645b60330e3314234e3256f5baa6dd1d3d97a85ef`, matching
metadata/results, and `same-version-reinstall.png` was visually inspected. This
qualifies the installed lifecycle only; offline/no-runtime installation, true
upgrade, live-provider behavior, author data, and full Workshop-native/quality
qualification remain open. The next hosted run [CI 34132271096](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34132271096)
on exact source `9ad3b0b4042ac0c33c2059a3a6bdb26247e53af7` completed as a harness
failure after 10 of 15 intended Workshop groups. It had no page errors on
WebView2 `151.0.4129.101`; Ubuntu and Windows checks/build passed, while all
later native suites were skipped. After reopen, the harness used the Overview
brief label, but the People lens labels the same field “What you want to
explore” (`Workshop.tsx:278`), so line 346 waited for a textbox name that was
absent. This is a harness failure; it does not establish a broad native pass.
The harness uses the correct role and asserts the People heading in commit
`47a50d9f230fe06c3f46eba700fbac57b03bff11`; the headless DOM reproduction now
passes collapsed-summary expansion and seed reading through the correct role,
with the old role absent. Artifacts are under
`.local/ci-workshop-34132271096/workshop`; the reproduction is
`apps/desktop/node_modules/.cache/workshop-qa/reopen-brief.mjs`.

Latest hosted bounded-native checkpoint [CI 34133645198](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34133645198)
on exact source `47a50d9f230fe06c3f46eba700fbac57b03bff11` finished at
14:46:57 UTC on 7 September. Real Tauri 3.0.0/WebView2 `151.0.4129.101`
checks were clean: Workshop 15/15 in about 14 seconds, main native 52, HTTP 6,
close 2, interruption 4, recovery 4, and memory lookup 3. The Workshop report
covers blank Develop/zero chapters, seed save, zero-generation World navigation,
one explicit mock request with three directions, detail tray and local working
edit, zero-write adoption preview, world adoption with an authorRoom decision,
the Develop-to-Write barrier, directional relationships with both source heads, history and
full Library reopen, linked atomic existing/new Unicode character relationship
adoption with exact heads and impact provenance, Unicode title/body reopen, and
a second explicit STYLE voice-guidance request whose sample stayed unchanged until
Develop with no automatic documents, decisions, or adoption. Errors, page errors,
and runtime observations were clean. This is bounded native evidence, not full
specification or quality completion: physical keyboard/accessibility, broader
late/stale/failure UI, live-provider behavior, and the author study remain open.
The earlier harness failures are retained as historical evidence above.
Artifacts are under `.local/ci-workshop-34133645198`; the suite reports and
multi-target/history/voice screenshots were inspected.
The [formative study protocol](STORY_WORKSHOP_AUTHOR_STUDY.md) is prepared; no
observed human study is claimed.

### V3.0.0 workspace preparation — 7 September

Development-speed optimization and workspace preparation for the private
V3.0.0 candidate are complete. Rust packages inherit workspace version `3.0.0`; npm metadata and
locks agree, and Tauri derives the installer version from Cargo. The stable
application identifier and author-data paths are unchanged. Six focused
version-guard checks cover agreement, inheritance, lock drift and line endings.

The candidate's local full check passed all 721 Rust and 434 frontend tests,
the six version checks, formatting, strict Clippy, TypeScript and production
build in 85.64 seconds including recompilation in the normal root `target/`.
Evidence: `.local/release-3.0.0-check.{log,json}`. Final hosted standard CI and
the synthetic installed-package lifecycle pass. See [release preparation](RELEASE_3_0_0.md)
and [changelog](../CHANGELOG.md); neither announces a public release.

The warm candidate check then passed in **43.98 seconds** using the normal
root `target/` cache (`.local/release-3.0.0-warm-check.{log,json}`). It includes
the version guard and the complete Rust/frontend matrix.

The follow-up release-tooling full check passed in **39.41 seconds** with
721 Rust tests, 434 frontend tests and 11 Node tooling checks. Evidence:
`.local/release-tooling-check.{log,json}`. The package harness now follows the
current Library/chapter controls, verifies the installed product version,
retains installers before lifecycle checks, and supports a separately validated
harness-only retest without rebuilding application code.

Candidate `729d6bd` passed Ubuntu and all Windows test/native steps in
[CI 34066304241](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34066304241),
but the overall run was cancelled after exceeding its 25-minute job limit during
cache saving. It is not a green CI result. The cache finished uploading during
cleanup: 657,680,598 bytes with limited debug information versus 1,067,977,623
bytes at the earlier full-debug checkpoint, about 38% smaller. The next run
needed to confirm warm behavior; the job now allows 35 minutes for cold setup and
cache work without changing individual harness limits. Its native evidence is
retained under `.local/ci-34066304241`.

The final standard CI qualification for source
`63770b9122f598b3e32ea1b0f5f4020c4325115f` is [run
34067931031](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067931031).
Both jobs passed: Ubuntu covers core/frontend checks and Windows covers the
full 721-test Rust workspace; each runs 434 frontend tests and 11 tooling checks.
The Windows job also passed all 52 main native checks plus the
HTTP, normal-close, strict-interruption, recovered-project, and reviewed-memory
lookup flows. Logs are `.local/release-ci-qualified.log` and
`.local/release-ci-qualified-jobs.json`.

The limited-debug-information cache is now adopted for standard CI:
657,680,598 restored bytes versus 1,067,977,623 at the earlier full-debug
checkpoint, about 38% smaller. The qualified run recorded a 38-second cache
restore, 71-second Clippy step, and 289-second Rust test step; the post-cache
step was already up to date, so no archive was created. Local debugging and
release profiles remain unchanged.

The 3.0.0 installer from that same source passed [package run
34067936098](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34067936098):
synthetic project/chapter creation, writing, save/reopen, normal close, and
same-version uninstall/reinstall with retained text, no errors and no forced
stop. [Retest 34068729080](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34068729080)
passed with that exact installer and the later documentation-only checkout
`ded8f3d`, recording the build and qualification sources separately. Retesting
took 1 minute 59 seconds versus the fresh package job's 16 minutes 20 seconds.
The preceding docs-only push also started no redundant CI run. Broader release,
live-provider and author-trial gates remain open; the primary Create with AI
redesign is still deferred.

Workspace cleanup reclaimed **73.94 GiB** of obsolete project build products
across 23 scratch Cargo trees. Automatic review blocked bulk directory deletion;
the narrower `cargo clean --workspace` route succeeded and retained dependency
caches, the normal root build cache, delivered builds and qualification reports.
The stale root Vite cache and old root log were also removed. Before/after
inventories are `.local/workspace-cleanup-inventory.json` and
`.local/workspace-cleanup-after.json`. Thirteen obsolete GitHub cache entries
were removed, reclaiming **5.37 GiB** while retaining current caches and fallbacks.

### Test-runtime checkpoint — 7 September

The core integration files now compile as modules in one standard Cargo test
target. All existing test bodies remain intact; ten suites import the shared
legacy-schema helper once from the harness. A new registration test rejects
unregistered top-level suite files. Cargo still runs the unit, binary and
documentation tests, including the existing Windows process fixtures.

Four test threads are the default, with environment/CLI overrides available.
This reduces contention between durable SQLite writers without changing WAL,
FULL synchronization, migration, recovery, or timeout behavior. CI retains
Ubuntu core/frontend checks and Windows workspace/frontend/native checks while
removing the redundant Windows core job.

The first grouped workspace run passed **721 Rust tests** in **38.83 seconds**
including compilation, against the **92.58-second**, 720-test baseline. The
extra test is the registration guard. The complete wrapper then passed all
721 Rust tests and **434 frontend tests**, formatting, strict workspace Clippy,
TypeScript and production build in **52.04 seconds**. The guard's negative
qualification refused a temporary omitted file, which was removed afterward.
Evidence: `.local/test-performance-local-baseline.{log,json}`,
`.local/test-performance-grouped-workspace.{log,json}`,
`.local/test-performance-registration-refusal.log` and
`.local/test-performance-full-check-final.{log,json}`.

The final warm workspace run passed all 721 Rust tests in **31.01 seconds**,
about **66.5% less wall time** than the warm baseline. Both had under half a second of
compilation. Evidence: `.local/test-performance-grouped-warm.{log,json}`.
Independent name-level comparison confirmed all 587 original integration
tests across 63 files, with the 62 core unit tests, 71 desktop tests and one
intentionally ignored unit fixture unchanged. Only the registration guard was
added; no production application code changed.

The hosted Rust test steps also passed on the optimized commit `db3df283`.
Compared with the preceding implementation run `6efb184d`:

| CI test step | Previous run | Optimized run |
| --- | ---: | ---: |
| Ubuntu core | 160 seconds | 96 seconds |
| Windows workspace | 566 seconds | 300 seconds |

These are single observed CI runs including compilation, with runner and cache
variation; the local warm comparison above isolates the development loop more
closely. Removing the duplicate Windows core job additionally reduces runner
work, but its duration is not an equivalent reduction in workflow wall time.

[CI 34064355153](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34064355153)
passed both jobs on `db3df283`: Ubuntu core/frontend and Windows workspace,
frontend, all 52 main native checks, HTTP, normal close, strict interruption,
recovered-project and reviewed-memory lookup qualification. Cache-save overhead
remains a separate CI optimization target.

Core/SQLite optimization flags and frontend pool changes were measured but not
adopted. No paid model request was made. This test-only change does not alter
the delivered development executable. See [development checks](TESTING.md) for
focused commands, suite registration and the retained full-check boundary.

### Current reviewed-memory lookup checkpoint — 6 September

Schema 34 raises the project reader floor for the optional reviewed-memory
capability. Fresh Working/AuthorRoom/Discuss lookup packets carry
`reviewed-memory.v1`; child packets retain the exact root capability, while
legacy packets with the field absent preserve their original instruction and
packet bytes through reconstruction, backup and recovery. The implementation
reuses the immutable lookup records and adds no tables or provider framework.

All four typed operations are implemented: `findEntities`,
`knowledgeHistory`, `promiseHistory`, and `possessionHistory`. Rust resolves
them from the authenticated frozen reviewed sets, validates complete typed
results before persistence, and refuses memory reads without the capability.
The focused storage suite covers fresh execution, child retention, legacy
search/read replay through backup/recovery, durable memory refusal, and
rehashed receipt rejection.

The synthetic native mock route exercised all four operations across three
invocations and reopened without hidden lookup work. The explicit bounded live
qualification also passed its three synthetic groups with three live calls,
exact-source inspection, unchanged prose, settled cleanup and no page errors,
using the Luna/xhigh/priority route. Evidence is retained in
`.local/native-results/memory-lookup-mock/qualification.json` and
`.local/native-results/memory-lookup-live/qualification.json`; the live slice
brings the cumulative confirmed live-call count to 24. The native build
used for this slice has SHA-256
`ccec4c7b0e630a9abf17786d872d70decd08074990a66e70bc38b39e6a1a3167`. This
remains bounded development evidence. The pre-review full check passed 718
Rust tests and 434 frontend tests in 35 files, and the native regression passed
51/52 checks with zero page errors, omitting only the local OS clipboard case.
Logs are `.local/memory-lookup-final-check.log` and
`.local/memory-lookup-native-regression/report.json`.

Two subsequent connection-recovery fixes preserve loaded settings after a
bounded join timeout and publish Codex readiness only after the discovered
catalog is durably saved. The rebuilt native app passed the three-group offline
memory lookup trial with zero live calls and page errors; its real read-only
Codex check reported ready, checked and no longer checking. All 434 frontend
tests passed again after the timeout repair. Evidence is in
`.local/memory-lookup-delivery-native.log`,
`.local/memory-lookup-delivery-frontend.log` and the current offline native report.
The final 71-test desktop suite, including both new publication regressions,
passes alongside the unchanged 649-test core baseline: 720 active Rust tests
in total, with one existing ignored fixture. Formatting, strict Clippy and
the native build also pass. The desktop log is
`.local/memory-lookup-delivery-desktop-tests.log`.
The delivered 44,623,360-byte executable was built at
`2026-09-06T21:56:34Z`, SHA-256
`a4ee6c4822e32a47dbb38b9def02595c3cb215272939d7631140db0ed11b5b57`:
`.local/builds/2026-09-06-reviewed-memory-lookups/webnovel-desktop.exe`.
The live three-call evidence above used the earlier stated hash; no additional
paid call was needed for this connection-recovery retest.

Broader live-provider, author-trial and release qualification remain open, and
this slice does not establish narrative understanding or exhaustive continuity.

The implementation was pushed as `6efb184dd20e147fc02503222d881cb2d380ca7c`.
[CI 34062589124](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34062589124)
passed both Windows and Ubuntu contract jobs and the complete native job,
including editor, HTTP, close, interruption, recovery and memory-lookup flows.
This is the pre-optimization implementation baseline; the reorganized test
workflow is independently verified by CI 34064355153 above.

The author requested test-runtime optimization before further feature work on
7 September (local time). Measure and shorten the development checks while
preserving the complete test inventory and native safety gates.

Deferred author-UX slice: combine title and a short creative brief in a
**Create with AI** path for empty projects/tabs, then hand off to the existing
Draft/Develop composer. Keep **Start blank** available. This remains planned:
the current UI still separates project creation, document creation and the
assistant brief. Reuse the existing proposal/Preview/Apply contracts and require
explicit Send; creation itself must never start a model call.

### Historical character knowledge checkpoint — 6 September

Schema 33 adds optional passage-backed character knowledge to chapter review.
The author records a character, topic, attitude, statement, timing and exact
quotation, then explicitly accepts the complete review. Stable identities can
be reused across chapters; existing possession holders also appear as character
choices. Set/inherit/Clear, saved-stage resumption, stale-source refusal,
immutable history and recovered-copy authority follow the existing review
contract. Knowledge observations never become independent world truth or
automatic claims that a character is unaware. See
[ADR 0031](ADR_0031_CHARACTER_KNOWLEDGE.md).

Fresh working discussions and reviewed continuations retain authenticated
knowledge sets. The compiler validates complete evidence before budgeting;
restricted packets and inspector views include only reader-disclosed records
from the eligible earlier story. The inspector distinguishes supplied from
available observations and opens exact-source knowledge history. Foreign
namespaces, duplicate sets and rehashed interpretations that disagree with the
immutable review bundle are rejected. Fictional earlier timing never exposes
future learning to an earlier scene.

The complete local wrapper passed **700 Rust tests** and **426 frontend tests
in 35 files**, with formatting, strict workspace Clippy, TypeScript and the
production frontend build. Two added recovery/rollback cases subsequently
passed in the final 9-case storage suite; the 8-case context suite and strict
workspace Clippy also pass. Current coverage is **702 active Rust tests**
(633 core and 69 desktop; one existing ignored child fixture). The native
debug build passes. Logs: `.local/knowledge-full-check-final.log`,
`.local/knowledge-focused-final.log`, `.local/knowledge-final-clippy.log` and
`.local/knowledge-native-build.log`.

The native WebView2 `152.0.4191.66` diagnostic passed **51 of 52 checks** with
zero page errors at `2026-09-06T20:47:57Z`. Both new knowledge journeys passed:
exact quotations, identity reuse, immutable acceptance, author-room history and
source opening without another request, and private-record exclusion from
restricted continuation and inspector display. The three new screenshots were
visually inspected. Only the established local OS clipboard case was omitted;
the tracked harness retains that strict check. Report:
`.local/knowledge-native-regression/report.json`.

The interruption diagnostic also passed all **three durable-boundary groups**
at `2026-09-06T20:51:42.803Z` with Node 24.12.0 as its test driver, including Save renderer loss, Apply process loss
and running anonymous HTTP request process loss. The six native refresh-key
checks remain omitted only from this local diagnostic and enabled in CI.
Report: `.local/knowledge-interruption-diagnostic/qualification.json`.
These checks use the 43,885,568-byte development executable built at
`2026-09-06T20:40:38Z`, SHA-256
`2b40246465eba384fb01a15fed130ad1c719d68aa61f0aa1e3fd56b44727e313`.
Delivered copy: `.local/builds/2026-09-06-character-knowledge/webnovel-desktop.exe`.
Installed-release and strict hosted qualification remain separate.

The prior [CI 34057097573](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34057097573)
passed both contract jobs and the main native/HTTP/close flows, then failed
before interruption qualification because a Windows short temp-path alias
differed from Rust's canonical path. The fixture now resolves both paths
before its existing containment assertion. This does not relax containment.

At this earlier character-knowledge checkpoint, no new live model request was
made; its cumulative live CLI dispatch count was 21.
F2/C5 remain partial: relationship/rule views, generated knowledge extraction,
richer knowledge transitions, narrative evaluation and broader
native/provider/release and author-trial gates remain open. This checkpoint is
historical; the current reviewed-memory slice is recorded at the top of this
document.

### W6 refresh protection and interruption checkpoint — 6 September

A native OS Ctrl+R test reproduced loss of unsaved text in the preceding
accepted-summary executable while its next save was held before Rust dispatch.
The renderer restarted and the synthetic database still held its original
version-zero body. Windows now consumes browser refresh accelerators in the
WebView2 controller and removes only the native menu's invariant `reload` item.
Other edit, zoom, and navigation shortcuts remain enabled. Registration failures
stop startup; a menu-enumeration failure hides that menu. The policy follows
[WebView2 browser features](https://github.com/MicrosoftDocs/edge-developer/blob/main/microsoft-edge/webview2/concepts/browser-features.md)
and [selective native menu customization](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/context-menus).
No schema changes are needed.

Current local validation totals **685 active Rust tests** (616 core and 69
desktop; one existing ignored child fixture) and **411 frontend tests in 32
files**. The full wrapper passed before the four native policy tests were added;
all 69 desktop tests, formatting, strict workspace Clippy, TypeScript and the
native build passed afterward. Logs: `.local/interruption-workspace-check.log`,
`.local/reload-desktop-tests.log`, and `.local/reload-native-build.log`.
The context-preparation process-loss test terminates a child after commit but
before acknowledgment, fences a new renderer session, and proves exact local
retry returns the original packet row byte-for-byte. Failed insertion can be
retried after rollback; changed payload reuse is rejected. The crash hook is
compiled only into Rust tests.

The new native interruption fixture passes its three durable-boundary groups:
a lost Save acknowledgment followed by renderer reload and newer writing; a
lost Apply acknowledgment followed by process termination/reopen; and a running
anonymous HTTP reply across renderer replacement and native process loss. It
checks retired writer leases, old receipt versus latest head, one Apply
receipt/decision, both immutable revisions, exact frozen context, interrupted
history, and no automatic model resend. The final three-group diagnostic after
fixture cleanup passes at `.local/reload-interruption-diagnostic/qualification.json`.
Its six-key refresh group retained the
renderer in local attempts, including Ctrl+R, but extra KeyS/KeyD/KeyW events
entered the focused fixture during two runs. The strict equality assertions
correctly failed; these attempts are not a six-key pass. Hosted CI retains all
six keys without a local omission. Native menu inspection remains unqualified:
PID-targeted WM_RBUTTON input did not expose a popup through UI Automation.
The initial CDP attempt found only the persistent System menu; the fixture now
excludes that false positive. The separate experimental `test:native-menus`
command retains strict assertions and a failed report at
`.local/reload-menu-qualification/context-menu.json`; it is not a CI pass claim.

The independent backup/recovery fixture passes **four checks**, one synthetic
HTTP request, and zero page errors. It creates a source revision, backs up A
and recovers it through PID-owned native dialogs while B owns a running reply,
then verifies independent recovered identity, exact retained prose/history,
and unchanged B source/packet/job ownership. Stop records one outcome against
B's original packet. Report:
`.local/native-results/project-recovery/qualification.json`.
The broader main native diagnostic passes **49 of 50 checks**, with zero page
errors; only the known local OS clipboard case is omitted. Report:
`.local/reload-native-regression/report.json`, dated
`2026-09-06T19:57:55.946Z`.

This evidence uses WebView2 `152.0.4191.66` and the development executable
SHA-256 `bbe6d2f2a053581ecfd9d176e387b7f9a95773351870d5ee202d2965f5f0db5d`,
43,318,784 bytes, built `2026-09-06T19:46:42Z`. The new independent CI commands
are `test:native-interruption` and `test:native-recovery`. Native registration
and fixture assertions received separate source review. The pre-fix loss
report is `.local/reload-before-fix-qualification.json`; local shortcut input
interference is retained in `.local/reload-shortcuts-extra-input.json`.

These checks do not establish a physical renderer crash, native process loss
at every transaction statement, actual disk-full/ACL behavior, installed-release
safety, or author understandability. W6/B/E4 and F5 remain open. No new live
model request was made; the cumulative live CLI count remains 21. Full V3 is
still in progress.

### Accepted narrative summary checkpoint — 6 September

Schema 32 adds optional immutable narrative summaries to the existing staged
chapter review and selected ready bundle. The author can enter a summary or
explicitly copy a current generated chapter digest as editable starting text,
then save/resume and accept the review. Summary text, audience, exact chapter
revision, and earlier reviewed prefix remain bound together. Set/Clear and
strict same-basis inheritance preserve explicit author decisions. Summary-only
reviews participate in frozen context; full prose and layered accepted-summary
coverage remain distinct from generated navigation digests. See
[ADR 0030](ADR_0030_ACCEPTED_SUMMARIES.md).

Current integrated Rust validation passes formatting, strict workspace Clippy,
and **680 active tests** (615 core and 65 desktop; one existing ignored fixture).
The new storage suite passes four cases and the context suite eight, including
semantic binding tampering after typed canonical rehashing. The inspector suite
passes 31 tests. After correcting a selector in a new regression, the final
complete frontend suite passes **411 tests in 32 files**, and TypeScript and the
native debug build pass. Review fixes preserve staged Set/Clear decisions and
complete detail sets across restaging, validate summary fingerprints, and fence
late memory copies across edits/session changes. Logs: `.local/accepted-summary-workspace-check.log`,
`.local/accepted-summary-frontend-final.log`, and
`.local/accepted-summary-native-build.log`. The tracked native harness adds two
summary journeys and now has 50 checks.

The native WebView2 `152.0.4191.66` diagnostic passes **49 of 50 checks**,
with zero page errors, at `2026-09-06T19:05:10.679Z`. It includes explicit
current-memory seeding, edited summary staging/resumption/acceptance, immutable
prose, no additional AI work during review, exact context inspection, and
clearing after changed prose while retaining the old bundle. Both new screens
were visually inspected. Only the known local OS clipboard case was omitted;
this is not a strict 50-check pass. Hosted CI retains that check. Report:
`.local/accepted-summary-native-regression/report.json`. The native development
executable is 43,271,680 bytes with SHA-256
`e05272fbae3f4e42f1e1be79159ec0a3d8f62d7857407288da2306c7f9471fd8`.
[CI 34053876032](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34053876032)
for `1e1b44f` passed both Windows/Ubuntu contract jobs and the Windows native
job, including the strict 50-check flow, HTTP fixture, and normal-close fixture.
Installed-release qualification remains separate.

No new live model call was made; the cumulative live CLI count remains 21. Full V3 and broader F2, narrative,
provider, author-trial, and installed-release gates remain open.

### Normal-close checkpoint — 6 September

Normal close now saves through the existing document lifecycle guard and checks
AI work across every open project. The author can stop active replies and
story-memory refreshes or stay open. Native generation admission prevents new
requests from starting while closing. Stop persists exact job intents and
cancels captured external workers even when a Stop write fails. Completed
results retained for a local save retry block exit. Failed saves, failed window
destruction, and cancelled or late acknowledgments keep the editor attached.
See [ADR 0029](ADR_0029_NORMAL_CLOSE.md).

The full local wrapper passes formatting, strict workspace Clippy, **668 active
Rust tests** (603 core and 65 desktop; one existing ignored fixture), TypeScript,
production frontend build, and **395 frontend tests in 31 files**. New coverage
includes five core background-work checks, six native coordinator tests, four
runtime admission/cancellation tests, and the close UI regressions. The final
native debug build succeeds. Logs are
`.local/normal-close-workspace-check.log` and
`.local/normal-close-native-build.log`.

Focused native WebView2 152.0.4191.66 qualification passes two grouped checks.
A PID-verified WM_CLOSE flushes a confirmed dirty editor and its exact text
survives a fresh process. A second fixture holds one discussion and one
Luna story-memory HTTP stream open in different projects: Stay open preserves
the editor, and Stop and close stores stopped outcomes for both requests.
Exactly two synthetic POSTs reach the anonymous loopback server, with zero
page errors or live model calls. Native pending-result fault blocking remains
separate; the core and frontend regression tests cover that boundary here.
The final hardened run completed at `2026-09-06T18:16:02Z`; its report is
`.local/native-results/app-close/qualification.json`. It explicitly checks that
Stay open leaves both SSE responses connected and that each final durable
outcome is `stopped`. The captured close dialog was visually inspected.
Native executable SHA-256:
`9f8cd3e40ea56ef2e6b2c21835ac9ba75f21630b8596d36afa26afd39f6bd3a4`.

The current broader native diagnostic passes **47 of 48 checks**, with zero
page errors, at `2026-09-06T18:12:24.963Z`. It includes the repaired recovery
journey and all following fixtures. Only the previously documented local OS
clipboard case is omitted; hosted CI retains that strict check. The report is
`.local/normal-close-native-regression/report.json`. This diagnostic does not
establish a strict 48-check pass or an installed-release qualification.

The preceding recovery checkpoint's
[CI 34049077380](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34049077380)
passes both contract jobs, but its native harness failed after the successful
recovery journey because it tried to create the next project while still in
the editor. The helper now returns to Library before continuing. The local
regression above passes this correction. The normal-close checkpoint [CI 34051224672](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34051224672) subsequently passed Windows/Ubuntu contracts and the native job for `a7e5d04`. This
correction does not turn that earlier failed run into a pass. No new live model
dispatch was made; the cumulative live CLI count remains 21. Full V3, author
acceptance, forced OS shutdown, and current installed-release gates remain open.

### Recovery-copy and context handoff checkpoint — 6 September

Save failures now offer **Save recovery copy** from the live editor. The copy
captures the current document before the destination dialog, writes a new
Markdown file independently of project storage, and leaves the saved-generation
watermark unchanged. Cancellation, failed destinations, and uncertain replies
retain the buffer and never trigger an automatic retry. Existing files are
never replaced. See [ADR 0028](ADR_0028_RECOVERY_COPY.md).

Chapter writing no longer carries discussion-only source pins across a mode
change. Private worldbuilding and character material remain available in the
author's discussion; the existing explicitly approved writing brief is the
current path for carrying selected directions into chapter prose. Automatic
world/character projections with reviewed disclosure grants remain unbuilt.

The editor trial and its sample text are compiled out of release frontend
assets. The actual Tauri debug build keeps the lazy trial chunk through
`TAURI_ENV_DEBUG`; this does not qualify a new installed package.

The full local wrapper passes formatting, strict workspace Clippy, **653 active
Rust tests** (598 core, 55 desktop; one existing ignored fixture), TypeScript,
production frontend build, and **383 frontend tests in 30 files**. This includes
five new core recovery-copy tests and five frontend recovery-copy checks.
The final source-pin follow-up passes the complete **385-test frontend suite**
and TypeScript check. A final error-message correction passes all five affected
recovery UI tests; the final native debug build also succeeds. No new live model
dispatch has been made (cumulative live CLI count remains 21).
Ignored wrapper/build logs are `.local/recovery-workspace-check.log` and
`.local/recovery-native-final-build.log`.

The focused native WebView2 152.0.4191.66 run passes **two grouped checks**:
opening the lazy editor trial and the recovery-copy journey. A real SQLite
trigger aborts saves in a temporary project. Native Save writes the exact rich
buffer as Markdown; Save and Cancel keep the durable body/hash/version
unchanged and the editor marked unsaved. Navigation stays on the faulted
document. Removing the fault and choosing Retry persists the exact buffer.
No provider/discussion records or page errors are produced. The first diagnostic
used a key-order-sensitive JSON comparison; the corrected harness compares
the document structure and passes. The owned test process was terminated after
the checks; this is not normal-close qualification.

Report: `.local/recovery-copy-qualification/report.json`, completed
`2026-09-06T17:32:22.419Z`. Native executable SHA-256:
`99cca724776f7d2662fe5d22397400c889ccb63ab5b1aff1feaf4aa75c678f66`.
The tracked native suite now includes this recovery group (**48 checks**).
The full updated native suite's CI attempt failed in harness sequencing after
this journey; the repair and current evidence are recorded above. The 47-check
hosted pass below qualifies the preceding workspace commit. Full W6, actual disk-full
and ACL qualification, author-trial, and release gates remain open.

### AI writing workspace checkpoint — 6 September, 17:00 UTC

Committed source `75c4e4716cbfdc5377bd2b2b4283b2ac18e33061` passes
[CI 34047441683](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34047441683):
both Windows/Ubuntu contract jobs and the Windows native job completed
successfully, including all **47 native checks**, six HTTP/Claude fixture
checks, and zero page errors or live calls. Downloaded hosted evidence is
`.local/ci-34047441683/` (WebView2 151.0.4129.101). This qualifies the AI workspace checkpoint below; the newer
recovery-copy follow-up has its own evidence above.

The author's revised direction is implemented as a development slice:

- Each project has Chapters, Worldbuilding, Characters, Plot & themes, and
  Notes tabs. Chapter navigation shares one editor and retains the existing
  save-before-switch barrier. Each project remembers its tab and last document.
- Draft/Continue and Develop are primary actions. Worldbuilding and character
  development use reviewable whole-document proposals and the existing atomic
  Preview/Apply path. Chapter prose retains its restricted disclosure policy.
- Native startup checks the installed Codex for an untouched library or a
  saved Codex selection. A fresh fallback adopts Luna/xhigh/Fast after a
  successful check. Explicit saved choices remain unchanged. Reasoning effort
  and service tier are directly selectable beside the model picker.
- Schema 31 adds the minimum reader fence for author-room development records.
  Schema-30 migration, backup, and reopening regressions pass. Historical
  packet bytes and receipts are not rewritten.

Evidence at this historical checkpoint: 648 active Rust tests (593 core, 55 desktop), one existing
ignored fixture, strict workspace Clippy, formatting, TypeScript, production
frontend build, and all 376 frontend tests in 29 files pass. The first wrapper
run exposed a picker focus-return regression after disabling the loading
trigger; that implementation was corrected and the entire frontend suite
rerun successfully. Rust did not change after its successful wrapper checks.

Native WebView2 152.0.4191.66 passes all 46 local regression checks, excluding
the existing OS clipboard case. A separate temporary-project native flow
passes fresh Codex detection, chapter Draft/Preview/Apply, world and character
development, chapter navigation, tab/project isolation, and renderer reopen.
The compatible-API/Claude picker fixture passes all six grouped checks, six
synthetic HTTP POSTs, zero live API calls, and credential cleanup. An independent
visual review accepted the supplied normal and 800×600 CSS-viewport captures;
physical DPI, assistive technology, installed release, and author acceptance
remain separate gates.

One real native Codex request (cumulative live CLI dispatch **21**) used
Luna/xhigh/priority, returned a 131-word chapter proposal, and changed the
manuscript only after Preview and Apply. Same-data reopen made no new request.
The database records one completed run, one provider result, one Apply decision,
3,443 confirmed stdin bytes, and settled cleanup. This is a bounded drafting
qualification, not broad narrative-quality evidence.

Accepted native executable SHA-256:
`5b17bd7e85f13d4f2348bfa83940bfc6b996b7dd05367539f17b312e0046592f`.
Ignored evidence is retained under `.local/ai-workspace-native-regression/`,
`.local/ai-workspace-qualification/`, `.local/ai-workspace-live-qualification/`,
and `.local/native-results/http/`. The initial workspace visual/mock capture
used `83c785a…`; the final binary additionally fixes picker focus return and is
the one used for the full local native regression and live draft.
See [ADR 0027](ADR_0027_AI_WRITING_WORKSPACE.md).

The native development app supports persistent projects, free-order English writing, document discussion, exact context inspection, adopted guidance, saved discussion sources, optional approved writing briefs, selected-passage and structured block suggestions, saved versions, bounded Windows Codex assistance, independent V2 schema-8 import, and exact Markdown/TXT export. Author-only chapter review stages exact saved prose and its earlier reviewed basis for explicit acceptance. Story memory provides explicit source-linked chapter digests and reuses current views in working discussions when full prose does not fit. Continuation offers explicit Working/Reviewed basis, restricted append-only proposals, editable paragraph previews, and atomic Apply/Reject. The schema-20 package adds an explicit **Author-reviewed snapshot** export basis, exact immutable review provenance, and a final freshness check after the native destination dialog. Schema 21 now adds optional passage-backed reviewed evidence with immutable record sets and audience-filtered delivery. C5 adds partial project-entity reuse, one-pass current-evidence freeze, authenticated object history, and schema-23 promise observations/history. Schema 32 adds accepted narrative summaries, schema 33 adds passage-backed character knowledge, and schema 34 adds the optional reviewed-memory lookup capability over the existing bounded read loop. Current author projects use schema 34; app-local model preferences and endpoint profiles use library schema 4.

C0–C2, parts of C3, the F2 review/context core, and C4-A/B/C development slices are implemented. The C6 bounded lookup implementation now includes frozen source-title projection and all four opt-in reviewed-memory operations, with schema-34 reader validation and focused protocol, packet, boundary, core, frontend, and synthetic native mock evidence. The schema-27 author-binding boundary is preserved; schema 28 adds the frozen source-title reader boundary, schema 29 adds memory HTTP delivery receipts, schema 30 adds Claude reported-model receipts, schema 32 adds accepted narrative summaries, schema 33 adds passage-backed character knowledge, and schema 34 adds the reviewed-memory lookup reader boundary. The wrapper counts and CI checkpoint below are historical evidence from the earlier source-title slice; they do not qualify the later schema-34 native or live-provider path. C6 remains a development slice: broader live lookup, provider, native, author-trial, and release qualification remain open. Continuation is CI-qualified as a development slice with 36 strict native checks and one bounded live result. Reviewed export passes the integrated wrapper, local native diagnostic, and strict CI with all 38 strict native checks. The schema-21 reviewed-evidence package is implemented and passes the final local native diagnostic at 39/40 checks, omitting only the known local OS clipboard case; [CI 34012813796](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34012813796) passes Windows and Ubuntu contracts and all 40 strict native checks with zero errors. Full V3 remains unfinished: higher-level C4 digests, remaining C5 state views, broader Apply, narrative evaluation, and broader provider/native/release qualification remain open.

Settings now compatibility-checks the installed Codex CLI without a fixed
version or executable hash and records the observed identity with each request.
A startup check for a fresh library or saved Codex selection, or an explicit Settings check, runs the bounded interactive `initialize`,
`initialized`, and paginated `model/list` exchange inside the owned Windows
process boundary. The sanitized catalog is display-only; the current checked
connection must authorize the exact selected model and concrete traits before
an author request is bound as `codex-stdin.author.v1`, including
`runtime.catalogSha256`. Missing models or traits remain selected and
unavailable for explicit repair. Historical 0.153.3 dispatches remain dated
  evidence only. Background summary and native Codex Story Memory use the fixed
  GPT-5.6 Luna/xhigh/priority profile; configured HTTP Story Memory uses Luna
  with xhigh reasoning and no service tier. Author writing and revision follow
  the persistent V2-style model picker. V2-style model picker behavior and
configurable OpenAI-compatible endpoint APIs have a development
implementation. The current Codex path freezes the selected model, exact
packet, application byte allowance, CLI identity, and sanitized descriptor,
runs in an owned Windows process, and saves validated output plus delivery,
  usage, cleanup, and outcome evidence. Application byte caps are not provider
  token limits. Story Memory has an independent CAS-backed provider choice for
  fixed Codex, explicit mock, or a configured HTTP endpoint. Its HTTP binding
  captures the accepted route/configuration and private credential without
  persisting the key in the job binding, requires a schema-29 delivery receipt,
  and never replays a POST during local recovery. The accepted native HTTP
  fixture passes six synthetic POSTs with zero live calls and no page errors;
  hosted/live HTTP qualification remains pending. HTTP
  context lookup and V2 CLI adapter parity remain pending; dynamic Codex
  discovery and its bounded Luna/Mini qualification are implemented as a
  development slice; broader provider and release qualification remains
  pending. See [ADR 0011](ADR_0011_LIVE_CODEX.md), [ADR 0023](ADR_0023_OPENAI_COMPATIBLE.md), [ADR 0024](ADR_0024_DYNAMIC_CODEX_MODELS.md), [ADR 0025](ADR_0025_API_STORY_MEMORY.md), and [qualification](CODEX_QUALIFICATION.md); this is development integration, not full W8 acceptance.

Claude author integration now connects the static reference catalog, explicit
Settings check, frozen author binding, contained worker, Stop, and local result
recovery. Schema 30 adds an optional provider-reported model claim separately
from the requested model. A completed response requires an exact match;
failures retain safe unexpected identities without becoming stuck in local
save retries. Claude maintenance and story lookup remain unsupported. Its
installed-CLI evidence and remaining gates are recorded in
[Claude qualification](CLAUDE_QUALIFICATION.md); no live Claude qualification
is claimed.

| Provider | Author requests | Story Memory | Current boundary |
| --- | --- | --- | --- |
| Local test model | Implemented offline | Explicit offline choice | Synthetic responses |
| Codex CLI | Checked dynamic model/traits | Fixed Luna/xhigh/priority | Bounded live evidence; broader qualification open |
| Claude Code CLI | Static Fable/Opus/Sonnet 5 with checked native connection | Unsupported | Integrated development path; no live Claude calls |
| OpenAI-compatible endpoint | Configured URL/key/model, independent of CLIs | Fixed Luna/xhigh, no tier | Native synthetic qualification; hosted endpoints unqualified |
| Cursor Agent, OpenCode, Grok Build | Deferred | Unsupported | Further adapter work paused by the author |
| Anthropic/Gemini native HTTP protocols | Deferred | Unsupported | Further adapter work paused by the author |

OpenAI, OpenRouter, and local services can use the generic endpoint route when
they implement its Chat Completions contract; this is not a claim of separate
provider-specific adapters or hosted qualification. HTTP and Claude reject
story lookup explicitly; Codex and the local test model support the current
bounded lookup route. No unavailable selection silently falls back to another
provider. All summary and maintenance model calls remain fixed to Luna/xhigh;
changing the author picker never changes the maintenance provider preference.

**Adapter scope, 6 September 2026:** the author asked to stop after Claude;
Codex is the primary use case. Keep the implemented OpenAI-compatible endpoint
route and finish Claude verification, then defer additional adapter ports.
This prioritization does not replace an author's saved provider selection.

### Current Claude author integration checkpoint

The Claude author slice implements the static picker, explicit native connection
check, exact runtime/model/effort binding, contained response worker, and
schema-30 terminal identity history. It uses the existing discussion, proposal,
Apply, Stop, and local recovery protocols. The worker independently refuses a
completed response with missing or mismatched model identity. Safe unknown
reported IDs remain inspectable on failed results; unsafe values cannot strand
a local save. An orphaned accepted operation cannot acquire a fresh connection
and silently submit the old request again.

Current local validation passes formatting, strict workspace Clippy, 644 active
Rust tests (589 core and 55 desktop), one existing ignored fixture, TypeScript,
the production build, and 357 frontend tests in 27 files. The wrapper log
`.local/claude-author-accepted-check.log` contains the passing Rust/build checks
and an initial frontend failure from one stale usage-copy assertion. After that
test assertion was corrected, the complete frontend run passes in
`.local/claude-author-frontend-final.log`; the wrapper was not rerun end to end.

The rebuilt native binary SHA-256 is
`563f6bfd7ba88c30711ebef7dd50da8423d7147fe4fee24bf1c4fd0aab7eb576`.
The native HTTP/picker fixture passes six grouped checks and six synthetic
loopback POSTs, with zero live model calls, zero page errors, and removed test
credentials. It verifies all three Claude models, five efforts, saved selection,
unchecked Send blocking, independent HTTP Luna memory, frozen route/key changes,
scoped Apply, Stop, error history, reopen, and local memory-save retry without a
POST. Evidence is `.local/native-results/http/qualification.json`, finished at
`2026-09-06T15:42:18.033Z`; the Claude picker and Settings screenshots were
inspected. The general local native diagnostic passes 46/47 checks with zero
errors at `2026-09-06T15:41:48.435Z` on WebView2 `152.0.4191.66`; it omits only
the documented local OS clipboard case. Tracked CI retains that strict check.
The fuzzy search assertion now checks the unique, first-ranked Luna result
instead of assuming that no other catalog row can match.

No live Claude calls or new authentication probes were made. Cumulative native
live CLI dispatches at that checkpoint remained 20. Hosted endpoints, live Claude behavior, the
remaining V2 adapters, and full release qualification remain open. See
[ADR 0026](ADR_0026_CLAUDE_AUTHOR.md) and
[Claude qualification](CLAUDE_QUALIFICATION.md).

The pushed implementation is `1f7c761c9a292911b9f01a6faa1f91d0425fd434`.
[CI 34043206073](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34043206073)
passes the complete Windows and Ubuntu contract jobs and the Windows native
job. The downloaded native artifact records all 47 strict checks with zero
errors at `2026-09-06T15:59:13.739Z`, on WebView2 `151.0.4129.101`.
Its HTTP/picker fixture passes all six grouped checks and six synthetic POSTs,
zero live calls, zero page errors, and removed test credentials, finishing at
`2026-09-06T15:59:29.719Z`. Evidence is under
`.local/ci-34043206073/native-spike-evidence/`; the native binary SHA-256 is
`f8d4f4e493b015183735a9b23a9523f6690c5bcaa5d8f8c0fda98378d07dd54f`.
The final documentation-only update records this evidence and the author's
Codex-first scope without changing the qualified implementation.

### Prior provider checkpoints: Codex compatibility and HTTP development surface

The dated C6 checkpoint is `c41a420c4760ef9eadf1ac54534e62d15cebae30`.
CI run [34038325733](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34038325733)
passes both Windows and Ubuntu contract jobs. Its native evidence records 47
strict checks with zero errors on WebView2 `151.0.4129.101` at
`2026-09-06T14:25:13.432Z`, and its HTTP fixture passes with zero live calls,
finishing at `2026-09-06T14:25:24.750Z`. The evidence files are
`.local/ci-34038325733/native-spike-evidence/report.json` and
`.local/ci-34038325733/http/qualification.json`; the native binary SHA-256 is
`9b4a18008ac90cc255fa8805ef52e2d71e6dae98186a6972dd502e3b905a2e83`.
The checkpoint wrapper passes 593 active Rust tests (552 core and 41 desktop),
one existing ignored fixture, and 353 frontend tests in 27 files, with formatting,
strict Clippy, TypeScript, and the production build. The local diagnostic remains
46/47 because it omits only the known local OS clipboard check. Broader provider,
HTTP-live, and release qualification remain open.

The preceding provider checkpoint is `1b0048f87961f1160955047812b93057e9c5e402`;
the preceding HTTP checkpoint is `7ce8b76f0d63e9c56d9bb8338ef8eb268079a465`.
  The earlier provider wrapper and CI evidence below remain dated records.

The independent Story Memory HTTP route has preliminary native synthetic evidence
on binary SHA-256
`e034e69a4bcf49ee7c837d12704c2bfaf67f44abd13317e08a3d923e28daa2f4`, built at
`2026-09-06T14:57:19.985Z`. The HTTP qualification record
`.local/native-results/http/qualification.json` finished at
`2026-09-06T15:02:25.842Z` after six loopback POSTs and zero live calls. It
covers Settings and exact author-choice checks, Story Memory Luna/xhigh behavior,
local-save failure recovery, reopen, and Stop. Later stop-race, unknown-usage,
cleanup, and receipt fixes were not rebuilt into this binary, so this is
preliminary evidence rather than final qualification.

The accepted current local wrapper subsequently passes 626 active Rust tests
(579 core and 47 desktop), one existing ignored fixture, and 355 frontend tests
in 27 files, with formatting, strict workspace Clippy, TypeScript, the
production build, and frontend tests; evidence is
`.local/api-memory-accepted-check.log`. The rebuilt native HTTP fixture passes
six synthetic POSTs (four discussion and two memory), zero live calls, zero page
errors, and settled credential cleanup in
`.local/native-results/http/qualification.json`, finished at
`2026-09-06T15:08:22.095Z`. Its binary SHA-256 is
`222641cba94047178941c127e5c2a12e4c7362c29f357209b2d626bca2a2893b`.
The pushed API-only checkpoint is `fe7a8b3bde93a7bacddf58dd4f6a9235a737a1d0`.
[CI 34041814151](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34041814151)
passes Windows and Ubuntu contracts, all 47 strict native checks with zero
errors, and all six synthetic HTTP POSTs with zero live calls, no page errors,
and removed test credentials. Downloaded evidence is under
`.local/ci-34041814151/native-spike-evidence/`: `report.json` is dated
`2026-09-06T15:32:17.077Z` on WebView2 `151.0.4129.101`, and
`http/qualification.json` finished at `2026-09-06T15:32:29.547Z` on binary
SHA-256 `72a25c9c41e3fcde4b72e8f230e659e2d149f8c63413368987fe21be95d1153f`.
This qualifies the API-only development slice, not hosted endpoint behavior
or the subsequent Claude integration.

CI run [34033575745](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34033575745)
passed the native strict 46-check suite and the native HTTP fixture. The overall
workflow failed Ubuntu Clippy because a Windows-only credential helper was
compiled there; that helper is fixed in the current source, and the Windows
contract jobs were canceled after the overall failure. The
previous provider wrapper passed 589 active Rust tests (548 core and
41 desktop), one existing ignored fixture, and 350 frontend tests in 27 files,
with formatting, strict Clippy, TypeScript, and the production build; this is
dated provider evidence. The
pre-parser-fix dynamic development binary (SHA-256
`775962e975c7dc5b3f0171ba2d3724212b5921295eae897fd2052af92dcf7539`) also
passes the native HTTP fixture: four local-mock POSTs, zero live calls, and no
page errors. The broad native diagnostic now passes 45/46 checks with zero
errors, omitting only the known local OS clipboard check; evidence is
`.local/native-other-results/report.json`, dated `2026-09-06T13:19:24.007Z`,
on WebView2 `152.0.4191.62`. The parser fix accepts a null
`defaultServiceTier`, handles valid non-text audio modalities while excluding
audio-only rows, and includes a sanitized 0.153.4 seven-model regression
fixture. Focused catalog, discovery, and integration checks plus strict
workspace Clippy pass. A current native build discovered seven models. The
initial Luna/xhigh/priority request completed; the first Mini/low request
failed because an inherited Luna-only `X-OpenAI-Internal-Codex-Responses-Lite`
route was applied. Dispatch 19 diagnosed that route; `model/list` does not
declare it, transport now uses it only for Luna and standard Responses for
other models, without substitution. The explicit Mini/low/no-tier follow-up on
binary SHA-256
`c9068efffda7a2ed08f81afd65f691fac411eabd43837f0a9257daac5830c533` completed
with settled cleanup and usage 1477 input, 46 output, and 13 reasoning tokens.
The current bounded Luna/Mini qualification passes; the earlier failure and
successful manuscript remain retained, with no automatic replay. The binary
also passes the native HTTP fixture with live 0. Broader provider, HTTP-live,
and release qualification remain open.

The development provider surface now includes Settings endpoint profiles,
native credential readiness, manual and cached model catalog entries, the
V2-style model rail/search/favorites/keyboard/traits, and a bounded native
OpenAI-compatible HTTP worker. The transport is covered by local mock fixtures
and is not a claim of a live provider or installed-release qualification. The
first native HTTP fixture passed four local-mock POST scenarios plus discovery,
exact body receipts, scoped Apply, key/route capture, credential cleanup, and
zero live LLM calls. The final native run after visual fixes also passes:
`.local/native-results/http/qualification.json`, dated
`2026-09-06T12:28:57.651Z`. It adds discovery cancellation with cached-model
preservation, distinct model search, successful/partial/failed history, and
confirmed removal of the synthetic credential. The four POSTs target only the
test's loopback HTTP server; no real API service was called. The full local
native regression diagnostic passes **45/46** checks with no errors at
`2026-09-06T12:30:23.049Z`, excluding only the known local OS clipboard case.
Both use native executable SHA-256
`69871c4d3e2c6004709e5c5f80cdcaa1d060fff5651d0920c171e2eb37f7a70a`, built
`2026-09-06T12:28:29.524Z`, with WebView2 `152.0.4191.62`.
The strict CI clipboard assertion remains enabled, and
`test:native-http` now runs the separate HTTP fixture in CI.

HTTP context lookup remains unsupported. V2 CLI adapter parity remains pending.
Dynamic Codex model discovery and bounded Luna/Mini qualification remain a
separate development slice; the current C6 source-title projection has a
46/47 local native pass, and its full-wrapper verification now passes.

The C6 source-title projection now resolves exact frozen handles and `SourceRef`
values for returned search/read sources without relabeling historical evidence.
Old packet bytes remain preserved. Its local native and full-wrapper verification
now pass; the live call ledger and same-data reopen evidence remain in
[Codex qualification](CODEX_QUALIFICATION.md).

Codex remains compatibility-checked against the installed executable without a
version or hash pin. Background summary and native Codex Story Memory use the
fixed Luna/xhigh/priority profile; configured HTTP Story Memory uses Luna/xhigh
with no service tier. Author-facing calls follow the selected
model and traits. The schema-27 author-binding boundary is preserved; the
schema-28 project reader floor adds frozen source-title validation. The
library schema-4 catalog preserves historical Codex packet bytes. The previous
`8edd061` hosted checkpoint ([CI 34030383334](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34030383334)) passed Ubuntu and the strict 46-check Windows native suite; a recurring Windows frontend timeout remains tracked. A CI-only two-worker Vitest cap is locally qualified without increasing test timeouts or weakening assertions. No real HTTP provider call is claimed; cumulative native dispatches now total **twenty**, including the isolated Mini diagnostic and explicit follow-up. Broader live-provider, narrative-quality, installed-release, HTTP context lookup, and hosted qualification gates remain open.

### Current C5 promise history slice

[ADR 0021](ADR_0021_PROMISE_HISTORY.md) adds optional, exact passage-backed
promise observations to schema-23 review stages and ReadyBundles. Authors can
reuse one opaque promise identity across chapters, record setup, payoff,
cancellation, or uncertainty, and inspect an authenticated history in chapter
order. Each observation retains its quotation, source revision, short note,
timing, and author-room or reader-disclosed audience. History always reports
incomplete evidence; a missing payoff is never proof of resolution or absence.

Omitted promise fields inherit the full previous set after source validation.
An explicit empty array clears the set through a new immutable review; storage
uses a nullable empty pair while operation receipts preserve the explicit
request. Possession fields and legacy hashes remain unchanged. Promise-only
review changes advance the context epoch and fence dependent later reviews
without editing their prose. Copied and recovered projects retain historical
records with independent namespaces and no copied review authority.

Working author-room discussion packets and restricted reviewed continuation
share exact promise authentication. Restricted delivery removes author-room
record identities, labels, and notes before history or packet projection.
Source prose has its own information policy: making a record private does not
hide prose already permitted by the reviewed chapter. Available evidence and
records actually delivered remain separate in the context inspector.

The complete local wrapper passed **485 active Rust tests** (460 core and 25
desktop; one existing ignored crash-child fixture), **319 frontend tests in 25
files**, formatting, strict Clippy, TypeScript, and the production frontend
build. Evidence: `.local/promises-check.log`. The local native diagnostic passes
**44/45 checks** with zero errors, omitting only the previously documented local
OS clipboard case. It covers exact lost-acknowledgment retry, cross-chapter
identity reuse, history reads, restricted continuation, record-only fencing,
clearing/reopen, and copied authority. Evidence: `.local/promises-native.log`
and `.local/native-other-results/report.json`, dated
`2026-09-06T08:00:51.377Z`, on WebView2 `152.0.4191.62`. The qualified executable SHA-256 is `b615a6c888cd5e48085967f44db582604c07807522920615c7722870f7bc48e1`, built `2026-09-06T07:57:34.418Z`. Native screenshots were
inspected.

The first hosted [CI run 34020879881](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34020879881)
at production source `12b8dd0c2ce50224de844ab8a557f912708d1bfc` passed Ubuntu
contracts and all **45 strict native checks**, including clipboard, with zero
native errors. Its Windows contract job passed Rust and the build, but one
frontend test clicked the resumed promise form before asynchronous review
verification finished. Test-only correction `904c0ae` waits for the verified
review state; all **319 frontend tests** and TypeScript passed locally afterward
(`.local/promises-frontend-final.log`). The first native report remains in
`.local/ci-34020879881/report.json`, dated `2026-09-06T08:16:16.267Z`, on
WebView2 `151.0.4129.101`.

[CI run 34021415774](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34021415774)
at `904c0ae1d439a3b004b166a3df08c06c65c9bf0c` again passed Ubuntu and all
**45 strict native checks**, with zero errors. Its native report is
`.local/ci-34021415774/report.json`, dated `2026-09-06T08:26:51.036Z`, on
WebView2 `151.0.4129.101`. Windows passed the corrected review test but hit a
five-second timeout in the existing context-inspector delivery-label test.
Investigation found no assertion/component failure; the unchanged inspector
suite and full **319-test** frontend suite passed locally. The failed job was
rerun without a source or timeout change. **Attempt 2 passed all three jobs**,
including all **319 frontend tests in 25 files** on Windows. Exact run/source
metadata and the final Windows log are retained in
`.local/ci-34021415774/run.json` and
`.local/ci-34021415774/windows-contracts-attempt2.log`. This qualifies the
schema-23 development checkpoint; the first failed attempts remain recorded
above rather than being treated as successful runs.

The same clean production source passed the installed lifecycle in
[package run 34020898065](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34020898065),
completed `2026-09-06T08:18:09.8122281Z` on Windows Server 2025 `10.0.26100`
x64 with WebView2 `151.0.4129.101`. The unsigned `0.1.0` installer SHA-256 is
`f4fb306a5d5775f0758cab0a78828b627afc2d9e40ce4b84a4237e9b9807d508`.
Release Library launch, synthetic English writing/reopening, normal close,
in-place uninstall without deleting author data, and same-version reinstall
with retained project/document/text all passed, with no errors or forced
process stop. The retained reinstall screenshot was inspected. This does not
qualify offline/no-runtime installation, a true upgrade, installed live
generation or promise editing, or full release acceptance. Exact artifacts
and the distinction from the later test/docs-only source are in
[Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

The twelfth bounded live generation delivered one reviewed promise in a
Luna/Max/Fast author-room discussion. It quoted the promise and correctly
distinguished missing payoff evidence from proof that it never happened.
Prose stayed unchanged; no proposal or decision was created, and the completed
discussion survived reopening. The response could not name its chapter because
the packet supplied source identifiers and positions without its title.
The corrected packet-v2 format now supplies frozen Author Room source names,
including a promise chapter name when its full body is omitted. Restricted
packets omit these labels. Stored v1 packets keep their exact original
serialization, selection, and receipts; both versions retain the same validation
checks and unknown versions are refused. Regression tests cover old packet
budget selection, exact bytes and hashes, restart/backup, unknown versions,
and missing or stray title fields. Original live evidence is
retained in `.local/live-promise-initial-qualification/qualification.json`.

The thirteenth generation was one fresh explicit request against the corrected
packet. It named **The key and the promise**, quoted the promise, and correctly
kept missing payoff evidence distinct from proof of non-occurrence. It
completed with 4,333 confirmed stdin bytes and reported 1,917 input tokens,
426 output tokens, and 344 reasoning tokens. Effective model settings remained
unknown. It created no proposal/decision, preserved prose, and survived
reopening without another call. Evidence:
`.local/live-promise-named-qualification/qualification.json` and
`.local/promises-live-named.log`. The cumulative total at that checkpoint was thirteen live generations.
These are bounded integration/evidence-answer results, not a narrative-quality
or general provider qualification.

### Prior structured suggestions slice

ADR 0020 adds explicit **Selected paragraphs** and **Whole chapter** scopes with
typed rich blocks, editor-owned fresh IDs, a single editable rich preview, and
the existing atomic Apply/Reject protocol. Rust and JavaScript both validate the
complete prepared snapshot; surrounding blocks, marks, scene breaks, hard
breaks, and IDs remain protected. Whole-document endpoints are null in both
wire representations, and changing the prepared scope revokes brief approval.
Schema 22 preserves legacy schema-21 payloads, receipts, and decisions while
using unique `(run_id, ordinal)` identities; that checkpoint required reader 22. The current schema-23 promise slice raises the reader floor to 23. The
preview acknowledgment helper verifies the exact proposal, version, body hash,
and typed payload without changing the wire protocol. Preview, preparation,
Apply/Reject, and inspection make no model call; an explicit suggestion request
may call the selected provider. Applying a suggestion does not establish
narrative truth.

The local wrapper passed **467 active Rust tests** (442 core and 25 desktop,
one existing ignored fixture), **301 frontend tests in 24 files**, formatting,
Clippy with `-D warnings`, TypeScript, and Vite. Evidence is
`.local/structured-check.log`. The focused native helper passed both structured
journeys with no page errors. The full local WebView2 diagnostic passed **42 of
43 checks** with zero errors, omitting only the known local OS clipboard case;
evidence is `.local/structured-native.log` and
`.local/native-other-results/report.json`. The executable SHA-256 is
`ca114dc73db5470332fd7b797c56380eb5244e303fa3595ca78c782eac685b15`,
30,628,864 bytes, built `2026-09-06T06:40:00.255Z`. The bundle is 718.66 KB
JavaScript / 37.16 KB CSS with the existing Vite chunk warning. The bounded
live structured request also passed with one retained proposal, version,
decision, and provider result; its evidence is
`.local/live-structured-qualification/qualification.json` and
`.local/structured-live.log`. It used the same requested Luna/Max/priority
profile, with effective provider identity unreported, and brought the total
live generations to eleven. [CI 34017484597](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017484597) passes Windows and Ubuntu contract jobs and all 43 strict native checks, including clipboard, with zero errors. Source is `c92df7708a4a48b83c11f25866c1f0a39d4a94b7`; the downloaded report is `.local/ci-34017484597/report.json`, dated `2026-09-06T07:01:18.297Z` on WebView2 `151.0.4129.101`. The two new hosted screenshots were visually inspected.

Two native failures found during qualification are retained in
`.local/structured-native-first-failure.json` and
`.local/structured-native-second-failure.json`. The fixes canonicalize scope
acknowledgments before comparison and read the local mock format from the
exact envelope scope. They do not relax structural or source validation.

The same clean source `c92df7708a4a48b83c11f25866c1f0a39d4a94b7` also passed [package run 34017567399](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017567399) on Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101`. It installed, opened the release Library without the debug trial, created and saved synthetic English prose, reopened it, closed normally, uninstalled in place, and reinstalled the same version with project/document/text retention. The result has no errors and no forced process stop. Completion was `2026-09-06T07:01:49.6493638Z`; installer SHA-256 is `5fa3af0e572c40c9321b9da15ec766d4d823fd46d770316bd285a3d4718f6947`. Downloaded evidence is `.local/ci-34017567399/build-metadata.json` and `.local/ci-34017567399/run-20260906-070045-364/result.json`; the retained reinstall screenshot was visually inspected. This is narrow installed-lifecycle evidence, not offline/no-runtime installation, an upgrade, installed live generation/structured editing, or full release acceptance.

### Prior C5-A evidence history slice

The prior C5-A slice adds one-pass batching of current selected review sets during Working author-room freezes and Restricted reviewed continuation, explicit project-wide entity reuse with first-chapter context, and authenticated object history over a frozen context. It preserves chapter and within-chapter record order, filters restricted records before labels or results, retains unknown holders and incomplete observations, and exposes no inferred current owner. It does not add a schema or paid model call. The local native diagnostic passes 40/41 checks with zero errors, omitting only the known local OS clipboard case. Its retained local log is `.local/evidence-history-native.log`; the executable SHA-256 is `7d59f072b578b2574734ddcc51eae4f28d037215c5439cf7a3c713dd9d8e662c`, 30,142,976 bytes, built `2026-09-06T05:38:17.9965544Z`. [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823) passes both contract jobs and all 41 strict native checks with zero errors.

The complete local wrapper passed: **459 active Rust tests** (435 core and 24 desktop, one existing ignored fixture), **275 frontend tests in 22 files**, formatting, Clippy, TypeScript, and the production frontend build. Evidence: `.local/evidence-history-check.log`. The native build is recorded in `.local/evidence-history-native-build.log`; the bundle is 706.26 KB JavaScript / 35.37 KB CSS, with the existing Vite chunk warning. No additional live generation was run; the total remains ten.

Two final release samples of Working/AuthorRoom freeze returned all expected sources and records at every size. At 50, 100, and 200 synthetic chapters, freeze took 7.124-7.493, 15.213-15.304, and 43.027-44.063 ms, respectively; the prior implementation took about 61, 305, and 1,742 ms. This measures freeze only. Separate authenticated history measurements and the remaining scaling issue are described below. Full evidence and environment details are retained in `.local/reviewed-evidence-freeze-benchmark/result-v3.json`.

The source checkpoint `284625b6576540939f3dabb065f1e9320d0bb01e` passed [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823): Windows and Ubuntu contracts, and all 41 Windows native checks with zero errors, including the clipboard and cross-chapter history journeys. Its retained report is `.local/ci-34014694823/report.json`, dated `2026-09-06T05:56:35.15Z`, on WebView2 `151.0.4129.101`.

### Prior reviewed-evidence checkpoint

[ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md) extends the immutable F2 review boundary with optional passage-backed possession records. The author can select one text-bearing block, capture an exact quotation, choose an opaque project-local object and holder identity (or unknown), record passage timing, and explicitly choose author-room or reader audience. An omitted record array inherits the complete prior set after revalidation; an explicit empty array clears it through a new stage and bundle. Records are never edited beneath an existing stage, silently dropped when stale, or promoted to canon by context delivery.

Rust schema-21 persistence, canonical hashing, exact UTF-16 anchor validation, replacement/inheritance/clear semantics, historical authentication, frozen context provenance, reader-only restricted projection, bounded packet accounting, omission counts, and context inspection are implemented. The prior local wrapper (`.local/reviewed-evidence-check.log`) was green with **446 active Rust tests** (422 core and 24 desktop, one existing ignored fixture) and **271 frontend tests in 22 files**. The prior native evidence is retained in `.local/reviewed-evidence-native.log`; prior hosted evidence is `.local/ci-34012813796/report.json`. Review actions call no model and do not change prose. This checkpoint used the local test model; existing Codex requests can consume evidence through the shared packet path, but no new live generation was run. Total authorized live generations remains ten.

### Current C6 bounded story lookup slice

The lookup records retain their schema-24 meaning. The current project reader
floor is schema 34: schema 28 added the frozen source-title projection that
must be validated by the reader, schema 29 adds optional memory HTTP delivery
receipts, schema 30 adds the optional Claude reported model, schema 32 adds
accepted narrative summaries, schema 33 adds passage-backed character
knowledge, and schema 34 adds the reviewed-memory lookup reader boundary.
Schema-25 runtime identity and schema-26 provider HTTP delivery receipts
remain compatibility boundaries. Library schema 4 is unchanged, and legacy
0.153.3 Max packets remain byte/hash compatible.

The implementation is a narrow opt-in route for Working, AuthorRoom, and
Discuss conversations. It uses the strict application response envelope
`story-lookup.v1`: the model may request bounded local search or exact source
reads, and Rust resolves them against one frozen project snapshot. The initial
invocation plus at most two fresh invocations are allowed. Each child packet,
read, response, and final answer is durably recorded; schema 24 raises the
reader floor for the new invocation/read records. The composer choice is off by
default, persists with the request identity, clears when switching to Suggest
edits or Continue, and is inspectable per packet. Intermediate protocol JSON
remains evidence, not chat text.

The route is application-controlled and does not claim provider-native function
calling. It does not expose restricted-writing, filesystem/session access,
automatic canon, or manuscript edits. In addition to search/read, the opt-in
reviewed-memory capability exposes only `findEntities`, `knowledgeHistory`,
`promiseHistory`, and `possessionHistory` over the authenticated frozen
reviewed sets. State/thread tools remain unavailable. Child lookup packets may
include the app-owned `story-lookup-source.v1` projection: exact frozen
handles, `SourceRef` values, and display names for only returned search/read
sources, deterministically de-duplicated. The compiler validates the complete
frozen set and AuthorRoom eligibility; projection metadata counts inside the
exact UTF-8 input budget and is never silently dropped. Initial or historical
absent capability and source sets remain absent, and old packet bytes/hashes
remain unchanged. Byte allowances are application limits, separate from model
token limits and billing. The ordinary one-packet discussion route remains
available with lookup disabled.

The current focused core evidence covers the four reviewed-memory operations,
capability refusal, child retention, legacy byte preservation through backup
and recovery, durable failed-result retention, and rehashed receipt rejection.
The synthetic native mock route exercised all four operations across three
invocations and reopened without hidden lookup work. The explicit bounded live
route passed the same three groups with three live calls, exact-source
inspection, unchanged prose, settled cleanup and no page errors; evidence is
under `.local/native-results/memory-lookup-{mock,live}/qualification.json`.
These remain development checks; hosted, author-trial and release
qualification are open.

#### Historical schema-30 source-title checkpoint

The following source-title projection and native counts are retained as
historical evidence from before schema 34. Focused protocol, packet, boundary, core, frontend, and source-title projection
checks pass in the current development tree; this source-title change made no
new LLM calls. The strict native set is now 47 checks after adding
frozen-rename/reopen coverage. The current local diagnostic passes 46/47 checks
with zero errors, omitting only the known local OS clipboard check, on WebView2
`152.0.4191.62`; evidence is `.local/lookup-titles-native-final.log` and
`.local/native-other-results/report.json`, executable SHA-256
`a66e07b155d1aedc24588e6d7b388f02bffb5e790c7d9991c5f3640206788cfe`. It
includes frozen source titles in serialized input and receipts plus unchanged
packet JSON after UI rename/reopen. The checkpoint wrapper passes 593 active Rust
tests (552 core and 41 desktop), one existing ignored fixture, and 353 frontend
tests in 27 files; focused migration checks also pass for the dynamic-author
binding case and the 13-case full-context migration set. C6 checkpoint CI
34038325733 records the current hosted contract and native evidence; broader live
lookup/provider and release qualification remain pending.

The prepared live lookup harness was run on 6 September 2026 and made **zero
model calls**. The installed CLI reported `0.153.4` while the old development
profile still required `0.153.3`, so compatibility preflight stopped before
dispatch. The preserved record is
`.local/live-lookup-preflight-01533/qualification.json`. This is a historical
provider compatibility finding, not a failed generation; the current cumulative
native dispatch total at that preflight remained twenty; the later bounded
reviewed-memory qualification brings the confirmed total to 24. The provider policy is now to discover
and compatibility-check the installed CLI without pinning a version or
executable hash, then record the observed identity per request.
Existing one-invocation requests and historical provider-result bytes remain
separate from the new invocation/read records. Crashes and lost
acknowledgments must remain explicit unknown or retained outcomes and must not
automatically replay a model call.

Broader work remains: C5 relationship/rule views, richer knowledge transitions,
generated extraction, and multi-resolution digest views; restricted-writing
lookup and state/thread tools; arbitrary partial multi-block editing, batch
Apply, and manual rebinding; broader providers; native author trials; and
narrative-quality and release qualification. Generated chapter memory remains
an unreviewed navigation aid, and the full V3 goal remains open.

The exact bundle/revision cache follow-up is measured in
`.local/reviewed-evidence-freeze-benchmark/review-validation-cache-result.json`.
Across two samples, authenticated history was 5.635/5.672 ms at 50 chapters,
16.129/15.976 ms at 100, and 65.550/62.221 ms at 200, compared with the prior
42.469/42.058, 225.729/229.732, and 1,486.716/1,437.192 ms measurements.
The cache preserves bundle and revision authentication and does not establish
whole-request latency or large-book readiness. Measure full requests as richer
C5 views are introduced before choosing further validation optimizations.

### Prior author-reviewed export checkpoint

[ADR 0017](ADR_0017_REVIEWED_EXPORT.md) is implemented across core, schema 20, native IPC, and the existing export dialog. The chapter-only basis choice requires an exact current author-reviewed bundle; a reviewed first chapter is valid with an empty earlier prefix. Reviewed preparation creates no checkpoint or model call. After destination selection, the project actor rechecks the selected bundle, exact source, earlier review basis, policy, and session before installing a new file. Historical records remain verifiable after later edits or review replacement. Copies and recovered projects retain history without inheriting review authority. Existing working-preview serialization, projections, and output records retain their legacy representation.

The local WebView2 diagnostic passes **37 of 38 checks with zero errors**, omitting only the known local OS clipboard case. New visible journeys cover author review, exact Markdown/TXT preview and native Save, bound records, no reviewed checkpoint, copied-authority refusal, retained output after a forced record failure, and refusal after a background save made while the real destination dialog was open. Reopening verifies historical records and changed prose. The root inspected the actual Markdown preview and stale-refusal screenshots. Evidence: `.local/reviewed-export-native.log` and the retained hosted report `.local/ci-34010306332/report.json` (`2026-09-06T04:10:43.182Z`, WebView2 `151.0.4129.101`), executable SHA `456a0cf1c327c330c0f56e4e472630e5dfeaf2976cb969cc17f052696208492d`, 29,184,000 bytes, built `2026-09-06T03:51:09Z`. The bundle is 690.55 KB JavaScript / 33.29 KB CSS with the existing Vite chunk warning.

The first native attempt stopped because the Save helper incorrectly expected a file for the intentionally refused stale export. The helper now explicitly allows that negative test to wait for the UI refusal and verify file absence; successful Save cases still require file creation. The failed attempt is preserved at `.local/reviewed-export-first-native-failure.json`. No production change or additional live request was needed. Total authorized live generations remains ten.

The full `scripts/desktop.ps1 -Command check` wrapper passed: rustfmt, workspace Clippy with `-D warnings`, **435 active Rust tests** (411 core and 24 desktop; one existing ignored fixture), TypeScript/Vite, and **256 frontend tests in 21 files**. Evidence is `.local/reviewed-export-check.log`. The seven new export tests cover exact projection, reviewed eligibility, final staleness, replay, historical recovery, record failure, and truthful schema-19 archive migration. An older synthetic schema-18 continuation archive fixture needed the schema-20 column removed before downgrade; that test-only correction is included. [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332) passes Windows/Ubuntu contracts and all 38 strict native checks, including clipboard, with zero errors. Downloaded evidence is `.local/ci-34010306332/report.json`, dated `2026-09-06T04:10:43.182Z`, WebView2 `151.0.4129.101`. Prior continuation evidence below remains historical.

### Prior story-continuation checkpoint

The current local slice lets an author choose Working draft or Reviewed story, request one typed append-only candidate for the exact target, edit its paragraphs, and Apply or Reject it through the existing atomic operation. Schema 19 retains the selected basis, continuation kind, prepared paragraphs, operation identity, generated IDs, and exact prepared body so an uncertain acknowledgment can be reconciled without regenerating prose or IDs.

The full `scripts/desktop.ps1` check passed and is recorded in `.local/continuation-check.log`: 428 active Rust tests (404 core and 24 desktop; one existing fixture remains ignored), 252 frontend tests in 21 files, rustfmt, workspace Clippy with `-D warnings`, TypeScript, and Vite. The final native build contains 688.70 KB JavaScript and 32.98 KB CSS with the existing Vite chunk warning. The rebuilt local WebView2 diagnostic passes 35 of 36 checks with zero errors; only the known local OS clipboard check is omitted. The continuation journey covers refusal of Reviewed without an eligible prefix followed by explicit Working selection, exact lost-preview-ack retry with one stored version/receipt, Apply in the same editor with preserved prefix IDs, visible Undo/Redo, and reopened history. The retained local evidence is `.local/continuation-native.log`; the hosted evidence is `.local/ci-34008911179/report.json`. The bounded live result used the earlier `cbb036fc…` executable and is recorded separately in [Codex qualification](CODEX_QUALIFICATION.md). [CI 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179) passed both contract jobs and all 36 strict native checks, including the full clipboard check, with zero errors. These development checks do not establish full provider or release qualification.

## Repository and CI evidence

- Evidence-history checkpoint `284625b6576540939f3dabb065f1e9320d0bb01e` is pushed and passed [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823): both contract jobs and all 41 strict native checks, with zero errors. Downloaded evidence is `.local/ci-34014694823/report.json`.

- Reviewed-evidence checkpoint `a8d73c36a3989e390682cf855f6eea071e58c91b` is pushed and passed [CI 34012813796](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34012813796): both contract jobs and all 40 strict native checks, with zero native errors. Downloaded evidence is `.local/ci-34012813796/report.json`.
- Reviewed-export checkpoint `a19e7bf7d30746cc02cd2769e6295779a4bcbb4d` is pushed. [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332) passed both contract jobs and all 38 strict native checks with zero errors.
- Story-continuation checkpoint `430831fc133dbe37be47fa477d7c3ea3b312e505` is pushed and passed [CI 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179): Windows contracts, Ubuntu contracts, and all **36 strict native checks**, with zero errors. Native report `2026-09-06T03:33:50.416Z`, WebView2 `151.0.4129.101`; downloaded evidence is `.local/ci-34008911179/report.json`. This includes the complete clipboard check and continuation preview retry/Apply/Undo/Redo/reopen journey.

- C4-C checkpoint `7c4599c92bdeb970ca847115553905fba999d39a` is pushed and passed [CI 34006858440](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34006858440): Windows contracts, Ubuntu contracts, and all **35 strict native checks**, with no errors. Native report `2026-09-06T02:44:27.539Z`, WebView2 `151.0.4129.101`. This qualifies chapter-only freshness, original generation provenance, schema-18 migration, and native reuse after unrelated writing while old requests remain stale. Broader V3 and release gates remain open.
- C4-B checkpoint `bd36e036eb3e8575cb856bf88972120e2bab88da` is pushed and passed [CI 34005563244](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34005563244): Windows contracts, Ubuntu contracts, and all **35 strict native checks**, with no errors. Native report `2026-09-06T02:16:39.763Z`, WebView2 `151.0.4129.101`. This includes the corrected source-confirmation wait, clipboard handling, and automatic chapter-memory reuse with immutable evidence and historical retention. It qualifies this development checkpoint; chapter-only freshness, richer memory, and broader provider/native/release gates remain separate.
- C4-B checkpoint `4c5ebbd24b31e443829591cede745556a7a6b451` is pushed. [CI 34004973069](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34004973069) passed both Windows and Ubuntu contracts. Its native job passed desktop Rust tests and compilation, then failed an existing source-confirmation assertion before reaching C4-B. The harness read a controlled chooser immediately after an inspector click, before its React adoption effect committed; the failure screenshot already showed the correct choice. The corrected harness waits for that exact choice before asserting and preserves the check that opening confirmation does not save a pin. Independent review confirmed the race. A local rerun using the same executable passed all 34 diagnostic checks at `2026-09-06T02:04:26.371Z`, with only clipboard omitted and no errors. The green `bd36e03` rerun above supersedes this failed native attempt.
- Chapter-memory checkpoint `7a271352cc95754866516735bf0438799f31d02d` is pushed and passed [CI 34003001409](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34003001409): Ubuntu contracts, Windows contracts, and all **34 strict native checks**, with no errors. Native report `2026-09-06T01:14:14.762Z`, WebView2 `151.0.4129.101`. This includes the three C4-A native flows and the full clipboard check. It qualifies this development checkpoint; installed-release, broader provider, and narrative-quality gates remain separate.
- Reviewed-context checkpoint `6618f6e9f6954e2fac5afc799c79b37ce56dcdf6` is pushed and passed [CI 33999551379](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33999551379): Ubuntu contracts, Windows contracts, and all **31 strict native checks**, with no errors. Native report `2026-09-05T23:55:38.385Z`, WebView2 `151.0.4129.101`. This qualifies the exact reviewed-source preparation checkpoint; it predates C4 chapter-memory implementation.
- Author-review checkpoint `c3f0f83d85bbb8e00caf58d46eb3787f0739197f` is pushed and passed [CI 33998398286](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33998398286): Ubuntu contracts, Windows contracts, and all **30 strict native checks**, with no errors. Native report `2026-09-05T23:27:34.812Z`, WebView2 `151.0.4191.62`. This includes author-only review, resumption, reconciliation, and the changed-earlier journey. This run predates the F2-B checkpoint above.
- Historical live-provider/import checkpoint `568959740045bfbb9202c4370f4e3d26db360c11` is pushed. [CI 33997192681](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997192681) passed the Windows-native job and all 28 strict native checks. Ubuntu Clippy failed on an unnecessary `return` in the non-Windows import-list branch; the sibling Windows contract job was cancelled by the matrix. The return was corrected locally afterward, but this partial run is not a green cross-platform result and is superseded in status by the later c3f0f83 author-review checkpoint.
- The same source passed [installed package run 33997290012](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997290012), completed `2026-09-05T23:06:52.6074002Z`, with installer SHA `78be293c04bfcd0b174f9ca2a54f9e0dbc2fba1c6cccb17c91860d3a98ee3639`. Windows Server 2025/WebView2 `151.0.4129.101` passed install, write/reopen, normal close, in-place uninstall, and same-version reinstall retaining prose. No errors or forced stop were reported; broader package gates remain open.
- Historical committed baseline `be1d93c579f50dcabf78b3151e4dd299b3c95d45` passed [CI 33994609086](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994609086): Windows/Ubuntu contracts and all 28 strict native checks, WebView2 `151.0.4129.101`, report `2026-09-05T22:07:38.437Z`, no errors. The same source passed [package run 33994616334](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994616334): install, create/write/reopen, normal close, in-place uninstall, same-version reinstall, and retained prose. Installer SHA `dec8ec5393c4453fd43a6f4ff36957f58eb32d45d08fe1f7f0068d4af02ce918`. These runs predate live-provider/import integration. Broader package gates remain open.
- The private repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3). Its default branch is `main`, whose current tip is `d0eebfd780e435c068ef1017cac580786360d36b`.
- C3 saved-source checkpoint `16bf8bca2448cbf767886c38c6cc54d9e35c3131` passed [CI run 33990073110](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990073110), including both contract platforms and all 25 strict native checks. A later run of the same app source, [33990403189](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990403189) for the package-harness-only `1e6e556`, passed both contract jobs but failed the duplicate/restart prose assertion. Its harness killed the app after clicking All projects without waiting for asynchronous navigation/flush to finish. The corrected harness waits for the Library heading before killing the process. This is a supported race diagnosis, not a claim that the failed run passed. The corrected wait and writing-brief flow subsequently passed all 26 strict native checks in run 33991463598 for `508aee194fafa44a76afcd2885d42559d9d77853`.
- C2 implementation checkpoint `cd1d8a68ef3e55733a6252f8625da6812819a639` contains the C0–C2 implementation and the context inspector/source-pin preparation. Its [CI run 33975322590](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33975322590) completed successfully across Windows native, Windows, and Ubuntu contract jobs.
- W4 checkpoint `e92eef8a212312972609057c61d6667db8d27c5f` introduced persistent discussion. W5 checkpoint `a2a01632890a44efbe84bc526674fc9bf06d3d94` is pushed; [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728) passed Ubuntu and Windows contracts plus the strict 21-check Windows-native flow. Earlier `9161fc8` and [run 33979310784](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33979310784) remain historical native19 evidence. W6 `1a6beb76456c49f1da559d6dd7325dbb6d508e44` is pushed; [run 33982596342](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33982596342) passed both contract jobs and the native build, then failed waiting for keyboard Redo.
- W7 `78a9fde847fb5d25375603d055a997d3e43ef3ab` is pushed. [Run 33986475862](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33986475862) passed Windows/Ubuntu contracts, workspace Clippy/tests, and the native build. On WebView2 `151.0.4129.101`, the native flow passed all 22 checks through history restore, including delayed caret-save acknowledgment and keyboard Redo. It failed at export because the native dialog saved to its default `Chapter draft.md`, despite the helper reading back its requested filename. The failure capture showed the successful-export presentation, but did not qualify the chosen path. The subsequent correction passed as recorded below.
- [CI run 33988660050](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33988660050) for `78367cd` passed Windows/Ubuntu contracts and the strict 24-check native flow on WebView2 `151.0.4129.101`, including clipboard, history recovery, exact chosen-path export, and focus restoration. This is the completed W7 development qualification checkpoint, separate from the newer C3 changes and installed-package lifecycle.
- Standard run [33993367813](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993367813) for `cd7fb77` passed the Windows and Ubuntu contract jobs. Its Windows-native job failed in the descendant process-cleanup regression before reaching the UI flow; the focused Windows process suite now passes 20/20 locally after the bounded cleanup fix. This is not a new native UI pass.
- Package run [33993370498](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993370498) for `cd7fb77` passed the narrow installed lifecycle: create, write, reopen, normal close, in-place uninstall, reinstall at the same version, and text retention. The source SHA was `cd7fb77f0bac8bc2ce1756f28044d406c356ff20`, installer SHA `9dcd7fc46dc8f259733d8487e11992b5c5b07d99dddd4a14f71c21baba8fd8ae`, and WebView2 was `151.0.4129.101`. Offline installation, upgrades, and broader W7 qualification remain open.
- Historical [CI run 33973213684](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973213684) for `73db1bc` passed all three jobs. [CI run 33973390433](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973390433) for `6456d33` exposed the native duplicate-edit failure; its ProseMirror transaction wait fix is included in pushed `c2a5262` and subsequently passed; the newer W6 failure is recorded above.
- The previous [CI run 33971040177](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33971040177), for `b8d4cb8`, passed both contract jobs, Windows Clippy/tests, and native build; native smoke reached the UI but timed out on a CSS-hidden diagnostic label. The committed fix waits for attachment and checks the actual runtime through IPC.
- The historical [W0 qualification record](W0_QUALIFICATION.md) remains the record for that spike. Current implementation status and remaining gates are maintained here.

## Current work

| Area | Status | Evidence or boundary | Remaining |
| --- | --- | --- | --- |
| W0 native editor | Partial baseline retained | Explicit sample editor trial in the real Tauri/WebView2 window, Rust snapshot validation over IPC, shared fixtures, session-only feedback/replacement | English native author trial, minimum-window behavior, external Word paste, screen-reader use, native backup/export dialog journey, and broader qualification |
| W1 structural scope | Implemented; P foundation covered | Shared JS/Rust snapshot/scope fixtures, exact structural validator, mutation/Unicode/size refusal tests, and local/CI contract checks | Requalify extensions when new editor nodes or grants are added; W0/release trials remain separate |
| W2 project/session/save | In progress | Core/frontend sessions, receipts/reconciliation, default persistent UI wiring, stable create-retry operation/document/block IDs, empty-project `reconcileProject`, mounted-editor lease adoption, and generic lost-acknowledgment tests are implemented | Complete file-backed persistence/reconciliation evidence and the broader author-trial integration; no full W2/V3 completion claim |
| W3 library/transfer | Implemented development surface | Registry, rename/archive/duplicate, isolated recovered projects, migration backups, persistent library/workspace, and schema-16 backup validation | Complete the A author trial and broader recovery/native-dialog evidence |
| W4 persistent discussions | Active local slice | Rust-owned threads/messages/runs/drafts, selected scope grants, frozen packet receipts, deterministic mock output, durable sequence/terminal events, queued Stop sealing, running Stop intent with Rust cleanup settlement, retained partial messages, retry/reload recovery, and native FeedbackPanel/ContextInspector integration | Finish malformed-output/cancellation/error/UI coverage, then durable Apply and broader W4 evidence |
| C2 context packet compiler | Implemented | Exact target/instruction/scope/mandatory sources; full eligible text or explicit whole-block packing; durable exact messages/options/hashes; live application byte allowance separate from Mock accounting | Qualified model token accounting, richer relevance, and broader context evaluation |
| C3 author guidance | Partial local implementation | Chat or direct entry opens an editable author-confirmed instruction; immutable exact versions support Next request, This document, and This project scopes, with CAS/idempotent mutation receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, separate guidance handles, and inspector display. Restricted writing excludes all current guidance; request use is consumed only with a successful persisted discussion start. Unchanged unsuccessful retries retain original active one-use guidance and durable composer mode. Schema-10 persistent AuthorRoom sources have exact required-source receipts, recovery retention, and uncertain-save controls. Schema 11 retains optional writing briefs with explicit approval and exact restricted-packet binding | Complete richer conversation selection and broader Apply integration; keep qualification evidence current |
| W5 proposal review and Apply | Implemented selected-passage slice; pushed and CI-covered | Schema-8 proposals, explicit intent, restricted context, immutable prepared versions/decisions, exact structural validation, atomic Apply/Reject, stale/replay fences, and mounted-editor handoff | Whole-chapter/block/manual-rebind work, broader B trial, and separately owned F5 batch Apply |
| W6 saved versions and restore | Implemented development slice | Bounded metadata paging, exact inert comparison, atomic whole-document restore, shared Apply/restore reconciliation, before/after retention, process-interruption and rollback evidence | Remaining lifecycle/renderer-loss combinations and B trial |
| W7 exports/package | Development slice and narrow installed lifecycle passed | Working and author-reviewed Markdown/TXT preview, exact native Save, immutable export records, reviewed freshness/record-failure native checks, stable release data, installed lifecycle CI33994616334 | Offline/no-runtime installation, true upgrade, physical/assistive native trials, full N gates |
| W8 bounded Codex | Integrated development path; dynamic Codex discovery, HTTP/profile, picker rails, and independent Story Memory provider choice implemented as development surfaces | Compatibility-check the installed Codex CLI at connection time, record observed version/hash per request, run bounded interactive app-server discovery, persist a sanitized display-only catalog, bind exact author model/traits with `codex-stdin.author.v1` and `runtime.catalogSha256`, preserve immutable packet/model state, Job-owned streaming, bounded output, durable provider results, Stop and explicit local save retry; Settings endpoint profiles, native credential readiness, native HTTP transport, V2-style provider search/favorites/keyboard/traits, native Codex Luna/xhigh/priority maintenance routing, and configured HTTP Story Memory Luna/xhigh/no-tier maintenance routing are implemented. Story Memory supports fixed Codex, explicit mock, or a configured HTTP endpoint with private credential capture, schema-29 delivery receipts, and no POST replay during local recovery | Complete current native/live qualification, refusal/truncation/auth/cleanup and isolation gates, model-specific token limits, V2 CLI adapter parity, hosted/live HTTP qualification, and W8/E3; HTTP context lookup remains unsupported |
| C6 bounded story lookup | Implemented development slice; broader qualification open | Opt-in `story-lookup.v1` route for Working, AuthorRoom, and Discuss; schema-24 invocation/read persistence; schema-34 reviewed-memory capability with `findEntities`, `knowledgeHistory`, `promiseHistory`, and `possessionHistory`; exact child packets; legacy byte-preserving reconstruction/backup/recovery; focused protocol/packet/boundary/core/frontend checks; synthetic native mock coverage with reopen/no-hidden-work evidence; bounded three-call live qualification | Final delivery-label qualification; hosted/live-provider qualification beyond the bounded slice; model-specific token accounting; broader crash/Stop/lost-ack, state/thread, restricted-writing, author-trial, and release support |
| F1 V2 import | Implemented schema-8 development slice | Explicit working-body choices, independent staged installation, inert history, Library Check import, source-free receipt recovery, full pre-move validation, and seven-check synthetic native import journey | Broader pending-import native recovery and representative author-approved acceptance |
| F2 author review | Prose review, reviewed-context core, continuation, and schema-21 possession evidence CI-qualified as a development slice | Exact stages/revisions/earlier prefix, explicit selected bundles, immutable reader-position pins, current validity, saved-stage resumption, independent recovered history, core/IPC freeze of reviewed prefix plus current target, schema-19 Working/Reviewed append preview with atomic Apply/Reject, and the first typed passage-backed record set with audience-filtered delivery | Broader records/exceptions, provider qualification, and full F2 qualification |
| C4-A chapter memory | Implemented development slice; CI-covered | Explicit Refresh for one full current chapter; schema 16 job/result/view records; strict evidence validation; recovery/cleanup regressions; native CI and one historical bounded Codex memory request; independent Story Memory provider choice and configured HTTP development route with schema-29 delivery receipts | Keep background summary/memory jobs on GPT-5.6-Luna/xhigh; complete current native/provider and hosted/live HTTP qualification, and narrative quality remain open |
| C4-B navigation context | Implemented development slice; CI-covered | Automatic current chapter-view selection, schema 17 immutable pins, exact generated coverage, full-text preference, mandatory-source protection, stale/copy/policy fences, and native evidence inspection | C4-C extends freshness; higher-level digests, restricted/reviewed integration, provider picker/adapters, temporal state, lookup loop, and narrative quality remain open |
| C4-C chapter freshness | Implemented development slice; CI-covered | Exact closed chapter dependencies preserve reuse after unrelated edits; original generation epochs and historical bytes remain intact; schema 18 reader floor | Richer memory, Luna/xhigh routing, and narrative qualification remain open |
| GitHub/CI | Reviewed-export checkpoint passed | `a19e7bf` passed CI 34010306332 with Windows/Ubuntu contracts and all 38 strict native checks | Requalify subsequent source changes; no full V3/release completion claim |

## Current local evidence

The C4-C integrated wrapper passed rustfmt, workspace Clippy with `-D warnings`, **407 active Rust tests** (384 core and 23 desktop; one existing ignored subprocess fixture), TypeScript/Vite, and **228 frontend tests** in 20 files. The final focused targets passed 16 memory storage, eight navigation storage, ten navigation packet, and ten migration tests. Regressions cover earlier generation provenance, malformed/future epochs, late completion after unrelated writing, immutable coarse-stale rows, reopening and reusing the exact view, ordinary discussion staleness, source edits, and cross-epoch backup/recovered-copy isolation. Schema-17 upgrade preserves nonempty generated snapshots, pins, candidate JSON, request bytes, and manuscript heads, with an unchanged pre-upgrade backup. Independent storage review found no actionable defects. The bundle remains 681.42 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning. No additional live model requests were made.

The C4-C rebuilt Tauri app passed **34 of 35 native diagnostic checks** at `2026-09-06T02:34:02.347Z`, WebView2 `152.0.4191.62`, with no errors. Only the previously documented local OS clipboard check is omitted; the strict CI harness retains all 35. The extended navigation flow starts a fresh local mock discussion after an unrelated chapter edit, confirms the previous packet is stale, reuses the same generated view and exact source with its original generation epoch, and confirms one total memory job. It then edits that view's source chapter and verifies stale-view exclusion, historical retention, unchanged target prose, and reload behavior. The native mock response is awaited to completion before the source edit. Executable SHA `3bfbcb23a95d46051bb0f1903bc4d9c85b4dc3165d78eab1bb616e6c4449738e`, 28,851,712 bytes, built `2026-09-06T02:33:10Z`. The refreshed context screenshot was inspected. This is development evidence; the strict CI result is recorded above, and broader provider/native/release qualification remains separate.

### Earlier C4-B checkpoint

The C4-B final wrapper passed rustfmt, workspace Clippy with `-D warnings`, **403 active Rust tests** (380 core and 23 desktop; one existing ignored subprocess fixture), TypeScript/Vite, and **228 frontend tests** in 20 files. Eight navigation packet tests cover full-text byte/hash preservation, exact mandatory sources, whole-view budget growth, typed omissions, and dependency/quotation tampering. Eight storage tests cover immutable pins, historical retention, changed unrelated evidence, policy revocation, recovered-copy isolation, recursion, and missing schema-17 tables even with no snapshots. The nine migration tests include schema-16 byte-preserving upgrade and its durable original backup. Seventeen inspector tests include policy changes during an evidence read/search and late results after a project switch. The bundle is 681.42 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning. No additional live model requests were made.

The final rebuilt Tauri executable passed **34 of 35 native diagnostic checks** at `2026-09-06T01:49:52.355Z`, WebView2 `152.0.4191.62`, with no errors. Only the documented local OS clipboard check was omitted. The new ordinary-discussion flow uses a synthetic story larger than the input allowance, supplies one pre-existing generated view, expands its evidence quotation, opens the complete original chapter, preserves the mounted manuscript, and retains the exact old packet after a source edit and renderer reload. The next freeze excludes the stale view; the project-wide job count remains one. Executable SHA `0c5d6be8bd6cff7a33d25be6cfc8b3e15e8f4cdd407c92d67be3c3173c4044db`, 28,847,616 bytes, built `2026-09-06T01:48:47.3850312Z`. The generated-summary native capture passed independent visual review. This proves development integration, not narrative understanding, full native accessibility, or release qualification.

### Earlier chapter-memory checkpoint

The C4-A wrapper passed rustfmt, workspace Clippy with `-D warnings`, and **386 active Rust tests** (363 core and 23 desktop; one existing ignored subprocess fixture). After the final connection-recovery UI correction, TypeScript/Vite and all **220 frontend tests** passed. Focused memory coverage includes 9 pure response-contract tests, 5 packet tests, 15 storage tests, and 14 controller/panel tests. The desktop suite covers lost durable claims, retained terminal and install faults, Stop, permission revocation, and explicit registry-close/reopen retention as interrupted history. A fenced background commit exposes a local check that renews the document lease before retrying the retained result. Recovered-source UI coverage uses the original project identity and validates the exact retained source without granting operation authority. The frontend bundle is 679.07 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning.

The final rebuilt Tauri executable passed **33 of 34 native diagnostic checks** at `2026-09-06T01:01:50.070Z`, WebView2 `152.0.4191.62`, with no errors. Only the documented local OS clipboard check was omitted; the strict tracked harness retains all 34. The three memory flows prove an explicit single refresh with a lost acknowledgment, unchanged editor/prose and no autosave generation, retained stale evidence, local terminal-save retry after navigation/reload, exact source lookup, and policy revocation. Executable SHA `abde8c8a1a9be9d90a711398fc146fdbdbae7c24c06d038db8ad140653f4d9b5`, 28,552,704 bytes, built `2026-09-06T01:01:05.5466457Z`. The chapter-memory screenshot was inspected. This is native development evidence; live-provider, strict CI, and installed-release qualification remain separate.

The ninth live Codex generation produced three evidence-linked chapter-memory items and one installed view with settled cleanup. Its frozen input was 3019 bytes; reported usage was 1257 input, 1931 output, and 1552 reasoning-output tokens. A Windows path-check error stopped the initial harness after generation; a same-data continuation verified the retained result, exact source, and unchanged prose with zero further requests. This is one narrow live-memory result, not narrative-quality qualification. See [Codex qualification](CODEX_QUALIFICATION.md#ninth-dispatch-native-chapter-memory).

### Earlier reviewed-context checkpoint

The reviewed-context wrapper passed rustfmt, workspace Clippy with `-D warnings`, **349 active Rust tests** (334 core and 15 desktop; one ignored subprocess fixture), TypeScript/Vite, and **206 frontend tests**. The seven reviewed-context tests cover exact prefix/replay, restricted boundaries, tied chapter positions, optional packing, historical reader-position integrity, missing-pin corruption, and schema-14 archive recovery. Five additional adversarial tests cover manifest substitution, ancestry, namespaces, stale history, and recovered-copy refusal. A schema-14 opening regression preserves old snapshot JSON and selected author reviews. Strengthened archive assertions then passed the seven-test target plus Clippy and formatting: recovery retains the exact old snapshot JSON, adds nullable pins, upgrades the independent copy, and leaves the source archive unchanged. The frontend bundle is 658.20 KB JavaScript and 28.84 KB CSS, with the existing Vite size warning. No model was called for this work.

The final rebuilt Tauri executable passed **30 of 31 native diagnostic checks** at `2026-09-05T23:45:44.310Z`, WebView2 `152.0.4191.62`, with no errors. The only omitted check is the documented local OS clipboard case; all 31 remain in the strict CI harness. The new check uses actual IPC and SQLite to freeze an exact reviewed prefix plus working target, exclude later/private material, prepare a local packet, retain stale historical evidence, refuse a newly invalid basis, and enforce policy revocation. It makes no provider request and does not qualify a continuation UI. Executable SHA `1791cf2a1ac8d8ba774b4dcf041bea2a3f3ab16db953f59d0f2c48c6e13ed37f`, 27,228,160 bytes, built `2026-09-05T23:44:13.1373407Z`. The refreshed changed-earlier screenshot was inspected; later prose remains present and editable.

### Earlier author-review checkpoint

The author-review wrapper passed rustfmt, workspace Clippy with `-D warnings`, TypeScript/Vite, and **205 frontend tests**. After four additional review regressions, the final workspace Rust run passed **336 active tests** (321 core and 15 desktop; one ignored subprocess fixture). The eleven review core tests cover exact source and prefix validity, stale activation, policy/order changes, replay after lease rotation, independent recovery, corrupt historical references, and refusal of unimplemented reviewed context. Eleven review UI tests cover explicit acceptance, stale/uncertain outcomes, retained editing, restart resumption, exact earlier prose, and callback ownership. The bundle is 657.79 KB JavaScript and 28.84 KB CSS, with the existing Vite size warning.

The rebuilt native app passed **29 of 30 diagnostic checks** at `2026-09-05T23:19:20.734Z` on actual Tauri/WebView2 `152.0.4191.62`, with no errors. Only the known local OS clipboard check was omitted; the tracked strict harness retains all 30 checks for CI. Executable SHA `921dbed4621f88e1d1ab40ac476755d710f1c0b86c3fd125c084eb60124f649b`, 27,053,568 bytes, built `2026-09-05T23:14:17Z`. The new flow previews and resumes an unaccepted exact review after restart, reconciles a real committed Mark whose acknowledgment was dropped, reads the exact earlier revision, and preserves later prose plus its changed-earlier status across reopen. Both review screenshots were inspected. No live model was called.

The initial review diagnostic stopped after 28 checks because it filled the old editor before the new chapter mounted. Read-only inspection of the synthetic database confirmed that the old chapter had received the new text, its accepted bundle was intact, and the new chapter was empty. The corrected harness waits for the requested chapter heading and for Library navigation before reload; the success above follows that correction. No product code changed for this harness failure.

The [V2 importer](V2_IMPORT_PREVIEW.md) has Windows read-only preview/list, schema-8 ownership validation, stable owned source copies, explicit missing-prose choices, staged independent installation, inert legacy retention, and immutable operation replay. Fifteen import tests cover source deletion, Library restart, an open target with a running discussion, hash tampering, backup/recovery, exact JSON choice binding, and rejecting a corrupt staged identity before moving it. Four dialog tests cover review/choice and uncertain-result retry/close. Library **Check import** reconstructs the recorded choices and reconciles the original operation; it never opens a new source picker or invents a new import identity. Reconciliation validates storage within one read transaction and preserves current legitimate edits.

Before C4-A, eight real Codex requests had been recorded. The seventh native request completed with ordinary prose, correctly yielding no proposal and no Apply. That exposed the missing response-format instruction. The eighth request used the fixed frozen `proposal-output.v1` contract and retained one suggestion (`short promise` → `solemn vow`). Explicit native Apply preserved the protected ending. A harness selector ambiguity stopped the first run after Apply; a read-only continuation reopened the same data and verified persisted prose, requested traits, and Used context without another generation. Total dispatches for that flow remained one. Its exact executable SHA was `529e3ff58bb6a23cbfe5123ac95eed447160958455022a3a29b056b274210155`, built `2026-09-05T22:37:06.694Z`. The receipt reports 3360 stdin bytes, 1375 input tokens, 188 output tokens, 144 reasoning tokens, settled cleanup, and no echoed effective identity. No page errors or owned process remained. This qualifies that narrow native live-edit development flow, not all W8 behavior or narrative quality. See [Codex qualification](CODEX_QUALIFICATION.md) for all experiments and limitations.

### Earlier checkpoint evidence

The final native V2 import journey passed seven checks at `2026-09-05T22:52:29.674Z`: fresh Tauri library, the owned native source chooser, explicit same-chapter draft choice, readable/editable imported narrative note, independent V3 identity/namespace, byte-for-byte unchanged source, and reopened chapter prose. The executable SHA was `9c13a273210ab0fc890645f176cf8b8c6c8bd4b3cd8808f48bc47479862c6d1f` (26,400,256 bytes, built `2026-09-05T22:47:52Z`). Earlier attempts exposed only chooser automation and accessible-name selector mismatches; the corrected ignored harness passed. Review/open/reopen screenshots were inspected. This uses a synthetic schema-8 fixture and does not qualify author databases, other schemas/platforms, the installer, or every pending-import native recovery case.

At `2026-09-05T22:45:52.293Z`, the native diagnostic passed 27 of 28 checks on WebView2 `152.0.4191.62`, with no errors. It used the same `529e3ff5…` development executable as live request eight and covered the common writing, project, model-preference, context, guidance, Apply/history, and export flows. It omitted the known local OS clipboard check; the strict tracked harness retains that check for CI. This build predates the final Library import-recovery addition. The diagnostic uses only the local test model; the separate live flow is recorded above.

The earlier saved-source C3 wrapper check passed rustfmt, workspace Clippy with `-D warnings`, **230 active Rust tests** (229 core and one desktop; one ignored subprocess entry), TypeScript/Vite, and **162 frontend tests** on Windows 11 Pro `10.0.26200`. The bundle is 623.41 KB and retains Vite's size warning. Source-choice tests cover scope/CAS/replay, recovery, unavailable sources, mandatory overflow, stale packets, retry identity, restricted exclusion, uncertain acknowledgments, current-list refresh, keyboard focus, and late owners. A legacy receipt regression preserves exact packet input after adding the optional required-source annotation. The earlier release compile passed; final C3 installed-release qualification remains separate. Final receipt-validation guards passed the 12-test source-pin target, the 24-test transfer target, the 10-test context-packet target, and workspace Clippy. These add one regression after the full wrapper run, for 231 active Rust tests across the executed targets. The final embedded development build again passed the 24-check native diagnostic with clipboard omitted.

The new [Windows process contract](WINDOWS_PROCESS_CONTRACT.md) distinguishes explicit `finish_or_stop` cleanup from best-effort Drop. The incremental observer delivers only accepted bounded stream prefixes, including the normal final drain, and tests retain live child/grandchild handles, verify Job accounting reaches zero, reject incomplete zero-exit input delivery, and prove a previously requested Stop sends no packet bytes. Pending overlapped I/O retains stable owned storage; unresolved cancellation can retain that bounded allocation and detach readers rather than claim successful cleanup. The focused Windows process suite now passes 20/20 locally after the bounded descendant-cleanup fix; the standard CI failure above remains the historical pre-fix result. The pure Claude and Codex parsers reject malformed/unknown/tool events, retain validated partial assistant text, and strip upstream diagnostic details. Their usage tests cover signed-negative conversion and bounded counters. These parser/process checks alone do not qualify the later live integration or upstream isolation/billing guarantees.

The Rust Stop lifecycle now seals a queued discussion immediately, records a durable stopping intent for a running discussion, and lets the worker settle cleanup as stopped or interrupted. Retained partial output is included in the inspectable terminal message; failed claim/completion/Stop local writes remain owner-keyed app-memory state with a visible “Retry saving response” action, current-lease/reconciliation fencing, and no generation replay. This local contract is explained in [ADR 0009](ADR_0009_DISCUSSION_RECOVERY.md).

Earlier runs exposed an Ubuntu test synchronization issue, unsupported UIA focus, a native-dialog default-filename error, and return-focus loss. Their corrections are covered by the successful strict run 33988660050. Run 33988179660 passed contracts but was canceled as superseded before native completion; its cancellation is not a functional failure.

The earlier full W7 wrapper check, before the ordinary-period export correction and W8 process work, passed rustfmt, workspace Clippy with `-D warnings`, **186 active Rust tests** (185 core and one desktop; one ignored subprocess target), TypeScript/Vite, and **154 frontend tests**. That historical bundle was 615.31 KB.

W7 adds thirteen core transfer regressions, including exact projection/bytes, source tampering, frozen older revisions, no overwrite, duplicate/concurrent finalization, post-install begin/insert failure, immutable-record recovery, schema-8 migration, and Markdown punctuation/URL/indentation. The final transfer target passed all 24 tests after ordinary-period and Windows-basename fixes; the old direct TXT export IPC/core path was removed. Ten ExportDialog tests cover ownership, corrupt preview, explicit save/cancel, possible writes, existing destinations, and late results. Six background-caret tests keep typing available during delayed acknowledgments, defer stale views, fence cleanup, and make later lifecycle operations wait before flushing current writing. The release-profile Windows x64 binary also compiled successfully; the installer and native journey remain separately tracked below.

The unsigned Windows x64 NSIS package builds with the bundled offline WebView2 installer and stable release data path. Hosted package run 33990404236 built and installed `1e6e556`, opened the release Library, created synthetic prose, and read it back, then failed before returning to Library. Its owned-window capture showed an uncertain save. Source inspection identified `validate_snapshot` incorrectly guarded by `debug_assertions`, although production autosave calls it. The shared command registration is corrected. The later package run [33993370498](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993370498) for `cd7fb77` passed the narrow installed lifecycle: create, write, reopen, normal close, in-place uninstall, reinstall at the same version, and text retention. It used source SHA `cd7fb77f0bac8bc2ce1756f28044d406c356ff20`, installer SHA `9dcd7fc46dc8f259733d8487e11992b5c5b07d99dddd4a14f71c21baba8fd8ae`, and WebView2 `151.0.4129.101`. Offline installation, upgrade coverage, and broader W7 qualification remain open. See [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

W6 adds six core history tests, including cursor pagination through the final page, a six-stage rollback matrix, backup decision-link tampering, and recovered-project namespace behavior. A real process is killed after restore COMMIT and before acknowledgment; replay recovers the historical result once. Eight frontend restore tests and eight HistoryPanel tests cover the input/lifecycle barrier, exact preflight, lost acknowledgment, fenced absence, later-head conflict, failed display recovery, and late panel responses. Existing W5 proposal, Apply, scope, and history-boundary tests remain green.

The tracked C3 flow now has **26 checks**, including approved writing briefs; the final rebuilt local diagnostic passed **25** with only clipboard omitted. Checkpoint `508aee1` has a strict **26-check** hosted pass, including clipboard and writing briefs. The earlier saved-source checkpoint retains its separate strict **25-check** hosted pass. Native checks include real committed Apply/restore with a discarded acknowledgment, same-editor reconciliation, one-step undo/redo, exact export bytes, and retained history. Transport-loss injection lives only in the external harness.

The strict local native flow still stops at W0 Ctrl+V: copy serializes correctly, but paste receives empty clipboard data. This also occurred with an older binary. A separate Win32 diagnostic received access denied from `OpenClipboard(NULL)` in 20 of 20 attempts; no owner window was reported. The cause remains unresolved; no clipboard service was restarted and no clipboard contents were inspected. The successful strict W5 GitHub run is separate evidence on the hosted Windows machine. Ignored `.local/native-other-results/` explicitly records omitted clipboard coverage; `.local/native-results/` contains strict-run output. A stale failure file does not supersede a later dated success report.

The owned-dialog export helper uses `WM_NEXTDLGCTL`, `EM_SETSEL`, and `EM_REPLACESEL`, verifies exact readback, and sends no global filename keystrokes. ExportDialog closes before parent focus is restored. Both behaviors passed the strict hosted native flow; installed-package qualification is separate.

Default-size history comparison and restored-history screenshots were inspected. An additional 800×600 CSS-viewport check inside actual WebView2 shows no horizontal overflow and keeps comparison, Restore, and manuscript controls reachable. This is emulated viewport layout evidence, not physical window-resize, DPI, accessibility, or screen-reader qualification. Native dialogs, normal close, packaging, and broader author trials remain separately open.

At the earlier mock-only checkpoint, three bounded Codex CLI `0.153.3` generation dispatches are recorded in [Codex qualification](CODEX_QUALIFICATION.md): an initial success, a dedicated-home authentication failure, and a success using the existing managed login with stricter requested capability controls. No tool events were observed, and effective model/traits were not independently echoed. The empty-cwd, requested-control, host-authentication, and upstream-retry boundaries remain explicit. Descendant containment, Stop, failure, and context-budget qualification remained open at that checkpoint; the current bounded adapter and later experiments are recorded above.

## Full completion checklist

C2 review added explicit scope requirements for prose-producing purposes, mandatory evidence-dependency closure, non-sensitive policy exclusion counts in the delivered envelope, and a 10,000-block regression for bounded packing memory. The context inspector is mounted in the persistent discussion surface, and source pins are captured. C3 now persists author-confirmed guidance and binds frozen typed guidance exactly into AuthorRoom packets; its mutation receipt is distinct from packet guidance handles. Recent author-room discussion context is implemented as a bounded recency selection: up to four complete delivered exchanges and 16 KiB of exact turn records from the same current document thread and policy. Frozen snapshots and receipts retain the exact messages and scopes; the inspector shows supplied exchanges and omissions separately from guidance and story evidence. Stopped or partial output, other documents, revoked-policy turns, and copied historical threads are excluded from automatic reuse. This is recency selection, not semantic retrieval or adopted story truth. See [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md). Preparing or reading a packet does not authorize dispatch: W4 atomically claims a run against current source/policy epochs before invoking the deterministic mock. An explicit linked retry of a stopped, failed, or interrupted discussion retains its exact original request-scoped guidance when those versions are still active. Feedback, selected scope, and ordered source pins must match the original request; editing any of them starts a new request. Current document/project guidance and current permitted story sources are compiled afresh. Newly waiting request guidance is reserved for the next new request. Edited or retired inherited instructions, revoked policies, completed runs, and recovered-copy links are refused. The schema-7 composer stores the retry link with its draft and immutable save receipt, so navigation/reload preserves the choice. The original guidance-use receipt remains the only consumption record; retries do not consume it again. Persistent document/project discussion sources are implemented with schema-10 CAS receipts and mandatory packet binding; optional approved writing briefs are implemented; the bounded Codex development path is integrated with full qualification still open; the selected-passage Apply slice is implemented.

The following checklist preserves the approved work-package order and gates. A package is complete only when its implementation, failure coverage, and named evidence gate are recorded. W2 completion does not complete A, B, C, or the full V3 goal. W5 development CI is green. A checked implementation item does not by itself close a broader author-trial or release gate.

Persistent document/project source choices are implemented for AuthorRoom discussions. **Keep source…** opens an explicit confirmation form; **Include next time** remains a one-request choice. Rust merges current saved choices with transient pins into the frozen packet, retains exact mandatory-source receipts, and refuses unavailable or oversized required sources. The current target is already mandatory and is included once. Restricted edit requests exclude these saved discussion choices. Changed choices advance the source epoch, while retries preserve their original transient request identity. See [ADR 0007](ADR_0007_DISCUSSION_SOURCE_PINS.md).

### W0 — native editor spike and contract lock

**Gate:** initial N-spike evidence; E1 begins here. **Status:** partial/in progress.

- [x] Keep a real Tauri/WebView2 development window with the restricted Tiptap schema and persistent mounted editor instance.
- [x] Record the snapshot, identity, scope, canonicalization, hash, short local barrier, and session-history boundary in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md).
- [x] Keep shared JS/Rust golden fixtures and real IPC snapshot validation.
- [x] Exercise session-only feedback, selection quotation/focus, preview/reject, local strict replacement, and undo/redo in the development surface.
- [ ] Complete the English native author trial, minimum-window/DPI behavior, external Word paste, native backup/export dialog journey, and assistive-technology trial.
- [x] Record a successful remote CI rerun after the native ProseMirror transaction wait fix and close the nominated Windows configuration evidence.
- [ ] Do not claim W0, N, an installed release, or real-manuscript readiness from the current spike alone.

### W1 — canonical documents and structural scope validation

**Gate:** P foundation. **Status:** implemented; P contract evidence recorded below.

The P foundation is covered by shared snapshot and scope fixtures, independent Rust validation, and file-backed persistence tests. `crates/core/src/lib.rs::shared_snapshot_fixtures_match`, `crates/core/tests/scope.rs::shared_scope_fixtures_match`, `scope.rs`/`text_replacement.rs` mutation tests, and frontend `document.test.ts`/`scope.test.ts` establish the current restricted schema. The full check recorded above passed, and W5 CI exercised the unchanged shared contract on Windows and Ubuntu. This qualifies the implemented document contract; new editor nodes or scope kinds require new evidence, and it does not close W0 or release trials.

- [x] Implement the restricted document schema and canonicalization/hash contract as shared JS/Rust behavior.
- [x] Implement block identity rules and Unicode endpoint conversion for the supported English editor surface.
- [x] Implement the independent Rust structural token iterator and scope validator.
- [x] Prove unchanged outside-scope text, marks, links, block style/identity, and scene boundaries.
- [x] Cover surrogate-pair, combining-mark, ZWJ, repeated-occurrence, empty-block, inline-only, cross-paragraph, malformed, and oversized fixtures as internal Unicode robustness cases.
- [x] Keep Rust free of a general ProseMirror-step interpreter.
- [x] Record P evidence before treating W1 as complete.

### W2 — real project persistence and document session

**Gate:** P. **Status:** in progress.

Core and frontend session work, receipts/reconciliation, and default persistent Library/Workspace wiring are implemented. Broader file-backed qualification and author-trial integration remain open; W2 does not complete the full V3 goal.

- [x] Implement core file-backed project storage with an owned connection/session boundary.
- [x] Implement migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations.
- [x] Implement frontend `DocumentSession` generation watermarks, immutable in-flight payloads, serialized saves, lifecycle identity, and error buffers.
- [x] Prove delayed acknowledgments cannot replace newer editor text.
- [x] Prove operation/payload idempotency, changed-payload rejection, stale-version/lease rejection, definite-save error retention, and uncertain-outcome fencing/reconciliation.
- [x] Carry project/document/session identity through callbacks and reject late callbacks.
- [x] Prove process interruption after commit and before acknowledgment recovers the correct body.
- [x] Read back WAL/FULL/foreign-key configuration in file-backed tests.
- [ ] Complete and qualify the persistence integration across the UI and author trial; W2 does not complete the full V3 goal.

### W3 — library, free-order work, recovery, and A trial

**Gate:** M/P plus the development-native author trial. **Status:** active work.

- [x] Implement New/Open/Rename/Duplicate/Archive/Locate and blank note/character/chapter creation.
- [x] Persist last item/caret state, switch only after flush, and enforce project locks and captured ownership.
- [x] Restore into a new recovered project with a new identity and isolated operation namespace; keep the original untouched on failure.
- [x] Keep copied receipts historical and unable to authorize new operations.
- [x] Add the explicit UTF-8 `Export draft` action from a flushed, frozen source.
- [ ] Complete the A trial across two offline projects, restart/resume, recovery copy, and draft export.
- [x] Expose Import V2 only after F1 staged import and reconciliation exist; native qualification remains separately recorded.

### W4 — persistent conversation and deterministic jobs

**Gate:** M/P; no B trial yet. **Status:** active local slice.

Integrate the C0–C3 context foundation before or alongside this package: frozen snapshots and exact eligible sources, source epoch, deterministic exact retrieval with dirty-index fallback, mandatory-budget refusal, scoped author guidance, and actual-packet receipts/inspector. C0 and C1 are implemented in the core; C2's pure compiler, durable exact receipts, and native IPC are pushed and covered by local smoke; C3 guidance and bounded recent discussion compilation are pushed; linked retry guidance and durable composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. These context packages do not replace the base save, Apply, lifecycle, or authority contracts.

- [x] Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a deterministic mock provider.
- [x] Implement queued Stop sealing, running Stop intent, cleanup settlement, retained partial output, and inspectable terminal recovery.
- [ ] Cover delayed output, malformed structured output, partial failure, cancellation, and exact repeatable suggestions.
- [ ] Keep provider code from mutating manuscript bodies.
- [x] Preserve author-room/prose-context separation and optional author-approved brief rules for deliberately transferred directions; see [ADR 0008](ADR_0008_WRITING_BRIEF.md).
- [x] Recover discussion and job state on reload and project switching; retry creates a linked new run.

### W5 — review cards, prepared snapshots, and single author Apply

**Gate:** M/P; B trial depends on W6. **Status:** selected-passage slice implemented and CI-covered; broader package remains open.

This W5 checkpoint adds explicit `Discuss`/`ProposeEdits` intent while retaining `Discuss` as the wire default. `ProposeEdits` currently requires a selected passage in a chapter and builds a `Working`/`RestrictedWriting`/`Revise` request at the chapter reader frontier; author-room private/future material, current guidance, and recent chat are excluded. Provider terminal handling accepts only a strict 1–3 candidate `ProposalOutput`; malformed or unsupported output is retained as raw unplaced discussion text without a repair call. The deterministic mock returns three alternatives.

Candidates, prepared versions, and decisions are immutable. Prepare accepts historical records against their immutable original source and validates exact JS result text, marks, and block scope with CAS versioning; only Apply requires the current head. Apply and Reject are explicit author actions. Apply atomically records body, source epoch, before/after revisions, decision, and receipt; any source edit or current-policy change makes a pending proposal stale. Reject leaves the epoch unchanged. Duplicate and cross-operation receipt collisions are fenced; Apply acknowledgment carries the separate latest result, and replay returns the latest head without reapplying. Recovered-copy proposals remain read-only.

The parent editor session flushes and preflights behind a pending-Apply barrier, commits the durable operation before dispatching the exact existing-editor transaction with `closeHistory`, retains newer edits on conflict, and reconciles uncertain acknowledgments without autosave. Native IPC mock commands are wired. Focused W5 tests and the diagnostic native subset pass locally; strict native21 passed in W5 GitHub CI. The local clipboard issue remains recorded above. Whole-chapter/block/manual-rebind proposals and F5 batch Apply remain open; W6 now owns explicit revision restore.

- [x] Implement source-bound proposals, editable prepared versions, exact before/after preview, one-at-a-time Apply, and Reject.
- [x] Bind each proposal to its context snapshot, exact target/scope, source epoch, policy, and context receipt; F2 alone owns reviewed authority.
- [x] Use the short local mutation barrier, preflighted editor transaction, durable decision/before/after/receipt transaction, and saved-generation handoff.
- [x] Add selection toolbar, context-menu action, and keyboard/menu alternative.
- [x] Prove undecided suggestions remain unchanged, repeated Apply cannot mutate twice, stale work is refused, and selected edits cannot change neighboring text, style, or boundaries.
- [x] Keep Apply all/batch Apply out of this package; F5 owns that later contract.

The checked items describe the selected-passage slice. Evidence includes `crates/core/tests/proposals.rs` preparation, scope, replay, rollback, recovery, and tamper cases; frontend `apply.test.ts` and `ProposalPanel.test.tsx`; `Writer.tsx` selection entry points; and the W5 strict native CI result. Whole-chapter/block/manual-rebind behavior and the broader B/E4 trial remain open.

### W6 — lost acknowledgment, shared lifecycle, history, and interruption hardening

**Gate:** P with native reruns; closes the B trial gate. **Status:** history/restore and shared pending-change reconciliation implemented; remaining trial and interruption combinations open.

See [ADR 0005](ADR_0005_DOCUMENT_HISTORY.md). Restore preserves the current writing in a checkpoint and advances the story source epoch. The operation receipt is its immutable author decision; schema 8 needs no new decision table.

- [x] Reconcile pending operation IDs and latest heads after lost acknowledgment.
- [ ] Finish native refresh qualification: Windows now blocks browser refresh shortcuts and removes the native Reload item; the complete six-key/menu run remains to be qualified. Apply, reconciliation, editor disposal, switching, and normal close use the shared guard.
- [x] Coordinate normal close across open-project AI work: admission fence, exact Stop, worker cleanup, retained-result blocking, same-editor Stay open, and final readiness before destruction. Core/frontend tests and two native close groups pass; forced OS shutdown and installed-release qualification remain separate.
- [x] Add in-session history boundaries, significant undo/redo checkpoints, restart comparison, and explicit restore.
- [ ] Cover forced renderer loss, process interruption, restore A while B runs, and old-or-new transaction outcomes.
- [x] Exercise native forced renderer reload after a committed Save, process termination after committed Apply, and active HTTP ownership across reload/process loss; verify exact receipts, current heads and frozen context without replay. Physical renderer crash and finer native crash timing remain separate.
- [x] Recover synthetic A through the real native backup/recovery dialogs while B has a running reply; retain independent identities, exact prose/history and B's original source/packet, then Stop B once without replay.
- [ ] Include context snapshots, source epoch, policy, delivered packet, and receipt in restart/fence/Stop coverage; no late context operation may trigger an implicit paid retry.
- [x] Add a context-preparation subprocess commit/acknowledgment-loss test and preserve exact frozen source/packet/receipt records in the new native interruption and recovery fixtures. Broader context/job combinations remain open.
- [ ] Retain the live buffer on disk-full and permission errors; never hide external retries or paid restarts.
- [x] Offer an explicit live-buffer Markdown recovery copy independent of project storage; native synthetic SQLite failure, Save/Cancel, retained unsaved state, blocked navigation, and Retry pass. Actual disk-full and ACL failures remain separate.
- [ ] Run the E4 stale-proposal friction check and keep conservative staleness unless measured evidence supports a bounded alternative.
- [ ] Run the B feedback trial only after the durable Apply and lifecycle evidence is complete.

### W7 — explicit exports and packaged native qualification

**Gate:** M/P/N complete. **Status:** export implementation and package build present; full native/release qualification remains open.

- [x] Add Markdown beside draft TXT with an exact frozen single-document preview, formatting/omission disclosure, immutable source record, and explicit working-draft export. Schema-20 author-reviewed chapter export is separately qualified; collection/publication export remain open.
- [ ] Finish keyboard navigation, accessible labels, focus restoration, resizing, native dialogs, and offline installation.
- [ ] Remove test-only command access and embedded automation from shipping builds.
- [ ] Qualify the packaged Windows WebView for keyboard/dead-key input, clipboard, focus, accessibility, high DPI, long chapters, recovery, Unicode/formatted projections, and export omissions.
- [ ] Do not infer publication, reviewed-story readiness, or continuity validity from export.

### W8 — one qualified live provider

**Gate:** L. **Status:** planned.

- [ ] Qualify one exact provider/model/configuration with explicit model and supported traits.
- [ ] Qualify one deterministic, fully recorded context packet first; treat the C6 bounded read loop as additional qualification after the one-packet route.
- [ ] Cover streamed completion, refusal/truncation, authentication failure, broken/partial streams, Stop, process cleanup, and recovered terminal history.
- [ ] Qualify credential entry/storage and inspect logs/backups for leakage.
- [ ] Document opaque upstream retries and the limits of local idempotency; do not claim exactly-once external billing.
- [ ] Keep unqualified adapters unavailable and never silently substitute a provider/model.

### F1 — V2 migration

**Gate:** named migration evidence. **Status:** planned.

- [x] Implement staged read-only import and reconciliation for schema 8, with explicit missing-prose choices and new V3 identities.
- [x] Retain original legacy evidence and rebuild V3 document projections without promoting V2 approvals into authority.
- [x] Distinguish editable imported writing/notes from inert legacy evidence and leave V2 source/application unchanged.
- [x] Cover synthetic supported-schema snapshots and reconciliation before exposing Import V2.
- [x] Complete the synthetic schema-8 native chooser/import/reopen journey with unchanged source evidence.
- [ ] Complete representative author-approved migration acceptance and broader pending-import native recovery cases.

### F2 — reviewed story boundary

**Gate:** architecture scenario 5. **Status:** author-only prose review implemented in development; full F2 remains open.

The first part stages an exact saved chapter and its complete earlier selected reviewed prefix, then records an explicit author-only bundle without changing prose. The native Story review panel previews the immutable revision before confirmation. Review remains optional for writing. Source edits and changed earlier selections invalidate current review eligibility while preserving historical reviews and later text. Schema 14 introduced stages, bundles, heads, and suffix fences; schema 15 adds the reviewed basis manifest and immutable reader-position pins, while recovered copies clear active review pointers. The core/IPC freeze resolves the exact earlier reviewed prefix plus current working target. The current continuation slice consumes that boundary for explicit Working/Reviewed generation and append-only Apply/Reject. Schema 20 adds reviewed export with exact immutable bundle provenance and final freshness validation. Schema 21 adds the locally checked passage-backed reviewed-evidence set and reader-only restricted projection described in [ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md). Schema 32 adds optional accepted narrative chapter summaries, explicit Set/Clear/inheritance, resumable author review, and distinct audience-filtered context packing. Broader native/live qualification, exceptions, and the wider C5 state model remain open. See [ADR 0012](ADR_0012_AUTHOR_REVIEW.md) and [ADR 0013](ADR_0013_REVIEWED_CONTEXT.md).

- [x] Stage exact author-only prose review, inspect earlier reviewed revisions, explicitly select immutable bundles, preserve history, and reject stale source/basis activation. Resume saved unaccepted reviews explicitly after restart.
- [x] Freeze the exact reviewed prefix and current working target through the core/IPC boundary, retaining immutable reader positions and historical namespace fencing.
- [x] Extend these bundles with the first typed passage-backed reviewed record set, including exact evidence, identity choice, audience filtering, inheritance, explicit clear, historical validation, and restricted projection. This bounded possession slice does not establish complete continuity.
- [x] Add optional immutable accepted narrative summaries to staged author review, with explicit source/basis decisions and separate context coverage. Current executed qualification is recorded above.
- [x] Add passage-backed character knowledge observations to staged review, preserving attitudes, stable identities, exact evidence, Set/inherit/Clear, restricted reader projection and incomplete source-ordered history. This schema-33 development slice is described in [ADR 0031](ADR_0031_CHARACTER_KNOWLEDGE.md); broader C5 and semantic extraction remain open.
- [ ] Extend the reviewed-story model with additional rules and records, issue decisions and exceptions, known dependency evidence, and broader continuity views.
- [x] Enable reviewed-source continuation and append-only preview/Apply only with explicit validity; do not imply exhaustive continuity.
- [x] Exercise continuation in local native WebView2, one bounded selected live provider, and strict CI 34008911179.
- [x] Implement author-reviewed chapter export, exact projection and record validation, final freshness refusal, local native acceptance, and strict CI 34010306332; this establishes no canon/publication authority.
- [ ] Complete broader provider/native/release gates and remaining author trials.
- [ ] Retain sole ownership of reviewed authority; context packets and generated digests cannot accept canon.

### F3 — context quality

**Gate:** measured task-specific context evidence. **Status:** C4-A chapter navigation memory, C4-B working AuthorRoom packet reuse, and C4-C chapter-only freshness are implemented development slices with local/native/CI evidence. C4-B integrates current generated views into working packets when full prose does not fit; C4-C preserves unchanged chapter views after unrelated edits while ordinary request/proposal freshness remains conservative. Higher-level arc/scene views, remaining C5 state views, C6 evaluation, background Luna/xhigh routing, and strict provider/release qualification remain open.

- [ ] Add source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search, and freshness checks.
- [x] Implement the C4-A source-linked chapter navigation digest slice with explicit Refresh, exact source revision, strict evidence validation, separate job/result/view records, stale/revocation fencing, and local recovery boundaries. Generated views remain inspection-only and are not supplied to future model packets.
- [x] Verify C4-A cleanup, uncertain claims, archive/reopen, native development flows, and one bounded live Codex result. Strict CI and broader provider/release gates remain separate.
- [x] Integrate current C4-B derived views into working AuthorRoom packets through a separate immutable dependency and coverage contract; generated views remain excluded from restricted writing and reviewed continuation.
- [ ] Own broader C4 generated views and C5 thin temporal/relationship/thread views, with C5 depending on F2; add richer quality only after the evidence-first C0–C3 foundation.
- [ ] Measure omissions and permissions before claiming a memory or prompt improvement.

### F4 — narrative evaluation

**Gate:** independent author-labelled evaluation. **Status:** planned.

- [ ] Run retrieval, continuity, prose, and author-acceptance cases using frozen model/settings.
- [ ] Limit conclusions to the evaluated English tasks, genres, lengths, and models.

### F5 — batch Apply

**Gate:** B complete and separate atomicity evidence. **Status:** planned.

- [ ] Implement same-base disjoint preparation and one atomic Apply/decision transaction.
- [ ] Validate the common context snapshot, policy, and source epoch as part of the atomic batch.
- [ ] Cover overlap, repeated operation IDs, stale/already-decided members, and lost acknowledgment.
- [ ] Preserve individual Apply as a single explicit author decision.

## Story Context extension completion ledger

The maintained [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) and [first-slice plan](V3_STORY_CONTEXT_FIRST_SLICE.md) are an adopted design extension. Their C0–C6 packages are part of the full V3 goal and preserve the base save, Apply, lifecycle, and reviewed-authority ownership. C0 is implemented with pure contracts and 16 adversarial tests; C1 is implemented as a working-basis snapshot/retrieval slice; C2 is implemented and pushed as a Rust pure deterministic compiler with durable exact packet receipts and native IPC, and is covered by the current native development flow and earlier hosted checkpoints; C3 guidance persistence and packet binding are pushed, as are bounded recent discussion context and inspector display. Linked retry guidance and saved composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. C6 is implemented as the schema-34 reviewed-memory development slice with focused lookup protocol, packet, boundary, core, frontend, native mock, and bounded live evidence; broader provider, author-trial, and release qualification remains open. No full C0–C6 completion is claimed.

| Package | Planned owner and scope | Status | Required evidence before completion |
| --- | --- | --- | --- |
| C0 | Before/alongside W4; freeze contracts and adversarial eligibility fixtures | Implemented | Pure contracts and 16 adversarial tests cover source eligibility, disclosure boundaries, digest restrictions, and authority separation |
| C1 | Before/alongside W4; immutable source snapshots, exact retrieval, source epoch, and dirty-index fallback | Implemented working basis | Rust actor snapshots pin canonical revisions; exact literal/lexical scan, source-only alias matches, Unicode UTF-16 spans, revocation epoch, conservative source staleness, snapshot retry/restart, and yielding disposable per-document index rebuild. Ten C1 tests include a 1,000-chapter, 8,280,000-byte cold snapshot measured at 1.1 seconds in local debug; six context migration tests cover schema-2/3 upgrade and recovery, with transfer coverage for schema-1 recovery. Reviewed/history/character policies remain unavailable until authority work; aliases remain private to AuthorRoom until safe grants, and AuthorRoom Revise/Continue is blocked |
| C2 | Before/alongside W4; deterministic multi-resolution packet compilation, mandatory-budget errors, and actual-packet receipts | Implemented and pushed; development smoke covered | Pure compiler and durable receipt tests pass; exact packet/messages/options/hash survive restart; source body/descriptor/projection/eligibility/scope validation, target/instruction/scope/mandatory-pin preservation, full-eligible-when-fitting and whole-block-prefix packing with explicit omissions are implemented. Mock accounting is UTF-8-byte based only; provider tokenization, live AI integration, and release qualification remain open |
| C3 | Before/alongside W4; scoped author guidance and the context inspector | Partial: guidance persistence, bounded recent exchanges, inspector, transient pins, persistent discussion sources, and approved writing briefs integrated | Chat or direct entry can be saved, edited, and retired as immutable exact versions at Next request, This document, or This project scope. CAS/idempotent guidance receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, one-use consumption after successful persisted start, separate guidance handles in the inspector, and GuidancePanel lost-ack/late-response coverage are covered locally. Recent complete exchanges are frozen and packed with exact message receipts and explicit omissions; stopped/partial, other-document, revoked-policy, and copied historical turns are excluded. Unchanged unsuccessful retries preserve original one-use instructions without consuming newly waiting guidance; request identity, current policy, active versions, restart, and recovered-copy boundaries are tested. Optional approved briefs preserve exact restricted request text without transferring private origin material. Richer conversation selection and broader Apply integration remain open |
| C4-A | F3; source-bound single-chapter navigation digest without automatic canon | Implemented development slice; local/native/live evidence recorded | Exact full-chapter revision, strict `navigation-digest.v1` UTF-16/evidence checks, separate job/result/view records, stale/revocation/recovery boundaries, native/provider evidence, and no paid autosave/open calls |
| C4 | F3; derived-view packet integration and richer quality without automatic canon | Partial: C4-B frozen chapter-view reuse and C4-C chapter-only freshness implemented and CI-qualified; higher-level views and quality evaluation open | Broader contextual/arc digests and measured interpretation quality; chapter-only views retain their exact source, evidence, and disclosure limits |
| C5 | F3 after F2; thin temporal, relationship, knowledge, and thread views | Partial: entity reuse, batched current-evidence freeze, authenticated object history, schema-23 promise history, schema-33 passage-backed character knowledge/history, and the schema-34 reviewed-memory lookup slice implemented; current qualification recorded above | Build relationship/rule views and richer knowledge transitions with source-bound retrieval, disclosure, uncertainty, and historical dependencies; add generated knowledge extraction, broader provider/native qualification, and quality evidence for supported English tasks |
| C6 | W8 additional qualification; bounded provider-side read loop | Implemented schema-34 development slice: opt-in `story-lookup.v1` route for Working, AuthorRoom, and Discuss; durable invocation/read records; typed reviewed-memory operations; focused protocol, packet, boundary, core, frontend, native mock and bounded live evidence | Final delivery-label qualification; hosted/live-provider qualification beyond the bounded slice; model-specific accounting; broader Stop/budget/duplicate-event/crash boundaries; visible unknown outcomes; fresh invocation labeling; restricted-writing/state/thread extensions |

The public promise is layered: stored evidence, permitted available sources, the packet actually delivered, and what a model understood are separate states; the last requires evaluation. C1 retains original source and does not make copied historical snapshots authoritative for a new project. Context work does not authorize automatic canon or replacement of source text with a large rolling summary.

## Completion rule

The full V3 goal is complete only after the applicable W0–W8, F1–F5, and C0–C6 gates have their implementation, failure coverage, and evidence recorded. Current W1/W2 progress and active W3 work are necessary groundwork; they do not close the A writing trial, B feedback trial, C release qualification, live-provider qualification, migration, reviewed-story, context-quality, narrative-evaluation, context-extension, or batch-Apply gates.
