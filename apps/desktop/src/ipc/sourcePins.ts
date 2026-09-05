import { invoke } from '@tauri-apps/api/core';
import type { ProjectAccess } from './projects';

export type SourcePinScope = 'document' | 'project';
export interface SourceChoice { id: string; title: string }
export interface SourcePinSet {
  scope: SourcePinScope; targetDocumentId: string | null; version: string;
  sourceDocumentIds: string[]; audience: 'authorRoom';
}
export interface SourcePinsView { project: SourcePinSet; document: SourcePinSet }
export interface SaveSourcePins {
  access: ProjectAccess; operationId: string; scope: SourcePinScope;
  targetDocumentId: string | null; expectedVersion: string; sourceDocumentIds: string[];
}
export const readSourcePins = (access: ProjectAccess, documentId: string): Promise<SourcePinsView> => invoke('read_source_pins', { access, documentId });
export const saveSourcePins = (request: SaveSourcePins): Promise<SourcePinSet> => invoke('save_source_pins', { request });
