import { invoke } from '@tauri-apps/api/core';
import type { CompiledPacket, MockContextBudget, ProviderBinding, ScopeGrant } from './context';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { ModelSelection } from './providers';

/** Author-authorized, bounded story lookups for a working-room discussion. */
export interface LookupAllowance {
  maxAdditionalInvocations: number;
  totalInputBytes: string;
  totalOutputBytes: string;
}

export const DEFAULT_LOOKUP_ALLOWANCE: LookupAllowance = {
  maxAdditionalInvocations: 2,
  totalInputBytes: '73728',
  totalOutputBytes: '196608',
};

/** The author action encoded in a discussion's immutable context packet. */
export type FeedbackIntent = 'discuss' | 'proposeEdits' | 'continue';
export type ContinuationBasis = 'working' | 'reviewed';
export const DEFAULT_FEEDBACK_INTENT: FeedbackIntent = 'discuss';

export interface DiscussionScope { kind: ScopeGrant['kind']; start: Endpoint | null; end: Endpoint | null; quote: string; sourceBodyHash: string }
export interface SafeBriefInput { text: string; originMessageId: string | null; confirmed: boolean }
export interface ComposerBody { text: string; scope: DiscussionScope | null; pinnedDocumentIds: string[]; intent?: FeedbackIntent; basis?: ContinuationBasis | null; previousRunId?: string | null; safeBrief?: SafeBriefInput | null; lookup?: LookupAllowance }
export interface DiscussionDraft extends ComposerBody { documentId: string; version: string; updatedAt: string }
export interface DiscussionMessage { id: string; threadId: string; runId: string | null; role: 'user' | 'assistant'; content: string; scope: ScopeGrant | null; packetId: string | null; createdAt: string }
export interface DiscussionRun {
  id: string; threadId: string; owner: { projectId: string; operationNamespace: string; runId: string };
  operationId: string; intent?: FeedbackIntent; basis?: ContinuationBasis | null; payloadHash: string; target: Head; packetId: string; previousRunId: string | null;
  status: 'queued' | 'running' | 'stopping' | 'completed' | 'stopped' | 'failed' | 'interrupted';
  dispatchState: string; sequence: string; outputText: string; stopReason: string | null; createdAt: string; updatedAt: string;
  providerBinding?: ProviderBinding; providerResult?: ProviderResult; lookup?: LookupRunSummary;
}
export type LookupInvocationState = 'prepared' | 'claimed' | 'needsContext' | 'completed' | 'failed' | 'stopped' | 'unknown';
export interface LookupInvocationSummary {
  ordinal: string; packetId: string; state: LookupInvocationState; inputDelivered: boolean; response: unknown | null; error: string | null;
}
export interface LookupRunSummary { allowance: LookupAllowance; invocations: LookupInvocationSummary[] }
export interface ProviderResult {
  binding: ProviderBinding; status: 'completed' | 'stopped' | 'timedOut' | 'outputLimit' | 'failed';
  confirmedStdinBytes: string; cleanup: 'settled' | 'unresolved'; error: string | null; effectiveIdentity: string | null;
  usage: { inputTokens: number; cachedInputTokens: number; cacheWriteInputTokens: number; outputTokens: number; reasoningOutputTokens: number } | null;
}
export interface DiscussionView { documentId: string; threadId: string | null; messages: DiscussionMessage[]; runs: DiscussionRun[]; draft: DiscussionDraft | null; workerIssues?: Array<{ runId: string; detail: string }> }
export interface StartDiscussion {
  modelSelection?: ModelSelection;
  access: ProjectAccess; operationId: string; expected: Head; instruction: string; scope: DiscussionScope | null;
  intent?: FeedbackIntent; basis?: ContinuationBasis | null; pinnedDocumentIds: string[]; budget: MockContextBudget; previousRunId: string | null;
  safeBrief?: SafeBriefInput | null; lookup?: LookupAllowance;
}
export interface DiscussionStart { threadId: string; run: DiscussionRun; userMessage: DiscussionMessage; packet: CompiledPacket }
export interface SaveDiscussionDraft extends ComposerBody { access: ProjectAccess; operationId: string; documentId: string; expectedVersion: string }
export const readDiscussion = (access: ProjectAccess, documentId: string): Promise<DiscussionView> => invoke('read_discussion', { access, documentId });
export const retryDiscussionSave = (access: ProjectAccess, documentId: string, runId: string): Promise<DiscussionView> => invoke('retry_discussion_save', { access, documentId, runId });
export const discussionRetry = (access: ProjectAccess, runId: string): Promise<ComposerBody & { previousRunId: string }> => invoke('discussion_retry', { access, runId });
export const startDiscussion = ({ modelSelection, ...request }: StartDiscussion): Promise<DiscussionStart> => invoke('start_discussion', { request, modelSelection: modelSelection ?? null });
export const stopDiscussion = (access: ProjectAccess, runId: string): Promise<{ run: DiscussionRun }> => invoke('stop_discussion', { access, runId });
export const saveDiscussionDraft = (request: SaveDiscussionDraft): Promise<DiscussionDraft> => invoke('save_discussion_draft', { request });
