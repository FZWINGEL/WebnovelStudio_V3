import type {
  DraftFormat,
  DraftExportPreview,
} from './generated/transfer';
export type {
  DraftFormat,
  DraftExportPreview,
};

import { invoke } from '@tauri-apps/api/core';
import type { Head, ProjectAccess } from './projects';

export interface DraftExportResult { path: string; previewId: string; sha256: string; utf8Bytes: number }
export const prepareDraftExport = (access: ProjectAccess, expected: Head, format: DraftFormat): Promise<DraftExportPreview> =>
  invoke('prepare_draft_export', { access, expected, format });
export const prepareReviewedDraftExport = (access: ProjectAccess, expected: Head, format: DraftFormat): Promise<DraftExportPreview> =>
  invoke('prepare_reviewed_draft_export', { access, expected, format });
export const exportPreparedDraft = (access: ProjectAccess, preview: DraftExportPreview): Promise<DraftExportResult | null> =>
  invoke('export_prepared_draft', { access, preview });
