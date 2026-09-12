import type {
  CompiledPacket,
  ContextEpochs,
  ContextPurpose,
  ConversationMessage,
  ConversationTurn,
  EvidenceHistory,
  EvidenceHistoryObservation,
  FrozenContext,
  FrozenConversation,
  FrozenNavigationView,
  InformationPolicy,
  KnowledgeHistory,
  KnowledgeHistoryObservation,
  LookupExchange,
  LookupPacketInput,
  LookupSourceProjection,
  MockContextBudget,
  NavigationViewOmission,
  NavigationViewRef,
  PacketReceipt,
  PromiseHistory,
  PromiseHistoryObservation,
  ReviewedBasisManifest,
  ReviewedEvidenceCoverage,
  ReviewedEvidenceOmission,
  ReviewedEvidenceSet,
  ReviewedHistoryResult,
  ReviewedKnowledgeHistoryResult,
  ReviewedKnowledgeSet,
  ReviewedPromiseHistoryResult,
  ReviewedPromiseSet,
  ReviewedSummaryCoverage,
  ReviewedSummaryOmission,
  ReviewedSummarySet,
  ScopeGrant,
  SourceDescriptor,
  SourcePassage,
  SourceRead,
  SourceRef,
  StorySnapshot,
} from './generated/context';
export type {
  CompiledPacket,
  ContextEpochs,
  ContextPurpose,
  ConversationMessage,
  ConversationTurn,
  EvidenceHistory,
  EvidenceHistoryObservation,
  FrozenContext,
  FrozenConversation,
  FrozenNavigationView,
  InformationPolicy,
  KnowledgeHistory,
  KnowledgeHistoryObservation,
  LookupExchange,
  LookupPacketInput,
  LookupSourceProjection,
  MockContextBudget,
  NavigationViewOmission,
  NavigationViewRef,
  PacketReceipt,
  PromiseHistory,
  PromiseHistoryObservation,
  ReviewedBasisManifest,
  ReviewedEvidenceCoverage,
  ReviewedEvidenceOmission,
  ReviewedEvidenceSet,
  ReviewedHistoryResult,
  ReviewedKnowledgeHistoryResult,
  ReviewedKnowledgeSet,
  ReviewedPromiseHistoryResult,
  ReviewedPromiseSet,
  ReviewedSummaryCoverage,
  ReviewedSummaryOmission,
  ReviewedSummarySet,
  ScopeGrant,
  SourceDescriptor,
  SourcePassage,
  SourceRead,
  SourceRef,
  StorySnapshot,
};

import type {
  AppServerConnectionSettlement,
  AppServerDelivery,
  AppServerDispatch,
  AppServerRuntimeIdentity,
  AppServerSubmission,
  AppServerTerminal,
  LookupAllowance,
  ProviderBinding,
  ProviderRuntimeIdentity,
} from './generated/workshop';
export type {
  AppServerConnectionSettlement,
  AppServerDelivery,
  AppServerDispatch,
  AppServerRuntimeIdentity,
  AppServerSubmission,
  AppServerTerminal,
  LookupAllowance,
  ProviderBinding,
  ProviderRuntimeIdentity,
};

import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor/document';
import type { Endpoint, Head, ProjectAccess } from './projects';
import type { FrozenGuidance } from './guidance';
import type { DigestCandidate } from './memory';
import type { KnowledgeRecord, PossessionRecord, PromiseRecord, StoryEntityRef, SummaryRevision } from './reviews';

export const reviewedEvidenceHistory = (access: ProjectAccess, snapshotId: string, objectId: string): Promise<ReviewedHistoryResult> => invoke('reviewed_evidence_history', { access, snapshotId, objectId });
export const reviewedPromiseHistory = (access: ProjectAccess, snapshotId: string, promiseId: string): Promise<ReviewedPromiseHistoryResult> => invoke('reviewed_promise_history', { access, snapshotId, promiseId });
export const reviewedKnowledgeHistory = (access: ProjectAccess, snapshotId: string, characterId: string, topicId: string | null = null): Promise<ReviewedKnowledgeHistoryResult> => invoke('reviewed_knowledge_history', { access, snapshotId, characterId, topicId });

export type ContextAudience = 'authorRoom' | 'restrictedWriting';
export type ContextBasis = 'working' | 'reviewed' | 'explicitHistory';
export type CoverageDetail = 'verbatim' | 'digest' | 'directoryOnly';
export interface DocumentAliasesRead { documentId: string; aliases: string[]; sourceEpoch: string }
export interface ReviewedKnowledgeOmission {
  sourceHandle: string; bundleId: string; recordsHash: string; reason: 'budget' | 'disclosure'; count: number;
}
export interface StorySearchResult {
  snapshotId: string;
  hits: Array<{ passage: SourcePassage; startUtf16: number; endUtf16: number }>;
  sourceMatches: SourceDescriptor[];
  searchedSources: number;
  hasMore: boolean;
  coverage: string;
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
