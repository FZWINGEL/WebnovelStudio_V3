import { invoke } from '@tauri-apps/api/core';
import type { Head, ProjectAccess, Revision } from './projects';

export interface StoryEntityRef { id: string; label: string }
export interface ReviewedEntityChoice { entity: StoryEntityRef; labelVariants: string[]; firstDocumentId: string; firstDocumentTitle: string }
export interface ReviewedEntityCatalog { projectId: string; operationNamespace: string; sourceEpoch: string; entities: ReviewedEntityChoice[] }
export const reviewedEntityCatalog = (access: ProjectAccess): Promise<ReviewedEntityCatalog> => invoke('reviewed_entity_catalog', { access });
export interface EvidenceAnchor { blockId: string; fromUtf16: number; toUtf16: number; quote: string; quoteHash: string }
export interface PossessionRecord {
  id: string;
  object: StoryEntityRef;
  holder: StoryEntityRef | null;
  timing: 'atPassage' | 'earlier' | 'unknown';
  audience: 'authorRoom' | 'reader';
  evidence: EvidenceAnchor;
}
export interface ReviewMember { documentId: string; title: string; bundleId: string; revisionId: string; head: Head }
export interface ReviewStage {
  id: string; projectId: string; operationNamespace: string; target: Head; revision: Revision;
  previousBundleId: string | null; prefix: ReviewMember[]; sourceEpoch: string; policyEpoch: string; createdAt: string;
  records?: PossessionRecord[]; recordsHash?: string;
}
export interface ReadyBundle { id: string; projectId: string; operationNamespace: string; stageId: string; target: Head; createdAt: string; records?: PossessionRecord[]; recordsHash?: string }
export interface ReviewStatus {
  documentId: string; title: string; head: Head; state: 'noReview' | 'ready' | 'changedProse' | 'earlierBasisChanged' | 'reviewNeeded';
  activeBundleId: string | null; pendingStageId: string | null; reason: string | null; canStage: boolean;
}
export interface StageAuthorReview { access: ProjectAccess; operationId: string; expected: Head; records?: PossessionRecord[] }
export interface MarkReady { access: ProjectAccess; operationId: string; stageId: string }
export interface ReviewedRecordSet {
  bundleId: string; projectId: string; operationNamespace: string; target: Head; revision: Revision;
  records: PossessionRecord[]; recordsHash?: string; current: boolean;
}
export const chapterReviewStatus = (access: ProjectAccess, documentId: string): Promise<ReviewStatus> => invoke('chapter_review_status', { access, documentId });
export const stageAuthorReview = (request: StageAuthorReview): Promise<ReviewStage> => invoke('stage_author_review', { request });
export const readReviewStage = (access: ProjectAccess, stageId: string): Promise<ReviewStage> => invoke('read_review_stage', { access, stageId });
export const readReviewedRecordSet = (access: ProjectAccess, documentId: string): Promise<ReviewedRecordSet | null> => invoke('read_reviewed_record_set', { access, documentId });
export const markReady = (request: MarkReady): Promise<ReadyBundle> => invoke('mark_ready', { request });
