import type {
  SaveGuidance,
} from './generated/conversation';
export type {
  SaveGuidance,
};

import type {
  FrozenGuidance,
  GuidanceScope,
  GuidanceVersion,
} from './generated/context';
export type {
  FrozenGuidance,
  GuidanceScope,
  GuidanceVersion,
};

import { invoke } from '@tauri-apps/api/core';
import type { ProjectAccess } from './projects';

export const readGuidance = (access: ProjectAccess, documentId: string): Promise<GuidanceVersion[]> => invoke('read_guidance', { access, documentId });
export const saveGuidance = (request: SaveGuidance): Promise<GuidanceVersion> => invoke('save_guidance', { request });
