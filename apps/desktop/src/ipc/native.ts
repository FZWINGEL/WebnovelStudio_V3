import { invoke, isTauri } from '@tauri-apps/api/core';
import { bodyHash, canonicalJson, type SnapshotReceipt, type WnsDocument } from '../editor';

export interface RuntimeInfo { host: string; appVersion: string; webviewVersion: string; persistence: boolean; editorTrial: boolean }
export async function runtimeInfo(): Promise<RuntimeInfo> {
  if (!isTauri()) throw new Error('Open the desktop app to validate with Rust.');
  return invoke<RuntimeInfo>('runtime_info');
}

export async function validateSnapshot(snapshot: WnsDocument): Promise<SnapshotReceipt> {
  if (!isTauri()) throw new Error('Rust validation requires the desktop app. Run desktop:dev.');
  const json = canonicalJson(snapshot);
  const expectedHash = await bodyHash(json);
  const receipt = await invoke<SnapshotReceipt>('validate_snapshot', { snapshotJson: json });
  if (receipt.hash !== expectedHash || receipt.canonicalJson !== json) {
    throw new Error('The editor and Rust disagree about this snapshot. The chapter has not been replaced.');
  }
  return receipt;
}
