// Generated from `documents` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { DocumentRecord, Head, ProjectAccess, StoredResult } from './kernel';
import type { Endpoint } from './context';

export type CheckpointReason = "manual" | "switch" | "close" | "source" | "export" | "interval"

export type CheckpointRequest = { access: ProjectAccess; expected: Head; reason: CheckpointReason }

export type HistoryPage = { items: RevisionSummary[]; nextBeforeVersion: string | null }

export type OperationReceipt = { operationId: string; operationKind: string; payloadHash: string; result: StoredResult }

export type ReconcileRequest = { projectId: string; operationNamespace: string; session: string; documentId: string; pendingOperationIds: string[] }

export type ReconciledDocument = { access: ProjectAccess; document: DocumentRecord; receipts: OperationReceipt[] }

/**
 * Restore has the same acknowledgement shape as Apply. On replay, `result`
 * is the historical first result while `document` is the latest committed
 * document, so the editor never replays an old mutation over newer text.
 */
export type RestoreAck = { access: ProjectAccess; operationId: string; alreadyApplied: boolean; result: StoredResult; document: DocumentRecord }

export type RestoreRevision = { access: ProjectAccess; operationId: string; expected: Head; revisionId: string; revisionHash: string; localGeneration: string }

export type RevisionSummary = { id: string; head: Head; reason: string; createdAt: string }

export type SaveAck = { projectId: string; documentId: string; session: string; operationNamespace: string; operationId: string; head: Head; savedGeneration: string }

export type SaveCause = "typing" | "undo" | "redo"

export type SaveSnapshot = { access: ProjectAccess; operationId: string; expected: Head; localGeneration: string; body: any; cause: SaveCause }

/**
 * A replacement block has no editor ID. The application assigns one while
 * preparing the complete result document.
 */
export type TypedReplacementBlock = { type: "paragraph"; content: TypedReplacementInline[] } | { type: "heading"; attrs: TypedReplacementHeadingAttrs; content: TypedReplacementInline[] } | { type: "sceneBreak" }

export type TypedReplacementHeadingAttrs = { level: number }

export type TypedReplacementInline = { type: "text"; text: string; marks?: TypedReplacementMark[] } | { type: "hardBreak" }

export type TypedReplacementLinkAttrs = { href: string }

export type TypedReplacementMark = { type: "bold" } | { type: "italic" } | { type: "link"; attrs: TypedReplacementLinkAttrs }

/**
 * Where the renderer's caret, anchor and focus sat in a document.
 */
export type ViewState = { documentId: string; head: Head; anchor: Endpoint; focus: Endpoint }

