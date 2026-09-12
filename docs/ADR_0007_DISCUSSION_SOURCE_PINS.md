# Persistent sources for discussion

**Decision:** extend C3 with saved document and project source choices for author-room discussion. English UI and authoring remain the product scope.

## Author experience

The Discussion panel offers a collapsed **Story sources** section. An author chooses an existing chapter or note, chooses **This document** or **This project**, and presses **Keep source**. Each saved row names its source and scope and offers **Remove**. Opening the form from **Keep source…** in the context inspector does not save anything until the author confirms it.

**Include next time** remains the separate request-scoped composer action. Saved source choices apply to future discussions, including fresh compilation for a linked retry. Changing the transient request sources still changes the retry's identity under ADR 0003.

Pins refer to documents, using their latest saved text when a new request freezes its context. Old requests retain their exact source revisions. Pins do not make a note canon, modify a manuscript, widen edit scope, or trigger analysis on autosave.

The initial persistent-pin audience is explicitly **AuthorRoom**. The interface explains that these saved sources are not automatically added to **Suggest edits**. Restricted writing continues to use its existing eligible-source and reader-frontier rules. Safe-brief transfer and persistent restricted-writing pins remain later work.

## Rust ownership

Schema 10 adds one mutable source set per scope/target and immutable mutation receipts. It does not create another source-text store. A document set has a target document ID; a project set has no target. Each set has a compare-and-swap version and sorted unique source document IDs, bounded to 64. An absent set reads as version zero with no sources.

`read_source_pins(access, documentId)` returns the current project and document sets. `save_source_pins(request)` takes an operation ID, scope, target, expected version, and complete selected source IDs. Rust checks project/session ownership, scope, referenced documents, and version before accepting a change.

An identical operation replays its original receipt. Reusing the operation ID with different content fails. An unchanged set does not advance its version or the story source epoch. A changed set and its receipt commit together and advance `context_source_epoch`, conservatively making earlier proposals stale.

Recovery copies retain source choices over their copied documents. Receipts retain their old project/operation namespace and cannot authorize a new operation in the recovered copy.

## Request compilation and inspection

Within the existing discussion-start transaction, Rust reads applicable persistent sets for AuthorRoom, merges them with the current request's source IDs, and resolves them against the new frozen manifest. The original transient duplicate checks remain. Missing or disallowed required sources cause an explicit refusal; they are not silently dropped.

Required sources use the existing mandatory context budget. If they and the other mandatory task material cannot fit, compilation fails with `MandatoryContextTooLarge` before dispatch. No extra model call is needed.

The optional `mandatorySourceHandles` receipt field identifies the exact additional required story sources for the inspector. Older receipts remain readable. The target already appears in full; a persistent pin for that same document adds no duplicate content or second required-source label. The immutable packet request retains the resolved source handles and exact submitted content.

## Verification boundary

Required checks cover scope and restart persistence; CAS, replay, no-op and epoch behavior; missing and foreign sources; recovery isolation; mandatory-budget refusal; and exclusion from restricted writing. UI checks cover explicit confirmation, immutable retry after an uncertain acknowledgment, malformed acknowledgments, late project responses, and removal. The native flow verifies saved choices and required-source labels through actual Tauri IPC.

Current executed results belong in [implementation status](IMPLEMENTATION_STATUS.md). This slice does not qualify live-provider tokenization, richer retrieval, safe briefs, generated memory, or narrative quality.
