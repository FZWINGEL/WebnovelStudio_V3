import type {
  ApplyAck,
  ApplyProposal,
  PrepareContinuation as WirePrepareContinuation,
  PrepareProposal as WirePrepareProposal,
  PrepareStructured as WirePrepareStructured,
  PreparedProposal as WirePreparedProposal,
  Proposal as WireProposal,
  ProposalCandidate,
  ProposalDecision,
} from './generated/conversation';

// Rust carries a document body as `serde_json::Value`, so the generated shapes
// say `any`. Both directions narrow it back to the editor's model, the same way
// `ipc/projects` narrows `Revision`: every *other* field still comes from Rust.
export type PreparedProposal = Omit<WirePreparedProposal, 'body'> & { body: WnsDocument };
export type Proposal = Omit<WireProposal, 'sourceBody'> & { sourceBody: WnsDocument };
export type PrepareProposal = Omit<WirePrepareProposal, 'body'> & { body: WnsDocument };
export type PrepareContinuation = Omit<WirePrepareContinuation, 'body'> & { body: WnsDocument };
export type PrepareStructured = Omit<WirePrepareStructured, 'body'> & { body: WnsDocument };
export type {
  ApplyAck,
  ApplyProposal,
  ProposalCandidate,
  ProposalDecision,
};

import type { AppliedDecision } from './generated/kernel';
import type { TypedReplacementBlock } from './generated/documents';
export type { AppliedDecision };

import { invoke } from '@tauri-apps/api/core';
import type { Inline, WnsDocument } from '../editor';
import type { DocumentRecord, Head, ProjectAccess } from './projects';
import type { ScopeGrant } from './context';

export interface ContinuationCandidate { title: string; paragraphs: string[]; explanation: string }
/**
 * Proposal content never assigns editor block identities — the application
 * assigns one while preparing the result document, which is why the wire type
 * carries no `id`. Generated from `wns_documents::structured`; the frontend
 * used to carry a hand-written copy that had drifted from it.
 */
export type StructuredBlock = TypedReplacementBlock;
export interface StructuredCandidate { title: string; blocks: StructuredBlock[]; explanation: string }
export const readProposals = (access: ProjectAccess, documentId: string): Promise<Proposal[]> => invoke('proposals', { access, documentId });
export const prepareProposal = (request: PrepareProposal): Promise<PreparedProposal> => invoke('prepare_proposal', { request });
export const prepareContinuationProposal = (request: PrepareContinuation): Promise<PreparedProposal> => invoke('prepare_continuation', { request });
export const prepareStructuredProposal = (request: PrepareStructured): Promise<PreparedProposal> => invoke('prepare_structured', { request });
export const applyProposal = (request: ApplyProposal): Promise<ApplyAck> => invoke('apply_proposal', { request });
export const rejectProposal = (access: ProjectAccess, proposalId: string, operationId: string): Promise<ProposalDecision> => invoke('reject_proposal', { request: { access, proposalId, operationId } });
