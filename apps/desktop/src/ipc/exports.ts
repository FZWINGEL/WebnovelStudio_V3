import { invoke } from '@tauri-apps/api/core';
import type { Head, ProjectAccess } from './projects';

export type DraftFormat = 'plainText' | 'markdown';
export interface DraftExportPreview {
  id: string; projectId: string; operationNamespace: string; sourceHead: Head; revisionId: string;
  format: DraftFormat; formatVersion: number; utf8Bytes: number; sha256: string; formatLoss: string; previewText: string;
  /** Present only for an author-reviewed chapter snapshot; working previews omit it. */
  reviewBundleId?: string;
}
export interface DraftExportResult { path: string; previewId: string; sha256: string; utf8Bytes: number }
export const prepareDraftExport = (access: ProjectAccess, expected: Head, format: DraftFormat): Promise<DraftExportPreview> =>
  invoke('prepare_draft_export', { access, expected, format });
export const prepareReviewedDraftExport = (access: ProjectAccess, expected: Head, format: DraftFormat): Promise<DraftExportPreview> =>
  invoke('prepare_reviewed_draft_export', { access, expected, format });
export const exportPreparedDraft = (access: ProjectAccess, preview: DraftExportPreview): Promise<DraftExportResult | null> =>
  invoke('export_prepared_draft', { access, preview });
