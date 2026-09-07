import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor/document';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { FrozenGuidance } from './guidance';
import type { DigestCandidate } from './memory';
import type { KnowledgeRecord, PossessionRecord, PromiseRecord, StoryEntityRef, SummaryRevision } from './reviews';

export interface EvidenceHistoryObservation extends Pick<PossessionRecord, 'object' | 'holder' | 'timing' | 'audience' | 'evidence'> {
  recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number;
}
export interface EvidenceHistory {
  objectId: string; labelVariants: string[]; observations: EvidenceHistoryObservation[];
  uncertainty: ('disclosureLimited' | 'excludedSources' | 'earlierTiming' | 'unknownTiming' | 'unknownHolder' | 'differingHolders')[];
  incomplete: boolean;
}
export interface ReviewedHistoryResult { snapshotId: string; current: boolean; history: EvidenceHistory }
export const reviewedEvidenceHistory = (access: ProjectAccess, snapshotId: string, objectId: string): Promise<ReviewedHistoryResult> => invoke('reviewed_evidence_history', { access, snapshotId, objectId });
export interface PromiseHistoryObservation extends Pick<PromiseRecord, 'promise' | 'phase' | 'timing' | 'note' | 'audience' | 'evidence'> {
  recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number;
}
export interface PromiseHistory {
  promiseId: string; labelVariants: string[]; observations: PromiseHistoryObservation[];
  uncertainty: ('disclosureLimited' | 'excludedSources' | 'earlierTiming' | 'unknownTiming' | 'unclearObservation' | 'conflictingOutcomes')[];
  incomplete: boolean; hasRecordedPayoff: boolean;
}
export interface ReviewedPromiseHistoryResult { snapshotId: string; current: boolean; history: PromiseHistory }
export const reviewedPromiseHistory = (access: ProjectAccess, snapshotId: string, promiseId: string): Promise<ReviewedPromiseHistoryResult> => invoke('reviewed_promise_history', { access, snapshotId, promiseId });
export interface KnowledgeHistoryObservation extends Pick<KnowledgeRecord, 'character' | 'topic' | 'attitude' | 'statement' | 'timing' | 'audience' | 'evidence'> {
  recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number;
}
export interface KnowledgeHistory {
  characterId: string; topicId: string | null; labelVariants: string[];
  observations: KnowledgeHistoryObservation[]; incomplete: true;
  uncertainty: ('noEligibleObservations' | 'earlierOrUnknownTiming' | 'multipleRecordedAttitudes' | 'disclosureLimited')[];
}
export interface ReviewedKnowledgeHistoryResult { snapshotId: string; current: boolean; history: KnowledgeHistory }
export const reviewedKnowledgeHistory = (access: ProjectAccess, snapshotId: string, characterId: string, topicId: string | null = null): Promise<ReviewedKnowledgeHistoryResult> => invoke('reviewed_knowledge_history', { access, snapshotId, characterId, topicId });

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
  reviewedEvidence?: ReviewedEvidenceSet[];
  reviewedPromises?: ReviewedPromiseSet[];
  reviewedKnowledge?: ReviewedKnowledgeSet[];
  reviewedSummaries?: ReviewedSummarySet[];
}
export interface ContextEpochs { source: string; policy: string }
export interface DocumentAliasesRead { documentId: string; aliases: string[]; sourceEpoch: string }
export interface ReviewedSummarySet {
  projectId: string; operationNamespace: string; bundleId: string; summaryHash: string;
  sourceHandle: string; summary: SummaryRevision;
}
export interface ReviewedSummaryCoverage { sourceHandle: string; bundleId: string; summaryId: string; summaryHash: string }
export interface ReviewedSummaryOmission { sourceHandle: string; reason: 'budget' | 'disclosure' | 'originalTextIncluded' | 'notSmaller' }
export interface ReviewedEvidenceSet {
  projectId: string; operationNamespace: string; bundleId: string; recordsHash: string;
  sourceHandle: string; source: SourceRef; records: PossessionRecord[];
}
export interface ReviewedEvidenceCoverage {
  sourceHandle: string; bundleId: string; recordsHash: string; projectionHash: string;
  completeRecordSet: boolean; recordIds: string[];
}
export interface ReviewedPromiseSet extends Omit<ReviewedEvidenceSet, 'records'> { records: PromiseRecord[] }
export interface ReviewedKnowledgeSet extends Omit<ReviewedEvidenceSet, 'records'> { records: KnowledgeRecord[] }
export interface ReviewedEvidenceOmission {
  sourceHandle: string; bundleId: string; recordsHash: string; reason: 'budget' | 'disclosure'; count: number;
}
export interface ReviewedKnowledgeOmission {
  sourceHandle: string; bundleId: string; recordsHash: string; reason: 'budget' | 'disclosure'; count: number;
}
export interface NavigationViewRef { viewId: string; projectId: string; operationNamespace: string; contentHash: string }
export interface FrozenNavigationView {
  reference: NavigationViewRef; sourceContextEpoch: string; disclosurePolicyVersion: string;
  dependencies: SourceRef[]; candidate: DigestCandidate;
}
export interface NavigationViewOmission { viewId: string; reason: 'originalTextIncluded' | 'acceptedSummaryIncluded' | 'budget' | 'notSmaller' }
export interface ConversationMessage { id: string; content: string; scope: ScopeGrant | null }
export interface ConversationTurn { runId: string; packetId: string; sourceSnapshotId: string; policyVersion: string; user: ConversationMessage; assistant: ConversationMessage }
export interface FrozenConversation { projectId: string; operationNamespace: string; documentId: string; threadId: string; turns: ConversationTurn[]; omittedTurns: number }
export interface SourcePassage { handle: string; source: SourceRef; blockId: string; blockOrder: number; text: string }
export interface SourceRead { descriptor: SourceDescriptor; passages: SourcePassage[]; body: WnsDocument; usedValidatedProjection: boolean }
export interface StorySearchResult {
  snapshotId: string;
  hits: Array<{ passage: SourcePassage; startUtf16: number; endUtf16: number }>;
  sourceMatches: SourceDescriptor[];
  searchedSources: number;
  hasMore: boolean;
  coverage: string;
}
export interface LookupAllowance {
  maxAdditionalInvocations: number;
  totalInputBytes: string;
  totalOutputBytes: string;
}
export type LookupSearchMode = 'literal' | 'lexical' | 'exactAlias';
export type LookupMemoryEntityKind = 'character' | 'topic' | 'object' | 'promise';
export interface LookupMemoryEntityEntry {
  entity: StoryEntityRef;
  labelVariants: string[];
  sourceHandle: string;
  source: SourceRef;
}
export interface LookupMemoryEntitiesResult {
  kind: 'findEntities';
  entityKind: LookupMemoryEntityKind;
  query: string;
  entries: LookupMemoryEntityEntry[];
  offset: number;
  totalMatches: number;
  nextOffset: number | null;
  incomplete: boolean;
}
export interface LookupMemoryKnowledgeHistoryResult {
  kind: 'knowledgeHistory';
  history: KnowledgeHistory;
  offset: number;
  totalObservations: number;
  nextOffset: number | null;
}
export interface LookupMemoryPromiseHistoryResult {
  kind: 'promiseHistory';
  history: PromiseHistory;
  offset: number;
  totalObservations: number;
  nextOffset: number | null;
}
export interface LookupMemoryPossessionHistoryResult {
  kind: 'possessionHistory';
  history: EvidenceHistory;
  offset: number;
  totalObservations: number;
  nextOffset: number | null;
}
export type LookupMemoryResult = LookupMemoryEntitiesResult | LookupMemoryKnowledgeHistoryResult | LookupMemoryPromiseHistoryResult | LookupMemoryPossessionHistoryResult;
export type LookupMemoryRequest =
  | { kind: 'findEntities'; id: string; entityKind: LookupMemoryEntityKind; query: string; offset?: number; limit: number }
  | { kind: 'knowledgeHistory'; id: string; characterId: string; topicId?: string; offset?: number; limit: number }
  | { kind: 'promiseHistory'; id: string; promiseId: string; offset?: number; limit: number }
  | { kind: 'possessionHistory'; id: string; objectId: string; offset?: number; limit: number };
export type LookupRequest =
  | { kind: 'search'; id: string; query: string; mode: LookupSearchMode; limit: number }
  | { kind: 'read'; id: string; handle: string; blockIds?: string[] }
  | LookupMemoryRequest;
export type LookupResult =
  | { kind: 'search'; result: StorySearchResult }
  | { kind: 'read'; handle: string; source: SourceRef; passages: SourcePassage[]; complete: boolean }
  | LookupMemoryResult
  | { kind: 'unavailable'; code: string; detail: string };
export interface LookupExchange { request: LookupRequest; result: LookupResult }
export interface LookupSourceProjection {
  schemaVersion: 'story-lookup-source.v1';
  sources: Array<{ handle: string; source: SourceRef; displayName: string }>;
}
export interface LookupPacketInput {
  allowance: LookupAllowance;
  completedInvocations: number;
  exchanges: LookupExchange[];
  reviewedMemory?: 'reviewed-memory.v1';
  sourceProjection?: LookupSourceProjection;
}
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
  lookup?: LookupPacketInput;
  packetId: string; sessionId: string; snapshotId: string; invocationOrdinal: string;
  sourceHandles: string[]; mandatorySourceHandles?: string[]; coverage: Array<{ handle: string; label: string; detail: CoverageDetail }>;
  guidanceHandles?: string[];
  navigationViews?: NavigationViewRef[];
  navigationOmissions?: NavigationViewOmission[];
  reviewedEvidence?: ReviewedEvidenceCoverage[];
  reviewedEvidenceOmissions?: ReviewedEvidenceOmission[];
  reviewedPromises?: ReviewedEvidenceCoverage[];
  reviewedPromiseOmissions?: ReviewedEvidenceOmission[];
  reviewedKnowledge?: ReviewedEvidenceCoverage[];
  reviewedKnowledgeOmissions?: ReviewedKnowledgeOmission[];
  reviewedSummaries?: ReviewedSummaryCoverage[];
  reviewedSummaryOmissions?: ReviewedSummaryOmission[];
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
  runtime?: { cliVersion: string; executableSha256: string; catalogSha256?: string };
  http?: { baseUrl: string; configRevision: string; stream: boolean; responseFormat: 'text' | 'jsonObject' };
  providerId: string; modelId: string; reasoning: string | null; serviceTier: string | null;
  profileVersion: string; inputLimitBytes: string; reservedOutputBytes: string;
  reservedProtocolBytes: string; outputLimitBytes: string; accountingMethod: string;
}
export interface ContextBudgetError {
  code: 'mandatoryContextTooLarge' | 'budgetExhausted' | 'invalidBudget';
  message: string; requiredInputTokens: string; availableInputTokens: string; mandatoryHandles: string[];
}
export type PreparationResult = { status: 'prepared'; packet: CompiledPacket; current: boolean } | { status: 'budgetRejected'; error: ContextBudgetError };

export const contextEpochs = (access: ProjectAccess): Promise<ContextEpochs> => invoke('context_epochs', { access });
export const readDocumentAliases = (access: ProjectAccess, documentId: string): Promise<DocumentAliasesRead> => invoke('read_document_aliases', { access, documentId });
export const setDocumentAliases = (access: ProjectAccess, documentId: string, expectedSourceEpoch: string, aliases: string[]): Promise<ContextEpochs> => invoke('set_document_aliases', { access, documentId, expectedSourceEpoch, aliases });
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
}): Promise<StorySearchResult> => invoke('search_story_context', { request });
export const captureStoryScope = (access: ProjectAccess, snapshotId: string, kind: ScopeGrant['kind'], start: Endpoint | null, end: Endpoint | null): Promise<ScopeGrant> => invoke('capture_story_scope', { access, snapshotId, kind, start, end });
export const prepareStoryContext = (request: {
  access: ProjectAccess; operationId: string; snapshotId: string; instruction: string; mandatoryHandles: string[]; scope: ScopeGrant | null; budget: MockContextBudget;
}): Promise<PreparationResult> => invoke('prepare_story_context', { request });
export const preparedStoryContext = (access: ProjectAccess, packetId: string): Promise<CompiledPacket> => invoke('prepared_story_context', { access, packetId });
export const preparedStoryContextIsCurrent = (access: ProjectAccess, packetId: string): Promise<boolean> => invoke('prepared_story_context_is_current', { access, packetId });
