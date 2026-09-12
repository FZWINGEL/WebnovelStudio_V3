import type {
  DiscussionDraft,
  DiscussionMessage as GeneratedDiscussionMessage,
  DiscussionScopeInput,
  SafeBriefInput as GeneratedSafeBriefInput,
  SaveDiscussionDraft,
} from './generated/conversation';
export type {
  DiscussionDraft,
  DiscussionScopeInput,
  SaveDiscussionDraft,
};

import type {
  BasisKind,
  DiscussionRun,
  FeedbackIntent,
  LookupAllowance,
  LookupInvocationState,
  LookupInvocationSummary,
  LookupRunSummary,
  ProviderResult,
} from './generated/workshop';
export type {
  BasisKind,
  DiscussionRun,
  FeedbackIntent,
  LookupAllowance,
  LookupInvocationState,
  LookupInvocationSummary,
  LookupRunSummary,
  ProviderResult,
};

import { invoke } from '@tauri-apps/api/core';
import type { AppServerDelivery, CompiledPacket, MockContextBudget, ProviderBinding, ScopeGrant } from './context';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { ModelSelection } from './providers';
import type { DiscussionStart as GeneratedDiscussionStart, StartDiscussion as GeneratedStartDiscussion } from './generated/story';

/** Author-authorized, bounded story lookups for a working-room discussion. */

export const DEFAULT_LOOKUP_ALLOWANCE: LookupAllowance = {
  maxAdditionalInvocations: 2,
  totalInputBytes: '73728',
  totalOutputBytes: '196608',
};

export const DEFAULT_FEEDBACK_INTENT: FeedbackIntent = 'discuss';

export type DiscussionScope = DiscussionScopeInput;
export type SafeBriefInput = GeneratedSafeBriefInput;
/**
 * The composer body is exactly a draft without its identity.
 *
 * It used to be a hand-written copy of those fields, and it had drifted: the
 * copy typed `basis` as `'working' | 'reviewed'` while the draft's is Rust's
 * `BasisKind`, which also carries `explicitHistory`. Deriving it means the two
 * cannot disagree again.
 */
export type ComposerBody = Omit<DiscussionDraft, 'documentId' | 'version' | 'updatedAt'>;
export type DiscussionMessage = GeneratedDiscussionMessage;
export interface DiscussionView { documentId: string; threadId: string | null; messages: DiscussionMessage[]; runs: DiscussionRun[]; draft: DiscussionDraft | null; workerIssues?: Array<{ runId: string; detail: string }> }
/**
 * Rust's request, plus the model choice the desktop command takes alongside it:
 * `start_discussion` receives `modelSelection` as its own argument, not inside
 * the request, so it is not part of the Rust struct.
 */
export type StartDiscussion = GeneratedStartDiscussion & { modelSelection?: ModelSelection };
export type DiscussionStart = GeneratedDiscussionStart;
export const readDiscussion = (access: ProjectAccess, documentId: string): Promise<DiscussionView> => invoke('read_discussion', { access, documentId });
export const retryDiscussionSave = (access: ProjectAccess, documentId: string, runId: string): Promise<DiscussionView> => invoke('retry_discussion_save', { access, documentId, runId });
export const discussionRetry = (access: ProjectAccess, runId: string): Promise<ComposerBody & { previousRunId: string }> => invoke('discussion_retry', { access, runId });
export const startDiscussion = ({ modelSelection, ...request }: StartDiscussion): Promise<DiscussionStart> => invoke('start_discussion', { request, modelSelection: modelSelection ?? null });
export const stopDiscussion = (access: ProjectAccess, runId: string): Promise<{ run: DiscussionRun }> => invoke('stop_discussion', { access, runId });
export const saveDiscussionDraft = (request: SaveDiscussionDraft): Promise<DiscussionDraft> => invoke('save_discussion_draft', { request });
