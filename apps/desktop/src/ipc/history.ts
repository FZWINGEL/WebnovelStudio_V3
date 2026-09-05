import { invoke } from '@tauri-apps/api/core';
import type { DocumentRecord, Head, ProjectAccess, Revision } from './projects';

export interface RevisionSummary { id: string; head: Head; reason: string; createdAt: string }
export interface HistoryPage { items: RevisionSummary[]; nextBeforeVersion: string | null }
export interface RestoredDecision { revisionId: string; beforeRevisionId: string; afterRevisionId: string }
export interface RestoreRevision { access: ProjectAccess; operationId: string; expected: Head; revisionId: string; revisionHash: string; localGeneration: string }
export interface RestoreAck {
  access: ProjectAccess; operationId: string; alreadyApplied: boolean;
  result: { head: Head; savedGeneration: string; restored: RestoredDecision }; document: DocumentRecord;
}
export const listDocumentHistory = (access: ProjectAccess, documentId: string, beforeVersion: string | null = null): Promise<HistoryPage> =>
  invoke('list_document_history', { access, documentId, beforeVersion, limit: 50 });
export const readDocumentRevision = (access: ProjectAccess, documentId: string, revisionId: string): Promise<Revision> =>
  invoke('read_document_revision', { access, documentId, revisionId });
export const restoreRevision = (request: RestoreRevision): Promise<RestoreAck> => invoke('restore_revision', { request });
