import { invoke } from '@tauri-apps/api/core';

export interface ModelKey { providerId: string; modelId: string }
export interface ModelSelection extends ModelKey { reasoning: string | null; serviceTier: string | null }
export interface ModelSettings { revision: string; active: ModelSelection; favorites: ModelKey[] }
export interface ModelDescriptor {
  key: ModelKey; label: string; providerLabel: string; reasoningLevels: string[];
  serviceTiers: Array<{ id: string; label: string }>;
  contextWindowTokens: string | null; maxOutputTokens: string | null;
  defaultReasoning?: string | null; defaultServiceTier?: string | null;
  origin: 'builtIn' | 'reference' | 'codexDiscovery' | 'openAiCompatible'; ready: boolean; statusDetail: string;
}
export interface ProviderState {
  settings: ModelSettings; catalog: { models: ModelDescriptor[] };
  dispatch: { kind: 'localMock' | 'codexCli' | 'openAiCompatible' | 'blocked'; detail: string };
  codexConnection?: { ready: boolean; memoryReady?: boolean; detail: string };
}
export const localModel: ModelSelection = { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null };
export const storyMemoryModel: ModelSelection = { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' };
export const sameModel = (left: ModelKey, right: ModelKey) => left.providerId === right.providerId && left.modelId === right.modelId;
export const readProviderState = (): Promise<ProviderState> => invoke('provider_state');
export const checkCodexConnection = (): Promise<ProviderState> => invoke('check_codex_connection');
export const saveModelSettings = (expectedRevision: string, active: ModelSelection, favorites: ModelKey[]): Promise<ProviderState> => invoke('save_model_settings', { expectedRevision, active, favorites });

export interface EndpointProfile {
  id: string; label: string; baseUrl: string; enabled: boolean; jsonMode: boolean;
  configRevision: string; hasApiKey: boolean; apiKeyConfigured: boolean; manualModelIds: string[]; cachedModelIds: string[];
}
export interface EndpointSettings { revision: string; profiles: EndpointProfile[] }
export interface SaveEndpoint {
  expectedRevision: string; profileId: string | null; label: string; baseUrl: string;
  enabled: boolean; jsonMode: boolean; manualModelIds: string[];
  apiKey: { kind: 'keep' | 'remove' } | { kind: 'replace'; value: string };
}
export const readEndpointSettings = (): Promise<EndpointSettings> => invoke('endpoint_settings');
export const saveEndpointSettings = (request: SaveEndpoint): Promise<EndpointSettings> => invoke('save_endpoint_settings', { request });
export const discoverEndpointModels = (profileId: string, configRevision: string, discoveryId: string): Promise<EndpointSettings> => invoke('discover_endpoint_models', { profileId, configRevision, discoveryId });
export const cancelEndpointDiscovery = (discoveryId: string): Promise<void> => invoke('cancel_endpoint_discovery', { discoveryId });
