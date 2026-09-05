import { invoke } from '@tauri-apps/api/core';
import type { Head, ProjectAccess, Revision } from './projects';

export interface ReviewMember { documentId: string; title: string; bundleId: string; revisionId: string; head: Head }
export interface ReviewStage {
  id: string; projectId: string; operationNamespace: string; target: Head; revision: Revision;
  previousBundleId: string | null; prefix: ReviewMember[]; sourceEpoch: string; policyEpoch: string; createdAt: string;
}
export interface ReadyBundle { id: string; projectId: string; operationNamespace: string; stageId: string; target: Head; createdAt: string }
export interface ReviewStatus {
  documentId: string; title: string; head: Head; state: 'noReview' | 'ready' | 'changedProse' | 'earlierBasisChanged' | 'reviewNeeded';
  activeBundleId: string | null; pendingStageId: string | null; reason: string | null; canStage: boolean;
}
export interface StageAuthorReview { access: ProjectAccess; operationId: string; expected: Head }
export interface MarkReady { access: ProjectAccess; operationId: string; stageId: string }
export const chapterReviewStatus = (access: ProjectAccess, documentId: string): Promise<ReviewStatus> => invoke('chapter_review_status', { access, documentId });
export const stageAuthorReview = (request: StageAuthorReview): Promise<ReviewStage> => invoke('stage_author_review', { request });
export const readReviewStage = (access: ProjectAccess, stageId: string): Promise<ReviewStage> => invoke('read_review_stage', { access, stageId });
export const markReady = (request: MarkReady): Promise<ReadyBundle> => invoke('mark_ready', { request });
