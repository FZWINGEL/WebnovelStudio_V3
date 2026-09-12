import type {
  V2ProjectSummary,
  V2WorkingProse,
  V2DraftPreview,
  V2ChapterPreview,
  V2ImportPreview,
  V2ChapterBodyChoice,
  V2ChapterBodyDecision,
  V2ImportRequest,
} from './generated/transfer';
export type {
  V2ProjectSummary,
  V2WorkingProse,
  V2DraftPreview,
  V2ChapterPreview,
  V2ImportPreview,
  V2ChapterBodyChoice,
  V2ChapterBodyDecision,
  V2ImportRequest,
};

import { invoke } from '@tauri-apps/api/core';
import type { OpenedProject } from './projects';

export interface V2SourceProjectList {
  sourcePath: string;
  projects: V2ProjectSummary[];
}
export interface V2ImportPreviewResult { sourcePath: string; preview: V2ImportPreview }
export const v2ImportListProjects = (path: string | null = null): Promise<V2SourceProjectList | null> =>
  invoke('v2_import_list_projects', { path });
export const v2ImportPreview = (path: string, sourceProjectId: string): Promise<V2ImportPreviewResult> =>
  invoke('v2_import_preview', { path, sourceProjectId });
export const v2Import = (request: V2ImportRequest, session: string): Promise<OpenedProject> =>
  invoke('v2_import', { request, session });
