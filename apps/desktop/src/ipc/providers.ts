import { invoke } from '@tauri-apps/api/core';

export interface ModelKey { providerId: string; modelId: string }
export interface ModelSelection extends ModelKey { reasoning: string | null; serviceTier: string | null }
export interface ModelSettings { revision: string; active: ModelSelection; favorites: ModelKey[] }
export interface ModelDescriptor {
  key: ModelKey; label: string; providerLabel: string; reasoningLevels: string[];
  serviceTiers: Array<{ id: string; label: string }>;
  contextWindowTokens: string | null; maxOutputTokens: string | null;
  origin: 'builtIn' | 'reference'; ready: boolean; statusDetail: string;
}
export interface ProviderState {
  settings: ModelSettings; catalog: { models: ModelDescriptor[] };
  dispatch: { kind: 'localMock' | 'codexCli' | 'blocked'; detail: string };
  codexConnection?: { ready: boolean; detail: string };
}
export const localModel: ModelSelection = { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null };
export const sameModel = (left: ModelKey, right: ModelKey) => left.providerId === right.providerId && left.modelId === right.modelId;
export const readProviderState = (): Promise<ProviderState> => invoke('provider_state');
export const checkCodexConnection = (): Promise<ProviderState> => invoke('check_codex_connection');
export const saveModelSettings = (expectedRevision: string, active: ModelSelection, favorites: ModelKey[]): Promise<ProviderState> => invoke('save_model_settings', { expectedRevision, active, favorites });
