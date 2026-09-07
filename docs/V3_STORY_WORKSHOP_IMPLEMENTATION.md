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
protected content, and adoption targets. Schema-35 storage, actor, adoption, and
provider-command integration pass the local standard check; native execution
remains pending. An uncertain save or Apply retains its immutable
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

The full `desktop.ps1 -Command check` passed: formatting, strict workspace Clippy,
all Rust workspace tests, production build, frontend tests, and 11 tooling checks.
Schema-35 tests cover the resolved chosen/alternative distinction, supersession,
exact before/after revisions, protected multiline additions, frozen ranges,
and operation replay. Five `workshop_boundaries` tests cover multi-target
atomicity, chapter preservation, secret exclusion, preference conflict, and
paragraph protection. There has been no new native run, live Workshop
provider run, current Workshop installer qualification, or author study. The
fetched specification above is unchanged.

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
| W10 | 6, 13 | Scoped direct/natural-language edits protect unselected and Keep fixed material | Candidate steering/edit paths exist, but protected additions and scope projection remain under review; requirement is partial and native/quality pending. |
| W11 | 6 | Concrete, consequences, alternatives, challenge, ordinary life, moment actions | Workshop exploration actions are present in the development surface; requirement-specific behavior and quality evidence remain pending. |
| W12 | 6, 16 | Use this version previews add/replace destination, exact source and complete packet | Adoption preview/read path is implemented in Workshop IPC/core; atomic new-document destination creation and complete-packet/native evidence remain pending. |
| W13 | 6, 13 | Recoverable alternatives, separate saved/chosen/archived/superseded and access | Working/local history and recoverable choice paths exist; chosen-versus-alternative and supersession review fixes remain in progress. |
| W14 | 7 | Neutral/Want/Avoid, optional Must/Never; meaning, examples, temporal intent and scopes | Scoped preferences are implemented in `apps/desktop/src/workshop/Preferences.tsx` and core Workshop state; semantic/native quality evidence pending. |
| W15 | 7 | Hard project/local conflicts explicit; unknown semantic conflicts never claimed solved | Local/project conflict handling is present; broader hard-conflict and unknown-semantic-conflict qualification remains pending. |
| W16 | 7 | Contextual suggestions, search/Browse all, families, custom tags, editable presets | Custom tags, preset review, and import/export UI are implemented in `Preferences.tsx`, `catalog.ts`, and Tauri preset commands; native/quality evidence pending. |
| W17 | 7 | Optional local rejection rationale, explicit promotion; no hidden global learning | Local rejection rationale is present; explicit promotion remains manual, so this row is partial. |
| W18 | 7 | Subversion distinct from inclusion/exclusion and explicitly selected transformation | No executed qualification recorded; remains pending. |
| W19 | 8 | World slices and four optional lenses; depth choice, ordinary life, open mysteries | Six-lens Workshop surface is implemented; depth, ordinary-life, and open-mystery quality evidence remains pending. |
| W20 | 8 | Conditional consequences expose basis/assumptions; accept/reject/contrast | Consequence exploration is present in the Workshop path; basis/assumption and accept/reject/contrast qualification remains pending. |
| W21 | 9 | Behavior-first people, optional spine and tentative situation responses | No executed qualification recorded for this complete people slice; remains pending. |
| W22 | 9, 16 | Directional typed relationship between stable existing people/groups; local view | Existing-document relationship UI/state is implemented in `Relationships.tsx` and core Workshop; native/quality pending. |
| W23 | 9 | English writing preserved; Unicode names, aliases and transliteration supported | No executed qualification recorded; remains pending. |
| W24 | 10 | Themes as open questions; reader tone distinct from intensity | Themes & tone is represented by the six-lens Workshop surface; distinction/quality evidence remains pending. |
| W25 | 10, 13 | Same-situation noncanon treatments, editable samples, explicit derived voice guidance | Noncanon moments and guidance are implemented in the Workshop frontend/core paths; native and derived-guidance quality evidence pending. |
| W26 | 11 | Optional story engines, varied progression, promises/payoffs/possible arcs not events | Story possibilities lens is present, but the full story-engine/promise/payoff behavior is not evidenced in this checkpoint. |
| W27 | 12 | Not now / Not relevant / Keep mysterious; author unknown vs reader unknown | No executed qualification recorded; remains pending. |
| W28 | 12 | Local saved-decision recap and specific handoff; no paid close summary/completeness score | Local history exists; saved-decision recap/handoff qualification remains pending. |
| W29 | 13 | Editable rationale, protected passages, independent authority/access/evidence axes | Rationale/protection paths are under review; protected additions and authority/access/evidence qualification remain pending. |
| W30 | 13 | Isolated what-if fork/compare; accepting proposes reviewed changes only | What-if and existing-parent compare are implemented in the Workshop paths; reviewed acceptance and native/quality evidence remain pending. |
| W31 | 13 | Affected material with links/reasons and four impact categories; no automatic repair | Current impact review only exposes relationship source-change flags and possible tension; richer four categories and candidate-affected targets are incomplete. |
| W32 | 14 | Actual delivered context with direction/preferences/current/chosen/fixed/included alternatives | Explicit read of saved packets is implemented in `RequestContext.tsx` and context IPC; complete delivered-context qualification remains pending. |
| W33 | 14 | Exclude unrelated chat/rejected/noncanon by default; rationale independently usable | No executed qualification recorded for this complete exclusion/rationale contract; remains pending. |
| W34 | 14 | Outside-current-direction retains hard constraints; budget omissions visible | No executed qualification recorded; remains pending. |
| W35 | 13–14 | Author secrets/intent cross into restricted writing only through explicit existing paths | No new restricted-writing/native evidence was produced in this checkpoint; remains pending. |
| W36 | 15 | One explicit request, visible model/scope/status, no generation on navigation or save | Provider-command integration is underway in `workshop_generation_commands.rs`; native generation/lifecycle evidence is pending. |
| W37 | 15 | Independent manual saves; late response stays alternative and requires explicit refresh | Save/retry paths exist, but retry payload/scope projection review remains in progress; native late-response evidence pending. |
| W38 | 15 | Partial/failed/stopped distinct; retry/recovery no blind provider replay; cost wording | Retry/recovery work is present but not finally verified; provider, stop, partial/failed, and cost qualification remain pending. |
| W39 | 15 | Offline manual development, preferences/history/organization and restart resume | Manual Workshop UI, scoped preferences, and local history are implemented; Rust schema-35/restart/provider qualification is pending. |
| W40 | 16 | Source-bound facets, stale-source refusal, no second truth database | Existing-document source binding is implemented in core Workshop adoption/relationship paths; stale-source and full source-bound qualification remain pending. |
| W41 | 16–17 | Atomic multi-target adoption, dependent creation, stale refusal, no chapter mutation | Rust schema-35 actor/adoption is compiling at checkpoints; atomic new-document/dependent creation and final stale/no-chapter verification remain pending. |
| W42 | 17 | Exportable/importable editable project presets with explicit adoption of preferences | Import/export UI and preset review are implemented; native adoption and quality evidence pending. |
| W43 | 18 | End-to-end behavioral acceptance scenarios, including hard conflicts and secret isolation | Frontend build/fixture evidence exists; no new native run or full end-to-end quality qualification has been performed. |
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
