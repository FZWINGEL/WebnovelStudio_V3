# Story Workshop implementation and acceptance

This work implements [the Story Workshop specification](V3_STORY_WORKSHOP_UX_SPEC.md),
fetched at `c83a127`, in the existing Rust/Tauri V3 application. The specification
is the target; this ledger records implementation and evidence separately. The
previous private 3.0.0 installer predates this work and does not qualify it.
The fresh [Workshop installer](WINDOWS_PACKAGE_QUALIFICATION.md) passed its
installed lifecycle, and [CI 34133645198](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34133645198)
passed all 15 Workshop native checks plus the existing native suites. Full
specification acceptance and author evaluation remain open.

The latest refinement adds candidate-level **Give alternatives** under **Explore
another angle**. It freezes that candidate's title/text and the displayed
comparison dimension, asks to preserve author-chosen invariants, and leaves the
tray, choices, working text, and story documents untouched. The two focused UI
files pass 46 tests; the complete wrapper passes 766 Rust, 546 frontend, and
11 tooling checks, formatting, strict Clippy, TypeScript, and production build
(`.local/workshop-alternatives-check.log`). The existing large-chunk warning
remains. The refreshed debug build and its exact identity are recorded in
[Windows qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

This refinement is pushed at `16540ea9f968e969f63b57817f36c2a187894516`.
Fresh [CI 34153931657](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34153931657)
and [package run 34153949693](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34153949693)
are checking that exact source. Their results remain pending; the previous
installer is not reused for this product change.

The native harness now also attempts adoption after an author saves a working
edit after preview. It checks visible refusal, unchanged documents/decisions/
receipts, retained later prose, and a fresh preview after deliberate editing.
Headless Chromium at 1440 and 800 pixels verifies the new candidate request
scope and the actual stale-preview notice/recovery path without a real provider
or adoption call (`.local/workshop-alternatives-qa/` and
`.local/workshop-stale-preview-qa/`). This is local exploration-version evidence,
not a substitute for concurrent target-document CAS qualification.

The current controls checkpoint adds implication-specific keep, reject, and
contrast actions. Reject/contrast preparation preserves the author composer,
choices, selected tray, and working text; the exact questioned candidate,
implication, basis, and assumption remain provisional request context. Explicit
Explore is still required. Candidate exploration no longer silently selects a
direction, and subversion requires the author to name a convention and choose
a transformation.

Relationship exploration now includes confirmed preferences for both validated
endpoints in the UI and frozen packet. Fixed decision protection survives
archive and supersession, with explicit unfix available in the UI; relevant
protection alone enters a request, while adoption checks every changed target.
Focused material shows its protection before adoption. All question entry paths
respect saved dispositions and preserve who the mystery is unknown to.
Pending request preparation/reconciliation prevents switching explorations,
forking, or leaving the project without disabling ordinary draft editing.

The standard wrapper passed 766 Rust tests (70 core unit, 623 integration,
73 desktop; one intentional subprocess ignore), 544 frontend tests in 48
files, 11 tooling checks, formatting, strict Clippy, TypeScript and production
build. The final frontend-only check after the request-navigation and explicit
convention corrections also passed 544/48. Logs are
`.local/workshop-controls-check.log` and
`.local/workshop-controls-final-frontend.log`. The 32-test Workshop integration
sweep passed in parallel after assertions were bound to the intended run ID;
the former positional assertion depended on the order of two generated runs.
Headless fixtures at 1440 and 800 pixels cover consequence preparation,
navigation and deliberate question reopening, and archived protection/unfix
under `.local/workshop-consequences-qa/`, `.local/workshop-navigation-qa/`, and
`.local/workshop-protection-qa/`. No page errors or horizontal overflow occurred.
The checkpoint is pushed at `fc3468829e037588458a05204c0ef92ddbea9cc2`.
On that exact source, [CI 34150860150](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150860150)
passed the contracts and 17 Workshop native groups, then stopped because the
character-row selector omitted the kind text in its accessible name. There were
no page errors; subsequent native suites were skipped. Both row selectors now
scope to the Documents navigation and match the exact title child. The retained
failure report and screenshot are under `.local/ci-workshop-34150860150/`.
[Package run 34150884037](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34150884037)
passed its installed write/reopen, normal-close, and same-version retention
lifecycle. It does not exercise the full Workshop controls. The current
debug executable was rebuilt successfully without launching it locally; its
byte identity is recorded in [Windows qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

The acceptance harness also checks that an unselected preference stays absent
from the delivered packet and that a local Want cannot replace a hard project
Never. The exact conflict form and alert selectors passed against the real
Preferences component in headless Chromium at 1440 and 800 pixels, with unchanged
preferences and no IPC calls, page errors, or overflow
(`.local/workshop-preferences-qa/report.json`). The current full wrapper again
passed 766 Rust, 544 frontend, and 11 tooling checks, formatting, strict Clippy,
TypeScript, and production build (`.local/workshop-acceptance-check.log`). Native
execution of the corrected and expanded harness reached 25 passing groups in
[CI 34152622887](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34152622887)
on `c7c1c6a99db9f408d3e421f176c920c0ecf557d9`, then failed in the what-if
unchanged-document assertion. All document values were identical; SQLite rows
had null prototypes while their structured-clone snapshots were plain objects.
The harness now normalizes rows before snapshotting, retaining strict comparison
of every selected field. A local SQLite reproduction confirms that unchanged
snapshots compare equal and changes to each field still fail. Artifacts are in
`.local/ci-workshop-34152622887/native-spike-evidence-direct/workshop/`; there were
no page errors. Contracts passed; later native suites were skipped. The new
candidate action and stale-preview check above require fresh qualification.

The preceding names/aliases source is
`b801ae9cfc00203a37c7de05da8de801a0214af1`. Fresh
[CI 34147701184](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147701184)
and [installer run 34147720364](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34147720364)
ran against it. CI passed the contract steps, then failed after 16 Workshop
groups because the aliases harness searched for the exact tab name
`Characters`, while the native accessible name was `Characters 2`. The retained
failure report and screenshot are in `.local/ci-workshop-34147701184/workshop/`;
there were no page errors. Both affected selectors now match the existing
count-bearing tab convention; CI 34150860150 passed that point before the
separate character-row mismatch described above.
The installer run passed its synthetic installed write/reopen, normal-close,
and same-version uninstall/reinstall retention lifecycle. It does not exercise
the aliases controls or the newer corrections below; see the exact package
identity in [Windows qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

The current navigation correction makes the full Workshop inert during a
workspace switch, fences delayed notes/relationship reads, and refuses a flush
while text composition is active. The navigation additions are included in the current 35-test Workshop suite.
Headless Chromium at 1440 and 800 pixels verifies keyboard/focus exclusion
during navigation, preserved saved text, discarded late notes after the
boundary releases, and resumed manual editing without AI calls or overflow
(`.local/workshop-navigation-qa/report.json`). This is synthetic frontend
evidence; native qualification remains separate.

The earlier local hardening checkpoint added reusable saved preset definitions, coherent
name/text editing, and explicit definition updates without adopting preferences.
Story Bible isolates unavailable sources, verifies exact current or historical
bodies, and retains each choice and rationale when its source cannot be read.
Rust refuses competing chosen decisions for one document and normalizes confirmed
hard-project conflicts across Unicode case and surrounding whitespace. Unconfirmed
and neutral preferences remain inactive for conflict enforcement.
The exact-revision reader also permits retained text from a trashed source,
while ordinary history listing, restore, and live writing continue to refuse
that source. A core regression checks document/project ownership and those
read-versus-write boundaries; the UI reports an unavailable item if the exact
retained revision itself cannot be read. Preference conflict detection is
symmetric when the author adds the hard project rule after a softer preference.

The previous relationship slice added schema-36 reader-floor handling for the
optional typed relationship packet fields. Existing Workshop state, context,
packet bytes, and hashes remain unchanged through the migration. The World and
People surfaces can prepare an independent relationship exploration without a
model call, display its named direction and uncertainty, and pin both endpoint
heads; stale or late reads are refused or ignored. RequestContext reads the
immutable relationship envelope. The taste-test contract now requires two or
three moment treatments, while a one-treatment raw response remains recoverable.
That previous relationship slice's focused UI checks passed (32 tests in 3 files); headless checks at 1440 and 800
pixels covered keyboard use, two sources, an explicit destination, and frozen
context with no errors or overflow. Two visual rounds were inspected under
`.local/workshop-relationship-qa/`. After the UI wording correction, that
previous slice's standalone frontend build/check passed 514 tests in 46 files;
this was not a full workspace check.

The 17:19 Berlin standard check passed 749 Rust tests (69 core unit, 607 integration,
73 desktop; one intentional subprocess entry-point ignore), 488 frontend tests in
42 files, 11 tooling checks, strict Clippy, formatting, TypeScript, and build.
The subsequent source-unavailable copy and preset spacing correction passed the
16 focused frontend tests and the 17:22 Berlin final standard check with the same
749 Rust / 488 frontend / 11 tooling totals. Headless Chromium checks of preset review/reuse and a
mixed available/unavailable Story Bible passed at 1440 and 800 pixels without
page errors or horizontal overflow; screenshots were inspected under
`.local/workshop-hardening-qa/`. This is frontend fixture evidence only.
The native harness now includes exact Story Bible history after a manual source
edit, all six lenses, persisted preset definition review/edit/reuse, and an
untitled second project. These additions and the changed application still need
a new hosted native run and fresh installer qualification; the earlier artifacts
below retain their original source identities.
The final reviewed-source standard check at 17:29 Berlin passed 750 Rust tests
(69 core unit, 608 integration, 73 desktop; one intentional ignore), 488 frontend
tests in 42 files, 11 tooling checks, formatting, strict Clippy, TypeScript and
build. The context inspector native addition reads the actual immutable packet
and verifies confirmed local-mock delivery; it does not imply live-provider
understanding. The expanded native flow has 20 intended check groups.

Hosted [CI 34138626172](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34138626172)
on `0cd1c956aa07545fdf47f41c544c9bfe21aec20a` passed Ubuntu and the Windows
Rust/frontend/build steps. Workshop passed 19 groups with no page errors on
WebView2 `151.0.4129.101`, including exact delivered context inspection, exact
Story Bible history and focus, all six lenses, and persisted preset definition
editing/reuse. The final untitled-project group failed at a test selector:
the blank screen has both an h1 and h2 named “What are you excited about?”.
The harness now requests the level-one heading; a headless blank-Workshop
reproduction confirms the old two matches and the corrected unique match.
Later native suites were skipped, so this run is not a complete native pass.
Evidence is under `.local/ci-workshop-34138626172/workshop/`. Fresh installer run
[34138625419](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34138625419)
used the same application source and was intentionally cancelled before lifecycle
qualification finished. Native screenshots exposed a stale preset-adoption
notice, now corrected with frontend and native regression assertions. The 17:46
Berlin standard check passed 750 Rust, 488 frontend, and 11 tooling checks plus
formatting, strict Clippy, TypeScript, and build. This product correction and the
what-if changes need a fresh installer. The local debug build
from that source is 48,421,376 bytes, ProductVersion 3.0.0, SHA-256
`894008b51e800008fa3751c99d9d380056a6deac0d7bb8cf3ced47e31599a961`, built at
`2026-09-07T15:32:16.7406333Z`; see `.local/workshop-hardening-debug-build.json`.

## Boundaries

The corrected hosted run [CI 34139777354](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34139777354)
passed on exact head `27d1ef1a1b6571b84174e9210e0bc546fcbdaeda`: both jobs,
20 Workshop groups, 52 main native checks, HTTP 6, normal-close 2, interruption 4,
project recovery 4, and memory lookup 3. Downloaded reports under
`.local/ci-workshop-34139777354/` confirm clean runtime observations and WebView2
`151.0.4129.101`. This qualifies the preset/history/context/new-project harness
correction. It predates the preset-notice and what-if changes, which require
their own run; it is not installer or full-specification acceptance.

Documents remain the saved story material. Workshop sessions retain exploration
briefs, alternatives, selections, and working versions; decisions refer to exact
document revisions. The Story Bible reads those references. Choosing an idea
does not grant writing eligibility, establish manuscript evidence, or change
character knowledge.

Develop is a dedicated workbench. Write retains the existing editor, rich-text
transactions, save/detach barriers, review, and provider controls. Navigation and
local edits never generate. Explicit exploration is intended to use the existing
provider dispatch and owned Stop/recovery paths. Summary and story-memory jobs
retain their independent Luna/xhigh policy.

The Rust boundary validates project/session identity, expected versions,
protected content, and adoption targets. Schema-36 storage, actor,
adoption, provider-command integration, and the broadened same-packet
relationship/impact path is covered by focused storage checks, including exact
endpoint and impact provenance. The final relationship-context review passes: changed or cleared relationship
scope marks earlier results stale and blocks new candidate authority without
blocking historical saves. Broader native execution remains pending. An uncertain save or Apply retains its immutable
operation identity for receipt reconciliation. A provider response is intended
to remain an alternative until an author uses it; it cannot replace a working
version edited after dispatch.

## Dependency order

1. Persist sessions, scoped preferences, candidate provenance, selected details,
   working versions, and author decisions in the project database. Prove reopen,
   failure, stale-write, and recovered-copy behavior before attaching the UI.
2. Add Develop/Write entry and the world-first comparison workbench. Connect one
   explicit request to three typed directions, detail refinement, exact context
   disclosure, and a reviewed single-document adoption.
3. Extend the same records and UI for consequences, people, directional
   relationships, rationale, and an atomic preview across existing/new related
   nonchapter targets.
4. Add isolated what-if sessions, reviewable change impact, noncanon taste tests,
   promises/possible arcs, and editable exportable preference presets.
5. Qualify all requirements below with focused contracts, full development checks,
   isolated native execution, and the formative author evaluation. A completed
   slice or green mock suite does not complete the full specification.

## Implementation and evidence checkpoint — 7 September 2026

This ledger records the current implementation checkpoint and its dated bounded
native evidence; full three-slice acceptance and author gates remain open. The frontend implementation
is represented by `apps/desktop/src/shell/Workshop.tsx`,
`apps/desktop/src/workshop/`, `apps/desktop/src/shell/StoryBible.tsx`, and the
Workshop IPC adapters. Rust persistence and actor/adoption work is represented by
`crates/core/src/projects/workshop*.rs`,
`crates/core/src/storage/035_workshop.sql`, and
`crates/core/src/storage/036_workshop_relationship_context.sql`,
`apps/desktop/src-tauri/src/workshop*_commands.rs`, and
`crates/core/tests/workshop.rs`.

The 14:45 checkpoint passed the full frontend build and 465 tests in 39 files,
including frozen selected-range, stale-edit refusal, and offline reconciliation
regressions. A synthetic headless Chromium fixture reported no page errors and no
horizontal overflow at 600, 800, 1024, and 1440 pixels; captures are under
`.local/workshop-qa/` and are ignored. This is frontend/fixture evidence only.

The earlier full `desktop.ps1 -Command check` passed formatting, strict workspace
Clippy, all Rust workspace tests, the production build, frontend tests, and 11
tooling checks. The hosted checkpoint for source `a023511` failed its Ubuntu
ReviewPanel async-summary test (464 of 465 frontend tests passed)
([CI 34124050562](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34124050562)).
A local synchronization fix passes all 33 focused ReviewPanel tests. The
existing `test:native` suite passed at `a023511`. The subsequent hosted run
([CI 34127175895](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34127175895))
passed Ubuntu, Windows Rust/frontend/build, and the existing 52-check native
suite, then passed six bounded Workshop native groups before timing out at the
adoption **Kind** selector. It recorded no page errors on WebView2
`151.0.4129.101`; later native suites were skipped. Failure artifacts are under
`.local/ci-workshop-34127175895/workshop/failure.*`. A headless reproduction
showed the harness's exact-label `getByLabel` lookup failing while the exact
role/combobox lookup resolves. The initial harness correction was then exercised
by the native-first rerun below, so this is not a broad native pass. The native-first rerun [CI 34129236987](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34129236987)
on source `2a97a39` also failed in the six bounded Workshop groups at the prefilled
**Content** selector: the exact-label lookup found no element while the exact
textbox role resolved. Ubuntu and Windows checks/build passed; later native gates
were skipped. The harness correction was exercised by the later 341307 run. Earlier
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
The 15:56:22 Berlin local checkpoint passed `desktop.ps1 -Command check`: 746 Rust
tests (plus one intentional subprocess entry-point ignore), 474 frontend tests
in 41 files, 11 tooling checks, formatting, strict workspace Clippy, TypeScript,
and the production build. The final total input cap was restored before this
checkpoint. This includes atomic new-endpoint relationships,
candidate and relationship impact provenance, destination identity, explicit
rejection promotion and subversion, and reviewed voice guidance. One bounded
headless live Workshop smoke on source `2a97a39` completed at 13:47 UTC with one
Luna/xhigh/priority request, 7,289 confirmed stdin bytes, 2,106 input tokens,
2,354 output tokens, 1,167 reasoning tokens, and three valid directions; unchanged
anchor, manual path, and no-chapter boundaries held. The response named internal
anchors as affected material; source review showed that these could become
visible review flags. Current/historical flag projection and backend
anchor-override handling were then fixed while preserving raw responses and real
flags. A separate read-only reopen at `2026-09-07T14:00:25.138Z`, recorded in
`.local/workshop-qualification-585be29fd92341e5946690ba286e51ab-reopen.json`,
verified the exact frozen packet/input hash, one immutable receipt, raw output, and
usage with unchanged blank-anchor/manual state and no chapters or decisions; it
issued no additional model request. Broad live-provider, installer, native, and
author-study evidence remain pending.
Schema-36 tests cover the resolved chosen/alternative distinction, supersession,
exact before/after revisions, protected multiline additions, frozen ranges,
operation replay, atomic new-endpoint relationships, and candidate/relationship
impact provenance. Five `workshop_boundaries` tests cover multi-target
atomicity, chapter preservation, secret exclusion, preference conflict, and
paragraph protection. The Workshop native run has not qualified broadly; one
bounded headless live smoke exists, but broad live-provider, broader installer
coverage, and author-study evidence remain pending. The fetched specification above is
unchanged.

| Status phrase | Meaning in this checkpoint |
| --- | --- |
| Implemented — native/quality pending | The named source group contains the described development surface; native execution and/or requirement-specific quality evidence is still missing. |
| Partial — gap stated | A bounded path exists, but the adjacent requirement or explicit acceptance boundary remains incomplete. |
| Pending evidence | No executed implementation evidence is recorded for the requirement in this checkpoint. |

## Requirement ledger

Every row now records the narrowest implementation/evidence claim supported by
this checkpoint. Do not promote a row to complete from source presence or a
green mock/frontend run alone.

| ID | Spec | Required behavior | Evidence / status |
| --- | --- | --- | --- |
| W01 | 4–5 | Develop/Write, optional title, lightweight blank-project choice; existing-project resume | CI 34133645198 covers blank Develop, zero chapters, seed save, and Library reopen preservation without generation. CI 34152622887 additionally passed untitled second-project development and return to the first project's saved Workshop/preferences. Broader existing-project and author usability evidence remains open. |
| W02 | 4 | Overview, World, People, Themes & tone, Story possibilities, Notebook; free movement | All six lenses are implemented in catalog/Workshop. CI 34152622887 passed navigation through all six with saved position, unchanged documents, and zero generation. Broader navigation usability remains unqualified. |
| W03 | 4, 16 | Dedicated three-zone workbench, collapsible context; full generic editor retained | Dedicated Workshop frontend and generic Write surface are present; three-zone/collapsible-context and native focus qualification remain pending. |
| W04 | 4, 16 | Story Bible projects exact chosen document material and provenance | Six direct regressions cover exact current/historical sources, unavailable or mismatched revisions, deleted sources, project switching, source callbacks and focus. CI 34152622887 passed exact chosen history, superseded exclusion, preserved historical text after a manual source edit, and focus return without generation. Broader native unavailable-source cases remain open. |
| W05 | 4, 15 | Responsive comparison/list/drawer, keyboard/focus, save feedback, composition-safe input | Comparison UI and save-watermark path are implemented; fixture has no overflow at four widths, while native keyboard/focus/composition evidence is pending. |
| W06 | 5 | Fragment, direction help, existing notes; preserve originals; no genre/MC/ending gate | The native checkpoint covers seed save and local working edit; broader brief/direction preservation and native quality evidence remain pending. |
| W07 | 5, 12 | Editable You said / Possible direction / Still open; question, reason, free alternatives | Direction and open-question fields are present in the Workshop state/UI; full question/reason/free-alternative qualification is pending. |
| W08 | 6 | Three concise typed candidates with explicit differing dimensions and expandable details | CI 34133645198 passed one explicit mock generation returning three directions; broader candidate comparison and quality evidence remain pending. |
| W09 | 6 | Develop, select details, save later; visible editable tray; preserved details in synthesis | CI 34133645198 passed the detail tray and local working edit, with reopened working/chosen history preserved; broader synthesis and native quality evidence remain pending. |
| W10 | 6, 13 | Scoped direct/natural-language edits protect unselected and Keep fixed material | Candidate steering/edit paths exist and paragraph-level protected-content boundaries are covered locally; full scope projection and native/quality evidence remain pending. |
| W11 | 6 | Concrete, consequences, alternatives, challenge, ordinary life, moment actions | All named actions are available from candidate cards. The candidate-alternatives UI tests and headless checks prove exact candidate/dimension scope and unchanged selection/working material; broader native and narrative-quality evidence remains pending. |
| W12 | 6, 16 | Use this version previews add/replace destination, exact source and complete packet | CI 34133645198 passed an adoption preview with zero writes and a world adoption; complete-packet and broader adoption quality evidence remain pending. |
| W13 | 6, 13 | Recoverable alternatives, separate saved/chosen/archived/superseded and access | Schema-36 tests cover chosen/alternative resolution, supersession, authorRoom access, replay, and atomic refusal of a saved state re-promoting an older decision while another version is chosen. Archived history is retained. CI 34133645198 adds an authorRoom decision and history/Library reopen. Broader recoverability and access quality remain unqualified. |
| W14 | 7 | Neutral/Want/Avoid, optional Must/Never; meaning, examples, temporal intent and scopes | Scoped preferences are implemented in `apps/desktop/src/workshop/Preferences.tsx` and core Workshop state; semantic/native quality evidence pending. |
| W15 | 7 | Hard project/local conflicts explicit; unknown semantic conflicts never claimed solved | Rust/frontend tests cover normalized confirmed hard-project conflicts, unconfirmed/neutral behavior, and atomic refusal. CI 34152622887 passed visible local Want refusal with unchanged hard project Never and no generation. Unknown semantic conflicts and narrative compliance remain unclaimed. |
| W16 | 7 | Contextual suggestions, search/Browse all, families, custom tags, editable presets | Custom tags, preset review, and import/export UI are implemented in `Preferences.tsx`, `catalog.ts`, and Tauri preset commands; native/quality evidence pending. |
| W17 | 7 | Optional local rejection rationale, explicit promotion; no hidden global learning | A rejection can be reviewed into a scoped, editable preference in `Preferences.tsx`; `Preferences.test.tsx` covers author editing and scope selection. Native and quality evidence remains pending. |
| W18 | 7 | Subversion distinct from inclusion/exclusion and explicitly selected transformation | `catalog.ts` exposes explicit convention-transformation operations and `Workshop.test.tsx` covers the required convention and selected operation; core/native and quality evidence remain pending. |
| W19 | 8 | World slices and four optional lenses; depth choice, ordinary life, open mysteries | Six-lens Workshop surface is implemented; depth, ordinary-life, and open-mystery quality evidence remains pending. |
| W20 | 8 | Conditional consequences expose basis/assumptions; accept/reject/contrast | Implication-specific keep/reject/contrast actions preserve provisional evidence and author choices. Focused tests and 1440/800 headless flows pass; the native harness now covers local rejection/contrast. Native and creative-quality qualification remain pending. |
| W21 | 9 | Behavior-first people, optional spine and tentative situation responses | The people lens, behavior-first situation action, and durable session/decision fields provide the prompt-led path; no structured people database is required for this behavior. Focused/native quality evidence remains pending. |
| W22 | 9, 16 | Directional typed relationship between stable existing people/groups; local view | The World/People surfaces prepare an independent named-direction exploration, preserve uncertainty, and pin both exact endpoint heads; RequestContext shows the immutable relationship envelope. Focused UI/headless checks cover stale/late reads, two sources, and an explicit destination. Native relationship qualification remains pending after the bounded harness failure below. |
| W23 | 9 | English writing preserved; Unicode names, aliases and transliteration supported | Existing `document_aliases` uses atomic source-epoch CAS saves and read-only uncertainty reconciliation. Focused and headless checks cover dirty navigation, title/body preservation, and restricted-context exclusion. CI 34152622887 passed Writer Unicode/transliteration save, project reopen, and unsaved-name navigation refusal until explicit saving, without generation. Broader author/provider evidence remains open. |
| W24 | 10 | Themes as open questions; reader tone distinct from intensity | Themes & tone is represented by the six-lens Workshop surface; distinction/quality evidence remains pending. |
| W25 | 10, 13 | Same-situation noncanon treatments, editable samples, explicit derived voice guidance | Moment responses now require two or three same-situation treatments; a one-treatment raw response is rejected recoverably. The existing voice-guidance path remains explicitly reviewed before Develop/Use this version, with no automatic documents, decisions, or adoption. Broader voice/noncanon quality remains pending. |
| W26 | 11 | Optional story engines, varied progression, promises/payoffs/possible arcs not events | The Story possibilities lens, optional arc action, prompt template fields, and durable session/decision hooks provide the prompt-led story-engine path without a structured engine database. Quality qualification remains pending. |
| W27 | 12 | Not now / Not relevant / Keep mysterious; author unknown vs reader unknown | World, lens, and generated question selection respect saved dispositions until explicitly reopened; question-specific reasons and unknown-to distinctions persist. Focused and 1440/800 headless checks pass. Native/quality evidence remains pending. |
| W28 | 12 | Local saved-decision recap and specific handoff; no paid close summary/completeness score | CI 34133645198 passed history UI and full Library reopen preserving seed, working, chosen, relationship, and history without generation; broader recap quality remains pending. |
| W29 | 13 | Editable rationale, protected passages, independent authority/access/evidence axes | Protection remains effective across archive/supersession, explicitly removable in the UI, relevant to request context, and independently enforced for adoption targets. Three core protection regressions plus UI/headless checks pass. Broader native/access/evidence qualification remains pending. |
| W30 | 13 | Isolated what-if fork/compare; accepting proposes reviewed changes only | Implemented locally, native qualification pending: branch graph and immutable ancestry checks; branch-local draft protection; inherited chosen context; parent/alternate text, fields, decision revisions and linked impact evidence; explicit preview/adopt. Focused core, component and shell tests pass. A 21st hosted Workshop group now covers real fork/reopen/compare/preview/adoption. Broader creative-quality and author evidence remain open. |
| W31 | 13 | Affected material with links/reasons and four impact categories; no automatic repair | AdoptionImpacts exposes reasons and four categories. Core tests cover candidate/relationship provenance and uncertain claims defaulting to possibleTension/needsReview without repair. CI 34133645198 adds relationship-impact decision provenance with zero chapter writes. Broader category/review quality remains unqualified. |
| W32 | 14 | Actual delivered context with direction/preferences/current/chosen/fixed/included alternatives | Explicit read of saved packets is implemented in `RequestContext.tsx` and context IPC; queued wording now says “saved”. The bounded live smoke passed the unchanged-anchor/manual/no-chapter path, while complete delivered-context qualification remains pending. |
| W33 | 14 | Exclude unrelated chat/rejected/noncanon by default; rationale independently usable | A persisted core request test excludes unrelated note/chat text, rejected and archived prose, and an unadopted vignette; only the explicitly included saved alternative and rejection rationale remain. Raw excluded results stay recoverable and the exact packet survives reopen. Native delivered-context qualification remains pending. |
| W34 | 14 | Outside-current-direction retains hard constraints; budget omissions visible | A core request test proves outside-direction retains complete original notes, fixed details and hard exclusions. Mandatory overflow refuses preparation without a run or changed Workshop state; an ample budget preserves the inputs. Native budget/conflict messaging remains unqualified. |
| W35 | 13–14 | Author secrets/intent cross into restricted writing only through explicit existing paths | Local Rust boundary coverage confirms a chosen author-room secret is excluded from a restricted snapshot/search; native/live writing qualification remains pending. |
| W36 | 15 | One explicit request, visible model/scope/status, no generation on navigation or save | CI 34133645198 passed one explicit mock request with three directions and the Develop-to-Write barrier; broader generation/lifecycle quality remains pending. |
| W37 | 15 | Independent manual saves; late response stays alternative and requires explicit refresh | Offline manual editing/save, immutable lost-ack request replay, frozen selection scope, stale-result refusal, and late-response preservation are covered by focused Workshop tests and the current full check. Native late-response evidence remains pending. |
| W38 | 15 | Partial/failed/stopped distinct; retry/recovery no blind provider replay; cost wording | Workshop UI/core paths distinguish failed, stopped, interrupted, and partial output, retain recoverable text, and offer explicit retry/local save reconciliation; focused frontend/provider checks and the current full check pass. Native/live/provider-quality qualification remains pending. |
| W39 | 15 | Offline manual development, preferences/history/organization and restart resume | CI 34133645198 passed manual seed/working persistence and full Library reopen preservation without generation; broader offline organization/restart quality remains pending. |
| W40 | 16 | Source-bound facets, stale-source refusal, no second truth database | Existing-document source binding and stale multi-target refusal are covered in core Workshop tests; broader source-bound qualification remains pending. |
| W41 | 16–17 | Atomic multi-target adoption, dependent creation, stale refusal, no chapter mutation | Core tests cover linked atomic adoption, stale refusal without partial writes, exact history, and no chapter mutation. CI 34133645198 adds existing-world/new-character relationship adoption with exact heads, impact decision provenance, and zero chapter writes. Broader native stale/failure cases remain unqualified. |
| W42 | 17 | Exportable/importable editable project presets with explicit adoption of preferences | Local tests cover JSON/name synchronization, saved definition edits/reuse, adoption boundaries, invalid input and file errors. CI 34152622887 passed JSON review, explicit preference adoption, Library reopen, definition editing/reuse without duplication or automatic adoption. Native file-dialog qualification remains pending. |
| W43 | 18 | End-to-end behavioral acceptance scenarios, including hard conflicts and secret isolation | CI 34133645198 passed all 15 intended Workshop groups plus the listed auxiliary native suites with clean runtime observations; this is bounded native evidence, not full specification, physical keyboard/accessibility, late/stale/failure UI, quality, or author-study completion. |
| W44 | 18 | Counterbalanced formative author study, same model/budget, ownership/coherence/usefulness | A [facilitator kit](studies/workshop/README.md) supplies condition/seed allocation, an optional tag-heavy worksheet, comparable live Codex allowances, anonymous observation and human review forms, and a later revisit record. No author observations exist; W44 remains pending. |

## Qualification

### What-if comparison and isolation implementation

Working sessions have no parent; what-if sessions have an existing, distinct
parent and no ancestry cycles. Existing session identities cannot be reparented
or changed between working and what-if. New and nested forks remain supported,
as does ordinary navigation. Validation applies to durable reads, saves and
adoption; invalid saves leave the prior state intact.

Keep fixed details in an exploration protect that exploration's adoption.
They cannot block a parent or sibling's independent work. Chosen decision
protections still apply to the project. The child's explicit generation inherits
chosen ancestor material and its forked preference/tray data, while sibling
choices are excluded from that session-specific context.

`BranchComparison.tsx` compares the current parent with the alternate; nested
branches name their direct parent. It shows changed fields, proposed changes
to the chosen focus, latest relevant decision versions (including superseded
parent choices), and likely affected material with saved reasons and source
links. Only active selected details resolve candidate claims from the valid
lineage. Removing a copied detail removes its candidate from that comparison;
rejected/incomplete results do not contribute claims. Stale claims and changed
relationship sources are labelled, and missing evidence does not imply no effect.

Exact saved source revisions load when the comparison is opened. Their identity
and body hashes are checked; an unavailable source does not substitute current
prose or hide readable sources. Updating a rationale does not repeat revision
reads. Opening comparison and following its source callback do not generate or
adopt. Use this version still prepares destinations and a durable preview before
the existing explicit confirmation. Inherited candidate impacts now appear in
that review instead of being omitted by the old current-session-only scan.

Focused evidence comprises 24 core Workshop tests, 18 shell tests, and 9 new
comparison/evidence tests. A synthetic headless Chromium fixture passed at 1440
and 800 pixels, including keyboard opening, exact source reads, source callbacks,
and no overflow/page errors; two batched visual passes were inspected under
`.local/workshop-branch-qa/`. This does not establish native desktop behavior.
The new 21st hosted group drives fork, editing, Library reopen, comparison,
adoption preview and explicit confirmation through the real UI, asserting full
parent preservation, no provider calls, and unchanged non-target documents.
It still needs execution on the new source.

The 18:09 Berlin full standard check passed 755 Rust tests (69 core unit,
613 grouped integration, 73 desktop; one intentional subprocess fixture ignore),
499 frontend tests in 44 files, 11 tooling checks, formatting, strict Clippy,
TypeScript, and production build. The existing large-chunk build warning remains.
Log: `.local/workshop-branch-final-check.log`. No local native app or global
keyboard automation was used. A fresh installer must be built from these product
changes rather than reusing the previously qualified package.

Implementation is pushed as `ff276eb5eecfec3b38da3af758ca1f6377add8a4`.
[CI 34141998962](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34141998962)
on that exact source ended in a harness failure after 20 Workshop checks: there
were no page errors, but W30 could not reach its button while the context overlay
was open. The harness now closes that overlay through its real button and has 22
groups including relationship coverage; this run is not a broad native pass.
Package run [34142007124](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34142007124)
succeeded on the same source. Its 3.0.0 installer identity is SHA-256
`954416c66c8982f03113b643550dda6d39539539cfaf3a608be4911f13d1cc01`; the
installer was not downloaded for this checkpoint. Package evidence does not
qualify upgrade behavior, live/provider behavior, or the remaining Workshop
native and quality gates. Final relationship-context freshness checks pass, including unchanged endpoint
documents, edited/cleared scope, unrelated relationship isolation, and refusal
of old candidate authority with historical material retained.
The local debug application was rebuilt successfully at
`target/debug/webnovel-desktop.exe`, 48,429,056 bytes, ProductVersion 3.0.0,
SHA-256 `5880578d76ab848a8f07cff43ea7de0131776f40cc722e6e63d8b718e98110ec`,
built `2026-09-07T16:11:10.8753042Z`. Identity is retained in
`.local/workshop-branch-debug-build.json`. It was not launched on the author's
desktop. The earlier qualified installer remains a separate, older artifact.

### Completed bounded slice: relationship exploration and taste tests

The final 18:51 Berlin standard wrapper passes 762 Rust tests (70 core unit,
619 grouped integration, 73 desktop; one intentional subprocess fixture ignore),
515 frontend tests in 46 files, 11 tooling checks, formatting, strict Clippy,
TypeScript, and production build. The existing large-chunk warning remains.
Log: `.local/workshop-relationship-final-check.log`. A completed relationship
response is refreshed in the UI after a deliberate relationship edit; a fresh
proposal is required before adoption when the saved relationship scope changed.

The previous published relationship source is `e61640a4738128b9744919275e362e402c7ed0d8`.
[CI 34145255173](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145255173)
reached 21 groups, including relationship exploration and preset/lens/new-project
flows, then failed at W30 when the sidebar-close control was intercepted;
subsequent auxiliary suites were skipped. [Installer qualification 34145254658](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34145254658)
succeeded on that exact source with installer SHA-256
`59394f6903ff4cc633e47556cffc3e921a7784cfa32a3da4bd68fcfcaf3ea495`.
The current native harness includes the W23 names/aliases checks, but they are
unexecuted, so this is not a broad native pass. That earlier relationship-slice
debug rebuild could not replace the running executable: Windows returned access denied for
`target/debug/webnovel-desktop.exe`, owned by process 55084 since 18:14:33 Berlin.
No author application was closed. At that time the executable still belonged
to the earlier `ff276eb` build; the failed attempt is recorded in
`.local/workshop-relationship-debug-build.log`.

The current aliases debug build subsequently succeeded at 19:24 Berlin after
the author application was no longer running. It is
`target/debug/webnovel-desktop.exe`, 48,684,032 bytes, ProductVersion `3.0.0`,
SHA-256 `f171aa81b37e908b96b7d10d13b47d77054040fa35adad4dad67418a1022512e`.
See `.local/workshop-aliases-debug-build.log` and
`.local/workshop-aliases-debug-build.json`. It was not launched locally.

The relationship and taste-test work described above is implemented locally and
covered by focused UI/headless checks. Relationship-only adoption was not added:
adoption still requires an explicit destination and the ordinary preview path.
Schema 36 is a reader-floor migration only; it preserves older Workshop state,
context, packet bytes, and hashes rather than rewriting them. Native execution
and broader quality evidence remain separate gates.

Possible arcs and intended payoffs already remain author-room exploration text;
section 11 does not itself require a second structured truth database. Verify
that workflow before adding new persistence types. Full specification and author
study acceptance remain open after these slices.

Use synthetic temporary projects for contracts and native fixtures. Preserve
existing writing and provider regression coverage; register core integration
files in `crates/core/tests/integration.rs`. The 14:45 frontend checkpoint passed
the full frontend build and 465 tests in 39 files, including selected-range,
stale-edit refusal, and offline request reconciliation regressions.
The synthetic headless Chromium fixture reported no
page errors or horizontal overflow at 600, 800, 1024, and 1440 pixels, with
captures under `.local/workshop-qa/`. These checks do not qualify native WebView2,
the current installer, a live Workshop provider, or the author study. Run the
standard wrapper and hosted native workflow on the exact implementation head
before claiming those gates. Do not use global keyboard automation on the
author's desktop.

The author study compares the existing generic Develop action, a tag-heavy form,
and Workshop with comparable seed tasks, the same selected model and comparable
usage allowances. Counterbalance task/order, include world-first and discovery
writers, and collect knowingly endorsed decisions, ownership, correction burden,
navigation/re-prompting, later usefulness, and human coherence/specificity review.
Mock candidate diversity and scripted clicks cannot supply this evidence.
