# ADR 0016: Generate, preview, and apply a story continuation

Status: next implementation package; not implemented or qualified. Extends [reviewed context](ADR_0013_REVIEWED_CONTEXT.md) and the existing [Apply protocol](ADR_0004_PROPOSAL_APPLY.md). Current evidence remains in [implementation status](IMPLEMENTATION_STATUS.md).

## Author outcome

The next package must deliver a complete author action: choose a story basis, ask for a continuation, inspect the generated paragraphs beside the existing chapter, and explicitly Apply or Reject them. A preparation-only screen does not complete this package.

**Working draft** remains available without review. **Reviewed story** uses the exact reviewed earlier prefix plus the working target already supported by Rust. The interface explains that the target is still a draft and that author-only chapter review does not establish extracted facts or guarantee continuity. Missing or changed reviewed chapters block that basis with an explanation. Choosing Working draft afterward creates a new explicit request; it never changes the basis of an existing operation. The first chapter uses Working draft because it has no reviewed earlier prefix.

The chapter assistant owns this action alongside discussion and selected feedback. Keep the basis choice and continuation instructions there rather than adding project-wide toolbar configuration. Request identity and the persisted composer retain the chosen intent and basis; switching away from a captured passage must show the new end-of-chapter placement explicitly.

Continuation adds prose at the end of the current chapter. It does not rewrite existing prose. The preview shows the join with the existing ending and the complete added paragraphs. Writing may continue during generation; any intervening source edit makes the candidate stale for Apply. Manual writing, discussion, and selected feedback remain available independently.

## Output and placement contract

Use a separate versioned `continuation-output.v1` response contract. The current passage response contract forbids paragraph boundaries and cannot represent a continuation safely. The first continuation payload contains bounded plain paragraph text, with explicit total and per-paragraph limits. The producer does not supply document IDs, executable editor steps, HTML, or manuscript authority. Unsupported or incomplete output remains retained response text without an applicable candidate; no automatic repair generation occurs.

The initial wire shape is `{"schemaVersion":"continuation-output.v1","suggestions":[{"title":"...","paragraphs":["..."],"explanation":"..."}]}`. Require exactly one candidate, a nonblank title of at most 120 UTF-8 bytes, an explanation of at most 4096 UTF-8 bytes, and 1–128 nonblank paragraphs. Each paragraph is at most 8192 UTF-16 units; the total is at most 100,000 UTF-16 units and the complete raw JSON response at most 64 KiB. Paragraph strings contain neither CR nor LF; their array order supplies the paragraph boundaries. Preserve accepted whitespace and punctuation exactly. Generated formatting is outside this initial payload; existing manuscript formatting is protected and the author may format new prose after Apply. The immutable candidate row identifies the retained candidate; prepared-version hashes already bind author-edited text and result snapshots.

The request carries a typed append grant tied to the exact source head and end boundary, its frozen snapshot/packet, and its explicit basis. A whole-document grant is too broad for this operation. JavaScript prepares the append transaction and complete resulting snapshot. Rust independently validates the snapshot, exact candidate text, placement, and identity rules; it does not execute ProseMirror steps.

Add `ScopeKind::Append` to the existing grant family, with no start endpoint and an end bound to the final source block ID and its complete UTF-16 content length (zero for a scene break or empty paragraph). In this scope that endpoint authorizes insertion after the block; it does not select the block for replacement. Capture the final block's plain quotation and structural fingerprint, including its identity and formatting. Neither the existing Blocks nor WholeDocument scope can substitute for it: both permit replacing existing blocks. JavaScript owns creation of new block IDs during preparation; Rust verifies their freshness and exact placement.

For a nonempty chapter, every existing block, ID, mark, attribute, and boundary must remain identical, followed only by the approved new paragraph blocks with fresh unique IDs. For the canonical empty chapter containing one unformatted empty paragraph, filling that placeholder may retain its ID for the first generated paragraph; any later paragraphs require new IDs. This exception must match that exact empty shape and cannot authorize replacing a nonempty or formatted block. Existing trailing blank paragraphs in a nonempty chapter remain author content.

The empty exception depends on the exact current structural snapshot, including a chapter the author deliberately cleared; it does not depend on whether the document was newly created. Normal immutable history preserves earlier prose. The append builder must allocate valid IDs once and verify uniqueness before Tiptap preflight. It must not rely on `BlockIdentity` repairing IDs with fresh randomness during separate preflight and commit applications. Retain the exact prepared body and IDs across an uncertain preparation retry, or reconcile the stored prepared version before proceeding.

Tests must freeze this grammar before persistence or UI implementation, including empty chapters, formatting, scene breaks, Unicode, duplicate/reused IDs, edits before the insertion point, malformed text, and size limits. A shared golden fixture must demonstrate that the JavaScript transaction and Rust snapshot validator agree.

## Durable ownership and Apply

Extend the existing discussion/provider lifecycle and proposal review records with an explicit continuation variant. Do not create a second job supervisor or working manuscript. Old passage payloads, receipts, and hashes must retain their meaning. Review the minimum-reader schema floor before persisting new scope or candidate variants; perform the normal durable pre-upgrade backup.

Keep the existing proposal tables: add an immutable proposal kind defaulting existing rows to Passage, and a nullable prepared paragraph payload for continuation versions. Preserve old candidate JSON and single-line replacement text byte for byte. Reads, preparation, retention, and Apply branch on the validated kind; a continuation cannot enter `validate_text_replacement`, and a passage cannot acquire append authority by supplying a new field. A dedicated `PrepareContinuation` request carries the edited paragraph array and JavaScript-prepared result body, while Apply reuses the existing operation and acknowledgment fields. Open, backup, recovery, and policy-facing reads must validate the new variant and its full source/run/packet/decision chain.

Add a first-class continuation intent through request validation, intent reconstruction, run DTOs, mock output, provider completion, and retained composer/retry identity. Its basis field is omitted from old passage/discussion payloads so their hashes remain unchanged. The packet compiler accepts the new response contract only for Continue with RestrictedWriting and an append grant.

Rust freezes the chosen basis and complete request packet before dispatch. Restricted continuation excludes private author-room chat and unsupported generated memory; any approved brief transfers only its explicit content. Context coverage remains inspectable. The existing selected-model, delivery, usage, Stop, retained partial output, local-save recovery, and no-redispatch rules apply. A cancelled, incomplete, or unconfirmed external outcome cannot become an applicable continuation.

Candidate retention, preview preparation, and author decision remain separate. Editing the proposed paragraphs creates a new prepared version without a model call. Prepare validates the exact historical source and candidate; Apply additionally requires the current head, global source epoch, disclosure policy, owner namespace, current prepared version, and writer lease. Append validation does not inherit the chapter-memory freshness exception.

Reuse the existing short Apply barrier and atomic transaction: flush and preflight the mounted editor, then commit the new working body, before/after revisions, author decision, and operation receipt together. Only the confirmed result is displayed. Lost acknowledgments reconcile through the existing lease fence and operation identity. Repeated Apply cannot append twice. Undo/redo, restart, backup, recovery, and copied historical records follow the existing manuscript and namespace rules. Applying prose does not activate facts, summaries, ready bundles, or publication state.

Keep one Apply operation family and its existing receipt fields. Extend the typed proposal/prepared payloads and their validation through the transport and session; do not duplicate the author-operation supervisor. Definite source, reviewed-basis, or policy rejection must remain a terminal stale/refused attempt. It must not enter an uncertain-outcome retry that resubmits an operation known to be ineligible.

Reject is idempotent, remains available for stale candidates, and changes neither the manuscript nor the source epoch. A copied historical candidate cannot authorize an Apply or a new author decision for its original namespace.

## Dependency order and completion

1. Implement the pure response, append grant, and structural validation contracts with shared Rust/editor fixtures.
2. Extend durable request identity, candidate/prepared records, history validation, and migration while preserving old passage contracts.
3. Connect explicit Working/Reviewed continuation to the existing frozen packet and provider lifecycle, including mock behavior and failure retention.
4. Complete preview preparation, atomic Apply/Reject, replay, stale refusal, and reconciliation.
5. Integrate `FeedbackPanel`/composer, `ProposalPanel`, and `Writer` with typed continuation state. Keep the manuscript editor mounted and persist intent/basis. Verify editable preview, project switching, reload, Stop, exact context inspection, and stable prepared IDs after uncertain acknowledgment.
6. Run the native visible Writer journey from basis selection and reviewed earlier chapters through generation, edited preview without another model call, Apply/Reject, undo/redo, and reopen. Verify preserved block IDs, marks, scene breaks, trailing blanks, stale refusal after typing, policy revocation, retained incomplete output, and read-only copied history. Then qualify a bounded live request with the selected Codex profile; deterministic fault tests separately cover malformed/truncated output and uncertain outcomes.

This package is complete only when the author can use that entire journey. Full chapter rewrites, general block replacement, atomic batch Apply, manual rebind, typed accepted story records, richer navigation digests, temporal views, and model lookup remain explicit subsequent requirements for full V3.
