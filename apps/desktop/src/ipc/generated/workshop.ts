// Generated from `wns-workshop` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { DocumentRecord, Head, ProjectAccess } from './kernel';

export type AdoptionMode = "add" | "replace"

/**
 * Connection state is distinct from settlement of this one request. Closing
 * a server after a failed interrupt can settle ownership without establishing
 * whether its upstream generation completed.
 */
export type AppServerConnectionSettlement = "reusable" | "closed" | "unresolved"

export type AppServerDelivery = { dispatch: AppServerDispatch | null; submission: AppServerSubmission; turnId: string | null; terminal: AppServerTerminal | null; requestSettled: boolean; connection: AppServerConnectionSettlement }

/**
 * Committed before writing turn/start. The provider thread has been created,
 * but this record alone cannot authorize another external start after recovery.
 */
export type AppServerDispatch = { serverGeneration: string; threadId: string; rpcId: string; packetHash: string; requestHash: string }

/**
 * Non-secret identity of the process-level isolation and account contract.
 * The selected model descriptor remains in ProviderRuntimeIdentity.catalog_sha256.
 */
export type AppServerRuntimeIdentity = { accountSha256: string; securityConfigSha256: string; restrictiveCatalogSha256: string }

export type AppServerSubmission = "notSent" | "uncertain" | "acknowledged"

export type AppServerTerminal = "completed" | "interrupted" | "failed"

/**
 * The editorial basis selected for a frozen request.
 */
export type BasisKind = "working" | "reviewed" | "explicitHistory"

export type CandidateChoice = { candidateId: string; status: CandidateChoiceStatus; rationale: string; includeInContext: boolean }

export type CandidateChoiceStatus = "saved" | "rejected" | "archived"

export type DiscussionRun = { id: string; threadId: string; owner: RunOwner; operationId: string; intent: FeedbackIntent; basis?: BasisKind | null; payloadHash: string; target: Head; packetId: string; providerBinding?: ProviderBinding | null; providerResult?: ProviderResult | null; lookup?: LookupRunSummary | null; previousRunId: string | null; status: DiscussionRunStatus; dispatchState: string; sequence: string; outputText: string; stopReason: string | null; createdAt: string; updatedAt: string }

export type DiscussionRunStatus = "queued" | "running" | "stopping" | "completed" | "stopped" | "failed" | "interrupted"

/**
 * The two author-room actions supported by a discussion request.  `Discuss`
 * is intentionally the wire default so older clients produce the same
 * request hash they did before intent was added to the contract.
 */
export type FeedbackIntent = "discuss" | "proposeEdits" | "continue" | "workshopExplore"

/**
 * Evidence about the HTTP request itself.  This is intentionally separate
 * from Codex's local stdin count: an HTTP request can be accepted by a remote
 * server even when the local process loses the response before it is parsed.
 */
export type HttpDeliverySubmission = "notSent" | "uncertain" | "responseReceived"

export type HttpProviderBinding = { baseUrl: string; configRevision: string; stream: boolean; responseFormat: HttpResponseFormat }

export type HttpProviderUsage = { inputTokens?: number | null; outputTokens?: number | null; totalTokens?: number | null }

export type HttpResponseFormat = "text" | "jsonObject"

export type Lens = "overview" | "world" | "people" | "themes" | "possibilities" | "notebook"

/**
 * Application byte allowances for the initial lookup request and its
 * bounded context-expansion invocations. These are serialized decimal byte
 * caps, not provider token or billing limits.
 */
export type LookupAllowance = { maxAdditionalInvocations: number; totalInputBytes: string; totalOutputBytes: string }

export type LookupInvocationState = "prepared" | "claimed" | "needsContext" | "completed" | "failed" | "stopped" | "unknown"

export type LookupInvocationSummary = { ordinal: string; packetId: string; state: LookupInvocationState; inputDelivered: boolean; response: any | null; error: string | null }

export type LookupRunSummary = { allowance: LookupAllowance; invocations: LookupInvocationSummary[] }

export type PreferencePolarity = "neutral" | "want" | "avoid"

export type PreferenceScope = "project" | "element" | "exploration"

export type PreferenceStrength = "soft" | "hard"

export type PreviewWorkshopAdoption = { access: ProjectAccess; sessionId: string; expectedVersion: string; candidateIds: string[]; targets: WorkshopAdoptionTarget[]; rationale: string; protectedText: string[]; relationships?: WorkshopRelationshipDraft[]; impactDrafts?: WorkshopImpactDraft[] }

/**
 * These limits are application byte caps for the exact serialized stdin and
 * retained output. They are deliberately not model token-window claims. A
 * Provider-specific adapters may replace these fixed profiles with separately
 * qualified contracts; this slice accepts only the explicit Codex and
 * OpenAI-compatible contracts below.
 */
export type ProviderBinding = { providerId: string; modelId: string; reasoning: string | null; serviceTier: string | null; profileVersion: string; inputLimitBytes: string; reservedOutputBytes: string; reservedProtocolBytes: string; outputLimitBytes: string; accountingMethod: string; runtime?: ProviderRuntimeIdentity | null; http?: HttpProviderBinding | null }

export type ProviderCleanup = "settled" | "unresolved"

export type ProviderDeliveryReceipt = { bodyHash: string; bodyBytes: string; submission: HttpDeliverySubmission; usage?: HttpProviderUsage | null }

/**
 * The provider-side outcome is kept separate from the discussion lifecycle.
 * For example, a timed-out provider request with settled cleanup becomes a
 * durable failed discussion while retaining any validated prefix.
 */
export type ProviderOutcomeStatus = "completed" | "stopped" | "timedOut" | "outputLimit" | "failed"

export type ProviderResult = { runId: string; packetId: string; eventId: string; expectedSequence: string; assistantText: string; binding: ProviderBinding; status: ProviderOutcomeStatus; confirmedStdinBytes: string; usage: ProviderUsage | null; cleanup: ProviderCleanup; error: string | null; effectiveIdentity: string | null; reportedModel?: string | null; createdAt: string; delivery?: ProviderDeliveryReceipt | null; appServer?: AppServerDelivery | null }

export type ProviderRuntimeIdentity = { cliVersion: string; executableSha256: string; catalogSha256?: string | null; appServer?: AppServerRuntimeIdentity | null }

/**
 * Raw provider usage is optional. Missing usage is an explicit unknown value;
 * no estimate is substituted from the packet's byte accounting.
 */
export type ProviderUsage = { inputTokens: number; cachedInputTokens: number; cacheWriteInputTokens: number; outputTokens: number; reasoningOutputTokens: number }

export type RunOwner = { projectId: string; operationNamespace: string; runId: string }

export type SaveWorkshop = { access: ProjectAccess; operationId: string; expectedVersion: string; state: WorkshopState }

export type SelectedDetail = { id: string; candidateId: string | null; text: string; fixed: boolean }

export type StoryPossibility = { id: string; kind: StoryPossibilityKind; text: string; status: StoryPossibilityStatus }

export type StoryPossibilityKind = "unresolvedQuestion" | "intendedPayoff" | "possibleArc"

export type StoryPossibilityStatus = "open" | "archived"

export type UnknownTo = "author" | "reader" | "both"

export type WorkshopAdoptionAck = { snapshot: WorkshopSnapshot; documents: DocumentRecord[]; decisionIds: string[] }

export type WorkshopAdoptionImpact = { candidateId: string; documentId: string; kind: WorkshopImpactKind; reason: string; status: WorkshopImpactStatus }

export type WorkshopAdoptionPreview = { id: string; sessionId: string; expectedVersion: string; targets: WorkshopAdoptionTarget[]; before: DocumentRecord[]; rationale: string; protectedText: string[]; candidateIds: string[]; relationships?: WorkshopRelationship[]; endpointSources?: DocumentRecord[]; impacts?: WorkshopAdoptionImpact[] }

export type WorkshopAdoptionTarget = { documentId: string; expected: Head | null; title: string; kind: string; body: any; mode: AdoptionMode }

export type WorkshopBranchKind = "working" | "whatIf"

export type WorkshopCandidate = { id: string; title: string; content: string; dimensionValue: string; implications: WorkshopCandidateImplication[]; assumptions: string[]; affectedTargets: WorkshopCandidateAffectedTarget[]; preservedDetails: string[]; changedDetails: string[] }

export type WorkshopCandidateAffectedTarget = { documentId: string; reason: string }

export type WorkshopCandidateImplication = { text: string; basis: string; assumption: string }

export type WorkshopDecision = { id: string; sessionId: string; title: string; documentId: string; revisionId: string; head: Head; candidateIds: string[]; rationale: string; status: WorkshopDecisionStatus; fixed: boolean; protectedText: string[]; access: string; supersedesId: string | null }

export type WorkshopDecisionStatus = "chosen" | "archived" | "superseded"

export type WorkshopDepth = "sketch" | "develop" | "document"

export type WorkshopImpact = { id: string; decisionId: string; documentId: string; kind: WorkshopImpactKind; reason: string; status: WorkshopImpactStatus; candidateId?: string | null; relationshipId?: string | null }

/**
 * An author classification supplied with an adoption preview.  It is tied
 * to a candidate affected target and becomes an immutable impact provenance
 * record when the adoption commits.
 */
export type WorkshopImpactDraft = { documentId: string; kind: WorkshopImpactKind; reason: string }

export type WorkshopImpactKind = "contradiction" | "possibleTension" | "dependentAssumption" | "styleSuggestion"

export type WorkshopImpactStatus = "needsReview" | "acknowledged" | "intentional"

export type WorkshopOutput = { schemaVersion: string; requestKind: string; question: string; questionReason: string; dimension: string; interpretation: WorkshopOutputInterpretation; candidates: WorkshopCandidate[] }

export type WorkshopOutputInterpretation = { youSaid: string; possibleDirection: string; stillOpen: string }

export type WorkshopPreference = { id: string; label: string; family: string; meaning: string; examples: string; timing: string; polarity: PreferencePolarity; strength: PreferenceStrength; scope: PreferenceScope; targetId: string | null; confirmed: boolean }

export type WorkshopPreset = { id: string; name: string; preferences: WorkshopPreference[] }

export type WorkshopQuestion = { id: string; text: string; reason: string; status: WorkshopQuestionStatus; unknownTo: UnknownTo }

export type WorkshopQuestionStatus = "open" | "notNow" | "notRelevant" | "keepMysterious"

export type WorkshopRelationship = { id: string; fromDocumentId: string; toDocumentId: string; type: string; description: string; uncertainty: string; status: WorkshopRelationshipStatus; sourceHeads: Head[] }

/**
 * A relationship proposed as part of an atomic adoption.  This remains a
 * request shape until the adoption commits the exact endpoint heads into a
 * `WorkshopRelationship` record.
 */
export type WorkshopRelationshipDraft = { id: string; fromDocumentId: string; toDocumentId: string; type: string; description: string; uncertainty: string; fromExpected: Head | null; toExpected: Head | null }

export type WorkshopRelationshipStatus = "tentative" | "chosen" | "archived"

export type WorkshopResult = { run: DiscussionRun; sessionId: string; workingGeneration: string; action: string; workingSelection?: WorkshopWorkingSelection | null; output: WorkshopOutput | null; validationError: string | null; stale: boolean }

export type WorkshopSession = { id: string; title: string; lens: Lens; parentSessionId: string | null; branchKind: WorkshopBranchKind; brief: string; direction: string; stillOpen: string; focusQuestion: string; focusReason: string; focusDocumentId: string | null; anchorDocumentId: string | null; depth: WorkshopDepth; outsideDirection: boolean; includedDocumentIds: string[]; workingText: string; workingTitle: string; workingGeneration: string; selectedDetails: SelectedDetail[]; choices: CandidateChoice[]; questions: WorkshopQuestion[]; composer: string; selectedScope: string; originalNotes: string; activeRunId: string | null; relationshipId?: string | null; storyPossibilities?: StoryPossibility[] }

export type WorkshopSnapshot = { version: string; state: WorkshopState }

export type WorkshopState = { schemaVersion: number; currentSessionId: string | null; sessions: WorkshopSession[]; preferences: WorkshopPreference[]; decisions: WorkshopDecision[]; relationships: WorkshopRelationship[]; impacts: WorkshopImpact[]; presets: WorkshopPreset[] }

export type WorkshopView = { version: string; state: WorkshopState; results: WorkshopResult[] }

/**
 * document while holding the actor's CAS boundary.
 */
export type WorkshopWorkingSelection = { from: number; to: number; text: string }

