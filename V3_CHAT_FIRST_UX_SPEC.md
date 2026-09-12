# V3 Chat-First UX Specification

**Status:** Proposed product and interaction contract; design only.
**Baseline:** `FZWINGEL/WebnovelStudio_V3`, commit `6ab150505802b6c240db884a83f0a4eb9511df79`, requested branch `codex/v3-persistence`.
**Companion:** [Bounded implementation plan](V3_CHAT_FIRST_UX_IMPLEMENTATION_PLAN.md).

**Implementation update — 9 September 2026:** The conversation, isolated draft
review/adoption, and chapter-feedback paths are implemented in the opt-in native
build. [Current implementation and qualification status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md)
records the executed checks and remaining rollout gates. The baseline analysis
below is retained as the original design evidence, not a current test report.

## 1. Decision and evidence boundary

Make the **project conversation the default place to continue work**, with relevant documents and the chapter beside it. Conversation coordinates work; **documents, immutable revisions, explicit author decisions, and frozen context remain authoritative**. The assistant should understand an ordinary request without requiring the author to choose a lens, operation, or template.

This is an evolution of the existing native application, not an agent-platform rewrite. Preserve the manual Writer, project library, SQLite project actor, proposal preparation and Apply, review, history, source permissions, provider receipts, and recovery. Replace the mandatory Develop/Write/lens journey, not those guarantees.

### What the pinned source establishes

| Finding | Consequence |
| --- | --- |
| Discussions have document-owned threads; recent provider context selects bounded, completed exchanges from that document. | A persistent **project** conversation and its cross-document reference projection are additions, not an existing feature to rename. [S3](#s3)[S4](#s4) |
| Workshop already stores immutable adoption previews and commits multiple material targets, revisions, decisions, and a receipt in one transaction. | Reuse its atomic mechanics. Do not claim that generic chat-origin adoption or batch chapter Apply already exists. [S5](#s5) |
| Workshop output requires candidate-oriented JSON and question fields; the UI is lens/action-driven. | Merely changing prompts or adding skills cannot produce the proposed shell, history, draft isolation, or authority contracts. [S6](#s6)[S7](#s7) |
| `chooseQuestion` refuses an already-disposed question; “Start with an idea” calls it with reused wording. The notice is rendered near the end of the central main panel. | The reported “Not now” trap has a matching code path. This review did not reproduce it in a running app. Change both interaction and response instructions. [S7](#s7) |
| `DocumentSession` and Writer preserve a mounted editor and use guarded, exact prepared changes. | Streaming must never replace editor content or bypass these boundaries. [S8](#s8)[S9](#s9) |
| Project schema is 38. Earlier ADRs contain historical schema numbers, maintenance settings, and unfinished-slice descriptions. | Current source wins over old prose. In particular, old summary prose mentioning Luna does not change the fixed Astra maintenance input. [S2](#s2)[S13](#s13)[S19](#s19) |

The required architecture, first-slice, Story Context, Workshop, status, and applicable ADR documents were consulted alongside source and test definitions; the evidence register identifies the relevant boundaries. **No application tests, browser/native checks, installed application, or live provider calls were executed in this assessment.** No `.local` reports or author project data were accessed. Source inspection supports implementation findings, not release qualification. All repository links below pin the exact commit, never the default branch.

## 2. Goals, non-goals, and mental model

The author should be able to start with an attraction, note, character, world question, scene, or request to write immediately. The assistant helps turn that into useful material, proposes an organization, and maintains that material through visible revisions. It asks only questions that materially affect the current task.

The mental model is: **“We discuss my story here. These are the documents we are working on. These changes are suggestions until I adopt them.”** Opening a document does not leave the collaboration or start another assistant. Returning to chat restores the same position, references, unsent message, selected scope, and pending review.

Success is a knowingly endorsed creative decision and useful work on return—not document count, a completed questionnaire, or a high acceptance rate.

Non-goals are autonomous canon extraction, compulsory world/character/outline completion, multi-agent debate, a second orchestration framework, provider-history resume, default app-server migration, unrestricted story tools, and automatically publishing or exporting assistant output. English is the interface, authoring, and export language. Optional wuxia/xianxia, cultivation, or translated-webnovel register is a style preference, not a Chinese-authoring requirement.

“Maintain” initially means **prepare updated draft versions within the author’s requested task**, reconcile explicit changes with named related documents, and show what changed. It does not mean silently rewriting accepted documents, continuously analyzing autosaves, or manufacturing facts to fill empty fields.

## 3. Everyday shell and navigation

### Desktop

Use two principal surfaces, not a permanent transcript plus two crowded sidebars. The right surface contains a document, a grouped review, or the chapter. Its compact document switcher exposes other relevant material. Opening the chapter can give the editor more width without changing the conversation identity.

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [Library] [Untitled project ▾]  [New/Open]        [Model ▾] [Effort ▾] [Fast]  │
├──────────────────────────────────────────────────────────────────────────────┤
│ Current request: Draft ready · NOT ADOPTED          [Review 1 draft]          │
│ Author room · 2 saved sources · No chapter changes  [Sources & scope]         │
├──────────────────────────────────────┬───────────────────────────────────────┤
│ PROJECT CONVERSATION                 │ DOCUMENTS                     [Find] │
│                                     │ [Related ▾] [Chapter] [All documents] │
│ You: Help me develop this idea…      ├───────────────────────────────────────┤
│                                     │ World sketch · Assistant draft v2     │
│ Assistant: I’ll assume…              │ Based on: Notes r3                    │
│ One useful question, when needed.    │                                       │
│                                     │ Readable, editable document           │
│ ┌ Draft: World sketch ─────────────┐ │ or Before / Proposed review           │
│ │ What changed · assumption       │ │                                       │
│ │ [Open review] [Reject]           │ │ [What changed] [Full document]        │
│ └─────────────────────────────────┘ │ [Adopt this version] [Reject]         │
│                                     │                                       │
├──────────────────────────────────────┤                                       │
│ [World sketch r2 ×] [Selected text]  │                                       │
│ Ask, write, or request a revision…   │                                       │
│ Unsent message saved        [Send]  │ Document saved locally                │
└──────────────────────────────────────┴───────────────────────────────────────┘
```

The current-request strip and composer remain visible independently of transcript scroll. Its compact state is also present while reviewing a document. Detailed diagnostics may expand; **errors, uncertainty, stale status, scope, and the next useful action may not exist only inside collapsed content**.

The project picker shows the current project prominently, recent projects, and activity/draft badges. Library create/open/duplicate/recover remain native operations. A new project opens an empty conversation with optional shortcuts: **Bring a note**, **Open a blank chapter**, **Browse documents**. None requires a genre, title beyond an editable default, or preparatory form. These shortcuts prepare a surface; they do not submit a provider request.

Document browsing remains available independently of chat. Present **Related**, **Drafts to review**, and **All documents**, plus the existing chapter order. Categories may filter documents, but they are not steps. The assistant proposes titles and placement; the author can rename, edit, pin, or reorganize without learning internal `kind` values. A plan can use the existing note/scene/hook representations; do not invent an implemented `plan` storage kind. [S2](#s2)

### Narrow window and accessibility zoom

```text
┌────────────────────────────────────────┐
│ [Project ▾]       [Model/traits ▾]      │
│ Draft ready · Not adopted   [Review 1] │
│ Author room · Notes r3       [Sources] │
├────────────────────────────────────────┤
│ [Chat] [Documents 1] [Chapter]         │
│                                        │
│ One active surface, same project.       │
│ Review opens Documents; Back returns   │
│ to the exact chat message and scope.   │
│                                        │
├────────────────────────────────────────┤
│ Chat: saved composer / Send             │
│ Review: Adopt / Reject / Return to chat │
└────────────────────────────────────────┘
```

Proposed breakpoints are approximately 1,100 CSS pixels for the comfortable split and 800 for a single surface; qualify them with actual content, minimum native-window dimensions, and 200% zoom. No horizontal transcript scrolling. Preserve a visible request badge while the editor is full width. A drawer must restore focus and never hide an unresolved save/Apply outcome.

## 4. Material status and authority

Use separate **material status**, **evidence basis**, and **audience** labels. A single green “approved” badge would incorrectly collapse different guarantees.

| Label | Meaning and permitted transition |
| --- | --- |
| **Assistant reply / proposal** | Conversation output or a proposed change. Not a story fact, durable instruction, or permission to edit. |
| **Assistant draft · Not adopted** | Versioned, editable material saved automatically for the requested task, isolated from accepted documents. Editing it manually still does not adopt it. |
| **Working · Author-adopted** | The author explicitly incorporated an exact preview into a source document. Subsequent direct author edits create newer Working heads. Adoption is not chapter review. |
| **Draft prose · Working** | Manually written or explicitly Applied chapter prose. It may be used under the existing Working writing rules; it is not automatically Reviewed. |
| **Reviewed evidence** | An exact chapter/review bundle or explicitly reviewed evidence/summary with its source and audience. This label is never inferred from approving a planning draft. |
| **Author intent** | The current author request, or explicitly retained/adopted scoped guidance. Intent may describe a future possibility; it is not evidence that an event happened. |
| **Source document / canonical basis** | The exact saved document revision selected for the relevant task or reviewed basis. “Canonical” is not a new global flag and does not bypass disclosure or review rules. |
| **Rejected / superseded** | Retained for inspection and provenance; excluded from automatic model context. Reconsidering creates an explicit new candidate/reference, not a reversal hidden in chat. |
| **Private · Author room** | An audience restriction that can coexist with any appropriate material status. Adopting a private fact does not make it eligible for chapter writing. |

These distinctions extend the existing Working/Reviewed, source eligibility, proposal, and generated-memory boundaries. [S4](#s4)[S10](#s10)[S11](#s11)[S13](#s13)

### What may be saved automatically

Within a sent task such as “organize these notes” or “develop the character,” the assistant may produce and organize isolated document drafts. Completion and local persistence are separate visible events: **Response received → Saving draft → Draft saved, not adopted**. A plain informational answer need not create any document.

The author does not approve each field. A draft contains prose, a small assumptions section where useful, and source references. The app may choose a proposed title/type and place it under Drafts. It must not rename, move, delete, replace, or canonize existing accepted material automatically.

A request concerning one document normally produces one revision proposal. A request that necessarily changes related material produces a grouped proposal with every affected document visible. The first implementation deliberately supports **one nonchapter document draft per request**; a second linked request proves continuation. Later grouped material review is bounded initially to three targets. Do not pretend several independently Applied changes form one atomic group.

Manual typing in a Working document remains a direct author action using normal saves. It requires no AI-adoption dialog. Manual editing of an assistant draft saves the draft’s own revision. An assistant response never overwrites either live buffer.

## 5. Conversation behavior and question policy

Default to **zero to two useful questions per request**, not a required question at the end of every answer. Proceed with a few explicitly proposed assumptions when answers are not necessary. A question must explain the decision it affects, preferably in one sentence. “Who is every character?” is not a prerequisite to drafting a scene.

Task-specific assumptions appear with the draft and remain provisional. “I’m assuming a contemporary voice and an adult protagonist for this sketch” does not become project-wide guidance. The author can edit or reject the assumption directly or say “make the protagonist younger”; the next draft records that change. Persistent preferences use explicit existing guidance/preference adoption, with their scope shown.

| Author action | Required meaning |
| --- | --- |
| **Not now** | Defer this question. Do not ask it again automatically during the current task; continue other useful work. It never disables Send, a new idea, or an explicit request addressing that subject. |
| **Not relevant** | Stop proposing this question within its recorded story/task scope. No global veto on ordinary author requests. Show its scope and a one-action reversal. |
| **Keep mysterious** | Preserve uncertainty. Ask whether it is unknown to the author, the reader, or both only when that distinction affects this work. Do not invent an answer because a template has an empty field. |
| **Change my mind / Reconsider** | Reopen the explicitly referenced question or candidate and supersede its disposition with provenance. Accepted document changes still require a new reviewable preview and adoption. |

A fresh request has its own identity and does not inherit a previous question’s “blocked” status. An explicit “Let’s decide that now” reopens that question; a different request need not reopen it at all. Ambiguous assent must not change document authority. The current Workshop test that requires manual reopening before exploring the same question must change; persistence of dispositions and intentional uncertainty must remain tested. [S7](#s7)

One clarification round is the default before producing something useful. Further questions require a specific blocker, such as identifying which of two documents to replace or choosing a safe writing boundary. The interface may explain such a blocker; it may not silently refuse a valid request or enforce “all fields complete.”

## 6. Compact request and review model

This is a UI projection of existing run, save, and proposal records—not a new orchestration state machine.

```text
Unsent message
  → Preparing: save exact inputs; resolve scope; freeze permitted context
  → Accepted / Generating: durable run exists; provider work is separately owned
  → Saving / Validating result
  → Reply only OR Draft ready / Review ready
  → Adopted OR Rejected (or retained for later)

Orthogonal visible conditions:
  Unsaved locally · Sources changed · Stop requested · Interrupted
  Start outcome unknown · Result save failed · Adoption outcome unknown
```

Before dispatch, show **Author room** or **Chapter writing**, source count, target/scope, and requested provider/model/traits. Detailed source inspection is one action away. Changing model or traits during a run affects the next request only. The current run keeps its frozen requested identity and separately reported/effective values; unsupported Fast/effort choices are unavailable with an explanation, not silently remapped. A failed readiness check preserves the saved choice and offers connection checking without generating. Explicit source links, pinned documents, and the active document inform the app’s bounded source selection; browsing all documents does not authorize sending all of them.

Sending freezes the exact request, referenced revisions, current policy, permitted history, provider binding, and run identity. Saved inputs are required; a failed preflight does not send an unsaved buffer as though it were durable. The author may immediately continue typing in the composer or editor while generation runs. Changes belong to a later generation, not the frozen request.

Display genuine provider chunks when available and a clear activity state otherwise. Exec’s first visible output may be a completed message; do not simulate token streaming or silently switch transport to manufacture responsiveness. Incomplete structured output is never an applicable edit. The response and validated document draft may become visible at different times. [S12](#s12)[S14](#s14)

Default to one active conversation generation per project, without a hidden queue. Other projects keep their existing independently owned work. Typing the next message is always possible; sending it during active work requires Stop or waiting for settlement, with the reason visible.

## 7. Preview, adoption, and revision contract

### Review experience

The review panel starts with **What changed**, the author’s request, assumptions, and a list of affected documents. It provides complete readable proposed content, exact before versions, and a diff. “Preserved the ending” must be backed by the scope/structural check when possible; a model’s claim alone is not a guarantee.

One **Adopt this version** or **Adopt all N documents** action approves the identified preview, not every future assistant edit. Reading every tab is not a compulsory checkbox. The complete preview must nevertheless be available before the action. **Reject** leaves Working material unchanged. Closing review retains it as pending.

Editing a preview creates a new version. The previous preview remains historical; any approval bound to it is invalid for the edited version. Removing a target from a group requires a new preview and dependency validation, not partial execution of the original group.

### Required immutable preview manifest

Every adoptable preview identifies its project and operation namespace; preview ID/version/hash; originating run and packet; exact draft document/revision/head; affected existing target heads or reserved new target IDs; complete before and proposed bodies through immutable revision references; target metadata and its expected versions, including relevant order/relationship versions; source revision/head dependencies and policy fences; scope and protected content; candidate/disposition provenance; and any proposed relationships, impacts, or supersessions.

The displayed digest is a convenience, not authority supplied by the renderer or model. Rust validates the complete recorded manifest and bodies. Preview reads must not resolve “before” to today’s newer document. [S5](#s5)[S9](#s9)

### Atomic commit

After the author acts, the app briefly joins affected editor lifecycle guards, finishes composition, flushes local edits, and checks their heads. Within **one SQLite transaction**, validate all target/source heads, draft versions, ownership, policy, scope, protection, and preview identity before writing. Record all new Working bodies/revisions, adoption decisions, provenance, applicable relationships, one source-epoch change, and the idempotent receipt together. Either every target commits or none does.

Current Workshop provides this transaction pattern for material adoption. Generic chat-draft provenance and the new review surface still need integration and qualification. Chapter `Proposal` Apply remains its own scoped contract; mixing chapter Apply with a material group is deferred. [S5](#s5)[S9](#s9)

Any conflicting author edit, changed source, changed draft, rejected candidate, or revoked permission blocks adoption. Preserve the old preview, explain exactly what changed, and offer **Compare** and **Prepare against current versions**. Rebase is an explicit new preparation/request with a new preview and fresh approval; never rewrite the old proof or silently replay a model call.

### Conversational approval

The first slice uses the review button. A later conversational shortcut is safe only when an author-origin action names or explicitly attaches the **exact active preview ID, version, and digest**, such as “Adopt preview P7 version 2.” The application resolves it deterministically and invokes the same adoption contract. It must not ask a model whether “sounds good” authorized unspecified edits. A stale, historical, ambiguous, or changed preview cannot be approved this way. Approval receipts record the author message/action and the exact preview.

## 8. Context inspector and writing boundaries

The inspector answers: **What can influence this request, what actually did, and why?** Distinguish prepared input from confirmed delivery and final lookup packets. Show exact document/revision identities with friendly titles, source heads/hashes, selected text or block scope, source/policy epochs, Working versus Reviewed basis, reader frontier/available POV permissions, required sources, omissions, and supplied conversation turns. Separate proposed assumptions, adopted guidance, approved writing brief, reviewed summaries, and generated navigation aids. [S4](#s4)[S10](#s10)[S12](#s12)[S13](#s13)

Do not label available material “read by the model.” Show **Used**, **Available but not supplied**, **Excluded**, and **Changed since request** with reasons. Full source evidence remains inspectable locally; the provider receives only the eligible projection. Restricted packets must not leak excluded titles, private references, or private transcript snippets through the inspector’s serialized data.

### Author room

Planning may use permitted Working documents, explicitly included drafts, scoped intent, and selected conversation evidence. Private facts and future possibilities stay visibly private/tentative. Rejected and superseded candidates are excluded by default even when their words occur in old assistant messages. A current request to compare a rejected option can include it explicitly as an alternative, without reinstating it.

No assistant assumption or summary updates accepted facts automatically. Adopting a world/character/plan document changes Working source material. Explicitly adopting guidance changes scoped intent. Reviewed evidence and accepted summaries enter through their existing exact review contracts. Generated memory remains an unreviewed navigation aid. [S10](#s10)[S11](#s11)[S13](#s13)

### Chapter writing

A chapter request compiles a **fresh restricted packet**, even though it appears in the same project conversation. Use the current Working target and the existing eligible Working sources, or an explicitly chosen valid Reviewed earlier prefix. Do not require review of every planning document. A first chapter can use Working; the Reviewed continuation path needs its own valid earlier basis and must not silently fall back. [S10](#s10)[S11](#s11)

Private planning enters only through existing eligibility rules or an optional **author-approved writing brief**. Show its exact text and what it omits. Approving a brief is not adopting facts. The private origin message, rejected candidates, future notes, and raw project transcript do not accompany it into the writing packet.

The current brief contract validates an origin in the same document. A project-chat origin therefore needs a narrowly versioned cross-document provenance extension before this shortcut ships; do not strip the origin ID to evade validation. Until then, preserve the existing chapter-side brief workflow rather than pretending automatic transfer works. Editing brief text or changing its target/scope clears confirmation. [S15](#s15)

Preserve frozen POV and reader permissions. Do not expose unsupported character-grant combinations as functional; the current Reviewed continuation boundary does not support character-specific grants. A warm provider connection does not widen these permissions. Legitimate evidence or an approved brief may still imply a secret: the UI must not promise semantic secrecy the validator cannot prove. [S10](#s10)[S11](#s11)

## 9. Chapter feedback and direct writing

Chapter-wide **feedback** is discussion, not whole-chapter edit permission. Selected words and sentences retain exact quotations, block IDs, offsets, source head, and scope. Selected paragraphs use the existing explicit block scope. An absent selection never means “whole chapter,” and a partial selection is never silently expanded. [S9](#s9)[S16](#s16)

For “make this confrontation more emotional, but keep the ending,” the assistant uses an attached valid selection. Without one, it proposes a clearly identified range and asks the author to confirm the range—not to fill a questionnaire. Prefer selecting the confrontation blocks while leaving the ending outside scope, so the existing structural validator protects it exactly. Arbitrary protected islands inside a whole-chapter replacement are not assumed to be implemented.

The review shows the original passage, proposed edit, protected surrounding content, and exact source version. The author can discuss that proposal in chat with its scope chip intact. After Apply, retain the original quote as historical provenance; another edit requires a fresh capture against the new head. Do not reuse an old grant merely because the words still look similar.

Manual writing stays first-class: open a blank chapter, type, save, undo/redo, compare history, export, or ask for feedback at any point. The editor remains mounted while chat updates. Only short save/Apply/reconciliation boundaries may restrict affected input; provider generation never holds the editor barrier. [S8](#s8)[S9](#s9)

## 10. Failure, interruption, and multi-project behavior

| Situation | Visible state and required action |
| --- | --- |
| Local composer/document save fails before Send | **Not sent — local save failed.** Retain typing; offer retry saving and existing recovery-copy behavior. Never imply persistence before an acknowledgment. |
| Start acknowledgment is lost | **Request status unknown.** Keep the operation ID/payload and reconcile the existing run. Disable duplicate Send for that operation; no new provider submission to “check.” |
| Response received but terminal/draft save fails | **Response needs local saving.** Retry only the retained local result/materialization. Current native retention survives renderer replacement, not process loss; saved prefixes survive according to durable records. |
| Stop | **Stopping this request** then confirmed local terminal state or uncertainty. Retain partial output as non-applicable. Stop is neither Undo nor proof that upstream processing/charging stopped. |
| Source changes during streaming | Immediately show **Based on older sources**. Keep streaming only as an explicitly stale result where policy permits; never overwrite the author’s newer work. Adoption is disabled. |
| Policy revoked during work | Stop further authorized reads/dispatch/adoption and request cancellation. Already delivered data cannot be recalled. Preserve audit evidence under the existing inspection rules. |
| Adoption acknowledgment lost | **Adoption outcome unknown.** Fence affected mutation controls; inspect the exact receipt and current heads. Never present a second Apply as an ordinary retry. |
| App closes normally | Save composer/draft/editor state through existing guards. For active work across projects, show **Stop replies and close / Stay open**. Unresolved retained result writes block closing. |
| App/process terminates unexpectedly | Reopen durable conversation, drafts, packets, and decisions. Mark orphan work interrupted; do not resume upstream threads or submit automatically. Unsaved buffers or only-in-memory terminal output may be lost. |
| Reconnect or explicit new attempt | Distinguish **Retry local save**, **Check saved outcome**, and **Send a new request**. New generation has a new ID and visible cost/uncertainty implications. |

These semantics reuse existing runtime admission, owner-scoped cancellation, recovery retention, and `DocumentSession` reconciliation. An identical local write may be retried after a lease fence proves it absent; that is not permission for blind replay or provider resubmission. If the receipt exists but newer prose also exists, compare without restoring an older applied body over it. [S8](#s8)[S14](#s14)[S17](#s17)

Each project has its own conversation, drafts, composer, references, scope, request owners, and review state. Switching projects flushes the relevant local buffers and leaves already-owned work running under its original project. Activity badges lead back to that project. Late callbacks cannot update the newly selected project.

Duplicate and recovered projects obtain new project/operation identities. Retain old conversation, revisions, previews, and receipts as **historical copied evidence**; they cannot authorize new Apply, review, lookup, or dispatch. Start a new active conversation section, with links to earlier work. Reusing a copied pending draft requires an explicit current-project fork/rebase. Existing review-pointer reset and source re-freezing rules remain. A project backup includes private conversation and drafts; export of a manuscript does not. [S18](#s18)

## 11. Persistent application history, not ambient provider memory

The visible conversation is assembled from durable messages/runs, exact document-revision references, draft/proposal records, and author decisions. It is not another story bible. A document chip offers **Version discussed** and **Current version**; they must not silently resolve to the same moving target.

On returning from document review, show a short event card: “Adopted World sketch r2 as World r1,” “You edited Character r4 → r5,” or “Preview P7 remains unadopted; its source changed.” These are projections of receipts/revisions, not generated summaries. Preserve scroll position and the unsent composer; no model call runs merely because the author returned.

**Initial history default:** retain durable history locally, paginate it, and compile at most four complete eligible exchanges within the existing 16 KiB history allowance and the selected provider’s tighter overall packet budget. The current same-document selector must be extended explicitly for project references. Include exact, versioned source/status evidence; omit stopped, failed, copied, policy-ineligible, rejected, or superseded material. A rejected candidate hidden in an otherwise completed reply is still excluded: omit the producing turn or use a validated, versioned answer-only projection, never blindly forward the raw transcript. [S3](#s3)[S4](#s4)

Old instructions in conversation are evidence, not permanently active commands. Current author intent and explicitly adopted scoped guidance govern the request. Referencing an unadopted draft is an explicit task-local operation; it does not admit that draft into unrelated context. Mandatory context that exceeds the application bound is refused visibly, not truncated or silently offloaded to a different provider.

No generated conversation compaction is required for the first slice. A deterministic return recap links to source events. Later model-produced summaries must identify source messages/revisions, exact scope/audience, generated status, and invalidation dependencies. Saving one is allowed as an unreviewed aid within an authorized task; durable guidance or reviewed evidence still requires its own adoption. A summary cannot launder private or rejected material into chapter context.

Do not delete immutable history to save tokens. Archiving affects navigation, not retention or authority. Pruning, redaction, and garbage collection need a separate retention design that accounts for packet/proposal references and backups. Disclose both local retention and external delivery: fresh upstream threads are not a promise about provider-side retention.

Codex remains primary; Exec remains default and bounded story lookup stays on Exec. Optional app-server reuses infrastructure with fresh threads, not author history. The author’s model, reasoning effort, and Fast/service-tier selection stays independent of **GPT-6 Astra / low** maintenance, with Codex priority and HTTP no tier. Preserve Claude and HTTP adapters, exact requested-versus-reported identity, capability errors, and no silent substitution. Native story tools and provider history resume are not part of this contract. [S12](#s12)[S14](#s14)[S19](#s19)

## 12. Complete author walkthroughs

The following examples are invented for this specification, not taken from an author project.

### Rough idea → useful documents → chapter

1. **Begin.** Create an untitled project and type: “A harbor courier receives a letter addressed to a ship that never existed. Help me develop it.” No category selection is required. The strip says **Author room · Preparing**, then shows the saved request’s scope and sources.
2. **Ask only what matters.** The assistant asks: “Should the courier’s immediate stake be protecting someone or proving the delivery is real? That changes the opening conflict.” It also says: “For this sketch, I’ll assume an adult courier and plain contemporary English.” These are proposed assumptions, not persistent settings.
3. **Proceed without completion pressure.** The author chooses “protecting someone” and marks a question about the ship’s origin **Keep mysterious — both**. The assistant drafts a compact world sketch, leaving the origin unresolved. The author can keep typing while genuine response chunks arrive. A card says **World sketch v1 · Not adopted** only after local saving succeeds.
4. **Review once, meaningfully.** The right panel shows the full sketch, source request, assumption, and “No existing documents changed.” The author edits one paragraph. This becomes draft v2. **Adopt this version** records the exact preview and creates a Working world document. Nothing becomes Reviewed.
5. **Return and maintain.** Back in chat, “World sketch v2 adopted → World r1” links to both versions. The author asks: “Develop the courier and a short plan using that, but don’t explain the ship yet.” In the first slice, the assistant drafts one character document and offers the linked plan as a next request. With grouped review enabled, it may prepare Character and Plan together; one complete atomic preview covers both. There is no compulsory questionnaire.
6. **Change accepted material safely.** The author edits the character’s motivation directly, returns to chat, and says, “Update the plan to fit my change.” Chat shows the manual revision link. The assistant drafts a plan revision based on that exact character head and identifies what changed. It does not silently “keep everything synchronized.”
7. **Keep a private possibility separate.** The author discusses a possible future explanation for the letter. It is saved only as a private, tentative draft/intent when requested; it is not an established event or chapter source. Rejecting it keeps it out of subsequent automatic history selection.
8. **Write now.** “Draft the opening delivery scene.” The chapter panel opens with **Working · Restricted writing** and an append/empty-target scope. No planning-review gate appears. The author can approve a short safe brief using only the immediate courier stake and harbor conditions; the private explanation and raw planning conversation are excluded. The cross-document origin shortcut requires the implementation-plan gate described above.
9. **Review prose.** The assistant returns a chapter proposal through the existing continuation/structured path. The author reviews and Applies it; the chapter is still Working draft prose. Chat retains the exact packet and Apply receipt.
10. **Refine locally.** Select the confrontation paragraphs, excluding the ending, then ask: “Make this confrontation more emotional, but keep the ending.” The selection chip survives discussion and review. Apply changes only the granted blocks. Returning to chat shows the changed revision and preserved historical quote; another edit uses a fresh selection.

### Existing-note path

Open a saved note or paste one into a new author-owned note. “Organize this, keeping the original” attaches its exact revision. Save pasted material before dispatch. Preserve the original unchanged and generate an isolated organized draft with a source link. Mark invented connective details as assumptions. The author may adopt a new document or explicitly replace the original through an exact preview; naming the note does not itself authorize replacement. Reopening shows both the original and the adopted/draft relationship.

### Immediate-write path

Choose **Open a blank chapter** and type, or say “Draft a tense arrival at the harbor, ending before the courier opens the letter.” Use the current blank Working chapter and a visible restricted scope. No world or character form is required. The author can write the ending manually while the assistant works; that makes the older append/replacement proposal stale rather than displacing the ending. Pure manual writing remains available with no provider connection.

### Interrupted or uncertain operation

A draft response arrives, but its local finish acknowledgment is lost. The strip says **Response outcome unconfirmed — Check saved outcome**. Reconciliation finds the durable response and creates the draft once without another call. Alternatively, when a terminal result exists only in native memory after a failed save, **Retry local saving** uses that exact result; normal close remains blocked. Forced termination may leave only the durable prefix, shown as interrupted on reopen. Neither path silently resumes upstream work.

### Source change and stale proposal

A plan revision is generated from Character r3. During streaming, the author saves Character r4. The status strip immediately identifies that source change. The completed draft remains readable but **Adopt** is disabled. **Compare sources** shows r3 and r4; **Prepare against current versions** starts an explicit new preparation/request. Even a small unrelated source edit is not silently dismissed as harmless. After adopting the new preview, chat shows its current source heads and the superseded proposal separately.

## 13. Reuse / change / retire / new

| Disposition | Current files/concepts | Reason and boundary |
| --- | --- | --- |
| **Reuse** | `projects.rs`, `library.rs`, `storage/mod.rs`, `transfer.rs` | Project actors, ownership, SQLite durability, versioning, migration backups, independent copy/recovery. [S2](#s2)[S18](#s18) |
| **Reuse** | `editor/session.ts`, `Writer.tsx`, `projects/proposals.rs`, `documents` validators | Manual writing, single editor, exact scope, immutable preview and guarded Apply. [S8](#s8)[S9](#s9) |
| **Reuse with extraction** | `projects/workshop.rs`: `preview_workshop_adoption`, `adopt_workshop` | Existing atomic material commit; new chat origins need explicit validation, not an empty-candidate/manual bypass. [S5](#s5) |
| **Reuse** | `projects/discussions.rs`, native `discussion_commands.rs`, `discussion_recovery.rs`, `provider_runtime.rs` | One run lifecycle, admission, dispatch ownership, receipts, cancellation, local-save recovery. [S3](#s3)[S14](#s14) |
| **Change** | `conversation_context.rs`, `context/conversation.rs`, `context/packet.rs` | Bounded project-turn projection with exact document/status references; no ambient transcript feed. [S4](#s4)[S12](#s12) |
| **Change** | `Workspace.tsx`, `FeedbackPanel.tsx`, `ContextInspector`, `ProposalPanel` | Shared project conversation and review panel; retain chapter capabilities and receipt inspection. [S7](#s7)[S9](#s9) |
| **Retire as primary UI rules** | Develop/Write choice, mandatory lens/action entry, repeated three-direction output, deferred-question entry gate | They obstruct ordinary requests. Keep old explorations/history and optional comparison commands. [S1](#s1)[S6](#s6)[S7](#s7) |
| **New** | Project conversation reference records, isolated draft roles/provenance, project-response contract, visible request strip | Required app-owned persistence and safety; prompts/skills alone cannot implement them. |
| **New, narrow** | Project-chat origin for approved chapter briefs | Preserve exact cross-document provenance without forwarding private history. [S15](#s15) |

## 14. Formative evaluation and acceptance

Run comparable tasks with rough-idea, world-first, character-first, existing-note, and write-now authors. Compare the current Workshop flow with the new shell using matched task difficulty, provider settings, and context budgets; counterbalance order. Begin with approximately 8–10 participants as a formative study, not statistical proof of superiority. Use synthetic stories for safety fixtures and optional author-owned material only with consent.

Measure time to a **knowingly endorsed decision**, author ownership in a brief interview, usefulness after a next-day return, corrections required to preserve intent, questions before useful work, navigation/search steps, accidental adoption, successful stale/uncertain recovery, and whether the author can explain what will influence a chapter. Record why material was rejected; rejection can be a successful creative outcome.

Observe keyboard-only use, visible focus, 200% zoom/narrow layout, screen-reader announcements, selection retention, status discoverability, and interruption handling. Proposed safety gates are zero unnoticed adoption in the scripted tasks, no private/rejected cross-boundary leakage in deterministic fixtures, no lost acknowledged author text, and no hidden retry. Product acceptance also requires that participants can complete all five entry paths without a compulsory planning checklist.

Use semantic regions, named controls, non-color status labels, accessible diffs/full-text alternatives, resizable panels with keyboard controls, and sensible focus order. Preserve Ctrl+S, undo/redo, and Ctrl+Shift+F selected feedback. Default Ctrl+Enter sends; Enter inserts a newline, including during composition. Escape closes a panel, not silently discards text or cancels paid work. Announce meaningful state changes, not every streamed token; preserve the user’s scroll position with a “New reply” affordance. Adoption buttons must identify their document/version and must not receive an accidental default Enter activation after navigation.

## Evidence register

References name inspected source or the applicable design contract. Test files establish intended assertions only; no pass is claimed here. Line ranges are used where the relevant implementation was inspected directly; other links identify the file and symbols discussed above.

<a id="s1"></a>
**S1.** **Product and architecture reading.** [AGENTS.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/AGENTS.md); [PRODUCT.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/PRODUCT.md); [docs/README.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/README.md); [docs/IMPLEMENTATION_STATUS.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/IMPLEMENTATION_STATUS.md); [docs/V3_ARCHITECTURE_REFINED.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_ARCHITECTURE_REFINED.md); [docs/V3_FIRST_SLICE_PLAN.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_FIRST_SLICE_PLAN.md); [docs/V3_STORY_CONTEXT_SYSTEM.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_STORY_CONTEXT_SYSTEM.md); [docs/V3_STORY_CONTEXT_FIRST_SLICE.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_STORY_CONTEXT_FIRST_SLICE.md); [docs/V3_STORY_WORKSHOP_UX_SPEC.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_STORY_WORKSHOP_UX_SPEC.md); [docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_STORY_WORKSHOP_IMPLEMENTATION.md); [docs/V3_STORY_WORKSHOP_COMPLETION.md](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/V3_STORY_WORKSHOP_COMPLETION.md). Relevant contracts were compared with code; unchecked plans and local-report claims are not implementation or test proof.

<a id="s2"></a>
**S2.** **Core, persistence, and migration.** [snapshot validation](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/lib.rs#L1-L140); [ProjectAccess / Head / DocumentRecord / SaveSnapshot](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects.rs#L1-L220); [documents, immutable revisions, and command receipts](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/storage/001_projects.sql); [schema 38 and migration/backup boundary](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/storage/mod.rs).

<a id="s3"></a>
**S3.** **Document-owned discussions.** [thread/run/message/composer storage](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/storage/005_discussions.sql); [StartDiscussion / FeedbackIntent / run ownership](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/discussions.rs#L1-L255).

<a id="s4"></a>
**S4.** **Current bounded conversation context.** [same-document history selection and validation](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/conversation_context.rs); [ADR 0003: four complete exchanges / 16 KiB and authority limits](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0003_DISCUSSION_CONTEXT.md).

<a id="s5"></a>
**S5.** **Workshop persistence and atomic adoption.** [preview_workshop_adoption / adopt_workshop](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/workshop.rs#L3050-L3515); [immutable preview, snapshot, and receipt tables](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/storage/035_workshop.sql); [Workshop IPC](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src-tauri/src/workshop_commands.rs); [Workshop contract tests (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/tests/workshop.rs); [boundary tests (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/tests/workshop_boundaries.rs).

<a id="s6"></a>
**S6.** **Existing Workshop output shape.** [StartWorkshop / WORKSHOP_RESPONSE_CONTRACT / frozen exploration](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/workshop_generation.rs); [candidate-oriented Workshop response instruction](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/context/packet.rs#L1-L150).

<a id="s7"></a>
**S7.** **Shell and observed question-gate mechanism.** [chooseQuestion, idea entry, question dispositions, bottom notice](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workshop.tsx); [question-disposition expectations (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workshop.test.tsx); [library, modes, active editor, close integration](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workspace.tsx#L1-L165); [historical tab/action UX and preserved safety constraints](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0027_AI_WRITING_WORKSPACE.md).

<a id="s8"></a>
**S8.** **Editor lifecycle and uncertainty.** [DocumentSession](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/editor/session.ts); [durable Apply and exact-receipt reconciliation assertions (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/editor/apply.test.ts); [editor ownership contract](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0001_EDITOR_CONTRACT.md).

<a id="s9"></a>
**S9.** **Proposal preparation and Apply.** [Proposal / PreparedProposal / ProposalDecision](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/proposals.rs#L1-L180); [Tiptap preparation and guarded commit](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Writer.tsx#L1-L180); [scope, result/packet and provider-status presentation](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/assistant/FeedbackPanel.tsx#L1-L170); [explicit durable Apply contract](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0004_PROPOSAL_APPLY.md).

<a id="s10"></a>
**S10.** **Context authority.** [evaluate_sources / audience, basis and provenance checks](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/context/eligibility.rs#L1-L235); [FreezeStory / FrozenContext / immutable source views](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/story_context.rs#L1-L175).

<a id="s11"></a>
**S11.** **Working versus Reviewed.** [exact author review and independent-copy behavior](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0012_AUTHOR_REVIEW.md); [Working target, Reviewed earlier prefix and restrictions](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0013_REVIEWED_CONTEXT.md). Historical implementation-status passages are not treated as current capability inventories.

<a id="s12"></a>
**S12.** **Packet and progress limits.** [only compiled input is deliverable; response contracts and application byte caps](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/context/packet.rs#L1-L150); [bounded CodexStreamEvent / terminal and cancellation behavior](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/providers/codex_runner.rs#L1-L200).

<a id="s13"></a>
**S13.** **Generated versus accepted memory.** [generated navigation-memory ownership](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/memory.rs#L1-L180); [optional accepted summaries, exact sources, audience and review adoption](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0030_ACCEPTED_SUMMARIES.md). Its historical maintenance-model sentence is superseded by the fixed current input and source bindings.

<a id="s14"></a>
**S14.** **Native provider lifecycle.** [read/start/Stop/retry-save and proposal IPC](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src-tauri/src/discussion_commands.rs#L1-L180); [in-memory PendingSave / local-only recovery](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src-tauri/src/discussion_recovery.rs#L1-L170); [DesktopProviders / admission and owner-scoped runtime](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src-tauri/src/provider_runtime.rs#L1-L190); [installed executable qualification and bound invocation](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/providers/codex_runtime.rs#L1-L170).

<a id="s15"></a>
**S15.** **Writing brief provenance.** [projects/discussions.rs — validate_safe_brief_origin](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/discussions.rs); [same-document origin, exact approved text, scope and confirmation contract](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0008_WRITING_BRIEF.md).

<a id="s16"></a>
**S16.** **Structured scope.** [typed replacement blocks and explicit selected/whole scope; no implicit expansion](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0020_STRUCTURED_SUGGESTIONS.md).

<a id="s17"></a>
**S17.** **Normal close.** [all-project admission, save, Stop, retained-result and close gates](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0029_NORMAL_CLOSE.md).

<a id="s18"></a>
**S18.** **Project library and independent transfer.** [Library / pending creation and portable-project registry](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/library.rs#L1-L145); [backup manifests, exact heads and new restore identity](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/transfer.rs#L1-L165).

<a id="s19"></a>
**S19.** **Optional persistent transport boundary.** [fresh-thread optional app-server; Exec default; deferred tools/resume](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0033_CODEX_APP_SERVER.md); [author/maintenance and historical bindings](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/context/packet.rs#L1-L150). Provider settings in this specification also preserve the user-supplied fixed constraints; no new upstream capability is assumed.

## Open questions

| Choice still requiring evaluation | Proposed default that permits implementation |
| --- | --- |
| How long should conversation be retained, and when should compaction be offered? | Retain durable history locally; paginate; four eligible turns/16 KiB maximum before the overall packet cap; no automatic generated summary or destructive pruning. |
| When should cross-document atomic adoption ship? | One nonchapter target in the first loop; then qualify up to three targets using the existing Workshop atomic mechanics with new draft-origin validation. Chapter/material mixed batches remain deferred. |
| What does “maintain” mean for structured versus prose documents? | Versioned prose/block drafts and explicit known relationships, not field-level semantic synchronization or inferred canon. |
| When should app-server become the default? | Not in this initiative. Keep Exec default and lookup on Exec; require separate transport qualification and an explicit product decision. |
| How much provider-history disclosure is useful? | Always show that the app selected the supplied history; expose exact packet/history records on demand and disclose that fresh threads do not guarantee provider-side deletion. |
| How much automatic organization do authors welcome? | Suggest titles and placement and save task-scoped drafts; never automatically reorganize accepted documents. Evaluate the burden of draft proliferation before adding more automation. |
| Should exact conversational approval replace a button? | Keep buttons in the first slice; add a deterministic preview-bound shortcut only after ambiguity and accessibility tests. |

## Assumptions

The pinned commit is the sole baseline, and later branch changes require a fresh compatibility check. The target is the existing Windows native desktop experience; narrow layout is not a mobile-product commitment. The author owns local project data, but provider-side retention cannot be inferred from transport. A small additive schema and a versioned packet extension are acceptable; the companion plan makes their authority cost explicit. Draft material can remain unadopted indefinitely without preventing manual writing. Proposed question, draft-count, layout, and study limits are defaults to evaluate, not measured optimal values. This document authorizes no implementation, commit, provider invocation, or publication.
