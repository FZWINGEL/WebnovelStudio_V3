import { invoke } from '@tauri-apps/api/core';
import type { ProjectAccess } from './projects';

export type GuidanceScope = 'request' | 'document' | 'project';
export interface GuidanceVersion {
  guidanceId: string; versionId: string; version: string; scope: GuidanceScope;
  documentId: string | null; text: string; textHash: string; active: boolean;
  originMessageId: string | null; createdAt: string;
}
export interface FrozenGuidance { handle: string; projectId: string; version: GuidanceVersion }
export interface SaveGuidance {
  access: ProjectAccess; operationId: string; guidanceId: string; expectedVersion: string;
  text: string; scope: GuidanceScope; documentId: string | null; active: boolean;
  originMessageId: string | null;
}
export const readGuidance = (access: ProjectAccess, documentId: string): Promise<GuidanceVersion[]> => invoke('read_guidance', { access, documentId });
export const saveGuidance = (request: SaveGuidance): Promise<GuidanceVersion> => invoke('save_guidance', { request });
