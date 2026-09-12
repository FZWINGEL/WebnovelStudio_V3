import type { RestoredDecision } from './generated/kernel';
import type {
  HistoryPage,
  RestoreAck as WireRestoreAck,
  RestoreRevision,
  RevisionSummary,
} from './generated/documents';
export type { RestoredDecision, HistoryPage, RestoreRevision, RevisionSummary };

import { invoke } from '@tauri-apps/api/core';
import type { DocumentRecord, Head, ProjectAccess, Revision } from './projects';

// The wire shape carries the document body as `serde_json::Value`, so the
// generated type says `any`; the editor knows it is a `WnsDocument`. Same
// narrowing as `Revision` in `ipc/projects`.
export type RestoreAck = Omit<WireRestoreAck, 'document'> & { document: DocumentRecord };
export const listDocumentHistory = (access: ProjectAccess, documentId: string, beforeVersion: string | null = null): Promise<HistoryPage> =>
  invoke('list_document_history', { access, documentId, beforeVersion, limit: 50 });
export const readDocumentRevision = (access: ProjectAccess, documentId: string, revisionId: string): Promise<Revision> =>
  invoke('read_document_revision', { access, documentId, revisionId });
export const restoreRevision = (request: RestoreRevision): Promise<RestoreAck> => invoke('restore_revision', { request });
