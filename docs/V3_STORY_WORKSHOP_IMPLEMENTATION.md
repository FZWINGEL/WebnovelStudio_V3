# Story Workshop implementation and acceptance

This work implements [the Story Workshop specification](V3_STORY_WORKSHOP_UX_SPEC.md),
fetched at `c83a127`, in the existing Rust/Tauri V3 application. The specification
is the target; this ledger records implementation and evidence separately. The
previous private 3.0.0 installer predates this work and does not qualify it.

## Boundaries

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
protected content, and adoption targets. Existing schema-35 storage, actor,
adoption, provider-command integration, and the broadened same-packet
relationship/impact path passed the current local standard check and focused
storage checks, including exact endpoint and impact provenance. Native
execution remains pending. An uncertain save or Apply retains its immutable
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

This ledger records the current implementation checkpoint; it does not close the
three-slice specification or any native/author gate. The frontend implementation
is represented by `apps/desktop/src/shell/Workshop.tsx`,
`apps/desktop/src/workshop/`, `apps/desktop/src/shell/StoryBible.tsx`, and the
Workshop IPC adapters. Rust persistence and actor/adoption work is represented by
`crates/core/src/projects/workshop*.rs`, `crates/core/src/storage/035_workshop.sql`,
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
were skipped. The harness now uses exact textbox/combobox roles and a fresh rerun
is pending. Package run `34129253871` was intentionally cancelled because the
product fix requires a fresh installer; package qualification remains pending.

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
`.local/ci-workshop-34130744589/workshop/`. A fresh CI rerun is pending.
The fresh package run [34130743953](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34130743953)
remains in progress building the exact `df747b6d4f8d9496cb00f1fc5f0b53285d4b5edf`
product source. It was not cancelled because the new changes are harness/docs
only; no package result is claimed yet.
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
Schema-35 tests cover the resolved chosen/alternative distinction, supersession,
exact before/after revisions, protected multiline additions, frozen ranges,
operation replay, atomic new-endpoint relationships, and candidate/relationship
impact provenance. Five `workshop_boundaries` tests cover multi-target
atomicity, chapter preservation, secret exclusion, preference conflict, and
paragraph protection. The Workshop native run has not qualified broadly; one
bounded headless live smoke exists, but broad live-provider, current Workshop
installer qualification, and author-study evidence remain pending. The fetched specification above is
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
| W01 | 4–5 | Develop/Write, optional title, lightweight blank-project choice; existing-project resume | Implemented in `apps/desktop/src/shell/Workshop.tsx` and `workshop/store.ts`; native resume/creation evidence pending. |
| W02 | 4 | Overview, World, People, Themes & tone, Story possibilities, Notebook; free movement | Six-lens surface is implemented in `apps/desktop/src/workshop/catalog.ts` and `Workshop.tsx`; native navigation/quality evidence pending. |
| W03 | 4, 16 | Dedicated three-zone workbench, collapsible context; full generic editor retained | Dedicated Workshop frontend and generic Write surface are present; three-zone/collapsible-context and native focus qualification remain pending. |
| W04 | 4, 16 | Story Bible projects exact chosen document material and provenance | Implemented in `apps/desktop/src/shell/StoryBible.tsx` with Workshop IPC/source records; native and provenance qualification pending. |
| W05 | 4, 15 | Responsive comparison/list/drawer, keyboard/focus, save feedback, composition-safe input | Comparison UI and save-watermark path are implemented; fixture has no overflow at four widths, while native keyboard/focus/composition evidence is pending. |
| W06 | 5 | Fragment, direction help, existing notes; preserve originals; no genre/MC/ending gate | Editable brief/current direction/original notes are implemented in `Workshop.tsx` and `workshop/store.ts`; preservation and native quality evidence pending. |
| W07 | 5, 12 | Editable You said / Possible direction / Still open; question, reason, free alternatives | Direction and open-question fields are present in the Workshop state/UI; full question/reason/free-alternative qualification is pending. |
| W08 | 6 | Three concise typed candidates with explicit differing dimensions and expandable details | Implemented in `CandidateBoard.tsx` and `crates/core/src/projects/workshop_generation.rs`; frontend tests/fixture evidence exist, native/quality evidence pending. |
| W09 | 6 | Develop, select details, save later; visible editable tray; preserved details in synthesis | Detail tray, working draft, and local history are implemented in Workshop/CandidateBoard/store; native persistence and synthesis qualification pending. |
| W10 | 6, 13 | Scoped direct/natural-language edits protect unselected and Keep fixed material | Candidate steering/edit paths exist and paragraph-level protected-content boundaries are covered locally; full scope projection and native/quality evidence remain pending. |
| W11 | 6 | Concrete, consequences, alternatives, challenge, ordinary life, moment actions | Workshop exploration actions are present in the development surface; requirement-specific behavior and quality evidence remain pending. |
| W12 | 6, 16 | Use this version previews add/replace destination, exact source and complete packet | Adoption preview/read path and atomic multi-target add/replace are covered by the local Rust boundary tests; complete-packet and native evidence remain pending. |
| W13 | 6, 13 | Recoverable alternatives, separate saved/chosen/archived/superseded and access | Schema-35 tests cover chosen/alternative resolution, supersession, authorRoom access, and replay; native/quality evidence remains pending. |
| W14 | 7 | Neutral/Want/Avoid, optional Must/Never; meaning, examples, temporal intent and scopes | Scoped preferences are implemented in `apps/desktop/src/workshop/Preferences.tsx` and core Workshop state; semantic/native quality evidence pending. |
| W15 | 7 | Hard project/local conflicts explicit; unknown semantic conflicts never claimed solved | Rust and frontend local project-conflict handling, including neutral behavior, is covered; unknown semantic conflicts remain unclaimed and native/quality evidence remains pending. |
| W16 | 7 | Contextual suggestions, search/Browse all, families, custom tags, editable presets | Custom tags, preset review, and import/export UI are implemented in `Preferences.tsx`, `catalog.ts`, and Tauri preset commands; native/quality evidence pending. |
| W17 | 7 | Optional local rejection rationale, explicit promotion; no hidden global learning | A rejection can be reviewed into a scoped, editable preference in `Preferences.tsx`; `Preferences.test.tsx` covers author editing and scope selection. Native and quality evidence remains pending. |
| W18 | 7 | Subversion distinct from inclusion/exclusion and explicitly selected transformation | `catalog.ts` exposes explicit convention-transformation operations and `Workshop.test.tsx` covers the required convention and selected operation; core/native and quality evidence remain pending. |
| W19 | 8 | World slices and four optional lenses; depth choice, ordinary life, open mysteries | Six-lens Workshop surface is implemented; depth, ordinary-life, and open-mystery quality evidence remains pending. |
| W20 | 8 | Conditional consequences expose basis/assumptions; accept/reject/contrast | Consequence exploration is present in the Workshop path; basis/assumption and accept/reject/contrast qualification remains pending. |
| W21 | 9 | Behavior-first people, optional spine and tentative situation responses | The people lens, behavior-first situation action, and durable session/decision fields provide the prompt-led path; no structured people database is required for this behavior. Focused/native quality evidence remains pending. |
| W22 | 9, 16 | Directional typed relationship between stable existing people/groups; local view | Existing/new document relationship UI/state is implemented in `Relationships.tsx`; focused Workshop tests cover exact endpoint heads, atomic new linked endpoints, stale endpoint rollback, and preserved relationship provenance. Native/quality evidence remains pending. |
| W23 | 9 | English writing preserved; Unicode names, aliases and transliteration supported | No executed qualification recorded; remains pending. |
| W24 | 10 | Themes as open questions; reader tone distinct from intensity | Themes & tone is represented by the six-lens Workshop surface; distinction/quality evidence remains pending. |
| W25 | 10, 13 | Same-situation noncanon treatments, editable samples, explicit derived voice guidance | Same-situation noncanon treatments and explicit `voiceGuidance` are implemented in the Workshop frontend/core paths. The voice-guidance source slice passes seven focused core tests and two focused desktop tests, including the native mock lifecycle; live/native quality evidence remains pending. |
| W26 | 11 | Optional story engines, varied progression, promises/payoffs/possible arcs not events | The Story possibilities lens, optional arc action, prompt template fields, and durable session/decision hooks provide the prompt-led story-engine path without a structured engine database. Quality qualification remains pending. |
| W27 | 12 | Not now / Not relevant / Keep mysterious; author unknown vs reader unknown | Workshop question actions, durable statuses, unknown-to fields, and the local recap are implemented; the focused frontend question-cycle test passes. Native/quality evidence remains pending. |
| W28 | 12 | Local saved-decision recap and specific handoff; no paid close summary/completeness score | `WorkshopRecap.tsx` renders saved chosen decisions, rationale, exact version links, open questions, and next focus without a model call; native/quality evidence remains pending. |
| W29 | 13 | Editable rationale, protected passages, independent authority/access/evidence axes | Rationale and protected-content paths are implemented, with multiline/paragraph boundary checks covered locally; independent authority/access/evidence qualification remains pending. |
| W30 | 13 | Isolated what-if fork/compare; accepting proposes reviewed changes only | What-if and existing-parent compare are implemented in the Workshop paths; reviewed acceptance and native/quality evidence remain pending. |
| W31 | 13 | Affected material with links/reasons and four impact categories; no automatic repair | `AdoptionImpacts.tsx` exposes reviewable reasons and four categories; the Rust adoption packet persists candidate and relationship provenance, defaults uncertain model claims to `possibleTension`, and keeps impacts at `needsReview` without automatic repair. Focused Workshop tests pass; native/quality evidence remains pending. |
| W32 | 14 | Actual delivered context with direction/preferences/current/chosen/fixed/included alternatives | Explicit read of saved packets is implemented in `RequestContext.tsx` and context IPC; queued wording now says “saved”. The bounded live smoke passed the unchanged-anchor/manual/no-chapter path, while complete delivered-context qualification remains pending. |
| W33 | 14 | Exclude unrelated chat/rejected/noncanon by default; rationale independently usable | No executed qualification recorded for this complete exclusion/rationale contract; remains pending. |
| W34 | 14 | Outside-current-direction retains hard constraints; budget omissions visible | No executed qualification recorded; remains pending. |
| W35 | 13–14 | Author secrets/intent cross into restricted writing only through explicit existing paths | Local Rust boundary coverage confirms a chosen author-room secret is excluded from a restricted snapshot/search; native/live writing qualification remains pending. |
| W36 | 15 | One explicit request, visible model/scope/status, no generation on navigation or save | Provider-command integration is present in `workshop_generation_commands.rs` and passes the local standard check; one bounded headless live request completed, while native generation/lifecycle evidence remains pending. |
| W37 | 15 | Independent manual saves; late response stays alternative and requires explicit refresh | Offline manual editing/save, immutable lost-ack request replay, frozen selection scope, stale-result refusal, and late-response preservation are covered by focused Workshop tests and the current full check. Native late-response evidence remains pending. |
| W38 | 15 | Partial/failed/stopped distinct; retry/recovery no blind provider replay; cost wording | Workshop UI/core paths distinguish failed, stopped, interrupted, and partial output, retain recoverable text, and offer explicit retry/local save reconciliation; focused frontend/provider checks and the current full check pass. Native/live/provider-quality qualification remains pending. |
| W39 | 15 | Offline manual development, preferences/history/organization and restart resume | Manual Workshop UI and scoped preferences are implemented; schema-35 persistence, reopen, replay, and local history pass the standard check, while native/provider evidence remains pending. |
| W40 | 16 | Source-bound facets, stale-source refusal, no second truth database | Existing-document source binding and stale multi-target refusal are covered in core Workshop tests; broader source-bound qualification remains pending. |
| W41 | 16–17 | Atomic multi-target adoption, dependent creation, stale refusal, no chapter mutation | Schema-35 actor/adoption passes the current standard check; focused tests cover atomic existing/new linked endpoints with exact heads, stale refusal without partial writes, exact history, candidate/relationship impacts, and no chapter mutation. Native/quality evidence remains pending. |
| W42 | 17 | Exportable/importable editable project presets with explicit adoption of preferences | Import/export UI and preset review are implemented; native adoption and quality evidence pending. |
| W43 | 18 | End-to-end behavioral acceptance scenarios, including hard conflicts and secret isolation | Frontend build/fixture evidence and five focused core boundary tests cover hard conflicts, neutral preferences, secret isolation, and multi-target adoption. The bounded live smoke exposed and then fixed internal-anchor/`affectedTargets` projection; no broad native run or full end-to-end quality qualification has been completed. |
| W44 | 18 | Counterbalanced formative author study, same model/budget, ownership/coherence/usefulness | Pending observed author participation; protocol is prepared but no study evidence exists. |

## Qualification

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
