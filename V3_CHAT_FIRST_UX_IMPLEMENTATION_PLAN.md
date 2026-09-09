# V3 Chat-First UX Implementation Plan

**Status:** Proposed, bounded implementation plan. No code was implemented or committed in this pass.
**Baseline:** `FZWINGEL/WebnovelStudio_V3` at `6ab150505802b6c240db884a83f0a4eb9511df79`, requested branch `codex/v3-persistence`.
**Product contract:** [Chat-first UX specification](V3_CHAT_FIRST_UX_SPEC.md).

**Implementation update — 9 September 2026:** The development implementation is
available as an opt-in native surface. [Current implementation and qualification
status](docs/V3_CHAT_FIRST_UX_IMPLEMENTATION_STATUS.md) records the completed
work, executed checks, and outstanding author, accessibility, installed-package,
and default-rollout gates. Statements about the design pass below describe the
original baseline assessment; they do not negate the subsequent implementation.

## 1. Start with one complete loop

The first buildable slice is **one project conversation, one nonchapter draft per request, explicit adoption, and a second linked request**. It must support both a blank idea and an existing note, preserve typing, expose exact sources, and reopen with conversation, draft revisions, source heads, and decisions intact. It must also leave the existing direct Writer usable. This is not a preliminary chatbot that requires a later persistence rewrite.

The acceptance journey is: create/open project → send ordinary request → receive zero to two useful questions or proposed assumptions → receive genuine provider progress → inspect a saved versioned draft → adopt or reject → edit or revise the document through the same conversation → close/reopen. A natural request does not require a lens, document type, or operation selector.

**Initial limits:** one active project-chat generation per project; one nonchapter target per response; no model-driven routing loop, paid summary pass, native story tools, automatic canon extraction, or mixed chapter/material transaction. A second linked change is a separate visible request. The later grouped slice permits up to three nonchapter targets and uses a real all-or-nothing transaction.

### Why this fits V3—and where it does not yet fit

| Verified baseline | Implementation implication |
| --- | --- |
| `ProjectSession` owns SQLite and validates `ProjectAccess`; `Head` carries document/version/hash. | Add commands within the same actor. Do not open an independent story database or write from provider workers. [S2](#s2) |
| `discussion_threads.document_id` is required; `discussion_drafts` stores the unsent composer. | A project conversation needs a small project-level reference projection. Do not rename composer rows into AI document drafts. [S3](#s3) |
| `revisions` is unique by document and working version. | Do not insert competing AI branches under an accepted document’s current revision identity. Isolate draft documents and link their proposed target. [S2](#s2) |
| Workshop has immutable previews and atomic multi-target material adoption. | Reuse/extract its transaction mechanics while validating the new origin type. Do not claim arbitrary chat drafts already satisfy Workshop candidate proof. [S5](#s5) |
| Passage, structured, and continuation proposals already have exact preparation and Apply. | Keep these for chapter editing. Do not route chapter changes through a permissive author-room material endpoint. [S9](#s9)[S16](#s16) |
| Recent discussion input is same-document, bounded, completed/delivered AuthorRoom evidence. | Project-level history selection is an explicit compiler addition, not provider thread resume. [S4](#s4) |
| Native runtime owns admission, Stop, provider dispatch, and retained local result failures. | Reuse that lifecycle; no second orchestration framework or retry engine. [S14](#s14)[S17](#s17) |

The foundation documents establish contracts, not current completeness. Schema 38 and the inspected source override historical schema/settings claims. The current Workshop question gate is contradicted by this new product requirement and must change; the Working/Reviewed, scope, and recovery boundaries must not. [S1](#s1)[S2](#s2)[S7](#s7)[S19](#s19)

## 2. Architecture and explicit tradeoffs

Keep Rust/Tauri, React/Tiptap, the project SQLite actor, provider adapters, and document revision authority. Add a thin **application conversation projection** and a bounded **structured-response-to-draft** adapter.

```text
Project conversation / document review / existing Writer
                     │ typed IPC + ProjectAccess + operation identity
                     ▼
Existing ProjectSession actor
  ├─ project chat reference projection + isolated draft provenance [new]
  ├─ existing discussions, immutable packets, run receipts
  ├─ existing document/revision/history ownership
  └─ shared material adoption transaction; existing chapter Proposal Apply
                     │ claim exact frozen request; leave transaction
                     ▼
Existing native provider runtime → Codex Exec by default
  optional fresh-thread app-server / Claude / HTTP remain separate bindings
                     │ validated terminal report; no document write authority
                     ▼
Retain response → materialize isolated draft locally → explicit author adoption
```

Skills may supply writing style, examples, or question-selection guidance inside the versioned application contract. They cannot own project isolation, persistent conversation, exact draft references, UI state, atomic adoption, or provider uncertainty. **A skills-only solution is insufficient.** Conversely, a new general agent framework adds no necessary authority or persistence capability here.

### Structured response versus story tools

Use application-controlled structured responses first. Add a proposed `project-assistant-output.v1` contract with readable answer text, zero to two questions, zero to three task assumptions, and zero to one draft in the first slice. Reuse the restricted typed-block grammar for draft content; the provider supplies neither document IDs to create nor editor block IDs. Existing target references must be exact handles from the frozen request. A response can simply answer without producing material.

Rust validates the contract, bounds, referenced sources, candidate provenance, proposed target, and audience. The app allocates IDs, persists drafts, and presents changes. Malformed output remains retained conversation evidence without an adoptable artifact. No automatic repair invocation, speculative second call, or partial accepted-document mutation is allowed.

Native story tools are **not implemented** and are unnecessary for this loop. A future tool experiment would have to reuse the frozen snapshot, read-only handles, existing invocation/read/byte allowances, ownership and cancellation, and per-read receipts. It must not expose filesystem access, unrestricted SQL, adoption, or a generic write tool. The existing Exec bounded lookup is an application protocol, not evidence that provider-native story tools exist. Leave its contract and route unchanged. [S12](#s12)[S19](#s19)

### Transport changes actually required

Only wire the new response contract through the existing author request/response validation and provenance path. Preserve selected provider, model, traits, transport, byte caps, CLI identity/fingerprint, request admission, terminal receipts, and recovery. The optional warm app-server still creates fresh upstream threads. Do not add thread resume, change its default, port lookup to it, or expand parity as part of chat-first work.

Streaming means rendering genuine available progress while preserving editor/composer typing. Exec may first emit a completed message rather than token deltas. Qualify that honestly; do not animate fabricated tokens, silently fall back, or claim an app-server latency improvement not measured here. Maintenance remains **GPT-6 Astra / low**, Codex priority and HTTP no tier; the author picker is independent. Claude and HTTP author adapters remain available. [S12](#s12)[S14](#s14)[S19](#s19)

## 3. Minimal additive persistence and authority mapping

**Proposed migration:** `039_project_chat.sql` relative to this schema-38 baseline. Recheck the actual next number before implementation on a newer branch. Use the existing pre-upgrade backup, transaction, and reader-floor pattern. No rewriting of old packet JSON or hashes.

Three narrowly scoped records plus an explicit document role are the proposed minimum. They are not a new story bible or job system.

| Addition / mapping | Purpose and rules |
| --- | --- |
| `project_conversations` | One active root per current project/operation namespace: conversation ID, blank control-anchor document ID, composer CAS version/body, optional view state. The composer contains author input and exact references, not an assistant-generated story database. |
| `conversation_items` | Ordered immutable references/events: message/run links, draft links, exact manual revision links, question dispositions, immutable material previews, and author decision links. Include owner, sequence, event/operation identity, payload version/hash. Do not duplicate message text or authoritative document bodies. |
| `assistant_drafts` | Metadata connecting an isolated draft document to originating run/output ordinal, proposed target or new-target intent, exact base head, snapshot/packet, and predecessor draft. Status changes are CAS-backed and refer to immutable disposition/adoption events. Body history remains in `documents` and `revisions`. |
| `documents.role` | Add `ordinary` default, `assistantDraft`, and `conversationAnchor`. Enforce centrally, not through a title or ID-prefix convention. Existing ordinary documents remain unchanged. |
| Existing `command_receipts` | Reuse for new local projection/materialization/adoption operations, associated with the conversation’s anchor where a document FK is required. Extend cross-domain operation-collision validation; do not invent a separate retry ledger. |
| Existing discussion/run/packet tables | Store provider work and exact author/assistant messages. Root discussion uses the blank anchor; chapter discussions retain their actual chapter target. Conversation items link both without moving historical rows. |
| Existing proposals / proposal versions / decisions | Remain the authority for scoped chapter suggestions. Their review cards can appear in the shared conversation without converting them into general material drafts. |

The actual `command_receipts.operation_kind` is text, while Workshop and proposal receipt domains have their own checks/collision guards. Verify every collision path when adding operations. Existing `discussion_drafts.intent` has historical constrained migrations; **do not overload that composer table with project-draft state**. [S2](#s2)[S3](#s3)[S5](#s5)[S20](#s20)

### Isolated draft behavior

An assistant draft is an actual versioned document with an immutable role and a distinct ID from any Working target. Initial creation and author-edited checkpoints use the existing snapshot/revision machinery. A new model revision creates a successor candidate draft rather than overwriting a draft the author is editing. Provider IDs and editor steps are never accepted.

Drafts are excluded by default from ordinary source enumeration, source pins, lookup, alias/source directories, memory analysis, review, ordinary manuscript export, and chapter ordering. Explicit author-room work on a draft may include its exact revision as **unadopted task material**, through a narrowly validated draft-reference path. Restricted writing always rejects that path. Control anchors are blank request infrastructure, never manuscript, memory, export, or ordinary source material.

Draft saves and conversation bookkeeping must not advance the ordinary story-source epoch and make their own generation stale. Exact draft heads/disposition versions are separate freshness dependencies. Ordinary Working saves retain their current source-epoch behavior. Implement this centrally in Rust before enabling the UI; hiding a draft in React is not isolation.

A new accepted document receives a reserved ordinary target ID at preview time; adoption copies the exact reviewed draft revision into it. Updating an existing document writes a new Working revision at that target. The original isolated draft remains provenance, marked adopted/superseded as appropriate. Do not silently convert every saved draft into an eligible source.

Plans and themes use current representations: plans can be `note`, `scene`, or `hook`; character/world/theme use the matching existing kind. Initial automatic organization is proposed naming and grouping only. No schema for arbitrary semantic fields, no automatic entity merging, and no requirement to create one of each type. [S2](#s2)

### Identity and compatibility

All references carry project ID, operation namespace, document ID, revision ID, and head/hash when applicable. Preview metadata also captures expected metadata/order/relationship versions for any proposed metadata effects. Provider runs additionally retain run/packet/snapshot identity. Never resolve an old chat link to a newer head without offering the original separately.

Old JSON fields remain absent in old records. New packet/projection forms use explicit versioned contracts; the old compiler validation must still reproduce historical bytes. New document roles require a minimum-reader fence so a baseline executable cannot accidentally treat assistant drafts as ordinary story documents. Preserve database integrity and immutable triggers during upgrade; never use a downgrade to provide rollback.

## 4. API and interaction mapping

Names marked **new** below are proposed; the linked baseline does not already expose them.

| User interaction | App/core contract | Durable/result boundary |
| --- | --- | --- |
| Open/create project | Existing library create/open and project attachment; **new** `read_project_conversation` / idempotent `ensure_project_conversation` | Project-owned root/blank anchor; no provider invocation on open. |
| Type a message | **New** `save_project_composer(access, operationId, expectedVersion, body)` using `ComposerSession` patterns | CAS and saved-generation acknowledgment; conversation and document typing are independent. |
| Send ordinary planning request | **New** `start_project_chat` | Atomically freeze inputs, existing discussion message/run/packet, and conversation link before dispatch. |
| Stream response | Existing run ownership/events, `read_discussion`/native recovery view; project view references the run | No editor mutations; progress and saved/unsaved result status are distinct. |
| Create reviewable draft | **New** `materialize_chat_result(owner, terminalIdentity)` | Idempotent local transaction reads the retained validated result and creates draft document/revision + provenance + reference event. Zero model calls. |
| Edit a draft | Existing `DocumentSession` save/checkpoint against draft head, with new role-aware authorization | Only that draft advances; old prepared adoption is stale. |
| Review/adopt material | **New** `prepare_chat_adoption` / `adopt_chat_preview`, sharing extracted Workshop transaction helpers | Exact source/draft/target proof and immutable preview; atomic Working revisions/decisions/receipt. |
| Reject/reconsider | **New** `set_chat_disposition`, scoped to exact candidate/question/version | Append disposition event + CAS status projection; no source-document mutation. |
| Chapter feedback/edit | Existing `start_discussion`, `prepare_proposal`, `prepare_structured`, `prepare_continuation`, `apply_proposal`/`reject_proposal` | Exact chapter target and scope; project conversation adds links only. |
| Stop/check unknown outcome | Existing `stop_discussion`, reconciliation and native recovery APIs, projected at project level | Same owned run and operation; no new provider dispatch. |
| Retry failed result saving | Existing `retry_discussion_save`, then idempotent draft materialization if necessary | Local persistence only; preserve outcome/cleanup uncertainty. |

### State projection, not a second workflow engine

Project request state comes from existing `DiscussionRunStatus`: Queued/Running → Generating, Stopping → Stopping, Completed → Reply or reviewable result, and Stopped/Failed/Interrupted → retained non-applicable output with a visible reason. Provider submission/cleanup uncertainty remains separate. Local persistence state comes from save acknowledgments and native pending results; draft/adoption state comes from exact draft and decision records; staleness is computed from bound heads and policy. Do not add an independently mutable “success” flag that can contradict these records.

Question dispositions store exact question identity, author-selected scope, and an optional unknown-to audience. **Not now** defers that question without blocking any new request; **Not relevant** suppresses unsolicited reopening within its scope; **Keep mysterious** preserves the chosen uncertainty rather than filling a missing field. **Change my mind** creates an explicit superseding disposition. None changes accepted documents or bypasses adoption. Add fixtures for all four, including reload and a fresh request after deferral.

### Proposed acceptance DTO shape

This is a design contract, not code to implement in this pass:

```text
StartProjectChat {
  access: ProjectAccess,
  operationId,
  conversationId,
  expectedComposerVersion,
  instruction,
  sourceRefs[],                  // resolved exact saved document/revision/head
  taskDraftRefs[],               // explicit unadopted scope; author room only
  focusedDocumentRef?,
  selectedSourceScope?,          // validated against that source, not root anchor
  responseContract: project-assistant-output.v1,
  providerChoice                // existing independent author selection
}

ChatMaterialPreview {
  previewId, previewVersion, previewHash,
  projectId, operationNamespace,
  originatingRunId, packetId, snapshotId,
  draftRefs[], targetBeforeRefs[], targetAfterRefs[],
  sourceHeads[], metadataVersions[], policyFence, scopes[], protectedContent[],
  candidateDispositionVersions[], proposedMetadataAndRelationships[]
}
```

Use the existing `FeedbackIntent::Discuss` author-room run lifecycle with **explicit versioned project-chat metadata/response binding**, accepted only through the typed project-chat command. Do not detect this mode by searching untrusted instruction text, reinterpret `WorkshopExplore`, or change old Discuss packets. New metadata and projection validators belong in the app/core; old requests omit them. Chapter requests continue to use the existing appropriate intent and restricted policy.

Project acceptance must share transaction-level discussion helpers, not call the actor’s public API recursively. The provider executes only after the durable dispatch claim. Keep native worker/pending-result ownership registered through terminal saving and local draft materialization. After reopen, a durable completed result may finish the same idempotent local materialization without a model call; this never retries adoption. If only an unsaved native result existed, expose the actual retained prefix and uncertainty instead. Project references do not redefine a legacy thread’s `document_id` or permit cross-document selection fields to bypass existing source validation.

## 5. Application history and context compilation

The project UI assembles messages from existing runs and documents from exact references. It can show planning and chapter work in one chronological view while compiling different input for each. The application record is persistent; a provider request is a bounded, frozen selection.

Add a proposed `context/project_conversation.rs` selector/validator and integrate it into existing `context/packet.rs`, `projects/conversation_context.rs`, and `projects/story_context.rs` rather than introducing retrieval middleware. Initial automatic selection is chronological and bounded: at most four complete eligible turns within 16 KiB, subject to the author-provider packet cap. Codex’s current application cap is 24 KiB input/64 KiB output, not its advertised model context window. [S4](#s4)[S12](#s12)

Eligibility checks must authenticate conversation membership, current namespace, run completion/delivery, source revision/selection, current disposition, and audience. Old assistant output is contextual evidence, never a permanent instruction or accepted fact. Selected draft material is labeled unadopted; current request and explicitly adopted guidance retain instruction authority.

**Rejected-content rule:** raw historic assistant JSON may contain candidates that have since been rejected. The initial safe selector omits a producing turn when including it would reintroduce rejected/superseded content. Record an omission reason and keep it readable locally. A later answer-only projection must be versioned and validated against the original response and current disposition; it cannot be an unaudited summary. An explicit compare/reconsider request may attach that candidate as an alternative only.

Restricted chapter packets receive no automatic project transcript, project recap, private draft, or author-room summary. Preserve the current source eligibility kernel, frozen Working/Reviewed basis, disclosure filtering, and exact source receipts. Project history cannot override a frozen packet, and a warm runtime cannot override the project compiler.

### Retention, summaries, and disclosure

Retain local durable messages and immutable referenced material in the first slice; paginate instead of rewriting it. Show a deterministic return card derived from adoption receipts and manual revisions. Navigation archive/hide is not deletion. Every draft and event has a visible source, scope, status, and applicable adoption rule.

Do not generate conversation summaries automatically. Future summarization needs its own source-message/revision set, audience, generated status, version/hash, disposition invalidation, and explicit task/budget. It may be stored as an unreviewed aid; entering persistent guidance or reviewed evidence still uses the relevant author action. Existing chapter navigation memory and accepted narrative summaries retain their distinct authority and source requirements. [S13](#s13)

Disclose the history subset actually supplied, omissions, and prepared-versus-delivered status. Do not imply fresh threads mean the external service deletes its data. Backups include private local history and drafts. Destructive pruning/redaction and packet-reference garbage collection are deferred until a retention design handles immutable dependencies and backup copies.

## 6. Implementation order and phase gates

### Phase 0 — Establish safety seams inside the first vertical slice

**Dependency:** pinned source baseline. **Scope:** contracts and minimal storage/authority preparation, not a separate platform milestone.

1. Add golden contract fixtures for the new response, project references, draft roles, exact preview identities, and legacy packet preservation. Document the schema reader floor and source-epoch policy before changing code.
2. Add the migration and role-aware project operations. Centralize eligibility for ordinary sources versus explicit task drafts. Add projection/reference read validation and operation-collision checks.
3. Extract the existing material-adoption transaction helpers without changing Workshop behavior or serialized historical records. Do not combine this refactor with new UX behavior until old tests characterize the boundary.

**Likely files:** `crates/core/src/storage/mod.rs`; new `storage/039_project_chat.sql`; `projects.rs`; new `projects/project_chat.rs` and `projects/material_adoption.rs`; `projects/workshop.rs`; `projects/story_context.rs`; `projects/source_pins.rs`; `projects/memory.rs`; `projects/exports.rs`; `transfer.rs`; `context/eligibility.rs` and relevant packet/descriptor contracts. Audit other source enumeration through `projects/evidence_queries.rs`, `projects/reviewed_story.rs`, and alias/lookup paths rather than assuming one UI filter is sufficient.

**Tests:** add `tests/project_chat.rs`, `tests/assistant_drafts.rs`, and `tests/chat_adoption.rs` to `tests/integration.rs`. Extend `context_migration`, `context_eligibility`, `source_pins`, `memory_storage`, `reviewed_context` with added adversarial fixtures, `transfer`, `workshop`, and `workshop_protection`. Verify actual file-backed SQLite reopen, hashes, foreign keys, old receipt/packet bytes, same-ID collision refusal, draft exclusion from every relevant consumer, role tampering, and isolated draft changes not staling ordinary story snapshots.

**Checkpoint A:** core review demonstrates that a draft cannot become ordinary/restricted evidence through search, pins, export, review, aliases, backup validation, or a forged request. No UI feature flag is enabled before this gate. Any unresolved source-role leak blocks the slice rather than being left for UI review.

### Phase 1 — Complete the single-draft project conversation loop

**Dependencies:** Phase 0. **Release boundary:** opt-in chat shell in the new binary; existing Workshop and Writer remain accessible.

Implement in this order:

1. **Open/create and projection.** Create a project-owned blank anchor/root idempotently; persist composer/reference state. Display existing legacy work as references rather than inventing a synthetic chat transcript. Open/create/return must not invoke a provider.
2. **Request acceptance.** Build `start_project_chat` on the existing discussion acceptance/dispatch helpers. Save exact author input and source references before dispatch. Preserve author provider settings and the common admission/Stop owner.
3. **Bounded answer contract.** Compile the new contract and eligible project history. Permit answer-only output, a few questions/assumptions, or one nonchapter draft. Infer proposed organization without requiring a type picker. Do not add an extra planning call.
4. **Streaming and terminal storage.** Render available progress without remounting editors. Retain terminal result through the existing native recovery path; materialize only validated completed output. Make local materialization independently idempotent after the terminal receipt. A result with valid frozen provenance but changed current sources may be saved as a visibly stale, non-adoptable draft; storing it does not restore current eligibility.
5. **Review and adoption.** Show full draft, original source and target versions, proposed assumptions, scope, and change summary. Draft editing produces exact new revisions. Use the new chat-origin validator plus shared material-adoption transaction, initially limited to one target. Reject never changes the target.
6. **Continue and reopen.** Return to the same conversation with receipt/revision links. Support a second request modifying the adopted document or preparing a linked note. Save composer/draft/view state and reopen without generation, duplicate artifacts, or lost acknowledged decisions.
7. **Replace the observed trap.** Separate question disposition from request admission in both UI and prompt/contract handling. “Not now” followed by a fresh idea must work. Keep request status/error/action visible independently of the transcript’s scroll height.

**Likely files:** new `projects/project_chat.rs`, `context/project_conversation.rs`, `apps/desktop/src-tauri/src/project_chat_commands.rs`, `apps/desktop/src/ipc/projectChat.ts`, and `apps/desktop/src/chat/{ProjectConversation,RequestStatus,DraftReviewPanel,ProjectDocumentsPanel}.tsx`, `conversationStore.ts`, and chat styles. Integrate with `projects/discussions.rs`, `projects/conversation_context.rs`, `context/packet.rs`, native `discussion_commands.rs`/`discussion_recovery.rs`, `Workspace.tsx`, `Workshop.tsx`, `FeedbackPanel.tsx`, `assistant/composer.ts`, and existing provider context/selector. Register commands in `apps/desktop/src-tauri/src/main.rs`, preserving its existing runtime and close integration.

**Deterministic Rust gates:** same operation yields the same run/packet/draft; a changed payload with the same identity is refused; complete result creates at most one artifact; invalid output creates none; no new source authority before adoption; question disposition cannot veto an unrelated request; wrong-project/namespace/policy/draft reference fails; draft edit/target edit/source change blocks stale adoption. The blank control anchor cannot accidentally enter ordinary author material.

**Frontend/Tiptap gates:** one mounted editor while chunks arrive; composer typing and selection survive; two generations cannot overwrite each other; no automatic Send after answering/defer/navigation; exact draft versions and receipts appear on return; pinned error/status remains visible after a long transcript; keyboard-only adoption identifies the version; stale and uncertain states cannot be dismissed into an ordinary Apply. Update the old `Workshop.test.tsx` question-blocking expectation rather than retaining the defect as a compatibility requirement.

**Native gate:** a new `native-chat-smoke.mjs` or equivalent focused extension must exercise blank-idea and saved-note entry, draft editing, source inspection, adopt/reject, second linked request, and close/reopen through real Tauri/WebView2 IPC with synthetic data. Include lost start acknowledgment and failed local result save; prove zero extra provider dispatch. The direct Writer regression must still pass. Register its exact expected checks in the existing native consumer/aggregation infrastructure.

**Live gate:** after deterministic/native gates, one explicitly authorized synthetic Codex Exec request verifies the new response contract with the selected author model/traits and retained packet/result. Permit no automatic retry. A separate authorized second request is needed to qualify live conversational continuity; the single-call result alone does not prove it. Failed parser/output qualification is reported, not repaired with unbounded extra calls.

**Checkpoint B:** an author can knowingly endorse a useful document, revise it, and resume after reopen without learning Workshop lenses. Do not make chat the default merely because mock snapshots render correctly.

### Phase 2 — Grouped nonchapter maintenance and review

**Dependencies:** Phase 1 adoption and draft provenance gates. **Scope:** up to three related nonchapter targets, not general agent execution.

Expose multiple drafts from the same authorized task and one complete grouped preview. Reuse the existing atomic material transaction; add new chat-origin validation for every draft, exact target/base head, policy/source epoch, disposition, and proposed relationship. Store complete before/after references and all group effects. The assistant may propose organization and related-document changes, but existing document moves/renames/deletions are not automatic side effects.

One review action adopts the exact group. Changing a draft, target set, relationship, or protected content creates a new preview. Dropping a target requires explicit re-preparation and dependency checks; never loop over one-target adoption calls and label it atomic. Defer semantic consistency guarantees: “related documents updated” means the displayed transaction committed, not that the story has been proven contradiction-free.

**Likely files:** `projects/material_adoption.rs`, `projects/project_chat.rs`, `projects/workshop.rs`, new chat IPC types, `DraftReviewPanel.tsx`, `ProjectDocumentsPanel.tsx`, `RequestStatus.tsx`, `Workspace.tsx`. Preserve legacy Workshop preview parsing and receipt identity; do not change old candidate lists to new draft records.

**Tests:** extend `chat_adoption`, `workshop`, `workshop_protection`, `history`, and `transfer`; inject a SQL failure after the first proposed target write and verify neither target, any decision, nor the group receipt remains. Cover one stale member, relationship head drift, protected-content failure, rejected member, edited draft, duplicate operation, lost commit acknowledgment, and later author edits during reconciliation. Frontend tests must show all targets and complete content before approval and prohibit partial Apply of the saved group.

**Native/live gate:** a real two-document review/Apply/reopen and a two-target lost-acknowledgment test are required; code-level transaction tests alone do not establish correct editor handoff. Live qualification is a separate bounded structured-output task, not a new transport comparison.

**Optional approval shortcut:** only after grouped review passes, support exact author-origin preview/version/digest commands through the same adoption endpoint. Test vague assent, quoted approval, model-generated approval, wrong project, stale preview, and editing between message entry and commit. The button remains the accessible default.

**Checkpoint C:** all-or-nothing document state, grouped author understanding, and uncertainty recovery are demonstrated independently. No mixed chapter/material batch is included.

### Phase 3 — Chapter work in the same conversation, without author-room leakage

**Dependencies:** Phase 1 projection; existing chapter proposal path remains available throughout. Grouped material adoption need not block this phase.

Project chat links chapter runs into the same timeline while retaining their real chapter-owned discussion, target, policy, and scope. Use existing `FeedbackIntent::ProposeEdits` and `Continue`, Working/Reviewed selection, `ScopeGrant`, `PreparedProposal`, and `DocumentSession.applyPrepared`. Do not implement chapter replacement as generic chat-draft adoption.

When a chapter is active, the composer can show an inferred **Chapter writing** preparation with its target, scope, and basis. A request from author-room chat that needs a chapter transition must present that proposed boundary before a writing dispatch. A bounded author-room response may propose a handoff, but may not grant itself chapter edit authority or trigger an automatic second invocation. The author need not know an operation name; the app must still obtain the required scope/brief confirmation. “Open a blank chapter” and direct writing remain one-action paths.

Add a narrowly versioned `SafeBrief` origin for project conversation messages. Validate exact same project/current namespace, readable author-room source message, target chapter and scope, policy, text/hash, and explicit confirmation. Keep the old same-document origin contract unchanged. Only the approved text enters the restricted packet; original message content/private source handles do not. Never drop the origin reference to make old validation pass. Editing text, target, or scope clears approval. [S15](#s15)

Selected feedback keeps exact source quotes and block/offset identities. A sentence selection remains a passage unless the author explicitly chooses complete paragraphs. “Keep the ending” should leave the ending outside the edit grant, so the existing validator proves preservation. Whole-chapter feedback is not whole-chapter replacement permission. After Apply, retain the historical scope and require a fresh capture for a new edit.

**Likely files:** `Writer.tsx`, `FeedbackPanel.tsx`, `ContextInspector` and `SafeBriefEditor` components, `editor/session.ts` and existing selection/scope preparation helpers, `ipc/discussions.ts`, new project chat projection/IPC, `projects/discussions.rs`, `projects/proposals.rs`, `projects/story_context.rs`, `context/packet.rs` and brief/conversation contracts. Add a later migration/reader floor only if the versioned brief provenance cannot be represented safely by the already introduced format; do not rewrite old packets.

**Tests:** extend `scope`, `structured_proposals`, `proposals` continuation fixtures, `context_eligibility`, `reviewed_context` with added adversarial fixtures, `discussions`, and project history fixtures. Assert private planning, future possibilities, rejected candidates, raw chat, and private summaries are absent from restricted packet bytes, including titles and source labels. Test wrong-document legacy brief, valid new origin, text/scope edit reset, policy revocation, copied origins, and first-chapter Working with no planning review. Existing Reviewed continuation restrictions and unsupported character grants remain negative tests.

**Frontend/native gates:** selected words/sentence/paragraph and chapter-wide feedback; explicit scope expansion; emotional-confrontation revision with an unchanged ending; typing during generation; stale selection; return to chat with exact references; keyboard selection and focus restoration. Run installed-editor qualification separately, including actual clipboard and native window behavior rather than a standalone browser substitute.

**Checkpoint D:** the author can explain what will influence a chapter; deterministic packet evidence agrees. No hidden source widening, automatic Reviewed adoption, or compulsory planning review is introduced.

### Phase 4 — Compatibility, accessibility, and default rollout

**Dependencies:** first-loop safety and relevant chapter gates. Keep the chat shell opt-in until the formative and native results justify defaulting it.

Finish migration presentation, legacy Workshop navigation, paged history, persistent panel selection, document search, and clear private/draft labels. Preserve old Workshop records and pending states as inspectable history. Do not fabricate a chronological conversation where only a candidate or snapshot was stored; display its actual record type and timestamp.

Exercise normal close across projects, stale/unknown adoption after reopen, interrupted work, recovered/duplicated projects, and failed local saves. Include keyboard-only/assistive-technology checks and author study observations before retiring primary Workshop navigation. Keep a “Earlier Workshop work” route for legacy records; retirement of a UI pattern is not data deletion.

**Likely files:** `Workspace.tsx`, `workspaceModes.ts`/styles, `projectTabs.ts`, new chat components/store, `StoryBible.tsx` presentation, library/transfer validation, `AppCloseDialog.tsx` and native `app_close_commands.rs` where integration requires it; `.github/workflows/ci.yml`, native consumer/identity tooling, `docs/README.md`, `PRODUCT.md`, relevant ADRs and `IMPLEMENTATION_STATUS.md`.

**Acceptance gates:** all five entry-path tasks are possible; no silent failure after Not now; current request state is discoverable without transcript hunting; exact original/current document links survive reopen; accepted material and private drafts remain distinguishable; interruption recovery succeeds without blind replay. Required core/frontend/native checks and the applicable installed-package gate must be attached to the actual implementation commit, not inherited from this baseline’s historical reports.

**Rollback:** switch the new binary back to legacy navigation without dropping the new tables or drafts. The schema reader floor remains. Returning to an old binary requires an explicit independent recovery from a pre-upgrade backup and disclosure that later edits are not in that backup; it is not an in-place downgrade.

## 7. Shared failure and recovery contract

| Boundary | Required implementation behavior | Fault/verification target |
| --- | --- | --- |
| Input saving before acceptance | Save exact composer and referenced document heads; no dispatch on unresolved local-save failure. | Hold/lose save acknowledgment; type more; ensure later buffer is not marked saved or submitted. |
| Start transaction / lost start acknowledgment | Reconcile by operation identity; acceptance and run/reference insertion are atomic. Native dispatch claim is at most once for that accepted work. | Lose the IPC acknowledgment before/after commit; verify one run and no duplicate paid dispatch. |
| Streaming / source drift | Preserve frozen packet; mark source/draft conflict visibly. Do not mutate editor or silently re-freeze mid-run. | Save a source while chunks arrive; old result remains inspectable and non-adoptable. |
| Terminal output / local save | Retain exact provider result through existing recovery; retry saving locally only. Materialization can restart from a durable terminal record. | Fail terminal save and draft creation independently; distinguish in-memory retention from durable prefix on process loss. |
| Stop | Scope cancellation to original project/namespace/run. Preserve partial output and local/upstream settlement distinction. | Stop after project switch, during acceptance, and after terminal receipt; no cancellation of a newer run. |
| Preview / author edits | Bind exact draft, targets, source heads, scope and policy. Editing any bound version invalidates preview/approval. | Edit accepted target, isolated draft, or a source before adoption; no partial changes. |
| Adoption commit / lost acknowledgment | One transaction; reconcile receipt before any action. A receipt plus newer author prose must not restore the older body. | Fail statements and lose post-commit acknowledgment; assert all-or-none effects and history. |
| Proven-absent local operation | Preserve existing fenced, idempotent local replay semantics. | Only reuse the same payload/operation after lease reconciliation proves absence; never create a provider retry implicitly. |
| Close / renderer replacement | Join existing all-project admission/flush/Stop protocol; keep failed local results available; no new queue. | Stay open, Stop and close, canceled close, renderer loss, and process kill are separate scenarios. |
| Copy / recovery | New identity/namespace, historical references retained, old work inert, current context freshly compiled. | Wrong-project links, copied previews/receipts and old upstream thread IDs cannot authorize actions. |

Use existing `DocumentSession` failure semantics: ordinary save failure retains editable local text; uncertain mutation outcomes fence affected operations for reconciliation. Do not claim unacknowledged buffers or terminal output held only in `DiscussionRecovery` survive an application-process crash. Normal close prevents avoidable loss; forced termination is a different guarantee. [S8](#s8)[S14](#s14)[S17](#s17)

## 8. Migration and compatibility checklist

| Existing data | Treatment |
| --- | --- |
| Projects/documents/revisions/history | Preserve IDs, bodies, hashes, order, and immutable revisions. New ordinary-role default does not rewrite content. |
| Workshop state/snapshots/candidates | Keep intact and reachable. Link historical records to the project view with accurate labels, not inferred approvals. No bulk conversion of candidate text into Working documents. |
| Workshop adoption previews/receipts | Preserve old validation/operation domains. Already committed decisions remain evidence. Pending legacy previews use the legacy validator; importing them into chat does not upgrade authority. |
| Discussion threads/messages/runs | Leave document ownership and historical bytes intact. Link from project conversation; legacy history is not automatically eligible project context. |
| Provider bindings/receipts/lookup records | Preserve requested model/traits/transport and reported identity separately. Historical Luna settings remain historical; new maintenance defaults do not rewrite receipts. |
| Reviewed chapters/evidence/summaries | Preserve bundles, source references, selection rules, and copy/reset behavior. Draft adoption creates no review bundle. |
| UI preferences | Preserve legacy per-project mode/tab state as fallback; initialize chat navigation without changing story data or generating. |
| Duplicate/recovered project | Retain old history with source identity and a historical label; create new root/active namespace. Clear/retain active pointers according to existing transfer rules. Pending draft reuse requires a new explicit fork/rebase. |

Transfer validation must authenticate new roles, conversation references, draft origins, event hashes, previews, and adoption receipts; unknown/future formats fail closed. Retain original provenance for historical copied messages instead of rewriting project IDs inside hashed payloads. A newly active reference must resolve through an explicit current-project mapping and fresh permission check. [S18](#s18)

Keep migration backups and reader-floor refusal tests. Do not cite a fixture downgrade as a supported user rollback. Do not rely on `.local` reports being available to future repository readers.

## 9. Qualification matrix and execution discipline

**Evidence for this design pass:** source/document/test-definition inspection only. No tests, WebView2/native sessions, installed lifecycle checks, CI runs, or live provider requests were executed. Historical status documents and `.local` paths are not fresh evidence for chat-first behavior.

| Layer | Required evidence | What it does not establish |
| --- | --- | --- |
| Deterministic Rust/core | Actual SQLite durability, migration/backup/reopen, exact packet and output fixtures, CAS/atomic faults, role/audience exclusion, ownership, idempotency and recovery. Register new suites in `tests/integration.rs`. | Native editing, live model compliance, or usability. |
| Frontend/Tiptap | Component/store and editor-transaction tests, real typed scope preparation, typing during progress, saved-generation handling, focus/keyboard and visible status, no accidental adoption. | Real WebView2/OS dialogs, installed package, screen-reader usability. |
| Native development | Built Tauri app in real WebView2; owned synthetic processes/projects; exact IPC, SQLite and receipts; strict interruption, close, recovery and selection paths. | Installed lifecycle or live-provider quality. |
| Installed/WebView2 | Install/reopen/normal close/upgrade compatibility and actual editor/chat behavior on an identified build, including clipboard, narrow/zoom and accessibility observations. | General narrative quality or reliability of every provider/version. |
| Live-provider qualification | Explicit opt-in, bounded calls, synthetic material, exact author binding/transport and delivered packet, parser result, terminal/uncertain receipt. | Safety proven by one lucky response, literary usefulness, or unrestricted adapter parity. |

Retain the repository’s normal full check before a code submission:

```powershell
.\scripts\desktop.ps1 -Command check
# Focused iteration examples; not replacements for the full gate:
cargo test -p webnovel-core --test integration project_chat::
cargo test -p webnovel-core --test integration chat_adoption::
npm.cmd --prefix apps/desktop test -- src/chat/ProjectConversation.test.tsx
```

The new focused commands become valid only after their proposed files/suites exist. Native checks remain separate. Existing scripts include `native-workshop-smoke.mjs`, `native-smoke.mjs`, `native-interruption.mjs`, `native-app-close.mjs`, `native-project-recovery.mjs`, HTTP and memory-lookup flows. Preserve them while adding chat coverage. [S21](#s21)

At the pinned commit, CI defaults to build-once native fan-out and a final aggregate gate; documentation-only paths are ignored. Add new native checks to the producer/consumer expected-check inventory and aggregation, not only to a local script. A successful producer build is not a native pass. Installed-package qualification remains separate. This plan does not propose further CI optimization or transport benchmarking. [S22](#s22)

Live checks must select an explicit intended provider/model/effort/tier, never silently use mock or maintenance settings. Codex Exec is the initial live contract gate. Claude, HTTP, and optional app-server require separate applicable qualification before claiming the new contract works there; their existing adapters are not removed or substituted. Record CLI version/fingerprint or endpoint profile revision, exact source commit and executable hash, input/response contract, invocation count, provider-reported identity, cleanup/uncertainty, and any omitted gate. Never rerun an uncertain request automatically to obtain a passing report.

## 10. Formative metrics and release decision

Compare the existing Workshop and chat-first flow with matched tasks and provider/context settings. Counterbalance order across approximately 8–10 formative participants; do not present that sample as a statistical benchmark.

| Author starting point | Comparable task |
| --- | --- |
| Rough idea | Turn a two-sentence attraction into one knowingly endorsed story direction and a useful saved document. |
| World-first | Establish one rule and its human consequence without completing a world template. |
| Character-first | Develop a motivation/conflict, revise it, and update one linked plan without losing the original intent. |
| Existing notes | Organize supplied notes while retaining the source and distinguishing invented assumptions. |
| Write now | Begin prose immediately, revise a selected confrontation, and keep the ending unchanged. |

For each task collect elapsed time to an endorsed decision; author ownership/explanation; next-day return usefulness; intent-preservation corrections; number and perceived value of questions; navigation/search burden; accidental adoption or uncertainty about adoption; successful stale/unknown-operation recovery; and ability to explain the chapter’s actual inputs. Add brief qualitative observations rather than optimizing raw acceptance rates.

Inject a deferred question, a source edit during generation, an uncertain acknowledgment, and a project switch in matched scenarios. Observe keyboard-only completion, focus recovery, screen-reader status announcements, 200% zoom/narrow layout, and whether the latest action/error is found without transcript hunting. Distinguish user comprehension failures from underlying transaction failures.

Proposed stop-ship conditions are unnoticed adoption, loss of acknowledged text, cross-project or restricted-context leakage, partial group commit, and blind provider replay. Question burden and navigation time are improvement measures, not rigid quotas that force useless drafts. Keep raw output volume and model-only quality secondary. Only make chat the default after source/transaction safety, native behavior, and formative author understanding are all supported.

## 11. Deferred work

| Deferred | Reason / boundary to revisit |
| --- | --- |
| App-server default migration, transport parity, latency program | Not required for persistent application chat; separately qualified provider plumbing should remain stable. |
| Native story tools and provider-history resume | New authority/disclosure contracts and qualification are required; bounded structured responses already prove the loop. |
| Automatic canon extraction, semantic field synchronization, full story-graph maintenance | Would collapse proposal versus authority and expand the schema/workflow far beyond this slice. |
| Model-generated conversation compaction and semantic retrieval | Initial bounded exact history and source links are testable. Add only after return/context metrics expose a real need. |
| Destructive retention pruning or redaction | Immutable packet/proposal references and backups require an explicit deletion/retention design. |
| Mixed chapter/material atomic adoption and arbitrary multi-range edits | Current chapter preparation/scope and material adoption have different proof contracts. Do not fake atomicity by composing endpoint calls. |
| Unlimited automatic document creation/reorganization | Risks clutter and author ownership. Start with task-scoped drafts and proposed organization. |
| Multi-agent debate, background planning loops, model-selected provider fallback | No demonstrated necessity; add cost and uncertainty while violating the bounded first-loop objective. |

## Evidence register

Repository links are pinned to the baseline. Phase tables distinguish proposed new files from existing modules identified through inspected imports, source, or the pinned repository tree; not every transitive implementation file was read in full. Test sources describe assertions, not an execution claim. The register also identifies historical ADRs whose UI rules or temporal statements must not be mistaken for current code.

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

<a id="s7"></a>
**S7.** **Shell and observed question-gate mechanism.** [chooseQuestion, idea entry, question dispositions, bottom notice](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workshop.tsx); [question-disposition expectations (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workshop.test.tsx); [library, modes, active editor, close integration](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Workspace.tsx#L1-L165); [historical tab/action UX and preserved safety constraints](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0027_AI_WRITING_WORKSPACE.md).

<a id="s8"></a>
**S8.** **Editor lifecycle and uncertainty.** [DocumentSession](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/editor/session.ts); [durable Apply and exact-receipt reconciliation assertions (not executed here)](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/editor/apply.test.ts); [editor ownership contract](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0001_EDITOR_CONTRACT.md).

<a id="s9"></a>
**S9.** **Proposal preparation and Apply.** [Proposal / PreparedProposal / ProposalDecision](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/projects/proposals.rs#L1-L180); [Tiptap preparation and guarded commit](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/shell/Writer.tsx#L1-L180); [scope, result/packet and provider-status presentation](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/src/assistant/FeedbackPanel.tsx#L1-L170); [explicit durable Apply contract](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/ADR_0004_PROPOSAL_APPLY.md).

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

<a id="s20"></a>
**S20.** **Prepared proposal storage and operation domains.** [proposal versions/decisions, composer intent and cross-domain collision guards](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/src/storage/008_proposals.sql).

<a id="s21"></a>
**S21.** **Test and native-check definitions.** [integration-suite registration and coverage guard](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/crates/core/tests/integration.rs); [full/focused check commands and evidence boundaries](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/docs/TESTING.md); [native versus installed qualification](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/tests/native/README.md); [actual native script names](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/package.json); [real Tauri/WebView2 synthetic Workshop harness](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/apps/desktop/scripts/native-workshop-smoke.mjs#L1-L145). No execution is claimed.

<a id="s22"></a>
**S22.** **CI wiring.** [native artifact producer, consumer fan-out, aggregate gate and documentation path exclusions](https://github.com/FZWINGEL/WebnovelStudio_V3/blob/6ab150505802b6c240db884a83f0a4eb9511df79/.github/workflows/ci.yml). Workflow source is not a successful run result.

## Open questions

| Unresolved choice | Default for implementation |
| --- | --- |
| Conversation retention/compaction policy | Retain durable local records, paginate, and use bounded exact eligible history. No generated compaction or destructive deletion in the first slice. |
| Timing of cross-document atomic adoption | One target first; then up to three nonchapter targets through shared Workshop mechanics and new chat-origin proof. No mixed chapter group. |
| Meaning of “maintain” for structured versus prose material | Maintain versioned prose/block drafts and explicit relationships within a task. Defer semantic field synchronization and inferred facts. |
| Timing of app-server default | No change in this plan. Exec default and lookup-on-Exec remain fixed until a separate qualified decision. |
| Provider history disclosure | Always disclose app-selected history and exact packet evidence; expose upstream thread/settlement diagnostics on demand without making history authoritative. |
| Desired automatic organization | Proposed titles/placement and task-scoped draft creation only; evaluate before modifying existing organization automatically. |
| Draft storage versus reusing only `PreparedProposal` | Use isolated document revisions for generic planning drafts; retain `PreparedProposal` for chapter edits. Review the role-isolation cost at Checkpoint A, but do not substitute a lossy text blob or silently mutate accepted heads. |
| Exact conversational adoption | Optional after Phase 2; button first. Approval must bind a deterministic exact preview and pass the same transaction checks. |

## Assumptions

The exact pinned commit remains the reference, while implementation may need migration renumbering after an explicit newer-branch comparison. Three small tables, one document-role addition, and versioned optional packet metadata are acceptable for genuine project persistence; no second story-content or run store is acceptable. Existing portable SQLite projects remain the data boundary. New draft roles can be enforced across all core consumers before UI exposure; failure to prove that blocks the slice. Ordinary source-epoch behavior, manual writing, reviewed authority, provider selection, and historical byte validation remain unchanged. Proposed counts, study size, and phase limits are engineering defaults, not measured optima. Qualified Windows/native and live-provider environments are available to the implementer later, but were not available or exercised in this design pass. This plan authorizes no implementation, commit, provider call, or publication by itself.
