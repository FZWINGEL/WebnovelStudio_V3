import type {
  SourcePinScope,
  SourcePinSet,
  SourcePinsView,
  SaveSourcePins,
} from './generated/story';
export type {
  SourcePinScope,
  SourcePinSet,
  SourcePinsView,
  SaveSourcePins,
};

import { invoke } from '@tauri-apps/api/core';
import type { ProjectAccess } from './projects';

export interface SourceChoice { id: string; title: string }
export const readSourcePins = (access: ProjectAccess, documentId: string): Promise<SourcePinsView> => invoke('read_source_pins', { access, documentId });
export const saveSourcePins = (request: SaveSourcePins): Promise<SourcePinSet> => invoke('save_source_pins', { request });
