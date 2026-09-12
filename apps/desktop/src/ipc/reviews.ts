import type {
  ReviewedEntityChoice,
  ReviewedEntityCatalog,
  SummaryChange,
  ReviewStage,
  ReadyBundle,
  ReviewStatus,
  StageAuthorReview,
  MarkReady,
  ReviewedRecordSet,
} from './generated/story';
export type {
  ReviewedEntityChoice,
  ReviewedEntityCatalog,
  SummaryChange,
  ReviewStage,
  ReadyBundle,
  ReviewStatus,
  StageAuthorReview,
  MarkReady,
  ReviewedRecordSet,
};

import type {
  EvidenceAnchor,
  KnowledgeAttitude,
  KnowledgeRecord,
  PossessionRecord,
  PromisePhase,
  PromiseRecord,
  StoryEntityRef,
  SummaryRevision,
} from './generated/context';
export type {
  EvidenceAnchor,
  KnowledgeAttitude,
  KnowledgeRecord,
  PossessionRecord,
  PromisePhase,
  PromiseRecord,
  StoryEntityRef,
  SummaryRevision,
};

import { invoke } from '@tauri-apps/api/core';
import type { Head, ProjectAccess, Revision } from './projects';
import type { SourceRef } from './context';

export const reviewedEntityCatalog = (access: ProjectAccess): Promise<ReviewedEntityCatalog> => invoke('reviewed_entity_catalog', { access });
export const reviewedPromiseCatalog = (access: ProjectAccess): Promise<ReviewedEntityCatalog> => invoke('reviewed_promise_catalog', { access });
export const reviewedKnowledgeCharacterCatalog = (access: ProjectAccess): Promise<ReviewedEntityCatalog> => invoke('reviewed_knowledge_character_catalog', { access });
export const reviewedKnowledgeTopicCatalog = (access: ProjectAccess): Promise<ReviewedEntityCatalog> => invoke('reviewed_knowledge_topic_catalog', { access });
export const knowledgeAttitudeLabels: Record<KnowledgeAttitude, string> = {
  knows: 'Knows', believes: 'Believes', suspects: 'Suspects', rejects: 'Rejects', unaware: 'Explicitly unaware', unclear: 'Unclear',
};
export const promisePhaseLabels: Record<PromisePhase, string> = {
  setup: 'Promise introduced', payoff: 'Payoff recorded', cancelled: 'Cancellation recorded', unclear: 'Outcome unclear',
};
export interface ReviewMember { documentId: string; title: string; bundleId: string; revisionId: string; head: Head }
export type ReviewSummaryAudience = 'authorRoom' | 'reader';
export const chapterReviewStatus = (access: ProjectAccess, documentId: string): Promise<ReviewStatus> => invoke('chapter_review_status', { access, documentId });
export const stageAuthorReview = (request: StageAuthorReview): Promise<ReviewStage> => invoke('stage_author_review', { request });
export const readReviewStage = (access: ProjectAccess, stageId: string): Promise<ReviewStage> => invoke('read_review_stage', { access, stageId });
export const readReviewedRecordSet = (access: ProjectAccess, documentId: string): Promise<ReviewedRecordSet | null> => invoke('read_reviewed_record_set', { access, documentId });
export const markReady = (request: MarkReady): Promise<ReadyBundle> => invoke('mark_ready', { request });
