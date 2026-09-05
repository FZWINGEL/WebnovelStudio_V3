import { invoke } from '@tauri-apps/api/core';
import type { CompiledPacket, MockContextBudget, ScopeGrant } from './context';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { ModelSelection } from './providers';

/** The author action encoded in a discussion's immutable context packet. */
export type FeedbackIntent = 'discuss' | 'proposeEdits';
export const DEFAULT_FEEDBACK_INTENT: FeedbackIntent = 'discuss';

export interface DiscussionScope { kind: ScopeGrant['kind']; start: Endpoint | null; end: Endpoint | null; quote: string; sourceBodyHash: string }
export interface SafeBriefInput { text: string; originMessageId: string | null; confirmed: boolean }
export interface ComposerBody { text: string; scope: DiscussionScope | null; pinnedDocumentIds: string[]; intent?: FeedbackIntent; previousRunId?: string | null; safeBrief?: SafeBriefInput | null }
export interface DiscussionDraft extends ComposerBody { documentId: string; version: string; updatedAt: string }
export interface DiscussionMessage { id: string; threadId: string; runId: string | null; role: 'user' | 'assistant'; content: string; scope: ScopeGrant | null; packetId: string | null; createdAt: string }
export interface DiscussionRun {
  id: string; threadId: string; owner: { projectId: string; operationNamespace: string; runId: string };
  operationId: string; intent?: FeedbackIntent; payloadHash: string; target: Head; packetId: string; previousRunId: string | null;
  status: 'queued' | 'running' | 'stopping' | 'completed' | 'stopped' | 'failed' | 'interrupted';
  dispatchState: string; sequence: string; outputText: string; stopReason: string | null; createdAt: string; updatedAt: string;
}
export interface DiscussionView { documentId: string; threadId: string | null; messages: DiscussionMessage[]; runs: DiscussionRun[]; draft: DiscussionDraft | null; workerIssues?: Array<{ runId: string; detail: string }> }
export interface StartDiscussion {
  modelSelection?: ModelSelection;
  access: ProjectAccess; operationId: string; expected: Head; instruction: string; scope: DiscussionScope | null;
  intent?: FeedbackIntent; pinnedDocumentIds: string[]; budget: MockContextBudget; previousRunId: string | null;
  safeBrief?: SafeBriefInput | null;
}
export interface DiscussionStart { threadId: string; run: DiscussionRun; userMessage: DiscussionMessage; packet: CompiledPacket }
export interface SaveDiscussionDraft extends ComposerBody { access: ProjectAccess; operationId: string; documentId: string; expectedVersion: string }
export const readDiscussion = (access: ProjectAccess, documentId: string): Promise<DiscussionView> => invoke('read_discussion', { access, documentId });
export const retryDiscussionSave = (access: ProjectAccess, documentId: string, runId: string): Promise<DiscussionView> => invoke('retry_discussion_save', { access, documentId, runId });
export const discussionRetry = (access: ProjectAccess, runId: string): Promise<ComposerBody & { previousRunId: string }> => invoke('discussion_retry', { access, runId });
export const startDiscussion = ({ modelSelection, ...request }: StartDiscussion): Promise<DiscussionStart> => invoke('start_discussion', { request, modelSelection: modelSelection ?? null });
export const stopDiscussion = (access: ProjectAccess, runId: string): Promise<{ run: DiscussionRun }> => invoke('stop_discussion', { access, runId });
export const saveDiscussionDraft = (request: SaveDiscussionDraft): Promise<DiscussionDraft> => invoke('save_discussion_draft', { request });
