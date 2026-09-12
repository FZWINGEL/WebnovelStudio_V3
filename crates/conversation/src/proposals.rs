//! Explicit review and atomic author decisions over immutable source passages.
//! JavaScript prepares the full document; this module validates and stores it.
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, Head, ProjectAccess, Reply, StoredResult, check_id,
    SourceEpoch, logical_hash, new_id, parse_stored_version, parse_version, require_head,
    valid_hash, validate_snapshot_json,
};
use wns_storage::{
    checkpoint_at, existing_receipt, insert_receipt, read_document, read_revision,
};
use wns_story::host::StoryHost;
use wns_story::context_packets;
use wns_story::story_context;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_context::continuation::{
    ContinuationCandidate, validate_continuation_output, validate_continuation_paragraphs,
};
use wns_context::{Audience, ContextPurpose, author_room_structured_revision_allowed};
use wns_documents::{
    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT, ScopeGrant, ScopeKind, ScopeValidationRequest,
    TypedReplacementBlock, typed_replacement_snapshot, validate_append,
    validate_structured_replacement, validate_text_replacement, validate_typed_replacement_blocks,
};
use wns_story::run_vocabulary::DiscussionRun;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalCandidate {
    pub title: String,
    pub replacement_text: String,
    pub explanation: String,
}

/// A structured candidate replaces complete blocks selected by an explicit
/// blocks or whole-document scope. IDs are deliberately absent; the editor
/// allocates fresh identities while preparing the complete result snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredProposalCandidate {
    pub title: String,
    pub blocks: Vec<TypedReplacementBlock>,
    pub explanation: String,
}

/// The durable kind discriminator is read before parsing candidate JSON.  The
/// untagged wire representation preserves legacy passage bytes while allowing
/// continuation candidates to use their paragraph payload directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProposalKind {
    #[default]
    Passage,
    Continuation,
    Structured,
}

impl ProposalKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passage => "passage",
            Self::Continuation => "continuation",
            Self::Structured => "structured",
        }
    }

    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "passage" => Ok(Self::Passage),
            "continuation" => Ok(Self::Continuation),
            "structured" => Ok(Self::Structured),
            _ => Err(CoreError::new(
                "InvalidProposal",
                "The suggestion has an unknown durable kind.",
            )),
        }
    }

    fn is_passage(&self) -> bool {
        *self == Self::Passage
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum ProposalContent {
    Passage(ProposalCandidate),
    Continuation(ContinuationCandidate),
    Structured(StructuredProposalCandidate),
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalOutput {
    pub suggestions: Vec<ProposalCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredProposalOutput {
    pub schema_version: String,
    pub suggestions: Vec<StructuredProposalCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedProposal {
    pub id: String,
    pub proposal_id: String,
    pub version: String,
    pub replacement_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraphs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<TypedReplacementBlock>>,
    pub body: Value,
    pub body_hash: String,
}

// Moved to wns-kernel (L0) as receipt vocabulary: `StoredResult` names it, and
// `StoredResult` had to reach L0 with the receipt readers. Re-exported here so
// `proposals::AppliedDecision` resolves as before.
pub use wns_kernel::AppliedDecision;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalDecision {
    pub id: String,
    pub proposal_id: String,
    pub kind: String,
    pub prepared_id: Option<String>,
    pub before_revision_id: Option<String>,
    pub after_revision_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    pub id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "ProposalKind::is_passage")]
    pub kind: ProposalKind,
    pub candidate: ProposalContent,
    pub source: Head,
    pub source_body: Value,
    pub scope: ScopeGrant,
    pub snapshot_id: String,
    pub packet_id: String,
    pub current: bool,
    pub historical_copy: bool,
    pub prepared: Option<PreparedProposal>,
    pub decision: Option<ProposalDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareProposal {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub proposal_id: String,
    /// Zero for the first preparation. Editing creates another immutable version.
    pub expected_prepared_version: String,
    pub replacement_text: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareContinuation {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub proposal_id: String,
    /// Zero for the first preparation. Editing creates another immutable version.
    pub expected_prepared_version: String,
    pub paragraphs: Vec<String>,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareStructured {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub proposal_id: String,
    /// Zero for the first preparation. Editing creates another immutable version.
    pub expected_prepared_version: String,
    pub blocks: Vec<TypedReplacementBlock>,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyProposal {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub proposal_id: String,
    pub prepared_id: String,
    pub expected: Head,
    pub result_hash: String,
    pub local_generation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RejectProposal {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub proposal_id: String,
}

/// The operation's historical result and latest document are deliberately
/// separate. A duplicate Apply never asks the editor to replay an old change.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyAck {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub already_applied: bool,
    pub result: StoredResult,
    pub document: DocumentRecord,
}

pub enum ProposalCommand {
    List(ProjectAccess, String, Reply<Vec<Proposal>>),
    Prepare(PrepareProposal, Reply<PreparedProposal>),
    PrepareContinuation(PrepareContinuation, Reply<PreparedProposal>),
    PrepareStructured(PrepareStructured, Reply<PreparedProposal>),
    Apply(ApplyProposal, Reply<ApplyAck>),
    Reject(RejectProposal, Reply<ProposalDecision>),
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn handle_proposal(host: &mut impl StoryHost, command: ProposalCommand) {
    macro_rules! respond {
        ($reply:expr, $result:expr) => {{
            let result = $result;
            host.fence_uncertain(&result);
            let _ = $reply.send(result);
        }};
    }
    match command {
        ProposalCommand::List(access, id, reply) => respond!(
            reply,
            host.check_access(&access)
                .and_then(|()| list(host.db()?, &access, &id))
        ),
        ProposalCommand::Prepare(request, reply) => {
            respond!(reply, prepare_proposal(host, request))
        }
        ProposalCommand::PrepareContinuation(request, reply) => {
            respond!(reply, prepare_continuation(host, request))
        }
        ProposalCommand::PrepareStructured(request, reply) => {
            respond!(reply, prepare_structured(host, request))
        }
        ProposalCommand::Apply(request, reply) => respond!(reply, apply_proposal(host, request)),
        ProposalCommand::Reject(request, reply) => {
            respond!(reply, reject_proposal(host, request))
        }
    }
}

pub fn prepare_proposal(host: &mut impl StoryHost, request: PrepareProposal) -> CoreResult<PreparedProposal> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_prepared_version)?;
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(id) = receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
    )? {
        let prepared = read_prepared(&tx, &id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(prepared);
    }
    let proposal = read(&tx, &request.access, &request.proposal_id)?;
    require_owned_pending(&proposal)?;
    if proposal.kind != ProposalKind::Passage {
        return Err(CoreError::new(
            "InvalidProposal",
            "Continuation suggestions require the continuation preparation request.",
        ));
    }
    let current = proposal
        .prepared
        .as_ref()
        .map(|p| parse_version(&p.version))
        .transpose()?
        .unwrap_or(0);
    if expected != current {
        return Err(CoreError::new(
            "PreparedVersionConflict",
            "This suggestion was edited elsewhere. Load the latest prepared wording.",
        ));
    }
    // Historical preparation is useful for review. Only Apply requires
    // current prose and context; no implicit rebase takes place here.
    let validated = validate_replacement(&proposal, &request.replacement_text, &request.body)?;
    let version = current.checked_add(1).ok_or_else(|| {
        CoreError::new("VersionLimit", "The prepared version limit was reached.")
    })?;
    let id = new_id();
    tx.execute("INSERT INTO proposal_versions(id,proposal_id,version,replacement_text,body_json,body_hash,payload_json) VALUES(?,?,?,?,?,?,NULL)", params![id, proposal.id, version, request.replacement_text, validated.canonical_json, validated.hash])?;
    insert_review_receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
        &id,
    )?;
    let prepared = read_prepared(&tx, &id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(prepared)
}

pub fn prepare_continuation(
    host: &mut impl StoryHost,
    request: PrepareContinuation,
) -> CoreResult<PreparedProposal> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_prepared_version)?;
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(id) = receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
    )? {
        let prepared = read_prepared(&tx, &id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(prepared);
    }
    let proposal = read(&tx, &request.access, &request.proposal_id)?;
    require_owned_pending(&proposal)?;
    if proposal.kind != ProposalKind::Continuation {
        return Err(CoreError::new(
            "InvalidProposal",
            "Passage suggestions require the passage preparation request.",
        ));
    }
    let current = proposal
        .prepared
        .as_ref()
        .map(|p| parse_version(&p.version))
        .transpose()?
        .unwrap_or(0);
    if expected != current {
        return Err(CoreError::new(
            "PreparedVersionConflict",
            "This suggestion was edited elsewhere. Load the latest prepared wording.",
        ));
    }
    validate_continuation_paragraphs(&request.paragraphs)?;
    let validated = validate_continuation(&proposal, &request.paragraphs, &request.body)?;
    let version = current.checked_add(1).ok_or_else(|| {
        CoreError::new("VersionLimit", "The prepared version limit was reached.")
    })?;
    let id = new_id();
    let payload_json = serde_json::to_string(&request.paragraphs)?;
    tx.execute(
        "INSERT INTO proposal_versions(id,proposal_id,version,replacement_text,body_json,body_hash,payload_json) VALUES(?,?,?,?,?,?,?)",
        params![
            id,
            proposal.id,
            version,
            "",
            validated.canonical_json,
            validated.hash,
            payload_json,
        ],
    )?;
    insert_review_receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
        &id,
    )?;
    let prepared = read_prepared(&tx, &id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(prepared)
}

pub fn prepare_structured(host: &mut impl StoryHost, request: PrepareStructured) -> CoreResult<PreparedProposal> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_prepared_version)?;
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(id) = receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
    )? {
        let prepared = read_prepared(&tx, &id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(prepared);
    }
    let proposal = read(&tx, &request.access, &request.proposal_id)?;
    require_owned_pending(&proposal)?;
    if proposal.kind != ProposalKind::Structured
        || !matches!(
            proposal.scope.kind,
            ScopeKind::Blocks | ScopeKind::WholeDocument
        )
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "A structured suggestion requires an explicit block or whole-document scope.",
        ));
    }
    let current = proposal
        .prepared
        .as_ref()
        .map(|p| parse_version(&p.version))
        .transpose()?
        .unwrap_or(0);
    if expected != current {
        return Err(CoreError::new(
            "PreparedVersionConflict",
            "This suggestion was edited elsewhere. Load the latest prepared wording.",
        ));
    }
    let validated = validate_structured(&proposal, &request.blocks, &request.body)?;
    let version = current.checked_add(1).ok_or_else(|| {
        CoreError::new("VersionLimit", "The prepared version limit was reached.")
    })?;
    let id = new_id();
    let payload_json = serde_json::to_string(&request.blocks)?;
    tx.execute(
        "INSERT INTO proposal_versions(id,proposal_id,version,replacement_text,body_json,body_hash,payload_json) VALUES(?,?,?,?,?,?,?)",
        params![
            id,
            proposal.id,
            version,
            "",
            validated.canonical_json,
            validated.hash,
            payload_json,
        ],
    )?;
    insert_review_receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "prepare",
        &payload,
        &id,
    )?;
    let prepared = read_prepared(&tx, &id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(prepared)
}

pub fn apply_proposal(host: &mut impl StoryHost, request: ApplyProposal) -> CoreResult<ApplyAck> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.proposal_id)?;
    check_id(&request.prepared_id)?;
    parse_version(&request.local_generation)?;
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let prior = existing_receipt(
        &tx,
        &request.access.operation_namespace,
        &request.operation_id,
        "apply",
        &payload,
    )?;
    let already_applied = prior.is_some();
    let result = if let Some(result) = prior {
        result
    } else {
        let proposal = read(&tx, &request.access, &request.proposal_id)?;
        require_owned_pending(&proposal)?;
        let prepared = proposal.prepared.as_ref().ok_or_else(|| {
            CoreError::new(
                "ProposalNotPrepared",
                "Preview this suggestion before applying it.",
            )
        })?;
        if prepared.id != request.prepared_id || prepared.body_hash != request.result_hash {
            return Err(CoreError::new(
                "PreparedVersionConflict",
                "The reviewed suggestion changed. Preview its latest wording.",
            ));
        }
        let before = read_document(&tx, &request.expected.document_id)?;
        require_head(&before.head, &request.expected)?;
        if proposal.source != request.expected || !proposal.current {
            return Err(CoreError::new(
                "SuggestionStale",
                "The story changed after this request. Refresh the suggestion against the current text.",
            ));
        }
        let validated = validate_prepared(&proposal, prepared)?;
        if validated.hash != prepared.body_hash || validated.hash == before.head.body_hash {
            return Err(CoreError::new(
                "InvalidProposal",
                "The prepared edit must match its fingerprint and change the manuscript.",
            ));
        }
        let next = parse_version(&before.head.version)?
            .checked_add(1)
            .ok_or_else(|| {
                CoreError::new("VersionLimit", "The document version limit was reached.")
            })?;
        let before_revision = checkpoint_at(&tx, &before, "beforeApply")?;
        let changed = tx.execute("UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1 WHERE id=? AND working_version=? AND body_hash=?", params![next, validated.canonical_json, validated.hash, before.head.document_id, parse_version(&before.head.version)?, before.head.body_hash])?;
        if changed != 1 {
            return Err(CoreError::new(
                "VersionConflict",
                "The chapter changed before the suggestion was applied.",
            ));
        }
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        let after = read_document(&tx, &before.head.document_id)?;
        let after_revision = checkpoint_at(&tx, &after, "afterApply")?;
        let decision_id = new_id();
        tx.execute("INSERT INTO proposal_decisions(id,proposal_id,kind,prepared_id,before_revision_id,after_revision_id,operation_namespace,operation_id) VALUES(?,?,'apply',?,?,?,?,?)", params![decision_id, proposal.id, prepared.id, before_revision.id, after_revision.id, request.access.operation_namespace, request.operation_id])?;
        let result = StoredResult {
            head: after.head,
            saved_generation: request.local_generation.clone(),
            applied: Some(AppliedDecision {
                decision_id,
                proposal_id: proposal.id,
                prepared_id: prepared.id.clone(),
                before_revision_id: before_revision.id,
                after_revision_id: after_revision.id,
            }),
            restored: None,
        };
        insert_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "apply",
            &payload,
            &result,
        )?;
        result
    };
    let document = read_document(&tx, &request.expected.document_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    // No #[cfg(test)] here: this call is in a non-test crate now, where that
    // attribute would delete it. The gate lives on the StoryHost method.
    host.hold_context_after_commit_before_ack(&request.operation_id);
    Ok(ApplyAck {
        access: request.access,
        operation_id: request.operation_id,
        already_applied,
        result,
        document,
    })
}

pub fn reject_proposal(host: &mut impl StoryHost, request: RejectProposal) -> CoreResult<ProposalDecision> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(id) = receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "reject",
        &payload,
    )? {
        let decision = read_decision(&tx, &id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(decision);
    }
    let proposal = read(&tx, &request.access, &request.proposal_id)?;
    require_owned_pending(&proposal)?;
    let id = new_id();
    tx.execute("INSERT INTO proposal_decisions(id,proposal_id,kind,operation_namespace,operation_id) VALUES(?,?,'reject',?,?)", params![id, proposal.id, request.access.operation_namespace, request.operation_id])?;
    insert_review_receipt(
        &tx,
        &request.access,
        &request.operation_id,
        "reject",
        &payload,
        &id,
    )?;
    let decision = read_decision(&tx, &id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(decision)
}

fn validate_candidate(candidate: &ProposalCandidate) -> CoreResult<()> {
    if candidate.title.trim().is_empty()
        || candidate.title.len() > 120
        || candidate.explanation.len() > 4096
        || candidate.replacement_text.contains(['\n', '\r'])
        || candidate.replacement_text.encode_utf16().count() > 100_000
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "A passage suggestion must have a short title, bounded explanation, and a single-line replacement.",
        ));
    }
    Ok(())
}

fn validate_continuation_candidate(candidate: &ContinuationCandidate) -> CoreResult<()> {
    if candidate.title.trim().is_empty()
        || candidate.title.len() > 120
        || candidate.explanation.len() > 4096
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "A continuation suggestion must have a short title and bounded explanation.",
        ));
    }
    validate_continuation_paragraphs(&candidate.paragraphs)
}

fn validate_structured_candidate(candidate: &StructuredProposalCandidate) -> CoreResult<()> {
    if candidate.title.trim().is_empty()
        || candidate.title.len() > 120
        || candidate.explanation.len() > 4096
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "A structured suggestion must have a short title and bounded explanation.",
        ));
    }
    validate_typed_replacement_blocks(&candidate.blocks)
        .map_err(|error| CoreError::new("InvalidProposal", &error))?;
    if !candidate.blocks.is_empty() {
        let ids = (0..candidate.blocks.len())
            .map(|index| format!("candidate-{index}"))
            .collect::<Vec<_>>();
        typed_replacement_snapshot(&candidate.blocks, &ids)
            .map_err(|error| CoreError::new("InvalidProposal", &error))?;
    }
    Ok(())
}

/// Called within the terminal-result transaction. Malformed or unsupported
/// output remains retained discussion text, with no executable candidates.
pub fn retain_candidates_at(
    db: &Connection,
    run: &DiscussionRun,
    text: &str,
) -> CoreResult<()> {
    let packet = context_packets::validated_packet_record(db, &run.packet_id)?;
    let (frozen, namespace) =
        story_context::validated_snapshot_record(db, &packet.receipt.snapshot_id)?;
    let Some(scope) = packet_scope(db, &run.packet_id)? else {
        return Ok(());
    };
    let author_room_development =
        author_room_structured_revision_allowed(&frozen.snapshot, &frozen.policy, frozen.purpose);
    if frozen.policy.audience != Audience::RestrictedWriting
        && (!author_room_development
            || !matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument))
    {
        return Ok(());
    }
    let kind = match frozen.purpose {
        ContextPurpose::Revise => match scope.kind {
            ScopeKind::Passage => ProposalKind::Passage,
            ScopeKind::Blocks | ScopeKind::WholeDocument => ProposalKind::Structured,
            ScopeKind::Append => return Ok(()),
        },
        ContextPurpose::Continue => ProposalKind::Continuation,
        _ => return Ok(()),
    };
    let expected_scope = match kind {
        ProposalKind::Passage => ScopeKind::Passage,
        ProposalKind::Continuation => ScopeKind::Append,
        ProposalKind::Structured => scope.kind,
    };
    if scope.kind != expected_scope
        || namespace != run.owner.operation_namespace
        || frozen.snapshot.project_id != run.owner.project_id
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "The suggestion source does not belong to this run.",
        ));
    }
    match kind {
        ProposalKind::Passage => {
            let Ok(output) = serde_json::from_str::<ProposalOutput>(text) else {
                return Ok(());
            };
            if output.suggestions.is_empty()
                || output.suggestions.len() > 3
                || output
                    .suggestions
                    .iter()
                    .any(|candidate| validate_candidate(candidate).is_err())
            {
                return Ok(());
            }
            for (ordinal, candidate) in output.suggestions.iter().enumerate() {
                db.execute(
                    "INSERT INTO proposals(id,run_id,ordinal,candidate_json,kind) VALUES(?,?,?,?,?)",
                    params![
                        new_id(),
                        run.id,
                        ordinal as i64,
                        serde_json::to_string(candidate)?,
                        kind.as_str(),
                    ],
                )?;
            }
        }
        ProposalKind::Continuation => {
            let Ok(output) = validate_continuation_output(text) else {
                return Ok(());
            };
            let candidate = &output.suggestions[0];
            if validate_continuation_candidate(candidate).is_err() {
                return Ok(());
            }
            db.execute(
                "INSERT INTO proposals(id,run_id,ordinal,candidate_json,kind) VALUES(?,?,?,?,?)",
                params![
                    new_id(),
                    run.id,
                    0_i64,
                    serde_json::to_string(candidate)?,
                    kind.as_str(),
                ],
            )?;
        }
        ProposalKind::Structured => {
            let Ok(output) = serde_json::from_str::<StructuredProposalOutput>(text) else {
                return Ok(());
            };
            if output.schema_version != STRUCTURED_PROPOSAL_RESPONSE_CONTRACT
                || output.suggestions.is_empty()
                || output.suggestions.len() > 3
                || output
                    .suggestions
                    .iter()
                    .any(|candidate| validate_structured_candidate(candidate).is_err())
            {
                return Ok(());
            }
            for (ordinal, candidate) in output.suggestions.iter().enumerate() {
                db.execute(
                    "INSERT INTO proposals(id,run_id,ordinal,candidate_json,kind) VALUES(?,?,?,?,?)",
                    params![
                        new_id(),
                        run.id,
                        ordinal as i64,
                        serde_json::to_string(candidate)?,
                        kind.as_str(),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

fn list(db: &Connection, access: &ProjectAccess, document: &str) -> CoreResult<Vec<Proposal>> {
    check_id(document)?;
    let mut statement = db.prepare("SELECT p.id FROM proposals p JOIN discussion_runs r ON r.id=p.run_id WHERE r.target_document_id=? ORDER BY r.created_at,r.id,p.ordinal")?;
    let ids = statement
        .query_map([document], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|id| read(db, access, id)).collect()
}

fn read(db: &Connection, access: &ProjectAccess, id: &str) -> CoreResult<Proposal> {
    check_id(id)?;
    let (run_id, candidate, kind, packet_id, project_id, namespace): (
        String,
        String,
        String,
        String,
        String,
        String,
    ) = db
        .query_row(
            "SELECT p.run_id,p.candidate_json,p.kind,r.packet_id,r.project_id,r.operation_namespace \
             FROM proposals p JOIN discussion_runs r ON r.id=p.run_id WHERE p.id=?",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "ProposalNotFound",
                "This suggestion is not available in this project.",
            )
        })?;
    let kind = ProposalKind::parse(&kind)?;
    let packet = context_packets::validated_packet_record(db, &packet_id)?;
    let (frozen, stored_namespace) =
        story_context::validated_snapshot_record(db, &packet.receipt.snapshot_id)?;
    let scope = packet_scope(db, &packet_id)?
        .ok_or_else(|| CoreError::new("InvalidProposal", "The suggestion has no source scope."))?;
    let expected = match kind {
        ProposalKind::Passage => (ContextPurpose::Revise, ScopeKind::Passage),
        ProposalKind::Continuation => (ContextPurpose::Continue, ScopeKind::Append),
        ProposalKind::Structured => (ContextPurpose::Revise, scope.kind),
    };
    if frozen.purpose != expected.0
        || (frozen.policy.audience != Audience::RestrictedWriting
            && !(author_room_structured_revision_allowed(
                &frozen.snapshot,
                &frozen.policy,
                frozen.purpose,
            ) && kind == ProposalKind::Structured))
        || (kind == ProposalKind::Structured
            && !matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument))
        || scope.kind != expected.1
        || frozen.snapshot.project_id != project_id
        || namespace != stored_namespace
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "The suggestion lacks a restricted writing source.",
        ));
    }
    let source = read_revision(db, &frozen.snapshot.target.revision_id)?;
    let (ordinal, raw, status, doc, version, hash): (u32, String, String, String, i64, String) =
        db.query_row(
            "SELECT p.ordinal,r.output_text,r.status,r.target_document_id,r.target_version,r.target_body_hash \
             FROM proposals p JOIN discussion_runs r ON r.id=p.run_id WHERE p.id=?",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
    if status != "completed"
        || source.head
            != (Head {
                document_id: doc,
                version: parse_stored_version(version)?,
                body_hash: hash,
            })
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "The suggestion does not match its completed request and exact source.",
        ));
    }
    let candidate = match kind {
        ProposalKind::Passage => {
            let candidate: ProposalCandidate = serde_json::from_str(&candidate)?;
            let output: ProposalOutput = serde_json::from_str(&raw)?;
            if output.suggestions.len() > 3
                || output.suggestions.get(ordinal as usize) != Some(&candidate)
            {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "The passage suggestion does not match its retained output.",
                ));
            }
            validate_candidate(&candidate)?;
            ProposalContent::Passage(candidate)
        }
        ProposalKind::Continuation => {
            let candidate: ContinuationCandidate = serde_json::from_str(&candidate)?;
            let output = validate_continuation_output(&raw)?;
            if ordinal != 0 || output.suggestions.first() != Some(&candidate) {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "The continuation does not match its retained output.",
                ));
            }
            validate_continuation_candidate(&candidate)?;
            ProposalContent::Continuation(candidate)
        }
        ProposalKind::Structured => {
            let candidate: StructuredProposalCandidate = serde_json::from_str(&candidate)?;
            let output: StructuredProposalOutput = serde_json::from_str(&raw)?;
            if output.schema_version != STRUCTURED_PROPOSAL_RESPONSE_CONTRACT
                || output.suggestions.len() > 3
                || output.suggestions.get(ordinal as usize) != Some(&candidate)
            {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "The structured suggestion does not match its retained output.",
                ));
            }
            validate_structured_candidate(&candidate)?;
            ProposalContent::Structured(candidate)
        }
    };
    let historical_copy =
        access.project_id != project_id || access.operation_namespace != namespace;
    let (epoch, policy): (i64, i64) = db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let current = !historical_copy
        && frozen.snapshot.context_source_epoch == SourceEpoch::new(epoch.to_string())
        && frozen.policy.version == policy.to_string()
        && read_document(db, &source.head.document_id)?.head == source.head;
    let prepared_id: Option<String> = db
        .query_row(
            "SELECT id FROM proposal_versions WHERE proposal_id=? ORDER BY version DESC LIMIT 1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let decision_id: Option<String> = db
        .query_row(
            "SELECT id FROM proposal_decisions WHERE proposal_id=?",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Proposal {
        id: id.into(),
        run_id,
        kind,
        candidate,
        source: source.head,
        source_body: source.body,
        scope,
        snapshot_id: packet.receipt.snapshot_id,
        packet_id,
        current,
        historical_copy,
        prepared: prepared_id.map(|id| read_prepared(db, &id)).transpose()?,
        decision: decision_id.map(|id| read_decision(db, &id)).transpose()?,
    })
}

fn read_prepared(db: &Connection, id: &str) -> CoreResult<PreparedProposal> {
    let (proposal_id, kind, version, text, body, hash, payload): (
        String,
        String,
        i64,
        String,
        String,
        String,
        Option<String>,
    ) = db.query_row(
        "SELECT v.proposal_id,p.kind,v.version,v.replacement_text,v.body_json,v.body_hash,v.payload_json \
         FROM proposal_versions v JOIN proposals p ON p.id=v.proposal_id WHERE v.id=?",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
    )?;
    let kind = ProposalKind::parse(&kind)?;
    let valid =
        validate_snapshot_json(&body).map_err(|error| CoreError::new("InvalidProposal", &error))?;
    if valid.hash != hash || valid.canonical_json != body {
        return Err(CoreError::new(
            "InvalidProposal",
            "The prepared suggestion failed its fingerprint check.",
        ));
    }
    let (paragraphs, blocks) = match kind {
        ProposalKind::Passage => {
            if payload.is_some() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A passage preparation cannot contain continuation paragraphs.",
                ));
            }
            (None, None)
        }
        ProposalKind::Continuation => {
            if !text.is_empty() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation must not contain passage replacement text.",
                ));
            }
            let raw = payload.ok_or_else(|| {
                CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation is missing its paragraph payload.",
                )
            })?;
            let paragraphs: Vec<String> = serde_json::from_str(&raw).map_err(|_| {
                CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation has invalid paragraph JSON.",
                )
            })?;
            validate_continuation_paragraphs(&paragraphs)?;
            if serde_json::to_string(&paragraphs)? != raw {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation has noncanonical paragraph JSON.",
                ));
            }
            (Some(paragraphs), None)
        }
        ProposalKind::Structured => {
            if !text.is_empty() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A structured preparation must not contain passage replacement text.",
                ));
            }
            let raw = payload.ok_or_else(|| {
                CoreError::new(
                    "InvalidProposal",
                    "A structured preparation is missing its block payload.",
                )
            })?;
            let blocks: Vec<TypedReplacementBlock> = serde_json::from_str(&raw).map_err(|_| {
                CoreError::new(
                    "InvalidProposal",
                    "A structured preparation has invalid block JSON.",
                )
            })?;
            validate_typed_replacement_blocks(&blocks)
                .map_err(|error| CoreError::new("InvalidProposal", &error))?;
            if serde_json::to_string(&blocks)? != raw {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A structured preparation has noncanonical block JSON.",
                ));
            }
            (None, Some(blocks))
        }
    };
    Ok(PreparedProposal {
        id: id.into(),
        proposal_id,
        version: parse_stored_version(version)?,
        replacement_text: text,
        paragraphs,
        blocks,
        body: valid.snapshot,
        body_hash: hash,
    })
}

fn packet_scope(db: &Connection, packet_id: &str) -> CoreResult<Option<ScopeGrant>> {
    // Caller first validates the exact stored packet/request relationship.
    let json: String = db.query_row(
        "SELECT request_json FROM context_packets WHERE id=?",
        [packet_id],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str::<context_packets::PrepareContext>(&json)?.scope)
}

fn read_decision(db: &Connection, id: &str) -> CoreResult<ProposalDecision> {
    db.query_row("SELECT proposal_id,kind,prepared_id,before_revision_id,after_revision_id FROM proposal_decisions WHERE id=?", [id], |row| Ok(ProposalDecision {id:id.into(),proposal_id:row.get(0)?,kind:row.get(1)?,prepared_id:row.get(2)?,before_revision_id:row.get(3)?,after_revision_id:row.get(4)?})).map_err(Into::into)
}

fn require_owned_pending(proposal: &Proposal) -> CoreResult<()> {
    if proposal.historical_copy {
        return Err(CoreError::new(
            "ContextProjectMismatch",
            "A copied historical suggestion cannot authorize a new project edit.",
        ));
    }
    if proposal.decision.is_some() {
        return Err(CoreError::new(
            "SuggestionAlreadyDecided",
            "This suggestion already has an author decision.",
        ));
    }
    Ok(())
}

fn validate_replacement(
    proposal: &Proposal,
    text: &str,
    body: &Value,
) -> CoreResult<wns_kernel::SnapshotReceipt> {
    validate_text_replacement(
        &ScopeValidationRequest {
            source_snapshot: proposal.source_body.clone(),
            result_snapshot: body.clone(),
            scope: proposal.scope.clone(),
        },
        text,
    )
    .map_err(|error| CoreError::new("ScopeViolation", &error))?;
    validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidProposal", &error))
}

fn validate_continuation(
    proposal: &Proposal,
    paragraphs: &[String],
    body: &Value,
) -> CoreResult<wns_kernel::SnapshotReceipt> {
    if proposal.kind != ProposalKind::Continuation || proposal.scope.kind != ScopeKind::Append {
        return Err(CoreError::new(
            "InvalidProposal",
            "The suggestion does not carry append authority.",
        ));
    }
    validate_continuation_paragraphs(paragraphs)?;
    validate_append(
        &ScopeValidationRequest {
            source_snapshot: proposal.source_body.clone(),
            result_snapshot: body.clone(),
            scope: proposal.scope.clone(),
        },
        paragraphs,
    )
    .map_err(|error| CoreError::new("ScopeViolation", &error))?;
    validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidProposal", &error))
}

fn validate_structured(
    proposal: &Proposal,
    blocks: &[TypedReplacementBlock],
    body: &Value,
) -> CoreResult<wns_kernel::SnapshotReceipt> {
    if proposal.kind != ProposalKind::Structured
        || !matches!(
            proposal.scope.kind,
            ScopeKind::Blocks | ScopeKind::WholeDocument
        )
    {
        return Err(CoreError::new(
            "InvalidProposal",
            "The suggestion does not carry structured replacement authority.",
        ));
    }
    validate_typed_replacement_blocks(blocks)
        .map_err(|error| CoreError::new("InvalidProposal", &error))?;
    validate_structured_replacement(
        &ScopeValidationRequest {
            source_snapshot: proposal.source_body.clone(),
            result_snapshot: body.clone(),
            scope: proposal.scope.clone(),
        },
        blocks,
    )
    .map_err(|error| CoreError::new("ScopeViolation", &error))?;
    validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidProposal", &error))
}

fn validate_prepared(
    proposal: &Proposal,
    prepared: &PreparedProposal,
) -> CoreResult<wns_kernel::SnapshotReceipt> {
    match proposal.kind {
        ProposalKind::Passage => {
            if prepared.paragraphs.is_some() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A passage preparation cannot carry continuation paragraphs.",
                ));
            }
            validate_replacement(proposal, &prepared.replacement_text, &prepared.body)
        }
        ProposalKind::Continuation => {
            let paragraphs = prepared.paragraphs.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation has no paragraph payload.",
                )
            })?;
            if !prepared.replacement_text.is_empty() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A continuation preparation cannot carry passage replacement text.",
                ));
            }
            validate_continuation(proposal, paragraphs, &prepared.body)
        }
        ProposalKind::Structured => {
            let blocks = prepared.blocks.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidProposal",
                    "A structured preparation has no block payload.",
                )
            })?;
            if !prepared.replacement_text.is_empty() || prepared.paragraphs.is_some() {
                return Err(CoreError::new(
                    "InvalidProposal",
                    "A structured preparation cannot carry passage or continuation text.",
                ));
            }
            validate_structured(proposal, blocks, &prepared.body)
        }
    }
}

fn receipt(
    db: &Connection,
    access: &ProjectAccess,
    operation: &str,
    kind: &str,
    payload: &str,
) -> CoreResult<Option<String>> {
    let previous: Option<(String, String, String)> = db
        .query_row(
            "SELECT kind,payload_hash,result_id FROM proposal_receipts \
             WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((stored_kind, stored_payload, id)) = previous {
        if stored_kind != kind || stored_payload != payload {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation already refers to different suggestion wording or a different decision.",
            ));
        }
        return Ok(Some(id));
    }
    // The schema trigger is the final race-safe guard. This preflight keeps a
    // cross-domain collision in the same typed idempotency contract as a
    // duplicate proposal operation, rather than surfacing SQLite trigger text.
    let command_exists: Option<i64> = db
        .query_row(
            "SELECT 1 FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation],
            |row| row.get(0),
        )
        .optional()?;
    if command_exists.is_some() {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for a document command.",
        ));
    }
    Ok(None)
}

fn insert_review_receipt(
    db: &Connection,
    access: &ProjectAccess,
    operation: &str,
    kind: &str,
    payload: &str,
    id: &str,
) -> CoreResult<()> {
    db.execute("INSERT INTO proposal_receipts(operation_namespace,operation_id,kind,payload_hash,result_id) VALUES(?,?,?,?,?)", params![access.operation_namespace,operation,kind,payload,id])?;
    Ok(())
}

fn storage_error(detail: &str) -> CoreError {
    CoreError::new("InvalidProposal", detail)
}

fn storage_id(value: &str, label: &str) -> CoreResult<()> {
    if check_id(value).is_err() {
        return Err(storage_error(label));
    }
    Ok(())
}

fn storage_hash(value: &str, label: &str) -> CoreResult<()> {
    if !valid_hash(value) {
        return Err(storage_error(label));
    }
    Ok(())
}

fn apply_receipt(
    db: &Connection,
    namespace: &str,
    operation: &str,
) -> CoreResult<(String, String, StoredResult)> {
    storage_id(
        namespace,
        "An Apply receipt has an invalid operation namespace.",
    )?;
    storage_id(operation, "An Apply receipt has an invalid operation ID.")?;
    let row: Option<(String, String, String, String)> = db
        .query_row(
            "SELECT document_id,payload_hash,operation_kind,result_json \
             FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![namespace, operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((document, payload_hash, kind, json)) = row else {
        return Err(storage_error(
            "An applied decision is missing its command receipt.",
        ));
    };
    storage_id(
        &document,
        "An Apply receipt has an invalid document identity.",
    )?;
    storage_hash(
        &payload_hash,
        "An Apply receipt has an invalid request hash.",
    )?;
    if kind != "apply" {
        return Err(storage_error(
            "A proposal Apply decision points to a non-Apply receipt.",
        ));
    }
    let result: StoredResult = serde_json::from_str(&json)
        .map_err(|_| storage_error("An Apply receipt contains an invalid stored result."))?;
    if result.head.document_id != document
        || check_id(&result.head.document_id).is_err()
        || parse_version(&result.head.version).is_err()
        || parse_version(&result.saved_generation).is_err()
    {
        return Err(storage_error(
            "An Apply receipt contains invalid document head metadata.",
        ));
    }
    storage_hash(
        &result.head.body_hash,
        "An Apply receipt contains an invalid document body hash.",
    )?;
    if result.applied.is_none() {
        return Err(storage_error(
            "An Apply receipt is missing its applied decision metadata.",
        ));
    }
    Ok((document, payload_hash, result))
}

struct ApplyDecisionRow {
    proposal_id: String,
    kind: String,
    namespace: String,
    prepared_id: Option<String>,
    before_id: Option<String>,
    after_id: Option<String>,
}

fn validate_apply_links(
    db: &Connection,
    proposal: &Proposal,
    decision: &ProposalDecision,
    namespace: &str,
    operation: &str,
    result: &StoredResult,
) -> CoreResult<()> {
    let prepared_id = decision
        .prepared_id
        .as_deref()
        .ok_or_else(|| storage_error("The applied decision has no prepared version."))?;
    let before_id = decision
        .before_revision_id
        .as_deref()
        .ok_or_else(|| storage_error("The applied decision has no before revision."))?;
    let after_id = decision
        .after_revision_id
        .as_deref()
        .ok_or_else(|| storage_error("The applied decision has no after revision."))?;
    storage_id(
        prepared_id,
        "The applied decision has an invalid prepared ID.",
    )?;
    storage_id(
        before_id,
        "The applied decision has an invalid before revision ID.",
    )?;
    storage_id(
        after_id,
        "The applied decision has an invalid after revision ID.",
    )?;
    let prepared = read_prepared(db, prepared_id)?;
    let before = read_revision(db, before_id)?;
    let after = read_revision(db, after_id)?;
    validate_prepared(proposal, &prepared)?;
    let applied = AppliedDecision {
        decision_id: decision.id.clone(),
        proposal_id: proposal.id.clone(),
        prepared_id: prepared.id.clone(),
        before_revision_id: before.id.clone(),
        after_revision_id: after.id.clone(),
    };
    if decision.kind != "apply"
        || prepared.proposal_id != proposal.id
        || before.head != proposal.source
        || before.body != proposal.source_body
        || after.head.document_id != before.head.document_id
        || parse_version(&after.head.version)?
            != parse_version(&before.head.version)?
                .checked_add(1)
                .ok_or_else(|| storage_error("The applied version is out of range."))?
        || after.head.body_hash != prepared.body_hash
        || after.body != prepared.body
        || after.parent_id.as_deref() != Some(&before.id)
        || result.head != after.head
        || result.applied.as_ref() != Some(&applied)
    {
        return Err(storage_error(
            "The author decision, prepared edit, revisions, and receipt disagree.",
        ));
    }
    let (document, _, _) = apply_receipt(db, namespace, operation)?;
    if document != after.head.document_id {
        return Err(storage_error(
            "The Apply receipt belongs to a different document.",
        ));
    }
    Ok(())
}

fn validate_proposal_receipt(
    db: &Connection,
    namespace: &str,
    operation: &str,
    kind: &str,
    payload_hash: &str,
    result_id: &str,
) -> CoreResult<()> {
    storage_id(
        namespace,
        "A proposal receipt has an invalid operation namespace.",
    )?;
    storage_id(operation, "A proposal receipt has an invalid operation ID.")?;
    storage_hash(
        payload_hash,
        "A proposal receipt has an invalid request hash.",
    )?;
    storage_id(result_id, "A proposal receipt has an invalid result ID.")?;
    match kind {
        "prepare" => {
            let prepared = read_prepared(db, result_id)?;
            let owner_namespace: String = db.query_row(
                "SELECT r.operation_namespace \
                 FROM proposal_versions v JOIN proposals p ON p.id=v.proposal_id \
                 JOIN discussion_runs r ON r.id=p.run_id WHERE v.id=?",
                [result_id],
                |row| row.get(0),
            )?;
            if owner_namespace != namespace {
                return Err(storage_error(
                    "A prepare receipt belongs to a different operation namespace.",
                ));
            }
            if prepared.proposal_id.is_empty() {
                return Err(storage_error("A prepare receipt has no proposal target."));
            }
        }
        "reject" => {
            let decision = read_decision(db, result_id)?;
            if decision.kind != "reject" {
                return Err(storage_error(
                    "A reject receipt points to an Apply decision.",
                ));
            }
            let (decision_namespace, decision_operation): (String, String) = db.query_row(
                "SELECT operation_namespace,operation_id FROM proposal_decisions WHERE id=?",
                [result_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if decision_namespace != namespace || decision_operation != operation {
                return Err(storage_error(
                    "A reject receipt does not match its decision operation.",
                ));
            }
            let owner_namespace: String = db.query_row(
                "SELECT r.operation_namespace FROM proposals p \
                 JOIN discussion_runs r ON r.id=p.run_id WHERE p.id=?",
                [&decision.proposal_id],
                |row| row.get(0),
            )?;
            if owner_namespace != namespace {
                return Err(storage_error(
                    "A reject receipt belongs to a different operation namespace.",
                ));
            }
        }
        _ => {
            return Err(storage_error(
                "A proposal receipt has an unknown operation kind.",
            ));
        }
    }
    Ok(())
}

/// Validate retained history independently of current policy/namespace, so
/// revoked and copied results remain backupable but cannot authorize Apply.
/// Receipt payloads retain only their hash; this validates its stored shape,
/// while the original request is intentionally unavailable for recomputation.
pub fn validate_proposal_storage(db: &Connection) -> CoreResult<()> {
    let access = ProjectAccess {
        project_id: "history-inspection".into(),
        session: "history-inspection".into(),
        writer_lease: "history-inspection".into(),
        operation_namespace: "history-inspection".into(),
    };
    let mut statement = db.prepare("SELECT id FROM proposals")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        let proposal = read(db, &access, &id)?;
        let mut versions =
            db.prepare("SELECT id FROM proposal_versions WHERE proposal_id=? ORDER BY version")?;
        let version_ids = versions
            .query_map([&id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for (index, version_id) in version_ids.iter().enumerate() {
            let prepared = read_prepared(db, version_id)?;
            if prepared.version != (index + 1).to_string() {
                return Err(storage_error("Prepared versions are not contiguous."));
            }
            if prepared.proposal_id != proposal.id {
                return Err(storage_error(
                    "A prepared version belongs to a different suggestion.",
                ));
            }
            validate_prepared(&proposal, &prepared)?;
        }
        if let Some(decision) = &proposal.decision {
            storage_id(&decision.id, "A decision has an invalid identity.")?;
            let (namespace, operation): (String, String) = db.query_row(
                "SELECT operation_namespace,operation_id FROM proposal_decisions WHERE id=?",
                [&decision.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            storage_id(&namespace, "A decision has an invalid operation namespace.")?;
            storage_id(&operation, "A decision has an invalid operation ID.")?;
            let origin_namespace: String = db.query_row(
                "SELECT operation_namespace FROM discussion_runs WHERE id=?",
                [&proposal.run_id],
                |row| row.get(0),
            )?;
            if namespace != origin_namespace {
                return Err(storage_error(
                    "A decision belongs to a different project operation namespace.",
                ));
            }
            match decision.kind.as_str() {
                "apply" => {
                    let (_, _, result) = apply_receipt(db, &namespace, &operation)?;
                    validate_apply_links(db, &proposal, decision, &namespace, &operation, &result)?;
                }
                "reject" => {
                    let result_id: String = db.query_row(
                        "SELECT result_id FROM proposal_receipts \
                         WHERE operation_namespace=? AND operation_id=? AND kind='reject'",
                        params![namespace, operation],
                        |row| row.get(0),
                    )?;
                    if result_id != decision.id {
                        return Err(storage_error(
                            "The rejection receipt does not match its decision.",
                        ));
                    }
                }
                _ => return Err(storage_error("A decision has an unknown operation kind.")),
            }
        }
    }

    let mut receipts = db.prepare(
        "SELECT operation_namespace,operation_id,kind,payload_hash,result_id \
         FROM proposal_receipts",
    )?;
    let receipt_rows = receipts
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (namespace, operation, kind, payload_hash, result_id) in receipt_rows {
        validate_proposal_receipt(db, &namespace, &operation, &kind, &payload_hash, &result_id)?;
    }

    // Every Apply command receipt must reverse-resolve to one retained
    // proposal decision. This catches an orphaned or retargeted command row,
    // including rows synthetically introduced into a backup database.
    let mut command_receipts = db.prepare(
        "SELECT operation_namespace,operation_id FROM command_receipts \
         WHERE operation_kind='apply'",
    )?;
    let apply_rows = command_receipts
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (namespace, operation) in apply_rows {
        let (_, _, result) = apply_receipt(db, &namespace, &operation)?;
        let applied = result
            .applied
            .as_ref()
            .ok_or_else(|| storage_error("An Apply receipt has no decision link."))?;
        let decision: Option<ApplyDecisionRow> = db
            .query_row(
                "SELECT proposal_id,kind,operation_namespace,prepared_id,before_revision_id,after_revision_id \
                 FROM proposal_decisions WHERE id=?",
                [&applied.decision_id],
                |row| {
                    Ok(ApplyDecisionRow {
                        proposal_id: row.get(0)?,
                        kind: row.get(1)?,
                        namespace: row.get(2)?,
                        prepared_id: row.get(3)?,
                        before_id: row.get(4)?,
                        after_id: row.get(5)?,
                    })
                },
            )
            .optional()?;
        let Some(decision) = decision else {
            return Err(storage_error(
                "An Apply receipt points to a missing author decision.",
            ));
        };
        if decision.kind != "apply"
            || decision.namespace != namespace
            || decision.prepared_id.as_deref() != Some(applied.prepared_id.as_str())
            || decision.before_id.as_deref() != Some(applied.before_revision_id.as_str())
            || decision.after_id.as_deref() != Some(applied.after_revision_id.as_str())
        {
            return Err(storage_error(
                "An Apply receipt does not match its author decision.",
            ));
        }
        let proposal = read(db, &access, &decision.proposal_id)?;
        let decision_record = read_decision(db, &applied.decision_id)?;
        validate_apply_links(
            db,
            &proposal,
            &decision_record,
            &namespace,
            &operation,
            &result,
        )?;
        let decision_operation: String = db.query_row(
            "SELECT operation_id FROM proposal_decisions WHERE id=?",
            [&applied.decision_id],
            |row| row.get(0),
        )?;
        if decision_operation != operation {
            return Err(storage_error(
                "An Apply receipt does not match its decision operation.",
            ));
        }
    }
    Ok(())
}
