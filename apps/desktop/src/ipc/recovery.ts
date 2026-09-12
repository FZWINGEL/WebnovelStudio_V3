import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor';

export interface RecoveryCopyResult { path: string; snapshotHash: string; sha256: string; utf8Bytes: number }
/** Copies this exact capture; never flushes, reads, or updates the project. */
export const saveRecoveryCopy = (body: WnsDocument): Promise<RecoveryCopyResult | null> =>
  invoke('save_recovery_copy', { body });
