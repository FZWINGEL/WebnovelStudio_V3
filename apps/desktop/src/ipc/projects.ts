// The wire shapes come from Rust. `cargo test -p wns-kernel --test bindings`
// regenerates them and fails when they drift.
import type {
  DocumentRecord as WireDocumentRecord,
  DocumentRole,
  Head,
  ProjectAccess,
  ProjectInfo,
  Revision as WireRevision,
} from './generated/kernel';
export type { DocumentRole, Head, ProjectAccess, ProjectInfo };

// Rust carries a document body as `serde_json::Value`, so the generated shape
// says `any`. The editor knows better, and saying so once here is narrower than
// a hand-written mirror: every *other* field still comes from Rust, so a rename
// there is still a compile error rather than a runtime `undefined`.
export type Revision = Omit<WireRevision, 'body'> & { body: WnsDocument };
export type DocumentRecord = Omit<WireDocumentRecord, 'body' | 'role'> & {
  body: WnsDocument;
  role?: DocumentRole;
};

import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor/document';
import { validateSnapshot } from './native';
import { applyProposal, type AppliedDecision, type ApplyAck, type ApplyProposal } from './proposals';
import { restoreRevision, type RestoredDecision, type RestoreAck, type RestoreRevision } from './history';

export interface Endpoint { blockId: string; utf16Offset: number }
export interface ViewState { documentId: string; head: Head; anchor: Endpoint; focus: Endpoint }
export interface ProjectMetadata { project: ProjectInfo; metadataVersion: string; libraryWarning?: string | null }
export interface OpenedProject { project: ProjectInfo; access: ProjectAccess; documents: DocumentRecord[]; metadataVersion: string; viewState: ViewState | null; libraryWarning: string | null }
/**
 * The immutable identity and payload of one document creation intent.
 * Keeping these values outside the IPC wrapper makes a retry idempotent even
 * when the first renderer invocation lost its acknowledgment.
 */
export interface CreateDocumentIntent { operationId: string; documentId: string; title: string; kind: string; body: WnsDocument }
export interface SaveSnapshot {
  access: ProjectAccess; operationId: string; expected: Head; localGeneration: string;
  body: WnsDocument; cause: 'typing' | 'undo' | 'redo';
}
export interface SaveAck {
  projectId: string; documentId: string; session: string; operationNamespace: string;
  operationId: string; head: Head; savedGeneration: string;
}
export interface OperationReceipt {
  operationId: string; operationKind: string; payloadHash: string;
  result: { head: Head; savedGeneration: string; applied?: AppliedDecision; restored?: RestoredDecision };
}
export interface ReconcileRequest {
  projectId: string; operationNamespace: string; session: string; documentId: string; pendingOperationIds: string[];
}
export interface ReconciledDocument { access: ProjectAccess; document: DocumentRecord; receipts: OperationReceipt[] }
export interface CheckpointRequest {
  access: ProjectAccess; expected: Head; reason: 'manual' | 'switch' | 'close' | 'source' | 'export' | 'interval';
}
export interface ProjectTransport {
  validate(body: WnsDocument): Promise<void>;
  save(request: SaveSnapshot): Promise<SaveAck>;
  reconcile(request: ReconcileRequest): Promise<ReconciledDocument>;
  checkpoint(request: CheckpointRequest): Promise<Revision>;
  apply?(request: ApplyProposal): Promise<ApplyAck>;
  restore?(request: RestoreRevision): Promise<RestoreAck>;
}
export const projectTransport: ProjectTransport = {
  validate: async body => { await validateSnapshot(body); },
  save: request => invoke('save_snapshot', { request }),
  reconcile: request => invoke('reconcile_document', { request }),
  checkpoint: request => invoke('checkpoint_document', { request }),
  apply: applyProposal,
  restore: restoreRevision,
};
export const createProject = (path: string, title: string, session: string): Promise<OpenedProject> => invoke('create_project', { path, title, session });
export const openProject = (path: string, session: string): Promise<OpenedProject> => invoke('open_project', { path, session });
export const createDocument = (access: ProjectAccess, intent: CreateDocumentIntent): Promise<DocumentRecord> => invoke('create_document', {
  request: { access, ...intent },
});
/** Reopen the project actor and attach a fresh writer lease before retrying an uncertain create. */
export const reconcileProject = (projectId: string, session: string): Promise<OpenedProject> => invoke('reconcile_project', { projectId, session });
export const readDocument = (access: ProjectAccess, documentId: string): Promise<DocumentRecord> => invoke('read_document', { access, documentId });
export const documentHistory = (access: ProjectAccess, documentId: string): Promise<Revision[]> => invoke('document_history', { access, documentId });
export const projectMetadata = (projectId: string): Promise<ProjectMetadata> => invoke('project_metadata', { projectId });
export const renameProject = (access: ProjectAccess, expectedMetadataVersion: string, title: string): Promise<ProjectMetadata> => invoke('rename_project', { access, expectedMetadataVersion, title });
export const renameDocument = (access: ProjectAccess, documentId: string, expectedMetadataVersion: string, title: string): Promise<DocumentRecord> => invoke('rename_document', { access, documentId, expectedMetadataVersion, title });
export const readViewState = (access: ProjectAccess): Promise<ViewState | null> => invoke('read_view_state', { access });
export const saveViewState = (access: ProjectAccess, head: Head, anchor: Endpoint, focus: Endpoint): Promise<ViewState> => invoke('save_view_state', { access, head, anchor, focus });
