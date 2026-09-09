# WebnovelStudio V3 product requirements

## Current chat-first development status — 9 September 2026

The opt-in project conversation is now the active chat-first development
surface. It supports project questions and chapter feedback in one conversation
while preserving the exact chapter and selected scope for chapter requests.
Assistant output remains isolated as editable task drafts. The author reviews
exact before/after material and explicitly adopts one draft or a grouped set of
up to three nonchapter drafts; scoped question/assumption decisions and
read-only conversation history remain separate from story truth. The existing
workspace stays the default while native and formative qualification continues.

The conversation can attach an exact source from the Writer, stage an explicit
chapter handoff, and show the original request, assumptions, affected documents,
complete before/after bodies, and deterministic diff before Apply. Its
project-scoped resizer is keyboard accessible; small screens show one visible
surface at a time.

The current project reader floor is **schema 40**. Schema 39 adds document
roles and schema 40 adds project conversation, immutable conversation items, and
assistant-draft provenance. See [ADR 0034](docs/ADR_0034_PROJECT_CONVERSATION.md)
and the [chat-first implementation status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md).
Current evidence is development-only: live providers, installed-release
behavior, physical small-window/accessibility behavior, narrative quality, and
human author evaluation remain separate open gates.

The authoritative [chat-first implementation status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md)
maintains current test counts, native evidence, live-provider evidence, and
build identities. A local native 200% zoom geometry check is verified. Human
formative evaluation, screen-reader behavior, installed-package behavior, and
broader provider/narrative qualification remain open; chat stays opt-in.

The review inventory is complete rather than capped at an arbitrary first page;
older conversation timeline entries load through paging, stale draft refresh is
explicit, and unchanged old adoption previews remain blocked after source
changes.

**Status:** native development application with persistent English writing, project management, discussion, scoped Apply, context inspection, bounded Codex integration, an implemented Story Workshop development surface, an opt-in chat-first project conversation, an opt-in C6 story lookup development route, author-only exact chapter review, schema-19 story continuation, schema-20 reviewed export, schema-21 passage-backed reviewed story details, schema-22 structured paragraph/whole-chapter suggestions, schema-23 passage-backed promise history, C4-A chapter navigation memory, C5-A evidence history, and schema-8 V2 import. Full V3, author-trial, and release gates remain open, 9 September 2026. Current evidence and qualification boundaries are maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md).

WebnovelStudio is an AI writing application with the author directing, reviewing, and accepting the work. The author manages several projects, develops story material in any order, asks the AI to draft chapters or develop ideas, and gives whole-document or selected-text feedback. Manual writing and editing remain available throughout.

Story Workshop supports the three planned development slices: explore and
combine directions, connect material through reviewed adoption, and develop
what-if alternatives, voice samples, and future story possibilities. Questions,
intended payoffs, and possible arcs remain editable author intentions, separate
from established events. Reader experience and content intensity have distinct
optional preferences. Notes organization preserves its source in a separate
Notebook exploration. Editing and navigation never start generation.

See the [Workshop completion record](docs/V3_STORY_WORKSHOP_COMPLETION.md) for
current functional contracts, verification, and build identity, and the
[requirement ledger](docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md) for the complete
specification mapping. Full acceptance still requires current native and
provider qualification and actual author evaluation; source coverage and
synthetic tests do not establish those outcomes.

**Workspace direction, 7 September 2026:** each project has persistent Chapters,
Worldbuilding, Characters, Plot & themes, and Notes tabs. Chapters share one
writing workspace with an ordered chapter list, previous/next navigation, the
manuscript, and the writing assistant. The other tabs organize their own saved
material and offer AI development with explicit proposal review. The main loop
is **direct → generate → review → apply or steer again**, without compulsory
worldbuilding or character setup. Opening a project or tab never starts paid
generation. Reasoning effort and Fast/service tier are visible beside the
active model; provider configuration stays in Settings.

**Provider scope, 7 September 2026:** Codex is the primary use case. Finish the
Claude checkpoint and retain OpenAI-compatible endpoints; further adapter ports
are deferred at the author's request.

**Story Workshop development surface, 7 September 2026:** Develop provides six
author-room lenses—Overview, World, People, Themes, Possibilities, and Notebook—
for saved briefs, working versions, typed alternatives, selected details,
questions, scoped preferences, and local relationships. Provider responses remain
alternatives until the author explicitly uses a version. Story Bible shows chosen
document-backed material; documents and immutable revisions remain the story
authority. Schema-37 Rust storage and actor boundaries cover Workshop persistence,
reopen/replay, atomic multi-target adoption, stale refusal without partial writes,
chapter isolation, restricted-context secret exclusion, and local hard-preference
conflicts. One bounded headless Codex request and its persisted reopen are
verified, and the fresh installer passed its synthetic installed write/reopen
and same-version retention lifecycle. An earlier 15-check Workshop native flow
also passed with the local mock, including atomic linked adoption, Unicode
reopen, and voice guidance without automatic adoption. The latest
relationship/taste changes have focused local and headless evidence; the
current notes-organization slice adds the independent parentless notebook,
three editable organization candidates, preference cloning, duplicate-start
reconciliation, and stale-result fencing described above. Its SQLite evidence
proves frozen compact brief and exact notes, no parent prose/tray/selection
leakage, explicit adoption to a new note in the fixture, unchanged source and unrelated
manuscript heads through reopen, and exact original-packet replay. This is
synthetic contract evidence, not narrative-quality evidence. Full
specification acceptance, broader native/provider coverage, and author
evaluation remain open. The previous relationship slice adds no-call
directional relationship exploration with exact endpoint heads and a
two-or-three-treatment noncanon moment contract; schema 36 preserves older
Workshop state, context, packet bytes, and hashes.

W23 adds optional Unicode names, aliases, and transliterations in the existing
`document_aliases` metadata (no schema change) on character/world documents
through the World/People saved-material picker and
Writer **Names & aliases** surface. Saving is explicit, source-epoch CAS
checked, reconciled by read after uncertainty, and protected against dirty
navigation/close; title/body remain unchanged and aliases stay out of restricted
context. Component focused checks pass 11, Workshop checks pass 28, and
Writer/session checks pass 42. The aliases full wrapper passes 763 Rust tests
(70 core unit, 620 grouped integration, 73 desktop; one intentional subprocess
ignore), 534 frontend tests in 48 files, 11 tooling checks, formatting, strict
Clippy, TypeScript, and production build; the pre-existing large-chunk warning
is the only noted warning. The source-final headless fixture at
`.local/workshop-aliases-qa/report.json` passed 1440 and 800 pixel checks for
Unicode/transliterations, dirty navigation refusal, exactly one lost-ack
current-read confirmation, close/reopen, and no AI or manuscript calls, errors,
or overflow. The latest hosted native run reached 19 Workshop groups before the
aliases textbox timing assertion described in the current evidence above; no
alias-loss evidence or broad native pass is claimed. Full specification
acceptance and author evaluation remain open. On source
`e61640a4738128b9744919275e362e402c7ed0d8`, CI 34145255173 reached 21 groups
before a W30 sidebar-close harness failure; later auxiliary suites were skipped,
so no broad native pass is claimed.

**Provider direction (development surface):** Codex compatibility is checked
against the installed CLI at connection time and recorded per request; V3 must
not pin a Codex version or executable hash because Codex updates frequently.
Background summary and native Codex maintenance jobs use GPT-6 Astra with
low reasoning; configured HTTP Story Memory uses the same Astra/low request
without a service tier. Author-facing writing and revision uses the persistent
V2-style provider rail, model search, favorites, keyboard selection, and
separate traits. Native startup for a fresh library or saved Codex selection, and an explicit Settings check, run the bounded interactive
app-server model discovery and stores a sanitized, display-only catalog; the
current connection authorizes the exact selected model and resolved traits for
`codex-stdin.author.v1`. Missing models or traits remain selected and
unavailable until the author repairs the choice. Settings also supports
configurable OpenAI-compatible endpoint profiles, and the native HTTP worker
handles their development transport. Story Memory has a separate CAS-backed
provider preference with fixed Astra/low settings: Codex uses priority, HTTP
has no service tier, and mock is an explicit offline choice.
Claude author development exposes static Fable 5, Opus 5, and Sonnet 5 rows with
low/medium/high/xhigh/max effort choices; its unchecked native connection keeps
sending blocked, and Claude does not provide memory or lookup. HTTP context
lookup is not supported yet; V2 CLI adapter parity, hosted HTTP qualification, and
broader live-provider support remain open. See the
[OpenAI-compatible endpoint ADR](docs/ADR_0023_OPENAI_COMPATIBLE.md) and the
[dynamic Codex models ADR](docs/ADR_0024_DYNAMIC_CODEX_MODELS.md), the
[Story Memory provider ADR](docs/ADR_0025_API_STORY_MEMORY.md), and the
[Claude author ADR](docs/ADR_0026_CLAUDE_AUTHOR.md).

- **Installed desktop:** a native application window with native menus/dialogs and an installer. Rust owns the core and the web UI runs inside Tauri. An externally opened browser does not satisfy this requirement.
- **Project management:** creating, opening, finding, renaming, duplicating, archiving, switching, and resuming projects are central flows. Samples are explicitly chosen; a blank project remains useful without a model connection.
- **Free creative order:** an author may begin with a world, protagonist, theme, hook, scene, chapter, or ordinary note. These are optional entry points, not required stages.
- **AI drafting with author control:** chapter drafting and development of story material are primary actions. The manuscript, proposal review, and steering conversation are central. Plain language, familiar editing, persistent position, and recoverable saves take precedence over exposing backend workflow stages.
- **Whole-chapter feedback:** a chapter can have a persistent chat. Feedback is requested explicitly; creating or saving a chapter does not start a paid critique.
- **Selected feedback:** words, sentences, paragraphs, and larger selections can be discussed through a toolbar, context menu, and keyboard alternative. The UI shows the captured quotation and the possible edit scope.
- **Author control:** an explicit edit request produces a reviewable suggestion. Apply changes the manuscript; Reject leaves it unchanged. Discussion is not canon and never silently authorizes a whole-chapter rewrite.
- **Local ownership:** manual writing works offline; projects are portable and recoverable. A working manuscript is distinct from revisions, accepted story records, and export history.
- **Save recovery:** if project saving fails, the author can save the current editor text as a new Markdown recovery copy in another folder. This separate document copy does not mark the project saved or include its discussions and history.
- **Refresh protection:** browser refresh shortcuts and the native Reload menu item are disabled in the Windows app so they cannot discard a live editor buffer by bypassing save/close handling.
- **Normal close:** closing saves current writing and offers Stop replies and close or Stay open when AI work is active. Completed results that still need a local save keep the application open. Closing never starts another model request.
- **Explicit model choice:** recognizable model names and supported traits remain visible. Settings own credentials and configuration. There is no silent provider or model substitution.
- **Accepted narrative summaries:** optional summaries join exact chapter review through an explicit author decision. Generated memory can be copied as editable starting text. Accepted summaries retain their original source and earlier reviewed basis, and appear separately from generated digests in context inspection.
- **Honest context:** the product distinguishes evidence stored in the project, permitted sources available for lookup, the packet actually delivered to a model, and what the model appears to understand (an evaluation question). Context maintenance does not run paid analysis on autosave, automatically write canon, or replace source text with a large rolling summary.
- **Language and genre:** English is the authoring, UI, and export language. Translated-webnovel, wuxia, and xianxia register or terminology may be optional style support; Chinese authoring is not a product requirement, and no genre's stages, chapter lengths, or schedule are mandatory.

The current project database reader floor is schema 40, and the library model
preferences/endpoint profile schema is 4. Schema 27 adds the reader-floor
boundary for author-selected Codex bindings; schema 28 adds the frozen
source-title reader boundary, schema 29 adds optional memory HTTP delivery
receipts, and schema 30 adds nullable Claude reported-model evidence. Schema 31 fences author-room structured development records from older readers. Schema 32 adds optional immutable accepted narrative summaries to reviewed stages and bundles; schema 33 adds reviewed character knowledge, and schema 34 adds the reader boundary for typed reviewed-memory lookups. Legacy
Codex packet bytes and hashes, plus absent legacy delivery
values, remain preserved. Schema 35 adds the durable Story Workshop state,
immutable Workshop snapshots/adoption previews/receipts, and reader validation
for those tables. Schema 36 raises the reader floor for optional typed
relationship context without rewriting stored Workshop state/context bytes or
hashes. Schema 39 adds immutable document roles and schema 40 adds the
per-project conversation, immutable conversation items, and assistant-draft
provenance. The library catalog stores sanitized
model metadata, defaults, observed CLI identity, and discovery time, without
credentials. Cached catalog rows do not establish readiness.

## Current native surface

The native Library/Workspace supports blank projects and optional chapter, character, world, theme, hook, scene, and note documents. It connects rich-text editing to Rust-owned SQLite autosave, flush-before-switch, rename, duplicate, archive, native folder/backup/recovery dialogs, saved versions, and exact Markdown/TXT export previews. Current author projects use schema 40, retaining schema-19 continuation, schema-20 reviewed-export, schema-21 reviewed-evidence, schema-22 structured-suggestion, and schema-23 promise-history records; schema 24 adds bounded lookup invocation and read records, schema 25 adds optional observed provider runtime identity, schema 26 adds optional HTTP delivery receipts for provider results, schema 27 adds the reader-floor boundary for dynamic author Codex bindings, schema 28 adds frozen source-title validation, schema 29 adds optional memory HTTP delivery receipts, schema 30 adds nullable Claude reported-model evidence, schema 35 adds Story Workshop storage, schema 36 raises its reader floor without rewriting historical Workshop state/context bytes or packet hashes, schema 39 adds document roles, and schema 40 adds project-conversation and assistant-draft records. Current local and native evidence is maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md); the dated checkpoints below remain historical evidence. Recovered projects have independent identities and operation namespaces. Copied history cannot authorize new operations.

The Claude author development surface uses the existing discussion ownership and proposal Apply paths for Discuss, Propose edits, and Continue. Each `claude-stdin.author.v1` binding freezes observed CLI identity, the selected static model, and exact effort with 24 KiB stdin and 64 KiB retained output caps; it does not pin a future version or executable hash. Claude has no memory or lookup route. Completed results require an exact requested/reported model match; unknown or mismatched identity fails safely while retaining the raw result, and usage/effective identity remain unknown. Stop, failed local saves, and recovery never replay a Claude request. Native picker and synthetic transport checks pass; live Claude generation remains unqualified. See the [Claude author ADR](docs/ADR_0026_CLAUDE_AUTHOR.md).

The persistent assistant supports whole-document discussion, selected-passage feedback, adopted guidance, saved discussion sources, optional approved writing briefs, reviewable single-line passage suggestions, and explicitly scoped structured paragraph or whole-chapter suggestions. JavaScript prepares editor transactions; Rust validates scope and atomically accepts Apply. Other suggestions become stale after an intervening manuscript change. Manual rebind and atomic batch Apply remain open.

Author-only chapter review is available as the first F2-A development slice. It stages an immutable exact saved chapter, previews the exact earlier selected revisions, and records an explicit **Mark this version reviewed** decision without changing prose. Review remains optional for writing; a saved stage can be resumed after restart, changed earlier selections are surfaced as unavailable, and recovered or duplicated copies clear active review heads. The schema-21 extension optionally attaches a complete set of passage-backed possession observations to that review, with explicit audience, exact quotation, identity choice, inheritance, and clear semantics. The F2-B core slice can freeze that exact reviewed prefix together with the current working target over IPC, preserving immutable reader positions and historical namespace fencing; restricted delivery projects only reader-approved records. These records are reviewed evidence, not complete canon or state extraction. See the [author review contract](docs/ADR_0012_AUTHOR_REVIEW.md), [reviewed context contract](docs/ADR_0013_REVIEWED_CONTEXT.md), and [reviewed story evidence contract](docs/ADR_0018_REVIEWED_STORY_EVIDENCE.md).

C5-A is a partial evidence-history slice. A working freeze batches current selected review sets once per context request, preserves selected chapter order, and omits unreviewed gaps or stale earlier prose. The author can explicitly reuse a project entity across chapters, then open authenticated history for that object or promise. Restricted history filters private records before labels or results, preserves unknown and incomplete observations, and does not infer a current holder, an unrecorded transfer, or a promise resolution from absence. Promise record filtering does not hide source prose that is independently eligible under the request policy. Promise history's current native, bounded live, and CI evidence is maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md); broader live-provider and author-trial qualification remain open. See the [evidence history contract](docs/ADR_0019_EVIDENCE_HISTORY.md) and [promise history contract](docs/ADR_0021_PROMISE_HISTORY.md).

The committed schema-19 story-continuation slice lets an author choose Working or Reviewed basis, receive one restricted typed append-only candidate, edit its paragraphs, and Apply or Reject it through the existing atomic operation. The prior [CI run 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179) records the earlier continuation checkpoint; broader provider, native, and release qualification remains open. Schema-20 reviewed export reuses the exact reviewed ReadyBundle and export receipt path, remains an author-reviewed snapshot rather than canon or publication, and is CI-qualified as a development slice; strict CI 34010306332 passes both contract jobs and all 38 native checks.

The startup connection check or an explicit Settings check enables the bounded Windows Codex development path after compatibility-checking the installed executable and login. It performs the interactive `initialize`/`initialized`/paginated `model/list` probe inside the owned process boundary. Each author request freezes its selected model, concrete traits, sanitized model descriptor fingerprint, exact context packet, byte allowance, and ownership. The runtime streams validated text and retains terminal outcomes, reported usage, stdin delivery, and cleanup state. Missing effective settings or usage remain unknown. Stop and failed local saves never trigger automatic generation replay. The local test model remains available offline. Codex version and executable hash are recorded as request evidence when observed, but are not pins. Native Codex summary and native Story Memory maintenance use Astra/low with priority; configured HTTP Story Memory uses the same Astra/low request with no service tier. Author-facing calls follow the model picker. OpenAI-compatible endpoint profiles use explicit model choice and native credential readiness, with provider HTTP delivery receipts in schema 26 and Story Memory HTTP delivery receipts in schema 29. The Story Memory route captures its accepted configuration and private credential without persisting the key in its binding; local recovery never replays a POST. C6 checkpoint [CI 34038325733](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34038325733) passes both contract jobs, all 47 strict native checks, and the native HTTP fixture with zero live calls; the accepted current native HTTP fixture passes six synthetic POSTs with zero live calls and no page errors, while hosted/live-provider qualification remains open. Broader provider, HTTP-live, and full W8 or hosted HTTP qualification remain pending. See [the provider contract](docs/ADR_0011_LIVE_CODEX.md), [the dynamic Codex models ADR](docs/ADR_0024_DYNAMIC_CODEX_MODELS.md), the [OpenAI-compatible endpoint ADR](docs/ADR_0023_OPENAI_COMPATIBLE.md), [the Story Memory provider ADR](docs/ADR_0025_API_STORY_MEMORY.md), and [live evidence](docs/CODEX_QUALIFICATION.md).

The Story Context core implements C0 contracts, C1 exact working-basis evidence and retrieval, C2 deterministic frozen packets, parts of C3 guidance/conversation integration, and the C4-A chapter navigation memory slice. Exact references, literal/lexical search, source-only aliases, UTF-16 spans, conservative source/policy staleness, and disposable indexes are implemented. Story Memory requires an explicit Refresh action, pins one full current chapter revision, and validates `navigation-digest.v1` evidence against exact UTF-16 ranges and quoted text. The background producer is GPT-6 Astra with low reasoning; author-facing requests follow the model picker. Opening the panel, saving, and autosave do not invoke analysis or a paid call. Jobs, terminal results, and generated views are separate records; views can be marked stale or redacted after policy changes. The independent API-only Story Memory provider preference and HTTP route are implemented as development surfaces in [ADR 0025](docs/ADR_0025_API_STORY_MEMORY.md); they do not change the fixed native Astra/low maintenance route. C4-B adds automatic reuse of current non-target chapter views in working author-room discussions when full prose does not fit. C4-C keeps unchanged chapter views reusable after unrelated story edits while ordinary requests and proposals still use the global stale-basis fence. Generated summaries have separate frozen identities, coverage, and original-evidence inspection; restricted writing and reviewed continuation exclude them. The C6 development route adds opt-in, bounded `story-lookup.v1` search/read and reviewed-memory exchanges for Working, AuthorRoom, and Discuss packets, with schema-24 durable invocation/read records and per-packet inspection. The UI choice is off by default and does not widen edit permission. Reviewed-memory lookup can find explicit character/topic/object/promise identities and page their recorded histories from the same frozen source. It retains uncertainty and exact source evidence without inferring complete world truth. Current qualification is recorded in [implementation status](docs/IMPLEMENTATION_STATUS.md); broader provider, narrative-quality and restricted-writing lookup gates remain open. See [ADR 0032](docs/ADR_0032_STORY_MEMORY_LOOKUPS.md). Packets retain the exact target, instruction, edit scope, mandatory sources, and approved brief. Full eligible text is used when it fits; otherwise explicit whole-block packing records omissions. The live byte caps are application limits, not model token limits. Reading broadly never expands edit permission. Restricted prose packets exclude private author-room discussion; an explicitly approved brief transfers only its exact directions.

V2 schema-8 imports now have a native Library action. The importer previews a stable read-only source, requires a choice where working text is missing, and installs an independent V3 project with inert legacy evidence. Original V2 approvals are not promoted into reviewed V3 authority. Imports and recovery retain source/operation identities for reconciliation. See [supported imports and limitations](docs/V2_IMPORT_PREVIEW.md).

Current test, native, live-provider, and installed-package evidence is maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md). These are separate qualification gates. C4-A has local, strict native CI, and bounded live evidence. C4-B navigation packet integration, C4-C chapter freshness, and the schema-19 continuation slice have local contract and strict native CI evidence; schema-20 reviewed export has local wrapper and native development qualification, while strict CI 34010306332 passes both contract jobs and all 38 native checks. Schema-21 passage-backed reviewed details are CI-qualified as a development slice by CI 34012813796, which passes all 40 strict native checks; C5-A entity reuse, batched freeze, and evidence history are implemented as a partial slice with a passing 40/41 local native diagnostic; CI 34014694823 passes all 41 strict native checks. The schema-22 development slice adds explicit selected-paragraph and whole-chapter rich suggestions with the existing atomic Apply protocol. Schema-23 adds passage-backed promise history with reader projection and incomplete-evidence wording; its current qualification evidence is maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md). C6 has focused protocol, packet, boundary, core, frontend, and local native development evidence; historical C6 checkpoint CI 34038325733 passes both contract jobs and all 47 strict native checks, with the local diagnostic at 46/47 because it excludes the known OS clipboard check. The historical C41 checkpoint wrapper counts are dated at 593 active Rust tests (552 core and 41 desktop), one existing ignored fixture, and 353 frontend tests in 27 files; current local and native evidence is maintained in [implementation status](docs/IMPLEMENTATION_STATUS.md). The accepted Story Memory HTTP native fixture passes six synthetic POSTs and zero live calls; broader live qualification remains open. Broader live coverage, author-trial, narrative evaluation, remaining C5 state views, arbitrary partial multi-block editing, batch Apply, and full installed-release qualification remain open.

## Explicit W0 trial

The trial action is available only in development builds. The shared snapshot-validation IPC remains available in release because normal saves depend on it. Installed release builds open the persistent Library without a session-only trial entry point.

The implemented W0 surface is a native Tauri/WebView2 editor trial. It opens a built-in sample chapter, supports paragraph and heading styles, bold and italic marks, links, scene breaks, clipboard text/formatting notice, undo/redo, whole-chapter or selected-passage feedback notes, replacement preview/reject, a local replacement transaction, and Rust snapshot validation over IPC. The editor stays mounted while feedback state changes, and stale captured selections are refused after intervening edits.

The trial is not a product release or the persistent author-data path. Its sample text and feedback are session-only and disappear when the window closes. The W0 trial itself does not connect a provider or AI response, and its snapshot command does not persist the sample. The remaining English native author trial, minimum-window behavior, external Word paste, screen-reader use, broader native qualification, live providers, and installed-package qualification remain future gates. Unicode edge cases such as accents, emoji, and names are internal robustness fixtures, not a Chinese authoring feature. See the [W0 qualification record](docs/W0_QUALIFICATION.md) for executed evidence.

V3 lives in `D:\WebnovelStudio_V3`, alongside V2. Windows is the first qualification target; other operating systems, collaboration, cloud sync, and reviewed-story features remain separately gated. The design documents describe intended contracts; they do not establish native editing quality, provider reliability, continuity coverage, or literary quality.
