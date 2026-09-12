// Generated from `story` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { Head, ProjectAccess, Revision, SourceEpoch } from './kernel';
import type { AppServerDelivery, BasisKind, DiscussionRun, FeedbackIntent, LookupAllowance, ProviderBinding, ProviderCleanup, ProviderDeliveryReceipt, ProviderOutcomeStatus, ProviderUsage, WorkshopWorkingSelection } from './workshop';
import type { BudgetError, CompiledPacket, DigestCandidate, KnowledgeRecord, MockContextBudget, PossessionRecord, PromiseRecord, ReviewPrefixItem, ScopeGrant, SourceRef, StoryEntityRef, SummaryAudience, SummaryRevision } from './context';
import type { DiscussionMessageRole, DiscussionScopeInput, SafeBriefInput } from './conversation';

export type DiscussionMessage = { id: string; threadId: string; runId: string | null; role: DiscussionMessageRole; content: string; scope: ScopeGrant | null; packetId: string | null; createdAt: string }

export type DiscussionStart = { threadId: string; run: DiscussionRun; userMessage: DiscussionMessage; packet: CompiledPacket }

export type MarkReady = { access: ProjectAccess; operationId: string; stageId: string }

export type MemoryDispatchState = "pending" | "dispatched"

export type MemoryJob = { id: string; owner: MemoryOwner; operationId: string; payloadHash: string; target: Head; source: SourceRef; snapshotId: string; packetId: string; contextSourceEpoch: SourceEpoch; disclosurePolicyVersion: string; providerBinding: ProviderBinding | null; status: MemoryJobStatus; dispatchState: MemoryDispatchState; historical: boolean; stopReason: string | null; result: MemoryResult | null; view: MemoryView | null; createdAt: string; updatedAt: string }

export type MemoryJobStatus = "queued" | "running" | "stopping" | "completed" | "stopped" | "failed" | "interrupted"

export type MemoryOwner = { projectId: string; operationNamespace: string; jobId: string }

export type MemoryRead = { documentId: string; jobs: MemoryJob[]; views: MemoryView[]; pendingSave: boolean; pendingJobIds: string[] }

export type MemoryResult = { appServer?: AppServerDelivery; jobId: string; eventId: string; rawOutput: string | null; outcome: ProviderOutcomeStatus; confirmedStdinBytes: string | null; usage: ProviderUsage | null; cleanup: ProviderCleanup | null; error: string | null; validationError: string | null; candidate: DigestCandidate | null; effectiveIdentity: string | null; delivery?: ProviderDeliveryReceipt; createdAt: string }

export type MemoryView = { id: string; jobId: string; projectId: string; operationNamespace: string; documentId: string; target: Head; source: SourceRef; snapshotId: string; packetId: string; contextSourceEpoch: SourceEpoch; disclosurePolicyVersion: string; candidate: DigestCandidate | null; current: boolean; sourceChanged: boolean; policyAvailable: boolean; historical: boolean; createdAt: string }

export type PreparationResult = { status: "prepared"; packet: CompiledPacket; current: boolean } | { status: "budgetRejected"; error: BudgetError }

export type ReadyBundle = { id: string; projectId: string; operationNamespace: string; stageId: string; target: Head; records?: PossessionRecord[]; recordsHash?: string; promises?: PromiseRecord[]; promisesHash?: string; knowledge?: KnowledgeRecord[]; knowledgeHash?: string; summary?: SummaryRevision; summaryHash?: string; createdAt: string }

export type ReviewStage = { id: string; projectId: string; operationNamespace: string; target: Head; revision: Revision; previousBundleId: string | null; prefix: ReviewPrefixItem[]; records?: PossessionRecord[]; recordsHash?: string; promises?: PromiseRecord[]; promisesHash?: string; knowledge?: KnowledgeRecord[]; knowledgeHash?: string; summary?: SummaryRevision; summaryHash?: string; sourceEpoch: SourceEpoch; policyEpoch: string; createdAt: string }

export type ReviewState = "noReview" | "ready" | "changedProse" | "earlierBasisChanged" | "reviewNeeded"

export type ReviewStatus = { documentId: string; title: string; head: Head; state: ReviewState; activeBundleId: string | null; pendingStageId: string | null; reason: string | null; canStage: boolean }

export type ReviewedEntityCatalog = { projectId: string; operationNamespace: string; sourceEpoch: SourceEpoch; entities: ReviewedEntityChoice[] }

export type ReviewedEntityChoice = { entity: StoryEntityRef; labelVariants: string[]; firstDocumentId: string; firstDocumentTitle: string }

export type ReviewedRecordSet = { bundleId: string; projectId: string; operationNamespace: string; target: Head; revision: Revision; records: PossessionRecord[]; recordsHash?: string; promises?: PromiseRecord[]; promisesHash?: string; knowledge?: KnowledgeRecord[]; knowledgeHash?: string; summary?: SummaryRevision; summaryHash?: string; current: boolean }

export type SaveSourcePins = { access: ProjectAccess; operationId: string; scope: SourcePinScope; targetDocumentId: string | null; expectedVersion: string; sourceDocumentIds: string[] }

export type SourcePinScope = "project" | "document"

export type SourcePinSet = { scope: SourcePinScope; targetDocumentId: string | null; version: string; sourceDocumentIds: string[]; audience: string }

export type SourcePinsView = { project: SourcePinSet; document: SourcePinSet }

export type StageAuthorReview = { access: ProjectAccess; operationId: string; expected: Head; records?: PossessionRecord[]; promises?: PromiseRecord[]; knowledge?: KnowledgeRecord[]; summary?: SummaryChange }

export type StartDiscussion = { access: ProjectAccess; operationId: string; expected: Head; instruction: string; intent?: FeedbackIntent; basis?: BasisKind; scope: DiscussionScopeInput | null; pinnedDocumentIds: string[]; safeBrief?: SafeBriefInput; budget: MockContextBudget; providerBinding?: ProviderBinding; previousRunId: string | null; lookup?: LookupAllowance }

export type StartMemory = { access: ProjectAccess; operationId: string; expected: Head; budget: MockContextBudget; providerBinding?: ProviderBinding }

/**
 * An explicit author decision about the summary attached to a staged review.
 * The optional field on [`super::reviewed_story::StageAuthorReview`] is
 * omitted when absent so legacy request payload hashes remain unchanged.
 */
export type SummaryChange = { kind: "set"; text: string; audience: SummaryAudience } | { kind: "clear" }

export type WorkshopExploration = { sessionId: string; expectedVersion: string; workingGeneration: string; action: string; instruction: string; selectedScope: string; selectedText: string; workingSelection?: WorkshopWorkingSelection }

