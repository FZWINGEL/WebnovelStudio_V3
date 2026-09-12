// Generated from `context` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { Head } from './kernel';
import type { BasisKind, LookupAllowance, ProviderBinding } from './workshop';

/**
 * Whether the packet serves an author-room discussion or a restricted prose
 * request. Author-room knowledge is never an Apply authorization.
 */
export type Audience = "authorRoom" | "restrictedWriting"

/**
 * Structured budget failures preserve decimal counters and explicit gaps;
 * callers must not silently shorten a mandatory target into a different task.
 */
export type BudgetError = { code: BudgetErrorCode; message: string; requiredInputTokens: string; availableInputTokens: string; mandatoryHandles: string[] }

export type BudgetErrorCode = "mandatoryContextTooLarge" | "budgetExhausted" | "invalidBudget"

/**
 * An explicit grant that a limited-POV character may use a source already
 * disclosed to the reader at or before the stated frontier. It never bypasses
 * the reader frontier and never uses story time as a disclosure shortcut.
 */
export type CharacterGrant = { characterId: string; sourceHandle: string; readerFrontier: string }

export type ChatDispositionScope = { kind: ChatDispositionScopeKind; referenceId?: string | null }

/**
 * The bounded story surface to which a question decision applies. A missing
 * scope on an older client means `project`; references are authenticated
 * against the producing conversation before the event is stored.
 */
export type ChatDispositionScopeKind = "project" | "task" | "chapter" | "document"

export type ChatUnknownTo = "author" | "reader" | "both"

export type CompiledPacket = { messages: PacketMessage[]; options: PacketOptions; receipt: PacketReceipt }

export type ContextEpochs = { source: string; policy: string }

export type ContextPurpose = "discuss" | "revise" | "continue" | "plan" | "storyQuestion" | "memoryAnalysis"

export type ConversationMessage = { id: string; content: string; scope: ScopeGrant | null }

export type ConversationTurn = { runId: string; packetId: string; sourceSnapshotId: string; policyVersion: string; user: ConversationMessage; assistant: ConversationMessage }

/**
 * A packet coverage item records what representation was actually delivered.
 */
export type CoverageEntry = { handle: string; label: string; detail: CoverageLabel }

/**
 * Coverage describes how a source may be represented in a later packet. A
 * directory entry is navigable metadata, not semantic story evidence.
 */
export type CoverageLabel = "verbatim" | "digest" | "directoryOnly"

export type DigestCandidate = { schemaVersion: string; source: SourceRef; items: DigestItem[] }

export type DigestEvidence = { blockId: string; fromUtf16: number; toUtf16: number; quote: string }

export type DigestItem = { text: string; evidence: DigestEvidence[]; uncertainty: string | null }

/**
 * Reader-disclosure metadata is separate from optional fictional story time.
 */
export type Disclosure = { readerPosition: string | null; visibleToCharacters: string[]; authorOnly: boolean; futurePrivate: boolean }

/**
 * A UTF-16 endpoint inside one block's inline content.
 */
export type Endpoint = { blockId: string; utf16Offset: number }

export type EvidenceAnchor = { blockId: string; fromUtf16: number; toUtf16: number; quote: string; quoteHash: string }

export type EvidenceAudience = "authorRoom" | "reader"

/**
 * The bounded result of looking up one project-local object identity in a
 * frozen context. An empty result means that this frozen evidence did not
 * contain an observation; it does not mean the object never existed.
 */
export type EvidenceHistory = { objectId: string; labelVariants: string[]; observations: EvidenceHistoryObservation[]; uncertainty: EvidenceHistoryUncertainty[]; incomplete: boolean }

/**
 * One exact author-reviewed observation, retained in frozen source order.
 * The holder is the recorded value and is intentionally not a current-owner
 * assertion.
 */
export type EvidenceHistoryObservation = { recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number; object: StoryEntityRef; holder: StoryEntityRef | null; timing: PossessionTiming; audience: EvidenceAudience; evidence: EvidenceAnchor }

/**
 * Conditions that keep the observation list from supporting a stronger
 * conclusion. These are status markers only; no variant asserts a current
 * holder or an inferred transfer.
 */
export type EvidenceHistoryUncertainty = "disclosureLimited" | "excludedSources" | "earlierTiming" | "unknownTiming" | "unknownHolder" | "differingHolders"

export type FrozenContext = { snapshot: StorySnapshot; policy: InformationPolicy; purpose: ContextPurpose; aliases: { [key: string]: string[] }; excludedSourceCount: number; guidance?: FrozenGuidance[]; conversation?: FrozenConversation | null; navigationViews?: FrozenNavigationView[]; reviewedEvidence?: ReviewedEvidenceSet[]; reviewedPromises?: ReviewedPromiseSet[]; reviewedKnowledge?: ReviewedKnowledgeSet[]; reviewedSummaries?: ReviewedSummarySet[]; projectChat?: FrozenProjectChat | null }

export type FrozenConversation = { projectId: string; operationNamespace: string; documentId: string; projectConversationId?: string | null; threadId: string; turns: ConversationTurn[]; omittedTurns: number }

/**
 * An exact author-room instruction selected when a request freezes. The
 * referenced version also survives in its own authoritative local store.
 */
export type FrozenGuidance = { handle: string; projectId: string; version: GuidanceVersion }

export type FrozenNavigationView = { reference: NavigationViewRef; sourceContextEpoch: string; disclosurePolicyVersion: string; dependencies: SourceRef[]; candidate: DigestCandidate }

/**
 * Frozen project-chat identity stored inside the immutable context manifest.
 * `source_refs` and `task_draft_refs` retain the exact request claims after
 * the project owner has authenticated them against SQLite.
 */
export type FrozenProjectChat = { conversationId: string; anchorDocumentId: string; operationNamespace: string; sourceRefs: Head[]; taskDraftRefs: ProjectChatDraftRef[]; promptRecipeVersion?: string | null; dispositions?: FrozenProjectChatDisposition[] }

export type FrozenProjectChatDisposition = { itemId: string; payloadHash: string; referenceId: string; producerRunId: string; key: string; itemKind: string; text: string; disposition: string; version: string; rationale: string; scope?: ChatDispositionScope; unknownTo?: ChatUnknownTo | null }

export type GuidanceScope = "request" | "document" | "project"

export type GuidanceVersion = { guidanceId: string; versionId: string; version: string; scope: GuidanceScope; documentId: string | null; text: string; textHash: string; active: boolean; originMessageId: string | null; createdAt: string }

/**
 * A policy boundary for reader and character disclosure. All positions are
 * decimal strings because they cross the JavaScript boundary.
 */
export type InformationPolicy = { version: string; audience: Audience; readerFrontier: string | null; characterId: string | null; characterGrants: CharacterGrant[]; allowAlternatives: boolean; allowHistorical: boolean }

/**
 * The author's recorded relationship between a character and a topic at an
 * exact prose passage.  This is an observation about the character's mental
 * state, never an inferred fact about the world or a permission to read the
 * surrounding chapter.
 */
export type KnowledgeAttitude = "knows" | "believes" | "suspects" | "rejects" | "unaware" | "unclear"

export type KnowledgeHistory = { characterId: string; topicId: string | null; labelVariants: string[]; observations: KnowledgeHistoryObservation[]; uncertainty: KnowledgeHistoryUncertainty[]; incomplete: boolean }

export type KnowledgeHistoryObservation = { recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number; character: StoryEntityRef; topic: StoryEntityRef; attitude: KnowledgeAttitude; statement: string; timing: PossessionTiming; audience: EvidenceAudience; evidence: EvidenceAnchor }

export type KnowledgeHistoryUncertainty = "noEligibleObservations" | "earlierOrUnknownTiming" | "multipleRecordedAttitudes" | "disclosureLimited"

export type KnowledgeRecord = { id: string; character: StoryEntityRef; topic: StoryEntityRef; attitude: KnowledgeAttitude; statement: string; timing: PossessionTiming; audience: EvidenceAudience; evidence: EvidenceAnchor }

export type LookupExchange = { request: LookupRead; result: LookupReadResult }

/**
 * The bounded lookup state carried into packet compilation. The initial
 * invocation has zero completed expansions and no exchanges; each
 * application-executed expansion adds one authenticated exchange. The
 * packet compiler owns semantic validation of request/result correspondence,
 * source identity, and byte budgets.
 */
export type LookupPacketInput = { allowance: LookupAllowance; completedInvocations: number; exchanges: LookupExchange[]; sourceProjection?: LookupSourceProjection | null; reviewedMemory?: string | null }

/**
 * One application-executed lookup request. The provider cannot supply a
 * path, command, source body, or arbitrary tool arguments.
 */
export type LookupRead = { kind: "search"; id: string; query: string; mode: SearchMode; limit: number } | { kind: "read"; id: string; handle: string; blockIds: string[] | null } | { kind: "findEntities"; id: string; entityKind: MemoryEntityKind; query: string; offset: number; limit: number } | { kind: "knowledgeHistory"; id: string; characterId: string; topicId: string | null; offset: number; limit: number } | { kind: "promiseHistory"; id: string; promiseId: string; offset: number; limit: number } | { kind: "possessionHistory"; id: string; objectId: string; offset: number; limit: number }

/**
 * The application result for one previously authorized lookup request.
 * Search results and passages retain their Rust-owned source identities; a
 * provider cannot manufacture an arbitrary path, title, or source body.
 */
export type LookupReadResult = { kind: "search"; result: SearchResult } | { kind: "read"; handle: string; source: SourceRef; passages: SourcePassage[]; complete: boolean } | { kind: "findEntities"; entityKind: MemoryEntityKind; query: string; entries: MemoryEntityEntry[]; offset: number; totalMatches: number; nextOffset: number | null; incomplete: boolean } | { kind: "knowledgeHistory"; history: KnowledgeHistory; offset: number; totalObservations: number; nextOffset: number | null } | { kind: "promiseHistory"; history: PromiseHistory; offset: number; totalObservations: number; nextOffset: number | null } | { kind: "possessionHistory"; history: EvidenceHistory; offset: number; totalObservations: number; nextOffset: number | null } | { kind: "unavailable"; code: string; detail: string }

/**
 * Exact, author-room-only labels for sources that actually appear in lookup
 * read or search evidence. This is intentionally separate from the provider
 * `story-lookup.v1` response contract.
 */
export type LookupSourceProjection = { schemaVersion: string; sources: LookupSourceProjectionSource[] }

export type LookupSourceProjectionSource = { handle: string; source: SourceRef; displayName: string }

export type MemoryEntityEntry = { entity: StoryEntityRef; labelVariants: string[]; sourceHandle: string; source: SourceRef }

export type MemoryEntityKind = "character" | "topic" | "object" | "promise"

/**
 * A model-independent total context window and the reservations that must be
 * left for output and protocol framing.  All counters are decimal strings so
 * this contract can cross the JavaScript boundary without losing precision.
 */
export type MockContextBudget = { modelId: string; contextWindowTokens: string; reservedOutputTokens: string; reservedProtocolTokens: string }

export type NavigationOmissionReason = "originalTextIncluded" | "acceptedSummaryIncluded" | "budget" | "notSmaller"

export type NavigationViewOmission = { viewId: string; reason: NavigationOmissionReason }

export type NavigationViewRef = { viewId: string; projectId: string; operationNamespace: string; contentHash: string }

/**
 * Provider-facing chat message. The evidence message is a canonical JSON
 * context envelope; the final user content is the instruction byte-for-byte.
 */
export type PacketMessage = { role: string; content: string }

/**
 * Exact options sent with the deterministic packet. Provider-specific
 * options are intentionally deferred until a qualified adapter exists.
 */
export type PacketOptions = { modelId: string; maxOutputTokens?: string; tokenAccountingMethod: string; providerBinding?: ProviderBinding | null }

/**
 * Exact durable receipt contract for a compiled packet. C2 owns packet
 * construction; C0 defines the fields that must remain auditable.
 */
export type PacketReceipt = { lookup?: LookupPacketInput | null; packetId: string; sessionId: string; snapshotId: string; invocationOrdinal: string; sourceHandles: string[]; mandatorySourceHandles?: string[]; guidanceHandles?: string[]; conversationMessageIds?: string[]; omittedDiscussionTurns?: number; safeBrief?: SafeBriefReceipt | null; coverage: CoverageEntry[]; omissions: string[]; navigationViews?: NavigationViewRef[]; navigationOmissions?: NavigationViewOmission[]; reviewedEvidence?: ReviewedEvidenceCoverage[]; reviewedEvidenceOmissions?: ReviewedEvidenceOmission[]; reviewedPromises?: ReviewedEvidenceCoverage[]; reviewedPromiseOmissions?: ReviewedEvidenceOmission[]; reviewedKnowledge?: ReviewedEvidenceCoverage[]; reviewedKnowledgeOmissions?: ReviewedEvidenceOmission[]; reviewedSummaries?: ReviewedSummaryCoverage[]; reviewedSummaryOmissions?: ReviewedSummaryOmission[]; inputHash: string; inputTokens: string; tokenAccountingMethod: string }

export type PossessionRecord = { id: string; object: StoryEntityRef; holder: StoryEntityRef | null; timing: PossessionTiming; audience: EvidenceAudience; evidence: EvidenceAnchor }

export type PossessionTiming = "atPassage" | "earlier" | "unknown"

export type PreparationResult = { status: "prepared"; packet: CompiledPacket; current: boolean } | { status: "budgetRejected"; error: BudgetError }

/**
 * Versioned provenance for a brief selected from the project conversation.
 * The project owner authenticates every field before the brief can enter a
 * restricted packet; a conversation label or copied text is not sufficient.
 */
export type ProjectBriefOrigin = { version: string; projectId: string; operationNamespace: string; conversationId: string; messageId: string; target: Head; scopeHash: string; textHash: string }

/**
 * An unadopted assistant draft may be attached only by its exact current
 * draft head and disposition version.  A draft reference is never inferred
 * from a title, a document ID supplied by the renderer, or a quoted answer.
 */
export type ProjectChatDraftRef = { head: Head; dispositionVersion: string }

export type PromiseHistory = { promiseId: string; labelVariants: string[]; observations: PromiseHistoryObservation[]; uncertainty: PromiseHistoryUncertainty[]; incomplete: boolean; hasRecordedPayoff: boolean }

export type PromiseHistoryObservation = { recordId: string; sourceHandle: string; source: SourceRef; sourceDisplayName: string; sourceOrder: number; promise: StoryEntityRef; phase: PromisePhase; timing: PossessionTiming; note: string; audience: EvidenceAudience; evidence: EvidenceAnchor }

export type PromiseHistoryUncertainty = "disclosureLimited" | "excludedSources" | "earlierTiming" | "unknownTiming" | "unclearObservation" | "conflictingOutcomes"

/**
 * Explicit author-entered promise observations. A phase is an observation at
 * the cited passage; it is never interpreted as a current truth or inferred
 * transfer/state machine.
 */
export type PromisePhase = "setup" | "payoff" | "cancelled" | "unclear"

export type PromiseRecord = { id: string; promise: StoryEntityRef; phase: PromisePhase; timing: PossessionTiming; note: string; audience: EvidenceAudience; evidence: EvidenceAnchor }

export type ReviewPrefixItem = { documentId: string; title: string; bundleId: string; revisionId: string; head: Head }

/**
 * Exact immutable author-reviewed authority selected for a reviewed
 * continuation. The source descriptors still carry the prose references;
 * this manifest preserves the selected bundle identity so a later snapshot
 * cannot be mistaken for a different reaffirmation of the same revision.
 */
export type ReviewedBasisManifest = { projectId: string; operationNamespace: string; prefix: ReviewedBasisMember[] }

export type ReviewedBasisMember = { documentId: string; bundleId: string; revisionId: string; version: string; bodyHash: string }

/**
 * Receipt coverage for the records that actually reached the provider.
 */
export type ReviewedEvidenceCoverage = { sourceHandle: string; bundleId: string; recordsHash: string; projectionHash: string; completeRecordSet: boolean; recordIds: string[] }

/**
 * Omitted records are aggregated per set and reason.  This keeps private
 * record identifiers out of RestrictedWriting packets and inspector data.
 */
export type ReviewedEvidenceOmission = { sourceHandle: string; bundleId: string; recordsHash: string; reason: ReviewedEvidenceOmissionReason; count: number }

export type ReviewedEvidenceOmissionReason = "budget" | "disclosure"

/**
 * A complete immutable record set selected from one reviewed bundle and one
 * exact saved source. The array is provenance, not a claim that every record
 * is permitted in every audience. Restricted packets project it to reader
 * records before serialization and report only aggregate disclosure.
 */
export type ReviewedEvidenceSet = { projectId: string; operationNamespace: string; bundleId: string; recordsHash: string; sourceHandle: string; source: SourceRef; records: PossessionRecord[] }

export type ReviewedHistoryResult = { snapshotId: string; current: boolean; history: EvidenceHistory }

export type ReviewedKnowledgeHistoryResult = { snapshotId: string; current: boolean; history: KnowledgeHistory }

export type ReviewedKnowledgeSet = { projectId: string; operationNamespace: string; bundleId: string; recordsHash: string; sourceHandle: string; source: SourceRef; records: KnowledgeRecord[] }

export type ReviewedPromiseHistoryResult = { snapshotId: string; current: boolean; history: PromiseHistory }

/**
 * A complete immutable promise-observation set selected from one reviewed
 * bundle and one exact saved source.  Restricted packets project this set to
 * reader-approved observations while retaining the complete hash.
 */
export type ReviewedPromiseSet = { projectId: string; operationNamespace: string; bundleId: string; recordsHash: string; sourceHandle: string; source: SourceRef; records: PromiseRecord[] }

export type ReviewedSummaryCoverage = { sourceHandle: string; bundleId: string; summaryId: string; summaryHash: string }

export type ReviewedSummaryOmission = { sourceHandle: string; reason: ReviewedSummaryOmissionReason }

export type ReviewedSummaryOmissionReason = "budget" | "disclosure" | "originalTextIncluded" | "notSmaller"

export type ReviewedSummarySet = { projectId: string; operationNamespace: string; bundleId: string; summaryHash: string; sourceHandle: string; summary: SummaryRevision }

export type SafeBriefReceipt = { text: string; textHash: string; originMessageId: string | null; projectOrigin?: ProjectBriefOrigin | null }

/**
 * A source-bound scope grant. Endpoints are required for passage and blocks;
 * whole-document grants cover the canonical document token stream.
 */
export type ScopeGrant = { kind: ScopeKind; start?: Endpoint | null; end?: Endpoint | null; sourceHash: string; quote: string; quoteHash: string; prefix?: string | null; suffix?: string | null }

/**
 * The kind of structural authority granted to a prepared replacement.
 */
export type ScopeKind = "passage" | "blocks" | "wholeDocument" | "append"

export type SearchHit = { passage: SourcePassage; startUtf16: number; endUtf16: number }

export type SearchMode = "literal" | "lexical" | "exactAlias"

export type SearchResult = { snapshotId: string; hits: SearchHit[]; sourceMatches: SourceDescriptor[]; searchedSources: number; hasMore: boolean; coverage: string }

/**
 * A source descriptor is produced by the Rust project/source resolver. The
 * eligibility kernel does not accept a client-side "safe" or "reviewed"
 * claim; it checks this descriptor against the frozen snapshot and all of its
 * exact source dependencies.
 */
export type SourceDescriptor = { handle: string; source: SourceRef; displayName: string; kind: SourceKind; current: boolean; coverage: CoverageLabel; disclosure: Disclosure; storyTime: StoryTime | null; dependencies: SourceRef[] }

/**
 * The authority/provenance class resolved by the project owner. These are
 * intentionally distinct: an observation or digest can assist retrieval but
 * cannot become reviewed story authority merely by entering a packet.
 */
export type SourceKind = "currentDraft" | "reviewedAuthority" | "explicitRule" | "adoptedGuidance" | "generatedObservation" | "generatedDigest" | "planAlternative" | "historical" | "privateFuture" | "authorRoomDiscussion" | "assistantDraft" | "conversationControl"

export type SourcePassage = { handle: string; source: SourceRef; blockId: string; blockOrder: number; text: string }

export type SourceRead = { descriptor: SourceDescriptor; passages: SourcePassage[]; body: any; usedValidatedProjection: boolean }

/**
 * Exact identity of one source revision. Display names and labels live on the
 * resolved descriptor, so Unicode source names remain lossless here.
 */
export type SourceRef = { projectId: string; documentId: string; revisionId: string; bodyHash: string }

export type StoryEntityRef = { id: string; label: string }

/**
 * Immutable source and policy basis for one context request.
 */
export type StorySnapshot = { snapshotId: string; projectId: string; basis: BasisKind; target: SourceRef; contextSourceEpoch: string; orderingEpoch: string; disclosurePolicyVersion: string; sources: SourceDescriptor[]; reviewedBasis?: ReviewedBasisManifest | null }

/**
 * Optional fictional chronology. Eligibility never uses it to override the
 * reader disclosure frontier.
 */
export type StoryTime = { label: string; position: string | null }

export type SummaryAudience = "authorRoom" | "reader"

/**
 * Immutable summary text bound to one exact staged chapter revision and its
 * exact earlier reviewed prefix.
 */
export type SummaryRevision = { id: string; text: string; audience: SummaryAudience; source: SourceRef; dependencies: ReviewPrefixItem[] }

