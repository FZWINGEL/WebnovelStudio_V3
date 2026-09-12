import type {
  ModelKey,
  ModelSelection,
  ModelSettings,
  ModelDescriptor,
} from './generated/providers';
export type {
  ModelKey,
  ModelSelection,
  ModelSettings,
  ModelDescriptor,
};

import { invoke } from '@tauri-apps/api/core';

export interface StoryMemoryView {
  revision: string; providerId: string; providerLabel: string;
  modelId: string; reasoning: string | null; serviceTier: string | null;
  ready: boolean; detail: string;
}
export type CodexTransport = 'exec' | 'appServer';
export interface CodexTransportSettings { revision: string; transport: CodexTransport }
export interface ProviderState {
  settings: ModelSettings; catalog: { models: ModelDescriptor[] };
  dispatch: { kind: 'localMock' | 'codexCli' | 'claudeCli' | 'openAiCompatible' | 'blocked'; detail: string };
  codexConnection?: { ready: boolean; checked?: boolean; checking?: boolean; memoryReady?: boolean; detail: string };
  claudeConnection?: { ready: boolean; detail: string };
  /** Native production state always supplies this. Optional keeps isolated
   * picker fixtures compatible while preventing the memory panel from
   * inventing a live maintenance target when it is absent. */
  storyMemory?: StoryMemoryView;
}
export const localModel: ModelSelection = { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null };
export const storyMemoryMockModel: ModelSelection = { ...localModel };
/** Retained for older isolated fixtures; production memory uses storyMemory. */
export const storyMemoryModel: ModelSelection = { providerId: 'codex', modelId: 'gpt-6-astra', reasoning: 'low', serviceTier: 'priority' };
export const sameModel = (left: ModelKey, right: ModelKey) => left.providerId === right.providerId && left.modelId === right.modelId;
export const readProviderState = (): Promise<ProviderState> => invoke('provider_state');
export const checkCodexConnection = (): Promise<ProviderState> => invoke('check_codex_connection');
export const checkClaudeConnection = (): Promise<ProviderState> => invoke('check_claude_connection');
export const saveModelSettings = (expectedRevision: string, active: ModelSelection, favorites: ModelKey[]): Promise<ProviderState> => invoke('save_model_settings', { expectedRevision, active, favorites });
export const saveStoryMemoryProvider = (expectedRevision: string, providerId: string): Promise<ProviderState> => invoke('save_story_memory_provider', { expectedRevision, providerId });
export const readCodexTransport = (): Promise<CodexTransportSettings> => invoke('codex_transport_settings');
export const saveCodexTransport = (expectedRevision: string, transport: CodexTransport): Promise<CodexTransportSettings> => invoke('save_codex_transport', { expectedRevision, transport });

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
