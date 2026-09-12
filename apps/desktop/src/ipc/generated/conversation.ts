// Generated from `conversation` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { DocumentRecord, Head, ProjectAccess, Revision, StoredResult } from './kernel';
import type { BasisKind, DiscussionRun, FeedbackIntent, LookupAllowance, ProviderBinding } from './workshop';
import type { Endpoint, GuidanceScope, MockContextBudget, ProjectBriefOrigin, ProjectChatDraftRef, ScopeGrant, ScopeKind, SourceDescriptor, SourceKind } from './context';
import type { TypedReplacementBlock } from './documents';

/**
 * The operation's historical result and latest document are deliberately
 * separate. A duplicate Apply never asks the editor to replay an old change.
 */
export type ApplyAck = { access: ProjectAccess; operationId: string; alreadyApplied: boolean; result: StoredResult; document: DocumentRecord }

export type ApplyProposal = { access: ProjectAccess; operationId: string; proposalId: string; preparedId: string; expected: Head; resultHash: string; localGeneration: string }

export type AssistantDraft = { document: DocumentRecord; conversationId: string; originRunId: string; packetId: string; initialRevisionId: string; target: Head | null; predecessorDocumentId?: string; disposition: string; dispositionVersion: string; stale: boolean }

/**
 * Read-only, source-bound feedback from an unscoped chapter discussion. The
 * range is a suggestion for the writer; it is not a scope grant and cannot be
 * adopted without a fresh editor-captured request.
 */
export type ChapterDiscussionFeedback = { runId: string; target: Head; answer: string; rangeProposal?: ChapterRangeProposal; rangeError?: string }

export type ChapterRangeProposal = { sourceHead: Head; firstBlockId: string; lastBlockId: string; quote: string }

export type ChatAdoptionAck = { previewId: string; documents: DocumentRecord[]; decisionId: string }

/**
 * Complete immutable grouped-adoption manifest. Relationship dependencies
 * are read-only drift fences; proposed effects are explicit author-review
 * material and are never inferred from document prose.
 */
export type ChatAdoptionEffects = { version: string; sourceOutputHash: string; relationshipDependencies: ChatRelationshipDependency[]; protectedContent: ChatProtectedContent[]; proposedRelationships: ChatAdoptionRelationship[]; impacts: ChatAdoptionImpact[]; supersessions: ChatAdoptionSupersession[]; placements: ChatAdoptionPlacement[] }

export type ChatAdoptionImpact = { targetDocumentId: string; kind: string; reason: string; relationshipId?: string; relationshipKey?: string }

export type ChatAdoptionPlacement = { targetDocumentId: string; beforeDocumentId?: string; afterDocumentId?: string }

export type ChatAdoptionPreview = { id: string; version: string; digest: string; projectId: string; operationNamespace: string; conversationId: string; sourceEpoch: string; policyEpoch: string; workshopVersion: string; targets: ChatAdoptionTarget[]; effects: ChatAdoptionEffects | null }

export type ChatAdoptionRelationship = { key: string; relationshipId: string; fromDocumentId: string; toDocumentId: string; type: string; description: string; uncertainty: string; fromHead: Head; toHead: Head }

export type ChatAdoptionSupersession = { targetDocumentId: string; supersededDocumentId: string; reason: string }

export type ChatAdoptionTarget = { draft: ProjectChatDraftRef; draftRevisionId: string; documentId: string; title: string; kind: string; before: DocumentRecord | null; body: any }

export type ChatDocumentSave = { operationId: string; head: Head; title: string; createdAt: string; revisionId: string | null }

export type ChatProtectedContent = { targetDocumentId: string; sourceHead: Head; text: string; textHash: string }

export type ChatRelationshipDependency = { relationshipId: string; fromDocumentId: string; toDocumentId: string; relationshipType: string; fromHead: Head; toHead: Head }

export type ContinuationCandidate = { title: string; paragraphs: string[]; explanation: string }

export type ConversationItem = { id: string; sequence: string; kind: string; referenceId: string | null; payload: any; createdAt: string }

export type DiscussionDraft = { documentId: string; version: string; text: string; intent?: FeedbackIntent; basis?: BasisKind; scope: DiscussionScopeInput | null; pinnedDocumentIds: string[]; safeBrief?: SafeBriefInput; previousRunId?: string; updatedAt: string; lookup?: LookupAllowance }

export type DiscussionMessage = { id: string; threadId: string; runId: string | null; role: DiscussionMessageRole; content: string; scope: ScopeGrant | null; packetId: string | null; createdAt: string }

export type DiscussionMessageRole = "user" | "assistant"

export type DiscussionScopeInput = { kind: ScopeKind; start: Endpoint | null; end: Endpoint | null; quote: string; sourceBodyHash: string }

export type DiscussionView = { documentId: string; threadId: string | null; messages: DiscussionMessage[]; runs: DiscussionRun[]; draft: DiscussionDraft | null }

export type HistoricalConversation = { conversation: HistoricalConversationRef; anchorDocumentId: string; items: HistoricalConversationItem[]; olderBefore: string | null }

export type HistoricalConversationItem = { item: ConversationItem; run?: DiscussionRun; messages?: DiscussionMessage[]; sourceRevisions?: HistoricalSourceRevision[]; draftRevisions?: HistoricalDraftRevision[] }

export type HistoricalConversationRef = { projectId: string; operationNamespace: string; conversationId: string }

export type HistoricalConversationSummary = { conversation: HistoricalConversationRef; anchorDocumentId: string; itemCount: number; current: boolean }

export type HistoricalDraftRevision = { documentId: string; initial: boolean; revision: Revision }

export type HistoricalSourceRevision = { handle: string; kind: SourceKind; descriptor: SourceDescriptor; revision: Revision }

export type PrepareContinuation = { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; paragraphs: string[]; body: any }

export type PrepareProposal = { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; replacementText: string; body: any }

export type PrepareStructured = { access: ProjectAccess; operationId: string; proposalId: string; expectedPreparedVersion: string; blocks: TypedReplacementBlock[]; body: any }

export type PreparedProposal = { id: string; proposalId: string; version: string; replacementText: string; paragraphs?: string[]; blocks?: TypedReplacementBlock[]; body: any; bodyHash: string }

export type ProjectChapterComposer = { target: Head; intent: FeedbackIntent; basis?: BasisKind; scope?: DiscussionScopeInput; safeBrief?: SafeBriefInput }

export type ProjectComposer = { text: string; sourceRefs?: Head[]; taskDraftRefs?: ProjectChatDraftRef[]; focusedDocumentRef?: Head; chapter?: ProjectChapterComposer }

export type ProjectComposerSnapshot = { conversationId: string; version: string; body: ProjectComposer }

export type Proposal = { id: string; runId: string; kind?: ProposalKind; candidate: ProposalContent; source: Head; sourceBody: any; scope: ScopeGrant; snapshotId: string; packetId: string; current: boolean; historicalCopy: boolean; prepared: PreparedProposal | null; decision: ProposalDecision | null }

export type ProposalCandidate = { title: string; replacementText: string; explanation: string }

export type ProposalContent = ProposalCandidate | ContinuationCandidate | StructuredProposalCandidate

export type ProposalDecision = { id: string; proposalId: string; kind: string; preparedId: string | null; beforeRevisionId: string | null; afterRevisionId: string | null }

/**
 * The durable kind discriminator is read before parsing candidate JSON.  The
 * untagged wire representation preserves legacy passage bytes while allowing
 * continuation candidates to use their paragraph payload directly.
 */
export type ProposalKind = "passage" | "continuation" | "structured"

export type ReadProjectChatHistory = { access: ProjectAccess; conversation: HistoricalConversationRef; before?: string | null; limit?: number }

/**
 * A request-scoped author direction. It is separate from story evidence and
 * only enters a restricted writing packet after explicit confirmation.
 */
export type SafeBriefInput = { text: string; originMessageId: string | null; confirmed: boolean; projectOrigin?: ProjectBriefOrigin }

export type SaveDiscussionDraft = { access: ProjectAccess; operationId: string; documentId: string; expectedVersion: string; text: string; intent?: FeedbackIntent; basis?: BasisKind; scope: DiscussionScopeInput | null; pinnedDocumentIds: string[]; safeBrief?: SafeBriefInput; previousRunId?: string; lookup?: LookupAllowance }

export type SaveGuidance = { access: ProjectAccess; operationId: string; guidanceId: string; expectedVersion: string; text: string; scope: GuidanceScope; documentId: string | null; active: boolean; originMessageId: string | null }

export type SaveProjectComposer = { access: ProjectAccess; operationId: string; conversationId: string; expectedVersion: string; body: ProjectComposer }

export type StartProjectChapter = { access: ProjectAccess; operationId: string; conversationId: string; expectedComposerVersion: string; composer: ProjectComposer; budget: MockContextBudget; providerBinding?: ProviderBinding }

export type StartProjectChat = { access: ProjectAccess; operationId: string; conversationId: string; expectedComposerVersion: string; composer: ProjectComposer; budget: MockContextBudget; providerBinding?: ProviderBinding }

/**
 * A structured candidate replaces complete blocks selected by an explicit
 * blocks or whole-document scope. IDs are deliberately absent; the editor
 * allocates fresh identities while preparing the complete result snapshot.
 */
export type StructuredProposalCandidate = { title: string; blocks: TypedReplacementBlock[]; explanation: string }

