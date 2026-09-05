# ADR 0005 — Compare and restore saved writing

**Status:** implemented development slice, 5 September 2026. Qualification evidence and remaining W6 work live in [implementation status](IMPLEMENTATION_STATUS.md).

The writing desk offers **History** beside the document title. Opening it flushes current writing and keeps a checkpoint. A dated version selector loads bounded metadata first; choosing an entry reads that exact immutable revision into an inert preview beside the current manuscript. Reading history does not replace the editor. This is document history; project backup recovery still creates an independent recovered project.

## Persistence contract

Schema 8 already has immutable revisions and namespaced command receipts. Restore uses those records without adding another decision table or migration.

`RestoreRevision` carries project/session/lease identity, one operation ID, the expected current head, the selected revision ID/hash, and the next local generation. In one immediate transaction, Rust:

1. Checks the current writer and exact expected head, then verifies revision ownership and hash. An identical body is refused as a no-op.
2. Keeps the current body as a before checkpoint.
3. Advances the working version, replaces its body, marks its projection dirty, and advances the story source epoch.
4. Keeps an after checkpoint and records a `restore` command receipt with the selected, before, and after revision IDs.

The receipt is the author decision identity. It records what was restored, while the working body remains authoritative. Restoring an older revision makes outstanding story-dependent work stale; it does not change historical proposal decisions or adopt canon.

Replaying the same logical operation returns the original result plus the separately read latest document. Changed payloads cannot reuse its identity. Recovered projects may explicitly restore their locally retained revisions using a new operation in their own namespace; copied original receipts cannot authorize another write.

Backup validation checks receipt kind, document ownership, source/before/after bodies and hashes, version ordering, the after checkpoint's parent, and exact result-head agreement. Invalid decision links prevent recovery rather than being silently discarded.

## Editor and lifecycle contract

Apply and restore share the existing `DocumentSession` lifecycle guard and reconciliation path. Restore flushes pending typing, prevents document-changing input briefly, and preflights one full-document ProseMirror transaction. Rust commits before that transaction is dispatched in the existing editor. Its accepted body/generation handoff does not produce a second autosave.

An uncertain response retains the prepared operation and visible buffer. **Check saved version** obtains a new writer lease and queries the operation receipt with the latest head. A matching receipt displays the committed result once. A fenced absence permits the same logical request to retry. A later saved head produces an explicit comparison instead of replaying historical content over newer prose.

The transaction starts and closes an editor history event, so immediate Undo restores the writing that preceded it; Undo and Redo are ordinary new saved edits. After restart, saved versions remain available while the editor undo stack begins afresh.

## Read and presentation bounds

History pages contain up to 100 metadata entries; the interface requests 50. The exclusive cursor is the last returned working version. One selected body is read and hash-validated before display. Sequence and project/document/lease ownership guards discard late success and failure responses.

The preview renders restricted prose and formatting as inert elements. It creates no second editor and renders no raw HTML or active links. **Restore this version** explicitly states that current writing will remain in history. Restore progress stays visible while the request is pending; history reads are disabled while the session requires reconciliation.

This slice does not promise an unlimited persisted undo stack, automatic inverse application after intervening edits, or replacement of an open project's database. See [ADR 0004](ADR_0004_PROPOSAL_APPLY.md) for suggestion-specific authority and scope checks.
