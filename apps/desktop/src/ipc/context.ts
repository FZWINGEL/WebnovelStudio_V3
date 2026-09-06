import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor/document';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { FrozenGuidance } from './guidance';
import type { DigestCandidate } from './memory';

export type ContextPurpose = 'discuss' | 'revise' | 'continue' | 'plan' | 'storyQuestion' | 'memoryAnalysis';
export type ContextAudience = 'authorRoom' | 'restrictedWriting';
export type ContextBasis = 'working' | 'reviewed' | 'explicitHistory';
export type CoverageDetail = 'verbatim' | 'digest' | 'directoryOnly';
export interface SourceRef { projectId: string; documentId: string; revisionId: string; bodyHash: string }
export interface InformationPolicy {
  version: string;
  audience: ContextAudience;
  readerFrontier: string | null;
  characterId: string | null;
  characterGrants: Array<{ characterId: string; sourceHandle: string; readerFrontier: string }>;
  allowAlternatives: boolean;
  allowHistorical: boolean;
}
export interface SourceDescriptor {
  handle: string;
  source: SourceRef;
  displayName: string;
  kind: 'currentDraft' | 'reviewedAuthority' | 'explicitRule' | 'adoptedGuidance' | 'generatedObservation' | 'generatedDigest' | 'planAlternative' | 'historical' | 'privateFuture' | 'authorRoomDiscussion';
  current: boolean;
  coverage: CoverageDetail;
  disclosure: { readerPosition: string | null; visibleToCharacters: string[]; authorOnly: boolean; futurePrivate: boolean };
  storyTime: { label: string; position: string | null } | null;
  dependencies: SourceRef[];
}
export interface ReviewedBasisManifest {
  projectId: string; operationNamespace: string;
  prefix: Array<{ documentId: string; bundleId: string; revisionId: string; version: string; bodyHash: string }>;
}
export interface StorySnapshot {
  snapshotId: string; projectId: string; basis: ContextBasis; target: SourceRef;
  contextSourceEpoch: string; orderingEpoch: string; disclosurePolicyVersion: string;
  sources: SourceDescriptor[];
  reviewedBasis?: ReviewedBasisManifest;
}
export interface FrozenContext {
  snapshot: StorySnapshot; policy: InformationPolicy; purpose: ContextPurpose;
  aliases: Record<string, string[]>; excludedSourceCount: number;
  guidance?: FrozenGuidance[];
  conversation?: FrozenConversation;
  navigationViews?: FrozenNavigationView[];
}
export interface NavigationViewRef { viewId: string; projectId: string; operationNamespace: string; contentHash: string }
export interface FrozenNavigationView {
  reference: NavigationViewRef; sourceContextEpoch: string; disclosurePolicyVersion: string;
  dependencies: SourceRef[]; candidate: DigestCandidate;
}
export interface NavigationViewOmission { viewId: string; reason: 'originalTextIncluded' | 'budget' | 'notSmaller' }
export interface ConversationMessage { id: string; content: string; scope: ScopeGrant | null }
export interface ConversationTurn { runId: string; packetId: string; sourceSnapshotId: string; policyVersion: string; user: ConversationMessage; assistant: ConversationMessage }
export interface FrozenConversation { projectId: string; operationNamespace: string; documentId: string; threadId: string; turns: ConversationTurn[]; omittedTurns: number }
export interface SourcePassage { handle: string; source: SourceRef; blockId: string; blockOrder: number; text: string }
export interface SourceRead { descriptor: SourceDescriptor; passages: SourcePassage[]; body: WnsDocument; usedValidatedProjection: boolean }
export interface ScopeGrant {
  kind: 'passage' | 'blocks' | 'wholeDocument' | 'append';
  start: Endpoint | null; end: Endpoint | null;
  sourceHash: string; quote: string; quoteHash: string; prefix: string | null; suffix: string | null;
}
export interface MockContextBudget {
  modelId: 'mock-story-context'; contextWindowTokens: string;
  reservedOutputTokens: string; reservedProtocolTokens: string;
}
export interface PacketReceipt {
  packetId: string; sessionId: string; snapshotId: string; invocationOrdinal: string;
  sourceHandles: string[]; mandatorySourceHandles?: string[]; coverage: Array<{ handle: string; label: string; detail: CoverageDetail }>;
  guidanceHandles?: string[];
  navigationViews?: NavigationViewRef[];
  navigationOmissions?: NavigationViewOmission[];
  safeBrief?: { text: string; textHash: string; originMessageId: string | null };
  conversationMessageIds?: string[];
  omittedDiscussionTurns?: number;
  omissions: string[]; inputHash: string; inputTokens: string; tokenAccountingMethod: string;
}
export interface CompiledPacket {
  messages: Array<{ role: string; content: string }>;
  options: { modelId: string; maxOutputTokens?: string; tokenAccountingMethod: string; providerBinding?: ProviderBinding };
  receipt: PacketReceipt;
}
export interface ProviderBinding {
  providerId: string; modelId: string; reasoning: string | null; serviceTier: string | null;
  profileVersion: string; inputLimitBytes: string; reservedOutputBytes: string;
  reservedProtocolBytes: string; outputLimitBytes: string; accountingMethod: string;
}
export interface ContextBudgetError {
  code: 'mandatoryContextTooLarge' | 'budgetExhausted' | 'invalidBudget';
  message: string; requiredInputTokens: string; availableInputTokens: string; mandatoryHandles: string[];
}
export type PreparationResult = { status: 'prepared'; packet: CompiledPacket; current: boolean } | { status: 'budgetRejected'; error: ContextBudgetError };

export const contextEpochs = (access: ProjectAccess): Promise<{ source: string; policy: string }> => invoke('context_epochs', { access });
export const freezeStoryContext = (request: {
  access: ProjectAccess; operationId: string; expected: Head; basis: ContextBasis; purpose: ContextPurpose; policy: InformationPolicy;
}): Promise<FrozenContext> => invoke('freeze_story_context', { request });
export const freezeReviewedContinuation = (request: {
  access: ProjectAccess; operationId: string; expected: Head; policy: InformationPolicy;
}): Promise<FrozenContext> => invoke('freeze_reviewed_continuation', { request });
export const storyContextSnapshot = (access: ProjectAccess, snapshotId: string): Promise<FrozenContext> => invoke('story_context_snapshot', { access, snapshotId });
export const readStoryContextSource = (access: ProjectAccess, snapshotId: string, handle: string): Promise<SourceRead> => invoke('read_story_context_source', { access, snapshotId, handle });
export const searchStoryContext = (request: {
  access: ProjectAccess; snapshotId: string; query: string; mode: 'literal' | 'lexical' | 'exactAlias'; limit: number;
}): Promise<{ snapshotId: string; hits: Array<{ passage: SourcePassage; startUtf16: number; endUtf16: number }>; sourceMatches: SourceDescriptor[]; searchedSources: number; hasMore: boolean; coverage: string }> => invoke('search_story_context', { request });
export const captureStoryScope = (access: ProjectAccess, snapshotId: string, kind: ScopeGrant['kind'], start: Endpoint | null, end: Endpoint | null): Promise<ScopeGrant> => invoke('capture_story_scope', { access, snapshotId, kind, start, end });
export const prepareStoryContext = (request: {
  access: ProjectAccess; operationId: string; snapshotId: string; instruction: string; mandatoryHandles: string[]; scope: ScopeGrant | null; budget: MockContextBudget;
}): Promise<PreparationResult> => invoke('prepare_story_context', { request });
export const preparedStoryContext = (access: ProjectAccess, packetId: string): Promise<CompiledPacket> => invoke('prepared_story_context', { access, packetId });
export const preparedStoryContextIsCurrent = (access: ProjectAccess, packetId: string): Promise<boolean> => invoke('prepared_story_context_is_current', { access, packetId });
