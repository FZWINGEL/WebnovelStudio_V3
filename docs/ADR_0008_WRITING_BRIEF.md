# Author-approved writing brief

**Decision:** add an optional request-scoped brief to selected-passage edit requests. This extends C3's author-room/writing-view boundary without changing manuscript authority or Apply.

## Author action

In **Suggest edits**, **Add writing brief** opens an editable set of directions. **Adapt as writing brief** can start from either an author message or an assistant reply in this document's author-room discussion. Both are source text for editing, not pre-approved directions. The author edits the text and presses **Approve this brief**. Adoption alone does not send a request. Ordinary edit requests need no brief and no additional approval step.

Editing the brief or choosing a different passage clears its confirmation. Switching back to **Discuss** removes it from the composer. The unsent text, origin reference, and confirmation persist with the existing discussion draft; a restart does not silently approve an unfinished brief. The author can remove the brief before sending.

For example, a private planning message can say that the mentor stole the lantern. The approved brief might say only, “Let him hesitate when he sees the lantern. Mei interprets his pause as grief.” The approved text is what crosses into the edit request. The original message and its private sources do not follow it.

“Approved” records an author choice about this text. It does not prove the prose cannot imply a secret, establish a story fact, mark a chapter reviewed, or grant permission to change anything outside the selection.

## Contract and persistence

`SafeBriefInput` contains `text`, optional `originMessageId`, and `confirmed`. The confirmation is an explicit interaction guard, not a proof of a human gesture. Rust checks the actual request, its project/session, exact chapter target, selected passage, and restricted writing policy. Empty, oversized, or unconfirmed briefs cannot start a request. A referenced origin must be a persisted author-room message from the same current project namespace and document, readable under the current policy.

The optional `safeBrief` field travels with the existing start, draft, and retry DTOs. Schema 11 adds a nullable JSON column to `discussion_drafts`. Unconfirmed draft text is allowed while the author edits it. The immutable context packet request records the exact submitted brief. No reusable brief table, new run state machine, or independent story-memory store is introduced.

The frozen story snapshot is unchanged. A brief is explicit request direction, separate from retained story evidence, persistent source choices, and author-room guidance. Creating or changing a composer brief does not advance the story source epoch or trigger a model invocation.

## Model input and inspection

The restricted packet envelope receives `approvedWritingBrief` as exact text only. It receives no origin message ID, original chat text, private source handles, or origin packet contents through this field. Existing policy/source exclusion checks still apply. The compiler treats the entire brief as mandatory input and refuses budget overflow rather than shortening it.

`PacketReceipt.safeBrief` retains the exact text, its hash, and the optional origin message ID for local inspection. The inspector displays the approved wording separately from supplied evidence and never loads the original private material to render that section. JavaScript checks that a start acknowledgment matches the submitted brief's text, hash, and origin. An uncertain result keeps the same operation available for reconciliation.

Absent optional fields preserve old input bytes and receipt validation. Packet revalidation recompiles from the original stored request and frozen sources; receipt metadata does not become compiler authority. Linked retries preserve the original brief exactly. Editing it starts a new request; origin ownership and policy checks still apply. Copied historical records cannot authorize new operations in a recovered project.

An identical brief-start operation can return its retained result after a later policy change to resolve a lost acknowledgment. This returns already frozen historical input; it does not authorize another dispatch or further source reads. New operations, ordinary packet inspection, and the worker's dispatch claim still enforce the current policy. Generic context preparation refuses a brief because origin checks and its durable start belong to the atomic discussion operation.

## Verification

The slice requires exact inclusion and old-record compatibility, wrong-mode/scope/origin refusal, private-source exclusion, mandatory overflow, unconfirmed draft persistence, retry equality, lost-acknowledgment handling, and recovered-copy fencing. Native evidence must cover approval, draft reload, exact delivered text, inspection, and unchanged prose before explicit Apply. Executed results belong in [implementation status](IMPLEMENTATION_STATUS.md).
