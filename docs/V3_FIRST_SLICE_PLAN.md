# WebnovelStudio V3 — first-slice implementation plan

**Current provider direction:** the user authorized the V2 Codex reference and real LLM requests. Codex is compatibility-checked against the installed CLI at connection time; V3 does not pin a Codex version or executable hash. All background summary, story-memory, and other maintenance calls use GPT-5.6 Luna with xhigh reasoning. Author-facing writing and revision uses the persistent V2-style provider rail with model search, favorites, keyboard selection, and separate traits. Configurable OpenAI-compatible endpoint profiles and the native HTTP worker are implemented as a development surface with explicit Settings configuration and no silent substitution. HTTP lookup is not supported yet; V2 CLI adapter parity, dynamic Codex model discovery, hosted HTTP qualification, and broader live-provider coverage remain open. Historical 0.153.3 dispatches remain dated evidence only. See [ADR 0011](ADR_0011_LIVE_CODEX.md), [ADR 0023](ADR_0023_OPENAI_COMPATIBLE.md), and [implementation status](IMPLEMENTATION_STATUS.md).

**Baseline snapshot, 5 September 2026:** W0 was implemented as a native editor spike; W1/W2 work had landed at the stated boundaries and W3 registry/transfer work was active. Later implementation status and qualification evidence are maintained in [implementation status](IMPLEMENTATION_STATUS.md).

**Language scope:** English authoring, UI, and export. Wuxia, xianxia, cultivation, and translated-Chinese-webnovel register/terminology are optional English writing styles. Chinese-language authoring and Pinyin qualification are not product requirements. Unicode regression fixtures remain internal correctness checks.

**Authority:** [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md). The detailed contract, evidence limits, and source references in that document apply here.

**Original proposal plan:** [Supplied Pro plan](references/pro/V3_FIRST_SLICE_PLAN.md). This document narrows its delivery claims; it does not replace the architecture contract.

**Companions:** [Workspace plan](V3_WORKSPACE_PLAN.md) and [V2 migration evidence](V2_MIGRATION_EVIDENCE.md).

**Adopted context extension:** [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) and its [first-slice plan](V3_STORY_CONTEXT_FIRST_SLICE.md) add planned C0–C6 work to this dependency order. They preserve the base document, save, Apply, lifecycle, and authority contracts; they do not replace them.

The W0 native editor and shared contract fixtures are now exercised. See [W0 qualification](W0_QUALIFICATION.md) for the executed evidence and remaining English native author trials. This does not close milestone A or later native release gates.

## 1. Three milestones, one traceable dependency order

The plan has three deliberately different outcomes. A and B are executable development proofs. They are useful for author trials, but they are not shipping claims. C is the first release qualification gate because it includes the installed native package and the evidence required for a real-manuscript trial.

### A — Writing: development-native author trial

W0–W3 should produce a real Tauri development build in which an author can create project A from a character note, create project B from a blank chapter, write offline, switch between them, resume each independently, make a consistent manual backup, recover it as a new project, and export an explicit UTF-8 plain-text draft. The trial uses the Rust/SQLite persistence path from the beginning and does not substitute an in-memory database.

This milestone proves ordinary writing and project ownership after the save/reconcile safeguards are in place. It does not prove an installer, native accessibility qualification, provider support, continuity checking, or V2 migration. There is no Import V2 button or disabled imitation of one in A; that action begins only at the evidence-backed F1 gate.

### B — Feedback: deterministic, author-controlled assistance

W4–W6 should add persistent discussion, a deterministic mock job, and one explicit passage-edit request that produces reviewable proposals. The author may inspect, edit, apply, or reject one proposal at a time. A proposal uses a saved source revision and exact scope; privileged author-room material enters a prose-producing request only through an author-approved safe brief and permitted sources.

The feedback trial begins only after W6 has proven Apply lost-acknowledgment recovery, a shared project/document/session lifecycle guard, and history safety around Apply, undo, restart, and Stop. The early E4 friction check asks an author to type while feedback is running, observes the proposal becoming stale, and records whether the resulting refresh choice is understandable. It does not justify automatic rebasing.

Batch Apply, Apply all, automatic fact activation, and broad provider support remain deferred until a named later gate. Reviewed-story continuation and ready-bundle/export slices now exist as development surfaces with separate qualification boundaries. If Apply all is enabled later, its contract is all-or-nothing and every member must be validated against the same source.

### C — Release qualification

W7–W8 should qualify the installed Windows package and one explicitly supported live provider. W7 is the native release gate: packaged WebView behavior, offline installation, keyboard/dead-key input, clipboard, focus, accessibility, recovery, and draft/export behavior are tested on the nominated Windows configuration. W8 is the separate live-provider gate; it is required before describing that provider as supported, but it is not required for manual offline writing.

The product must not be called shipped, native-qualified, or ready for an actual manuscript trial from A or B alone. Q remains a separate narrative-quality evaluation; infrastructure and mock pass counts do not establish it.

### Work-package traceability

| Milestone | Work packages | Required evidence | Claim enabled |
|---|---|---|---|
| A — Writing | W0, W1, W2, W3 | M/P plus a development-native Windows author trial | Offline writing, project switching, recovery, and explicit draft export are usable in the development build |
| B — Feedback | W4, W5, W6 | M/P, native reruns of the failure cases, and the early E4 friction check | Deterministic discussion and author-applied scoped suggestions are understandable and recoverable |
| C — Release | W7, then W8 when live assistance is claimed | M/P/N; L for the selected provider; Q remains separate | Installed manual-writing release; one supported live assistant only after its own qualification |

## 2. Keep five kinds of evidence separate

| Gate | What it proves | What it does not prove |
|---|---|---|
| M — mocked UX proof | Author flows and understandable scopes with deterministic responses | External protocol reliability, billing behavior, prose quality |
| P — persistence correctness | Real file-backed transactions, CAS, idempotency, recovery, and project ownership | Native keyboard/accessibility or narrative correctness |
| N — native qualification | Actual installed Windows/WebView editor, clipboard, focus, keyboard input, accessibility, recovery, and packaging | Reliability of an untested provider or another OS |
| L — live-provider acceptance | One explicitly qualified provider/model configuration honors its advertised contract | Exactly-once upstream execution, all provider versions, full-novel quality |
| Q — narrative-quality evaluation | Measured usefulness of context and suggestions on selected author tasks | General quality across genres, languages, lengths, or all models |

M/P establish the technical foundation. A development-native trial is evidence toward N, not N itself. L is additionally required before describing a real assistant as supported. Q informs later narrative work and is never replaced by infrastructure pass counts. Do not retire V2 for an actual manuscript merely because M or P passes.

## 3. Dependency-ordered work packages

Each package should be reviewable without reading the whole application. Write its contract and fixtures before wiring the next package. The completion criteria below define their owning work packages; only the checks explicitly recorded in [W0 qualification](W0_QUALIFICATION.md) are claimed as executed. All save/reconcile safeguards required for a persistent-writing trial belong in W0–W2. W5 may prepare the feedback path, but the B author trial remains gated on W6.

### W0 — Native editor spike and contract lock

**Dependencies:** none. **Gate:** initial N-spike evidence; E1 begins here.

Create the smallest Tauri development window with the intended restricted Tiptap schema and a persistent editor instance. Record exact dependencies and runtime versions. Write the ADR fixing snapshot authority, block IDs, UTF-16 anchors, inline versus structural scope, Apply barrier, lifecycle identity, and recovered-copy restore. Keep generated DTOs and shared golden fixtures in the repository.

**Done when:** English keyboard/dead-key input, formatted cross-paragraph selection, repeated English quotations and Unicode names/emoji, focus transfer to a selection composer, scene breaks, clipboard paste, and a strict replacement/undo cycle work in the actual development window. Capture failures as reproducible fixtures. A browser-only prototype does not close this package. This is a qualification spike, not the production shell or the N release gate.

**Current W0 status:** source and automated native checks exist, with [ADR 0001](ADR_0001_EDITOR_CONTRACT.md) fixing the implemented snapshot/identity boundary. Feedback and replacement are session-only; Rust validates snapshots but does not persist them. The English author trial, minimum-window/DPI behavior, external Word paste, and assistive-technology trial remain open.

### W1 — Canonical documents and scope validation

**Dependencies:** W0 representation decision. **Gate:** P foundation.

Implement the restricted document schema, canonicalization/hash contract, block identity rules, Unicode endpoint conversion, structural token iterator, and independent Rust scope validator. Implement JS strict replacement preparation in detached state. Keep document concerns independent of Tauri, model providers, and SQLite.

**Done when:** shared JS/Rust fixtures agree on body hashes, quote boundaries, valid/invalid anchors, split/merge/move/copy IDs, and proposed result snapshots. Mutation tests reject changes to every kind of unselected content: text, marks, links, block style/identity, and scene boundaries. Include surrogate-pair and combining/ZWJ cases, repeated occurrences, empty blocks, inline-only grants, cross-paragraph replacement, and malformed/oversized input. No general PM-step interpreter is introduced in Rust.

**Current W1 status:** Rust structural scope validation is implemented with fixtures. The complete validator evidence, mutation/failure coverage, and JS/Rust qualification remain open.

### W2 — Real project persistence and document session

**Dependencies:** W1. **Gate:** P.

Build the core project's owned connection thread, migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations. Add the frontend `DocumentSession` with immutable in-flight payloads, generation watermarks, serialized saves, lifecycle identity, and error buffers. Use file-backed SQLite in integration tests.

**Done when:** delayed acknowledgments never replace newer editor text; the same operation/payload is idempotent; changed payload with a reused ID fails; stale versions/leases fail; definite save errors retain the live buffer; uncertain outcomes fence and reconcile before further writes; and every command carries the project/document/session identity needed to reject late callbacks. Kill the process after commit but before acknowledgment and recover the correct current body. Read back WAL/FULL/foreign-key configuration in the test binary. These safeguards precede the A author trial and a polished editor toolbar.

**Current W2 status:** core and frontend session work, persistent UI integration, default Workspace wiring, and file-backed save/reconciliation paths are implemented at development boundaries. The broader author trial and complete persistence/recovery evidence remain open.

### W3 — Library, free-order work, recovery, and the A trial

**Dependencies:** W2. **Gates:** M/P plus development-native author trial.

**Current W3 status:** registry, library, transfer, recovery, and free-order workspace flows are implemented at development boundaries. The A-trial gate, broader native dialogs, and release qualification remain open.

Implement New/Open/Rename/Duplicate/Archive/Locate, blank note/character/chapter creation, last item/caret persistence, and detach-after-flush switching. Add thread/composer resume state with W4 when conversations exist. Add the OS project lock and normal second-launch activation behavior. Keep project identity and receipt namespaces explicit across copies. Do not add Import V2; its evidence gate is F1.

The first export is an explicit UTF-8 plain-text **Export draft** action from a flushed, frozen current source. It is labelled as a draft, may lose rich formatting by design, and does not imply ready, publication, or continuity validity. Markdown and richer selected-source exports remain C work.

The recovery fixture must cover a consistent database snapshot, archive manifest, hashes for any referenced files, a recovered project with a new identity and fresh active operation namespace, and an untouched original. Copied receipts retain their historical namespace and cannot satisfy new-operation lookups; test reusing the same operation ID in the recovered project. Cover interruption at each staging boundary, registry failure after installation, and duplicate physical project identity. Empty attachment sets are valid in A; full legacy/asset-import fixtures belong to their feature gates. A failed restore never replaces the original or lists incomplete staging as completed. Automatic daily/weekly retention is deferred until a named release task.

W3 tests connection/session isolation using a controlled background ownership fixture. The actual “restore A while B streams” test follows W4's job implementation and is mandatory at W6; it is not a circular prerequisite for A.

**A author trial:** in the actual Tauri development build, create A from a character note and B from a blank chapter, type offline in both, switch only after flush, restart, resume each exact item, create the recovery copy, and export a draft TXT. This demonstrates an executable native author path; it is not an installed-package or N qualification claim.

### W4 — Persistent conversation and deterministic jobs

**Dependencies:** W2/W3. **Gates:** M/P; no B trial yet.

Before or alongside the persistent conversation work, integrate C0–C3 from the adopted Story Context extension. Freeze source snapshots and exact eligible sources with a source epoch; provide deterministic exact retrieval with dirty-index fallback; reject requests when mandatory content exceeds the selected budget; carry scoped author guidance; and persist the actual delivered packet plus an inspectable receipt. C0–C3 are implemented at their stated local boundaries; their evidence does not transfer save/Apply or authority ownership into the context subsystem.

Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a mock provider with controllable barriers. The mock supports delayed first output, malformed structured response, partial failure, slow cancellation, completion/Stop order reversal, and exact repeatable edit suggestions. Create no manuscript mutation path in the provider layer.

Keep the author room separate from prose-producing context. Whole-chapter discussion may use the saved whole chapter and author-room material. When privileged author-room material is deliberately selected for transfer, the prose-producing request requires an author-approved safe brief and permitted source set; privileged discussion text is not replayed automatically. Merely having private notes does not add a confirmation step. An ordinary explicit edit is one author request that yields a reviewable proposal from permitted context. Saving or opening a chapter never starts a request.

**Done when:** passage discussion displays the historical quote and limited scope; missing models leave writing usable; Stop targets the exact run; terminal states never regress; duplicated/reordered events reconstruct one output; reload and project switching recover discussion and job state; and explicit retry creates a linked new run, not a disguised resume. These are proposed checks.

### W5 — Review cards, prepared snapshots, and single author Apply

**Dependencies:** W1/W2/W4. **Gates:** M/P; B trial remains blocked until W6.

Implement source-bound proposal preparation, editable prepared versions, exact diff preview, single Apply, and Reject. Apply uses the short local mutation barrier, a preflighted in-place editor transaction, a durable decision/before/after/receipt transaction, and saved-generation handoff. Add selection toolbar, context-menu action, and keyboard/menu alternative. Do not implement Apply all or a batch control in this milestone.

Every proposal also binds the frozen context snapshot, exact target and scope, source epoch, policy, and context receipt. F2 alone owns reviewed authority; a context packet or generated digest cannot accept canon.

**Done when:** architecture scenarios 2 and 3 pass through real IPC/database commands; applying one of three edits leaves the others undecided and stale; repeated Apply cannot mutate twice, including with a new operation ID; and a selected sentence cannot change neighboring paragraph text, style, or boundaries. A whole-chapter rewrite is explicitly different. Composition is allowed to finish before Apply, and no provider wait occurs behind the barrier. If batch Apply is added later, its all-or-nothing contract must be separately tested.

### W6 — Lost acknowledgment, shared lifecycle, history, and interruption hardening

**Dependencies:** W3/W4/W5. **Gate:** P with native reruns; this closes the B trial gate.

Implement in-session history boundaries, significant undo/redo checkpoints, post-restart comparison/explicit restore, and full reconciliation of pending operation IDs and latest heads. Use the shared document lifecycle guard for Apply/reconciliation, editor disposal, switching, normal close, and application-controlled reload. Forced renderer loss starts a new fenced session. Background jobs retain their own project/run ownership outside that guard. Add process-level crash tests and Windows process-tree fixture support. Prioritize fixing invariants over adding UI surface.

Restart, fence, and Stop coverage includes the context snapshot, source epoch, policy, delivered packet, and receipt so a late or repeated context operation cannot alter a newer request or trigger an implicit paid retry.

Run the early E4 friction check before considering selective invalidation or rebase: type while a mock feedback request runs, make the proposal stale, and record whether the author understands refresh and scope. Keep conservative staleness after the check unless measured evidence justifies a bounded alternative.

**Done when:** a committed Apply whose response is lost is recovered once; a later saved body remains newer than an old receipt; Ctrl+Z after restart is not presented as durable historical undo; Stop passes both orderings; restore A while B runs preserves B's database/job; and failure after each transaction statement leaves an inspectable old-or-new state. Disk-full and permission errors retain the live buffer. No hidden external retry or automatic paid restart occurs.

**B feedback trial:** only after W6, exercise chapter discussion and one explicit scoped edit through the real IPC/database path with the deterministic mock. Inspect, edit, Apply, Reject, refresh stale work, Stop, restart, and project-switch while recording author-understandability evidence. This is a feedback proof, not a release claim.

### W7 — Explicit exports, optional author-reviewed snapshot, and packaged native qualification

**Dependencies:** W3/W5/W6. **Gates:** M/P/N complete.

Add Markdown beside the early draft TXT export, frozen source manifests, omission preview, and separate working-draft export. An optional author-review checkpoint records its exact source and author-only coverage, without creating a second working body or implying continuity validation. Author-only ready bundles and the F2-B reviewed basis freeze now exist in core; continuation UI/live dispatch and ready export exist as later development slices, while automatic fact activation remains gated F2 work. The initial export action is Export draft; broader publication and ready-story workflows remain separately qualified.

Finish keyboard navigation, accessible labels, focus restoration, resizing, native dialogs, and offline NSIS/WebView2 installation. Remove test-only command access and embedded automation services from shipping builds. Qualify the actual packaged WebView on the nominated Windows configuration: keyboard/dead-key input, clipboard, focus, accessibility, high DPI, long-chapter behavior, process/renderer recovery, backup recovery, Unicode/formatted projections, and export omission behavior.

**Done when:** the packaged two-project writing/feedback/restart/backup/export journey passes every P0 case applicable to its enabled features, the offline installer launches without model discovery, and the recorded native evidence supports an N claim. Deferred batch/import/ready operations remain unreachable until their own gates pass. Publication is not inferred from exporting. Manual paths remain usable without model configuration.

### W8 — One qualified live provider

**Dependencies:** W4/W6 and early E3 results. **Gate:** L.

Qualify one exact installed provider/model configuration, initially the documented noninteractive Claude Code adapter if E3 supports its containment and honesty contract. Use explicit model/traits, frozen application input, disabled tools, a qualified executable path, Windows Job Object containment, and visible internal-retry limitations. If safe text-only isolation cannot be demonstrated, use the direct HTTP Responses adapter for the first supported live path and leave the CLI version disabled. This is a qualification decision, not permission to add a generic shell runner.

Use one deterministic, fully recorded context packet for the first provider qualification. The C6 bounded read loop is an additional qualification step after that one-packet route; it is not required to establish the offline writing path.

Qualification order is not the author's default selection. Preserve explicit model preferences, including a configured Codex preference, and label an unqualified adapter unavailable. Any choice of a different adapter for a live trial is explicit; a failed run never silently retries on the qualification candidate or HTTP fallback.

**Done when:** opt-in trials verify selected/reported model handling, supported/unsupported traits, streamed completion, refusal/truncation, authentication error, partial/broken stream, Stop behavior, process cleanup, and recovered terminal history. Qualify native key entry/credential storage and inspect logs/backups for credential leakage. Record where the provider's final upstream prompt or internal retries are opaque. Explain that local cancellation/idempotency do not prove one external billable attempt.

The mock remains the reproducible failure harness. A second live adapter or broader provider catalogue requires the same contract suite and a new named qualification gate; it is not part of the first supported release by default.

## 4. Follow-on packages before stronger claims or adoption

| Package | Dependency and scope | Claim it enables |
|---|---|---|
| F1 — V2 migration | Use the source and schema evidence recorded in [V2_MIGRATION_EVIDENCE.md](V2_MIGRATION_EVIDENCE.md); implement staged read-only import and reconciliation for the tested schema, preserving active semantics versus inert legacy evidence; import context evidence and rebuild projections | Safe migration of those tested schemas, not arbitrary versions or lossless inference of missing anchors |
| F2 — Reviewed story boundary | Add typed accepted rules/records, explicit exceptions, dependency links, and conservative suffix fences; retain sole ownership of reviewed authority and complete scenario 5 | Core now freezes an exact reviewed prefix plus current working target with immutable reader-position pins; continuation UI/live dispatch and ready export exist as development slices with separate qualification, and this is not exhaustive continuity |
| F3 — Context quality | Own C4 generated digests, C5 thin temporal/relationship/thread views, richer source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search, and freshness checks; C5 depends on F2; evaluate permissions and omissions | Measurable task-appropriate context, not guaranteed knowledge safety or better novels |
| F4 — Narrative evaluation | Run author-labelled retrieval/continuity cases and independent prose/author-acceptance trials for supported English tasks | Evidence for a specific memory/prompt improvement; possible justification for richer retrieval or less conservative rebasing |
| F5 — Batch Apply | After B, implement same-base disjoint preparation and one atomic Apply/decision transaction; validate the common context snapshot, policy, and source epoch in that batch; test overlap, repeated operation IDs, stale/already-decided members, and lost acknowledgment | Apply selected batch with all-or-nothing behavior; individual Apply never implicitly accepts the rest |

F2 begins with the bounded author-only prose review in [ADR 0012](ADR_0012_AUTHOR_REVIEW.md): durable staging, exact chapter versions and earlier prefix, explicit review, current validity, and preserved historical bundles. Its empty typed-record set is deliberate and requires no paid analysis. Later slices now add reviewed generation, ready export, and passage-backed reviewed evidence through [ADR 0016](ADR_0016_STORY_CONTINUATION.md), [ADR 0017](ADR_0017_REVIEWED_EXPORT.md), and [ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md). Typed accepted rules, summaries, issue decisions/exceptions, and the complete reviewed source resolver remain future F2 work.

The resolver slice in [ADR 0013](ADR_0013_REVIEWED_CONTEXT.md) is implemented in core/IPC: it freezes an exact valid earlier reviewed prefix while retaining the continuation target as working prose, stores immutable reader positions, and preserves historical validation. A user-facing continuation action, bounded live model dispatch, and ready export now exist as later development slices; broader provider and release qualification remain separate.

The V2 schema/source inventory and bounded schema-8 importer are implemented with explicit source choices, staged independent installation, and synthetic reconciliation evidence; broader native recovery and representative author acceptance remain F1 work. Backups and independent recovery are part of W3. V2 retirement for a real manuscript requires W7, the applicable W8 provider gate when used, F1's tested-schema reconciliation, a successful recovered-copy exercise, and export checked against chosen source revisions. Keep the original V2 snapshot/application until the author accepts those results.

Basic source freezing, exact scope, and exclusion of privileged planning material from prose-producing requests belong to B and are not deferred to F3. C0–C3 add the deterministic context evidence foundation before or alongside W4; F3 owns C4/C5 and extends context quality and narrative coverage. C0–C6 are planned and tracked in the maintained Story Context documents.

## 5. Experiments that can change the architecture

| Experiment | Smallest decisive setup | Default and consequence |
|---|---|---|
| E1 — Native editor suitability | Restricted editor in the actual Tauri development window; English keyboard/dead-key input, selection composer, clipboard, screen reader, resize/focus; repeat minimal failures on a supported WebView | Keep Tauri/Tiptap. Trial Electron with the same editor/core only for an unresolved release blocker; direct PM only if wrapper behavior is the blocker. |
| E2 — Snapshot/Apply cost | Real IPC and file-backed FULL commits with approximately 20k and 250k UTF-16-unit fixtures, many marks/blocks, concurrent streaming; measure serialization, input-to-paint, barrier, memory | Keep snapshots and the short barrier. Targets remain proposals. Optimize renders/copies first; sustained measured failure is required before an incremental protocol. |
| E3 — CLI containment and honesty | Installed-CLI compatibility discovery; record the observed version/hash per request; isolated temp working directory; unexpected config/tool-loading attempt; child/grandchild fixture; Stop and internal-retry observation | Support only qualified modes. Do not pin a frequently updated Codex release. HTTP/OpenAI-compatible adapters remain explicit configurable routes. Billing/exactly-once guarantees remain out of scope. |
| E4 — Conservative conflict burden | At B: type while feedback runs, inspect stale proposals, refresh, and apply one of three suggestions. At F2: separately test the earlier-chapter review fence. | Keep conservative staleness initially. Design bounded rebase/selective invalidation only for a demonstrated burden and prove it against the adversarial suite. |
| E5 — Memory value | Labelled exact evidence/knowledge-boundary tasks plus author review of paired outputs using frozen model/settings; compare recent prose, exact-state/search, and any richer memory | Add a feature only for a measured failure it fixes. Attractive examples do not establish long-novel quality. |

## 6. Review and completion discipline

Every package should identify the affected invariant, contract fixture, failure test, and evidence gate. Keep core transaction logic reviewable without Tauri or provider credentials. Keep a small fault-injection API behind test-only builds so lost acknowledgments and race orderings remain reproducible. Review scopes and normalization together whenever adding an editor node or extension.

Tests and native trials described here are proposed completion criteria, not reported results. No delivery dates or estimates are supplied. The critical path is **representation → persistence/reconciliation → executable development-native writing → scoped application and history safety → native release qualification**. Live provider breadth and narrative intelligence are separate work, not prerequisites for safe offline writing.

**A success:** an actual development-native writing session survives the specified persistence and recovery failures with understandable author choices and no silent manuscript overwrite.

**B success:** deterministic feedback can be discussed and applied through the real transaction path after stale/Apply/history safeguards.

**C success:** the installed native package passes its proposed N evidence and can be evaluated for a real manuscript; a live assistant is supported only after its L gate.

**Not first-slice success:** an attractive browser mock, a passing browser suite, a fake Import V2 button, an unqualified installer, or a model that produces an impressive chapter once.
