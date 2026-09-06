# WebnovelStudio V3 — Story Context Engine first slice

**Implementation plan · 5 September 2026**
**Contract:** `V3_STORY_CONTEXT_SYSTEM.md`.
**Base:** `V3_ARCHITECTURE_REFINED.md` and `V3_FIRST_SLICE_PLAN.md`.
**Status:** C0 contracts, C1 working-basis evidence/retrieval, C2 frozen packets, parts of C3 guidance/conversation/source integration, and C4-A explicit chapter navigation memory are implemented. C4-B automatic generated-view reuse in working author-room discussions has local contract and native diagnostic evidence. Single-passage Apply, history, export, optional approved writing briefs, and author-only exact chapter review are wired into the native app. F2-B adds the reviewed basis manifest, immutable reader-position pins, and an explicit core/IPC freeze of the exact earlier reviewed prefix plus current working target. This does not add continuation UI, live dispatch, ready export, or typed reviewed-story records. The bounded Codex path uses the exact compiled packet with explicit application byte caps and durable delivery/usage outcomes; full live-provider qualification remains open. Current evidence is maintained in [implementation status](IMPLEMENTATION_STATUS.md). Richer relevance selection, higher-level C4 digests, C5–C6, whole-chapter/block/manual-rebind, and batch Apply remain open.

The supplied research and proposal are rationale for this adopted design, not newly verified validation or completed implementation evidence.

**Language scope:** English authoring, UI, and export. Wuxia, xianxia, cultivation, and translated-webnovel register or terminology are optional English styles. Chinese authoring, Chinese IME, and mixed-language narrative evaluation are outside this slice; Unicode cases remain internal robustness fixtures.

## 1. Smallest useful result

An author opens a real project, writes or changes a chapter, asks for feedback that depends on an earlier passage, inspects the story context actually supplied, and receives an inspectable proposal. The old evidence is retrieved from the right project and revision. The existing scoped Apply and Undo mechanism remains unchanged.

This proof does not require embeddings, automatic fact extraction, a graph database, a hosted memory service, or a new agent framework. It does require genuine project persistence and exact source/version binding. A browser mock alone cannot prove it.

The minimum scenario uses two projects and a synthetic English story with an old promise, an object transfer, a future secret, and repeated quoted text. All authoring works without a model connection. A deterministic mock proves context construction before any live provider is added.

## 2. Integration order

Extend the existing plan instead of restarting it. The canonical document/scope and persistence work in base packages W1–W2 comes first. Add the deterministic context kernel before or alongside W4's persistent conversation jobs. W5's proposal/Apply contract stays intact. The existing native and live-provider gates remain prerequisites for their respective claims.

The context system has two milestones:

**Evidence-first slice:** packages C0–C3 below. It proves source snapshots, broad exact-text availability, useful retrieval, persistent author guidance, and honest receipts. The C3 proof is a local implementation checkpoint; it does not establish release qualification or narrative quality.

**Richer memory extension:** packages C4–C6. It adds versioned navigation digests, thin temporal/thread views, and bounded provider-side lookup. These must not delay a useful offline authoring and feedback product.

## 3. Reviewable work packages

### C0 — Freeze the contracts and build adversarial fixtures

Define `StorySnapshot`, source handles, information policy, coverage labels, packet receipts, and budget errors. Use V3's existing document revisions and anchors; do not create another text representation with independent authority.

Create fixtures for current drafts, ready bundles, alternative drafts, author decisions, future private notes, generated digests, and their source dependencies. Include English and romanized-name aliases, accents, emoji, repeated sentences, and a flashback.

**Completion:** unit tests specify which sources are eligible for working, reviewed, historical, author-room, and restricted writing requests. An unreviewed digest never becomes canon; a future-derived digest never becomes earlier-scene-safe by changing its label. A missing timestamp remains unknown.

### C1 — Implement source snapshots and exact local retrieval

Reuse the project DB owner. Freeze eligible source revision IDs and authority/order heads. Reuse existing checkpoints; mark indexing dirty on saves. Build current passage projections and source maps, exact alias lookup, lexical search, and a literal fallback for short names.

Dirty/unindexed current text must remain discoverable through a slower exact-source fallback. A stale hit is revalidated before inclusion. Lookup expansion remains within the same snapshot and policy. Record a collection-level context-source epoch so a new relevant passage invalidates old prose proposals even when their returned sources did not change.

**Completion:** a changed earlier chapter is found at its new version despite an old index. A frozen request can still read its original version; a disclosure-policy change stops further access. New evidence in a previously unretrieved document invalidates the old proposal through the source epoch. Project switching cannot redirect lookups. Repeated quotes return distinct anchors. Index deletion/rebuild loses no manuscript or accepted memory. Cold snapshot cost is measured on a large synthetic project.

### C2 — Compile full-text and multi-resolution packets with receipts

The local implementation is a Rust pure compiler plus a Rust-owned durable preparation path exposed through native IPC. It validates every supplied source body, descriptor, projection, eligibility result, and optional edit scope against the frozen manifest and target. The exact target, author instruction, protected scope, and mandatory pins are preserved. When the complete eligible set fits the authorized allowance, it is included in full; otherwise the compiler keeps the target and mandatory sources whole, then extends a stable source/block-order prefix of complete blocks and records explicit omissions. Mandatory overflow returns `MandatoryContextTooLarge` without shortening the target. Preparation persists the exact messages, options, input hash, packet hash, and source receipt before any future provider submission; it performs no model call and does not mutate manuscript bodies. Repeating an operation is idempotent, while a changed story blocks new preparation. Existing packets remain inspectable with `current=false`, and policy revocation blocks old reads. The `mock-story-context` counter is a UTF-8-byte accounting contract for the deterministic mock only; it is not live-provider tokenization qualification. Twenty compiler tests, nine durable-receipt tests, and six context migration tests cover this local boundary.

Implement task recipes, mandatory pins, exact target preservation, provider-aware budgeting, and explicit gaps. Include the full eligible text when it fits the authorized allowance. When it does not, pack valid existing summaries and targeted original passages; no generated summaries are required yet.

Coverage labels distinguish verbatim, digest, and directory-only. Preserve the exact submitted messages/options and source manifest before external submission. Avoid relying on a provider session as the only stored request state.

**Completion:** tiny-budget and oversized-target fixtures return `MandatoryContextTooLarge`, not a shortened target. No disallowed note, metadata title, source quote, or old chat crosses the writing policy. The receipt matches exactly what the mock receives. More budget increases useful evidence coverage without duplicating chunks merely to fill space. The local compiler coverage now includes 20 tests.

### C3 — Integrate conversation guidance and the author-facing inspector

The current W4 discussion surface integrates the local guidance portion of this package: “Keep as guidance” opens an editable form prefilled from the selected user or assistant message, and the author can also add guidance directly. Explicit Save creates or edits an immutable exact version; Retire marks the current head inactive. Scopes are Next request (one use on the target document), This document, and This project. A frozen guidance value is separately typed, included exactly as mandatory AuthorRoom packet input, receives distinct mutation/packet handles, and is visible in the inspector. The Rust-owned path uses CAS/idempotent operation receipts, advances `context_source_epoch` on changed guidance, retains guidance across recovery with old receipt authority fenced, and consumes request guidance only in the same transaction as a successfully persisted discussion request. GuidancePanel tests cover lost acknowledgments and late project/document responses. Restricted writing excludes all current guidance, and no guidance operation mutates the manuscript.

Recent author-room discussion context is implemented as a bounded recency selection: up to four complete delivered exchanges and 16 KiB of exact turn records from the same current document thread and policy. Frozen snapshots and receipts retain the exact messages and scopes; the inspector shows supplied exchanges and omissions separately from guidance and story evidence. Stopped or partial output, other documents, revoked-policy turns, and copied historical threads are excluded from automatic reuse. This is recency selection, not semantic retrieval or adopted story truth. See [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md).

An explicit linked retry of a stopped, failed, or interrupted discussion retains its exact original request-scoped guidance when those versions are still active. Feedback, selected scope, and ordered source pins must match the original request; editing any of them starts a new request. Current document/project guidance and current permitted story sources are compiled afresh. Newly waiting request guidance is reserved for the next new request. Edited or retired inherited instructions, revoked policies, completed runs, and recovered-copy links are refused. The schema-7 composer stores the retry link with its draft and immutable save receipt, so navigation/reload preserves the choice. The original guidance-use receipt remains the only consumption record; retries do not consume it again.

Persistent document/project source choices are implemented for AuthorRoom discussions. **Keep source…** opens an explicit confirmation form; **Include next time** remains a one-request choice. Rust merges current saved choices with transient pins into the frozen packet, retains exact mandatory-source receipts, and refuses unavailable or oversized required sources. The current target is already mandatory and is included once. Restricted edit requests exclude these saved discussion choices. Changed choices advance the source epoch, while retries preserve their original transient request identity. See [ADR 0007](ADR_0007_DISCUSSION_SOURCE_PINS.md).

The selected-passage `ProposeEdits` composer now supports an optional writing brief. **Adapt as writing brief** can start from either an author or assistant message; the author edits the text and presses **Approve this brief** before sending. A direct brief is also allowed. Editing the text or selected scope clears approval, switching to **Discuss** removes the brief, and ordinary edit requests need no brief. Schema 11 persists the optional brief and approval state with the discussion draft, including unconfirmed text while it is being edited; reload/restart does not silently approve unfinished text. The restricted packet receives exact `approvedWritingBrief` text only; its origin ID, source message, private chat, pins, and guidance are not delivered. Retry keeps the same approved identity, and old receipts without the optional field remain compatible; historical replay and recovered copies cannot authorize a new request. Creating or changing a brief does not trigger paid autosave analysis or change the existing Apply path. See [ADR 0008](ADR_0008_WRITING_BRIEF.md).

Local native source-choice and writing-brief checks are development evidence; hosted native checks, installed-release qualification, and narrative-quality gates remain separate.

The deterministic mock is wired through the existing discussion/proposal workflow. A selected sentence can use broad read-only context without widening its editable scope.

**Completion:** the author can inspect an old promise and its exact setup before accepting one proposed edit. Rejected alternatives remain unaccepted. A changed target makes the proposal stale. Restart reconstructs the context receipt and discussion without a provider session. An instruction confirmed as guidance survives omission of old chat but remains scoped and versioned. The implemented guidance, source-choice, and writing-brief portions meet their local persistence and policy boundaries; richer conversation selection still gates full C3 completion.

**Milestone:** C0–C3 is the evidence-first first slice.

### C4 — Add generated digests without creating automatic canon

Implement source-bound generated chapter/scene digests and optional higher-level navigation. Keep them separate from accepted summaries. Semantic generation uses the existing explicit job lifecycle and author-authorized model/settings. No paid analysis on autosave.

Record all input dependencies, not just displayed citations. A result for an old source version may remain historical but cannot become the current view. Rebuilds use original evidence, not an endless chain of lossy rolling summaries.

The C4-A development cut implements a **single-chapter navigation digest** with the deterministic mock and the bounded Codex worker. It uses an explicit Refresh story memory action and a dedicated frozen analysis request containing the complete saved chapter. It excludes other chapters, notes, chat, and generated summaries from this first recipe. It may describe only that chapter; claims about earlier promises, character knowledge, or later payoffs require a future contextual recipe with all of those dependencies recorded. The panel exposes retained results, source evidence, changed-source labels, Stop, and local save reconciliation. C4-B reuses current non-target chapter views in working author-room discussions when full prose does not fit, through the separate frozen generated-view contract in [ADR 0015](ADR_0015_NAVIGATION_CONTEXT.md). The original evidence remains available; restricted writing and reviewed continuation exclude generated views. Current verification is recorded in implementation status.

Implement in this order:

1. Add the pure `navigation-digest.v1` response contract and validation of exact evidence references, UTF-16 ranges, quotations, sizes, and uncertainty fields. This contract cannot propose manuscript edits or activate accepted records.
2. Persist the analysis operation, exact input packet, producer settings, and terminal candidate/outcome through the existing project owner. A separate short installation transaction rechecks the frozen source and policy, records the immutable generated view and all dependencies, and exposes it only after commit. Recovery between those transactions preserves the candidate and reconciles installation without another model call. Bind the job, packet, snapshot, terminal result, and installation to the same project and operation namespace; a project switch cannot redirect them. Reuse provider execution and cancellation primitives. A memory refresh is not a discussion turn. Repeated local operations reconcile; retries never silently make another model request. A crash after dispatch with no terminal receipt remains interrupted with an unknown external outcome. Changed source/policy, Stop, and independent recovery must prevent inappropriate current installation while preserving historical output.
3. Expose views and source links in a small memory surface with an explicit refresh action. Distinguish a changed source needing refresh from policy-revoked material that is unavailable. Display the selected model and application allowance before a live call. Opening the surface and autosave perform no generation. Use ordinary author-facing copy such as “Generated summary” and “Check against the chapter”; source validation does not establish semantic accuracy.
4. Add digest use to packet packing only after a distinct immutable derived-view reference is supported in frozen context and receipts. A digest ID is not a manuscript revision ID, and digest text must never pass as an original `SourceRead`. Retain the full dependency classification, label delivered digest coverage, avoid duplicate full text/summary padding, and preserve exact-text fallback. Until this integration passes its tests, stored views are inspectable aids and are not claimed as supplied model context.

Unaccepted view installation does not advance `context_source_epoch`; manuscript, authority, guidance, and ordering changes keep their existing invalidation rules. A running request always retains its original view and packet versions. F2 still owns accepted summaries and story records. This cut does not add temporal state, entity merging, automatic checkpoint analysis, or a new provider framework.

**Completion:** late results, early-chapter revisions, future-source contamination, deleted indexes, and restored projects all preserve source/authority rules. Citation range validation rejects malformed evidence. The UI states that semantic correctness is unverified. Exact-text fallback still works with all generated digests removed.

### C5 — Add thin state, relationship, and thread views

Reuse reviewed story records. Implement a few valuable queries: last established possession plus known transitions; character belief/knowledge at a boundary; unresolved promise plus known payoff; explicit creative decisions for a scope.

Keep world time, disclosure order, and editorial versions separate. Generated observations can assist search but cannot activate reviewed state or clear readiness fences. Ambiguous names or intervals return uncertainty.

**Completion:** an early transfer change invalidates the appropriate view and preserves later prose. A rumor does not become truth, and a future revelation does not become earlier knowledge. An old open promise remains retrievable despite many unrelated later chapters. A partial extraction cannot justify “this never happened.”

### C6 — Qualify one bounded live-provider lookup route

Add request-scoped search/read/state/thread tools to a single provider with verified support, or a strict `needs_context` envelope for a provider with qualified structured output. Reuse the existing job supervisor, cancellation, and model selection. Persist each invocation packet before submitting it.

Enforce invocation and total token allowances. Batch read requests. Disable uncontrolled CLI filesystem/session access for restricted requests. Providers without qualified lookup retain the deterministic one-packet route.

**Completion:** Stop prevents later authorized calls once committed; duplicate tool events do not duplicate local reads or silently cause paid retries; budget exhaustion is visible; a broken stream preserves incomplete output; a crash at dispatch remains an explicit unknown external outcome. A fresh invocation after repacking is not mislabeled as provider-session resume.

## 4. Separate the evidence gates

| Gate | Required evidence | Does not prove |
|---|---|---|
| Mocked UX | An author can see sources, pin guidance, discuss, inspect a proposal, and understand missing context. | Retrieval completeness, native stability, or narrative quality. |
| Persistence correctness | Fault-injected snapshots, receipts, late results, crashes, source changes, project isolation, and duplicate requests pass. | That a model will reason correctly from the packet. |
| Native qualification | Existing Windows English keyboard/dead-key, emoji, and focus tests still pass while indexing and context preparation run. | Live-provider protocol conformance. |
| Live-provider acceptance | Actual selected model/traits, serialized packet, supported lookup protocol, usage, cancellation, and stream handling are verified. | General superiority over another memory design. |
| Narrative evaluation | Matched tasks and human-reviewed evidence show useful recall and author-preferred prose without added contradictions/leakage. | Perfect memory or universal fiction quality. |

## 5. Proposed evaluation fixture set

Start with **80 author-checked cases**, a proposed manageable engineering target rather than a validated sample-size requirement. Use eight cases in each of ten groups: exact/alias retrieval; paraphrase retrieval; old promises; temporal transfers; false belief/knowledge; flashbacks; revision invalidation; secret leakage through derived material; conversation decisions; and missing/ambiguous evidence. Use English author tasks throughout, with romanized names and Unicode accents/emoji where relevant; mixed-language narrative evaluation is outside this slice.

Store the question/task, story basis, allowed boundary, expected source spans, forbidden sources, expected uncertainty/conflict behavior, and a short adjudication note. These fixtures evaluate the memory mechanism; they do not need 80 paid model calls to test deterministic source eligibility and packing.

For quality comparisons, use the same model/traits and author tasks across recent-context, full-eligible-context, packed-context, and packed-plus-read-loop conditions. Report evidence recall, incorrect current-state assertions, forbidden input exposure, narrative leakage, author edits, latency, and tokens. Separate successful retrieval from successful reading and successful prose generation.

## 6. Only add complexity when a test identifies its purpose

Embeddings are justified by important English paraphrase misses, not by a dependency list. Automatic checkpoint analysis is justified by useful author outcomes and explicitly authorized cost, not by the existence of a save event. Larger lookup limits are justified by multi-hop cases that need them, not by an aspiration to run an unbounded agent.

Do not expand first-slice scope to autonomous canon updates, full-book generation, automatic critique after each chapter, a general workflow engine, persistent provider memory as authority, cross-project retrieval, or a graph/vector service.

The first release claim is: **the assistant receives inspectable, current, task-appropriate story evidence and can recover it across restarts.** Claims about improved long-form writing require the separate evaluation gate.
