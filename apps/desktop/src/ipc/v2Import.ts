import { invoke } from '@tauri-apps/api/core';
import type { OpenedProject } from './projects';

export interface V2ProjectSummary {
  sourceProjectId: string;
  title: string;
  slug: string;
  chapterCount: number;
}
export interface V2SourceProjectList {
  sourcePath: string;
  projects: V2ProjectSummary[];
}
export type V2WorkingProse = { state: 'missing' } | { state: 'present'; text: string };
export interface V2DraftPreview { sourceId: string; version: number; prose: string; isApproved: boolean; createdAt: string }
export interface V2ChapterPreview {
  sourceId: string;
  chapterNumber: number;
  title: string;
  retiredAt: string | null;
  workingProse: V2WorkingProse;
  bodySelection: { kind: 'workingProse' } | { kind: 'requiresAuthorChoice' };
  workingProseBasedOnDraftId: string | null;
  approvedDraftId: string | null;
  draftCount: number;
  drafts: V2DraftPreview[];
}
export interface V2ImportPreview {
  importFormatVersion: number;
  source: { schemaVersion: number; sourceBytes: number; sourceSha256: string; migrationVersions: string[]; projectCount: number };
  project: { sourceProjectId: string; title: string; slug: string; chapterCount: number };
  chapters: V2ChapterPreview[];
  legacy: { recordCounts: Record<string, number>; totalJsonBytes: number };
}
export interface V2ImportPreviewResult { sourcePath: string; preview: V2ImportPreview }
export type V2ChapterBodyChoice = 'empty' | { draft: { sourceDraftId: string } };
export interface V2ChapterBodyDecision { sourceChapterId: string; choice: V2ChapterBodyChoice }
export interface V2ImportRequest {
  operationId: string;
  sourcePath: string;
  sourceProjectId: string;
  title: string;
  expectedSourceSha256: string;
  choices: V2ChapterBodyDecision[];
}

export const v2ImportListProjects = (path: string | null = null): Promise<V2SourceProjectList | null> =>
  invoke('v2_import_list_projects', { path });
export const v2ImportPreview = (path: string, sourceProjectId: string): Promise<V2ImportPreviewResult> =>
  invoke('v2_import_preview', { path, sourceProjectId });
export const v2Import = (request: V2ImportRequest, session: string): Promise<OpenedProject> =>
  invoke('v2_import', { request, session });
