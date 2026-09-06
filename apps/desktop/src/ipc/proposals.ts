import { invoke } from '@tauri-apps/api/core';
import type { Inline, WnsDocument } from '../editor/document';
import type { DocumentRecord, Head, ProjectAccess } from './projects';
import type { ScopeGrant } from './context';

export interface ProposalCandidate { title: string; replacementText: string; explanation: string }
export interface ContinuationCandidate { title: string; paragraphs: string[]; explanation: string }
/** Proposal content never assigns editor block identities. */
export type StructuredBlock = { type: 'paragraph'; content: Inline[] } | { type: 'heading'; attrs: { level: number }; content: Inline[] } | { type: 'sceneBreak' };
export interface StructuredCandidate { title: string; blocks: StructuredBlock[]; explanation: string }
export interface PreparedProposal { id: string; proposalId: string; version: string; replacementText: string; paragraphs?: string[]; blocks?: StructuredBlock[]; body: WnsDocument; bodyHash: string }
export interface AppliedDecision { decisionId: string; proposalId: string; preparedId: string; beforeRevisionId: string; afterRevisionId: string }
export interface ProposalDecision { id: string; proposalId: string; kind: 'apply' | 'reject'; preparedId: string | null; beforeRevisionId: string | null; afterRevisionId: string | null }
export interface Proposal {
  id: string; runId: string; kind?: 'passage' | 'continuation' | 'structured'; candidate: ProposalCandidate | ContinuationCandidate | StructuredCandidate; source: Head; sourceBody: WnsDocument; scope: ScopeGrant;
  snapshotId: string; packetId: string; current: boolean; historicalCopy: boolean; prepared: PreparedProposal | null; decision: ProposalDecision | null;
}
export interface PrepareProposal { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; replacementText: string; body: WnsDocument }
export interface PrepareContinuation { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; paragraphs: string[]; body: WnsDocument }
export interface PrepareStructured { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; blocks: StructuredBlock[]; body: WnsDocument }
export interface ApplyProposal { access: ProjectAccess; operationId: string; proposalId: string; preparedId: string; expected: Head; resultHash: string; localGeneration: string }
export interface ApplyAck {
  access: ProjectAccess; operationId: string; alreadyApplied: boolean;
  result: { head: Head; savedGeneration: string; applied: AppliedDecision }; document: DocumentRecord;
}
export const readProposals = (access: ProjectAccess, documentId: string): Promise<Proposal[]> => invoke('proposals', { access, documentId });
export const prepareProposal = (request: PrepareProposal): Promise<PreparedProposal> => invoke('prepare_proposal', { request });
export const prepareContinuationProposal = (request: PrepareContinuation): Promise<PreparedProposal> => invoke('prepare_continuation', { request });
export const prepareStructuredProposal = (request: PrepareStructured): Promise<PreparedProposal> => invoke('prepare_structured', { request });
export const applyProposal = (request: ApplyProposal): Promise<ApplyAck> => invoke('apply_proposal', { request });
export const rejectProposal = (access: ProjectAccess, proposalId: string, operationId: string): Promise<ProposalDecision> => invoke('reject_proposal', { request: { access, proposalId, operationId } });
