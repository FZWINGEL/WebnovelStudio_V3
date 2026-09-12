// Generated from `wns-kernel` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.

/**
 * The apply half of a command receipt's stored result.
 * 
 * Receipt vocabulary by the same argument as [`RestoredDecision`]: it moved
 * down as a leaf type, five strings and no behaviour, so that `StoredResult`
 * can follow it without dragging `proposals` along.
 */
export type AppliedDecision = { decisionId: string; proposalId: string; preparedId: string; beforeRevisionId: string; afterRevisionId: string }

export type CoreError = { code: string; detail: string; currentHead?: Head | null }

/**
 * One document row, typed.
 * 
 * Moved down from `webnovel-core::projects::records` so that the row readers
 * in `wns-storage` can name what they return without reaching up into the
 * crate under decomposition. The serde attributes are unchanged: they are what
 * keeps historical serialized records byte-compatible.
 */
export type DocumentRecord = { head: Head; title: string; kind: string; metadataVersion: string; body: any; lastCheckpointId: string | null; role?: DocumentRole }

/**
 * Authority role for a document row.  This is deliberately an enum rather
 * than a title/ID convention so every source consumer can apply the same
 * fence.  New roles must be added with a reader-floor migration.
 */
export type DocumentRole = "ordinary" | "assistantDraft" | "conversationAnchor"

/**
 * A document identity pinned to an exact version and body hash.
 */
export type Head = { documentId: string; version: string; bodyHash: string }

/**
 * One renderer's explicit lease on an open project.
 * 
 * Moved down from `projects/records.rs` because the packet vocabulary embeds
 * it — `SearchStory` and `FreezeStory` carry an `access` — so it has to sit at
 * or below the compiler. It is four strings and no behaviour.
 */
export type ProjectAccess = { projectId: string; session: string; writerLease: string; operationNamespace: string }

/**
 * A project's identity.
 * 
 * Moved down from `webnovel-core::projects::records` because the story-context
 * host trait has to name it: `validate_runtime_owner` reads exactly two of
 * these fields — `project_id` and `operation_namespace` — to decide whether a
 * dispatch belongs to the running operation. A trait declared below core cannot
 * name a type declared in core, and the two-field reading is not a reason to
 * widen the trait past what it is for.
 */
export type ProjectInfo = { projectId: string; operationNamespace: string; title: string; formatVersion: number }

/**
 * The restore half of a command receipt's stored result.
 * 
 * Receipt vocabulary: [`StoredResult`] names it, so it has to sit at or below
 * every crate that stores or reads a receipt.
 */
export type RestoredDecision = { revisionId: string; beforeRevisionId: string; afterRevisionId: string }

/**
 * One immutable saved revision of a document.
 * 
 * Moved down from `projects/records.rs` so the packet compiler's input
 * vocabulary can name it: `story_records` — the shapes a compiled packet
 * carries — depends on this type, and a layer-2 crate may not reach up to the
 * crate under decomposition for it.
 */
export type Revision = { id: string; head: Head; body: any; reason: string; parentId: string | null }

/**
 * The result of validating and canonicalizing a W0 snapshot.
 */
export type SnapshotReceipt = { snapshot: any; canonicalJson: string; hash: string; utf16Units: number; blockCount: number }

/**
 * The immutable decision a command receipt records.
 * 
 * This is the type `wns-storage::existing_receipt` returns and
 * `wns-storage::insert_receipt` writes, so it lives at L0 with them rather
 * than beside any one command that produces one.
 */
export type StoredResult = { head: Head; savedGeneration: string; applied?: AppliedDecision | null; restored?: RestoredDecision | null }

