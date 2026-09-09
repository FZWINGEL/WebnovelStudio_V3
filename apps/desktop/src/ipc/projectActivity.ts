import { invoke } from '@tauri-apps/api/core';

/** Read-only activity for project actors that are already open in Tauri. */
export interface ProjectActivitySnapshot {
  projectId: string;
  operationNamespace: string;
  activeWorkCount: number;
  pendingDrafts: number;
}

/**
 * Reads counts only. The native command does not open projects, attach a
 * renderer, acquire a lease, return story content, or contact a provider.
 */
export const readProjectActivity = (): Promise<ProjectActivitySnapshot[]> =>
  invoke('project_activity');
