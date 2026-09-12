//! Durable Story Workshop state and reviewed adoption boundary.
//!
//! The workshop is deliberately a small author-room projection.  Documents
//! and their immutable revisions remain the only story authority.  Workshop
//! rows retain exploration state, exact previews, and receipts so a lost IPC
//! acknowledgment can be reconciled without replaying a mutation.
pub use wns_story::workshop_metadata::{
    MAX_STORY_POSSIBILITIES, MAX_STORY_POSSIBILITY_TEXT_CHARS, validate_story_possibilities,
    validate_text,
};
pub use wns_story::workshop_vocabulary::{
    CandidateChoice, CandidateChoiceStatus, Lens, PreferencePolarity, PreferenceScope,
    PreferenceStrength, SelectedDetail, StoryPossibility, StoryPossibilityKind,
    StoryPossibilityStatus, UnknownTo, WorkshopBranchKind, WorkshopDecision,
    WorkshopDecisionStatus, WorkshopDepth, WorkshopImpact, WorkshopImpactKind,
    WorkshopImpactStatus, WorkshopPreference, WorkshopPreset, WorkshopQuestion,
    WorkshopQuestionStatus, WorkshopRelationship, WorkshopRelationshipStatus, WorkshopSession,
    WorkshopSnapshotOrigin, WorkshopState,
};
// Shared run vocabulary lives below conversation and Workshop; readers cross
// the host seam so these sibling crates remain independent.
use crate::host::WorkshopHost;
use wns_story::run_vocabulary::{
    CompletedDiscussionOutput, DiscussionRun, DiscussionRunStatus, DiscussionStart,
};
// Shared state vocabulary is re-exported at Workshop's existing public path.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, Head, ProjectAccess, check_id, logical_hash, new_id,
    parse_stored_version, parse_version, sha256_hex, valid_hash, validate_snapshot_json,
    validate_title,
};
use wns_storage::{read_document, read_revision};
pub use wns_story::workshop_state::*;

type WorkshopCandidateRecord = (String, String, Option<WorkshopRelationship>);
type WorkshopAdoptionPreviewRecord = (String, String, String, i64, String, String, String, String);
type WorkshopCandidateOutput = (String, WorkshopCandidate, Option<WorkshopRelationship>);

/// Carries the caller's active database view through candidate interpretation.
/// The stateless conversation reader executes only when the helper reaches the
/// original query boundary, including inside save/adoption transactions.
#[derive(Clone, Copy)]
struct CandidateReadContext<'a> {
    connection: &'a Connection,
    completed_outputs: fn(&Connection) -> CoreResult<Vec<CompletedDiscussionOutput>>,
}

impl<'a> CandidateReadContext<'a> {
    fn new<H: WorkshopHost>(connection: &'a Connection) -> Self {
        Self {
            connection,
            completed_outputs: H::completed_outputs_at,
        }
    }
}

fn storage_valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// A relationship proposed as part of an atomic adoption.  This remains a
/// request shape until the adoption commits the exact endpoint heads into a
/// `WorkshopRelationship` record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopRelationshipDraft {
    pub id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    #[serde(rename = "type")]
    pub relationship_type: String,
    pub description: String,
    pub uncertainty: String,
    pub from_expected: Option<Head>,
    pub to_expected: Option<Head>,
}

/// An author classification supplied with an adoption preview.  It is tied
/// to a candidate affected target and becomes an immutable impact provenance
/// record when the adoption commits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopImpactDraft {
    pub document_id: String,
    pub kind: WorkshopImpactKind,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionImpact {
    pub candidate_id: String,
    pub document_id: String,
    pub kind: WorkshopImpactKind,
    pub reason: String,
    pub status: WorkshopImpactStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopSnapshot {
    pub version: String,
    pub state: WorkshopState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidateImplication {
    pub text: String,
    pub basis: String,
    pub assumption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidateAffectedTarget {
    pub document_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidate {
    pub id: String,
    pub title: String,
    pub content: String,
    pub dimension_value: String,
    pub implications: Vec<WorkshopCandidateImplication>,
    pub assumptions: Vec<String>,
    pub affected_targets: Vec<WorkshopCandidateAffectedTarget>,
    pub preserved_details: Vec<String>,
    pub changed_details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopOutputInterpretation {
    pub you_said: String,
    pub possible_direction: String,
    pub still_open: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopOutput {
    pub schema_version: String,
    pub request_kind: String,
    pub question: String,
    pub question_reason: String,
    pub dimension: String,
    pub interpretation: WorkshopOutputInterpretation,
    pub candidates: Vec<WorkshopCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopResult {
    pub run: DiscussionRun,
    pub session_id: String,
    pub working_generation: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_selection: Option<crate::workshop_generation::WorkshopWorkingSelection>,
    pub output: Option<WorkshopOutput>,
    pub validation_error: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopView {
    pub version: String,
    pub state: WorkshopState,
    pub results: Vec<WorkshopResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveWorkshop {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected_version: String,
    pub state: WorkshopState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionTarget {
    pub document_id: String,
    pub expected: Option<Head>,
    pub title: String,
    pub kind: String,
    pub body: Value,
    pub mode: AdoptionMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AdoptionMode {
    Add,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewWorkshopAdoption {
    pub access: ProjectAccess,
    pub session_id: String,
    pub expected_version: String,
    pub candidate_ids: Vec<String>,
    pub targets: Vec<WorkshopAdoptionTarget>,
    pub rationale: String,
    pub protected_text: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<WorkshopRelationshipDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub impact_drafts: Vec<WorkshopImpactDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionPreview {
    pub id: String,
    pub session_id: String,
    pub expected_version: String,
    pub targets: Vec<WorkshopAdoptionTarget>,
    pub before: Vec<DocumentRecord>,
    pub rationale: String,
    pub protected_text: Vec<String>,
    pub candidate_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<WorkshopRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_sources: Vec<DocumentRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub impacts: Vec<WorkshopAdoptionImpact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionAck {
    pub snapshot: WorkshopSnapshot,
    pub documents: Vec<DocumentRecord>,
    pub decision_ids: Vec<String>,
}

fn operation_payload<T: Serialize>(value: &T) -> CoreResult<String> {
    logical_hash(value)
}

fn existing_workshop_receipt(
    connection: &Connection,
    namespace: &str,
    operation_id: &str,
    kind: &str,
    payload_hash: &str,
) -> CoreResult<Option<Value>> {
    if let Some((stored_kind, stored_hash, result)) = connection
        .query_row(
            "SELECT operation_kind,payload_hash,result_json FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
            params![namespace, operation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?
    {
        if stored_kind != kind || stored_hash != payload_hash {
            return Err(CoreError::new("OperationIdReusedWithDifferentPayload", "This operation ID was already used for another workshop request."));
        }
        return Ok(Some(serde_json::from_str(&result)?));
    }
    for table in ["command_receipts", "proposal_receipts"] {
        let kind_column = if table == "command_receipts" {
            "operation_kind"
        } else {
            "kind"
        };
        let found: Option<(String, String)> = connection
            .query_row(
                &format!("SELECT {kind_column},payload_hash FROM {table} WHERE operation_namespace=? AND operation_id=?"),
                params![namespace, operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if found.is_some() {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID belongs to another command family.",
            ));
        }
    }
    Ok(None)
}

fn read_workshop_results(
    host: &impl WorkshopHost,
    state: &WorkshopState,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
) -> CoreResult<Vec<WorkshopResult>> {
    let connection = host.db()?;
    let run_ids = host.run_ids()?;
    let mut results = Vec::new();
    for run_id in run_ids {
        let run = host.read_run(&run_id)?;
        if run.intent != wns_story::discussion_vocabulary::FeedbackIntent::WorkshopExplore {
            continue;
        }
        let packet =
            wns_story::context_packets::validated_packet_record(connection, &run.packet_id)?;
        let instruction = packet
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A workshop discussion packet has no user instruction.",
                )
            })?;
        let metadata = crate::workshop_generation::metadata_from_instruction(instruction)?;
        let (frozen, namespace) = wns_story::story_context::validated_snapshot_record(
            connection,
            &packet.receipt.snapshot_id,
        )?;
        let historical_identity =
            frozen.snapshot.project_id != project_id || namespace != operation_namespace;
        if historical_identity {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0)",
                [&run.target.document_id],
                |row| row.get(0),
            )?;
            if !exists {
                continue;
            }
        }
        let current_session = state
            .sessions
            .iter()
            .find(|session| session.id == metadata.exploration.session_id);
        let relationship_stale = current_session.is_none_or(|session| {
            !candidate_relationship_matches_session(
                state,
                &session.id,
                metadata.relationship.as_ref(),
            )
        });
        let stale = historical_identity
            || frozen.snapshot.context_source_epoch != source_epoch
            || current_session.is_none_or(|session| {
                session.working_generation != metadata.exploration.working_generation
            })
            || relationship_stale;
        let (output, validation_error) =
            if run.status == DiscussionRunStatus::Completed && run.dispatch_state == "delivered" {
                match crate::workshop_generation::validate_workshop_output(
                    &run.output_text,
                    &metadata,
                    &run.id,
                ) {
                    Ok(output) => (Some(output), None),
                    Err(error) => (None, Some(error.detail)),
                }
            } else {
                (None, None)
            };
        results.push(WorkshopResult {
            run,
            session_id: metadata.exploration.session_id,
            working_generation: metadata.exploration.working_generation,
            action: metadata.exploration.action,
            working_selection: metadata.exploration.working_selection,
            output,
            validation_error,
            stale,
        });
    }
    Ok(results)
}

fn resolve_workshop_relationship(
    connection: &Connection,
    state: &WorkshopState,
    session: &WorkshopSession,
) -> CoreResult<Option<WorkshopRelationship>> {
    let Some(relationship_id) = session.relationship_id.as_deref() else {
        return Ok(None);
    };
    check_id(relationship_id)?;
    let relationship = state
        .relationships
        .iter()
        .find(|relationship| relationship.id == relationship_id)
        .cloned()
        .ok_or_else(|| {
            CoreError::new(
                "InvalidWorkshopRelationship",
                "The relationship exploration target does not exist.",
            )
        })?;
    if relationship.status == WorkshopRelationshipStatus::Archived {
        return Err(CoreError::new(
            "InvalidWorkshopRelationship",
            "Archived relationships cannot be explored.",
        ));
    }
    if relationship.from_document_id == relationship.to_document_id
        || relationship.source_heads.len() != 2
    {
        return Err(CoreError::new(
            "InvalidWorkshopRelationship",
            "The relationship endpoints are incomplete.",
        ));
    }
    let mut source_heads = HashMap::new();
    for head in &relationship.source_heads {
        if head.document_id != relationship.from_document_id
            && head.document_id != relationship.to_document_id
        {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship source does not match its endpoint.",
            ));
        }
        if source_heads
            .insert(head.document_id.clone(), head.clone())
            .is_some()
        {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship contains duplicate endpoint sources.",
            ));
        }
    }
    if source_heads.len() != 2 {
        return Err(CoreError::new(
            "StaleRelationship",
            "A relationship is missing an exact endpoint source.",
        ));
    }
    for document_id in [&relationship.from_document_id, &relationship.to_document_id] {
        let document = read_document(connection, document_id).map_err(|error| {
            CoreError::new(
                "StaleRelationship",
                &format!("A relationship endpoint is unavailable: {}", error.detail),
            )
        })?;
        if !matches!(document.kind.as_str(), "character" | "world") {
            return Err(CoreError::new(
                "InvalidWorkshopRelationship",
                "Relationship exploration requires character or world endpoints.",
            ));
        }
        if source_heads.get(document_id.as_str()) != Some(&document.head) {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship endpoint changed; review the relationship before exploring it.",
            ));
        }
    }
    Ok(Some(relationship))
}

// Actor-side logic, as free functions over `WorkshopHost`.

/// Start is kept on the project actor so the workshop CAS, first-anchor
/// creation, frozen packet, and discussion run all use one serialized
/// project boundary. A durable workshop receipt is checked before reading
/// the current workshop version: retrying a lost acknowledgment must
/// replay the original run even when the editor has since advanced.
pub fn start_workshop<H: WorkshopHost>(
    host: &mut H,
    request: crate::workshop_generation::StartWorkshop,
) -> CoreResult<DiscussionStart> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let payload_hash = operation_payload(&request)?;
    if let Some(value) = existing_workshop_receipt(
        host.db()?,
        &request.access.operation_namespace,
        &request.operation_id,
        "startWorkshop",
        &payload_hash,
    )? {
        let started = serde_json::from_value(value).map_err(|error| {
            CoreError::new(
                "InvalidProject",
                &format!("The workshop start receipt is invalid: {error}"),
            )
        })?;
        return Ok(started);
    }

    // A process loss can occur after the discussion transaction commits but
    // before the workshop receipt is written. In that narrow recovery
    // window, the immutable discussion row is still the authoritative
    // operation result. Match the frozen Workshop envelope and binding
    // before replaying it; a different request remains an operation-ID
    // collision.
    if let Some(run_id) = existing_workshop_run(host, &request.access, &request.operation_id)? {
        let started = host.read_start(&run_id)?;
        let metadata = started
            .packet
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| crate::workshop_generation::metadata_from_instruction(&message.content))
            .transpose()?;
        let prepared_budget_json: String = host.db()?.query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&started.run.packet_id],
            |row| row.get(0),
        )?;
        let prepared: wns_story::context_packets::PrepareContext =
            serde_json::from_str(&prepared_budget_json)?;
        let budget_matches = prepared.budget.model_id == request.budget.model_id
            && prepared.budget.context_window_tokens == request.budget.context_window_tokens
            && prepared.budget.reserved_output_tokens == request.budget.reserved_output_tokens
            && prepared.budget.reserved_protocol_tokens == request.budget.reserved_protocol_tokens;
        let access_matches = prepared.access.project_id == request.access.project_id
            && prepared.access.operation_namespace == request.access.operation_namespace
            && prepared.operation_id == request.operation_id;
        let matches = started.run.intent
            == wns_story::discussion_vocabulary::FeedbackIntent::WorkshopExplore
            && metadata.is_some_and(|metadata| metadata.exploration == request.exploration)
            && started.packet.options.provider_binding == request.provider_binding
            && budget_matches
            && access_matches;
        if !matches {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for a different workshop request.",
            ));
        }
        return Ok(started);
    }

    let (version, state) = read_state(host.db()?)?;
    let expected_version = parse_version(&request.exploration.expected_version)?;
    if version != expected_version {
        return Err(CoreError::new(
            "VersionConflict",
            "The workshop changed; reload it before generating.",
        ));
    }
    let session = state
        .sessions
        .iter()
        .find(|session| session.id == request.exploration.session_id)
        .cloned()
        .ok_or_else(|| CoreError::new("InvalidWorkshop", "The workshop session does not exist."))?;
    if session.working_generation != request.exploration.working_generation {
        return Err(CoreError::new(
            "InvalidWorkshop",
            "The workshop working version changed before generation.",
        ));
    }
    let relationship = resolve_workshop_relationship(host.db()?, &state, &session)?;
    let anchor = ensure_workshop_anchor(host, &session)?;
    let source_epoch = current_context_epoch(host.db()?)?;
    let candidates = workshop_candidate_records(
        CandidateReadContext::new::<H>(host.db()?),
        &host.info().project_id,
        &request.access.operation_namespace,
        Some(&source_epoch),
    )?;
    let mut included_alternatives = Vec::new();
    for choice in session
        .choices
        .iter()
        .filter(|choice| choice.status == CandidateChoiceStatus::Saved && choice.include_in_context)
    {
        let Some((candidate_session_id, content, candidate_relationship)) =
            candidates.get(&choice.candidate_id)
        else {
            return Err(CoreError::new(
                "InvalidWorkshopCandidate",
                "A selected workshop candidate is stale; generate a fresh result before continuing.",
            ));
        };
        if !session_is_ancestor(&state, &session.id, candidate_session_id) {
            return Err(CoreError::new(
                "InvalidWorkshopCandidate",
                "A selected workshop candidate belongs to another exploration branch.",
            ));
        }
        if !candidate_relationship_matches_session(
            &state,
            &session.id,
            candidate_relationship.as_ref(),
        ) {
            return Err(CoreError::new(
                "InvalidWorkshopCandidate",
                "A selected workshop candidate belongs to a different relationship scope.",
            ));
        }
        included_alternatives.push(content.clone());
    }
    let mut chosen_details = Vec::new();
    let mut fixed_details = Vec::new();
    for decision in &state.decisions {
        let in_lineage = decision.status == WorkshopDecisionStatus::Chosen
            && session_is_ancestor(&state, &session.id, &decision.session_id);
        let relevant_fixed = decision.fixed
            && fixed_decision_is_relevant(&state, &session, decision, relationship.as_ref());
        let needs_full_protected_body = relevant_fixed && decision.protected_text.is_empty();
        if !in_lineage && !needs_full_protected_body {
            continue;
        }
        // Resolve the exact saved revision only for chosen lineage
        // material or a relevant fixed decision whose protection is a
        // whole source body. Unrelated fixed decisions stay out of the
        // packet entirely.
        let revision = read_revision(host.db()?, &decision.revision_id)?;
        let body = body_text(&revision.body);
        if in_lineage {
            chosen_details.push(format!(
                "{}: {}\nAuthor rationale: {}",
                decision.title, body, decision.rationale
            ));
        }
        if needs_full_protected_body {
            fixed_details.push(body);
        }
    }
    let generation = crate::workshop_generation::from_session_with_material(
        request.clone(),
        &session,
        &state,
        anchor.head,
        crate::workshop_generation::WorkshopResolvedMaterial {
            chosen_details,
            included_alternatives,
            fixed_details,
            relationship,
        },
    )?;
    let (discussion, _metadata) = generation.into_discussion()?;
    let started = host.start_discussion(discussion)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
        params![
            request.access.operation_namespace,
            request.operation_id,
            "startWorkshop",
            payload_hash,
            serde_json::to_string(&started)?
        ],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(started)
}

pub fn existing_workshop_run(
    host: &impl WorkshopHost,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<Option<String>> {
    host.run_id_for_operation(
        &access.project_id,
        &access.operation_namespace,
        operation_id,
    )
}

pub fn ensure_workshop_anchor(
    host: &mut impl WorkshopHost,
    session: &WorkshopSession,
) -> CoreResult<DocumentRecord> {
    let document_id = session.anchor_document_id.as_deref().ok_or_else(|| {
        CoreError::new(
            "InvalidWorkshop",
            "The workshop session has no anchor document for generation.",
        )
    })?;
    match read_document(host.db()?, document_id) {
        Ok(document) => return Ok(document),
        Err(error) if error.code == "DocumentNotFound" && document_id.starts_with("workshop-") => {}
        Err(error) => return Err(error),
    }
    let body = json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{"type":"paragraph","attrs":{"id":document_id},"content":[]}]
        }
    });
    let (canonical, hash) = canonical_body(&body)?;
    let title = if session.working_title.trim().is_empty() {
        if session.title.trim().is_empty() {
            "Workshop anchor"
        } else {
            &session.title
        }
    } else {
        &session.working_title
    };
    validate_title(title)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let inserted = tx.execute(
        "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,?,?,(SELECT COALESCE(MAX(position),-1)+1 FROM documents WHERE role='ordinary'),0,1,?,?) ON CONFLICT(id) DO NOTHING",
        params![document_id, "note", title, serde_json::to_string(&canonical)?, hash],
    )?;
    if inserted == 1 {
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
    }
    tx.commit().map_err(CoreError::uncertain)?;
    read_document(host.db()?, document_id)
}

pub fn read_workshop(host: &impl WorkshopHost, access: ProjectAccess) -> CoreResult<WorkshopView> {
    host.check_access(&access)?;
    let (version, state) = read_state(host.db()?)?;
    validate_session_relationship_references(&state)?;
    let source_epoch = current_context_epoch(host.db()?)?;
    Ok(WorkshopView {
        version: parse_stored_version(version)?,
        results: read_workshop_results(
            host,
            &state,
            &host.info().project_id,
            &access.operation_namespace,
            &source_epoch,
        )?,
        state,
    })
}

pub fn save_workshop<H: WorkshopHost>(
    host: &mut H,
    request: SaveWorkshop,
) -> CoreResult<WorkshopSnapshot> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_version)?;
    let payload_hash = operation_payload(&request)?;
    let current_project_id = host.info().project_id.clone();
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(result) = existing_workshop_receipt(
        &tx,
        &request.access.operation_namespace,
        &request.operation_id,
        "saveWorkshop",
        &payload_hash,
    )? {
        let snapshot: WorkshopSnapshot = serde_json::from_value(result)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(snapshot);
    }
    let (current_version, current_state) = read_state(&tx)?;
    validate_state_references(&tx, &request.state, Some(&current_state), &HashSet::new())?;
    validate_relationship_freshness(&tx, &request.state, Some(&current_state))?;
    let source_epoch = current_context_epoch(&tx)?;
    validate_candidate_provenance(
        CandidateReadContext::new::<H>(&tx),
        &request.state,
        &current_project_id,
        &request.access.operation_namespace,
        &source_epoch,
        None,
        &[],
    )?;
    if current_version != expected {
        return Err(CoreError::new(
            "VersionConflict",
            "The workshop changed; reload it before saving.",
        ));
    }
    let changed = request.state != current_state;
    let version = if changed {
        expected.checked_add(1).ok_or_else(|| {
            CoreError::new("VersionLimit", "The workshop version limit was reached.")
        })?
    } else {
        expected
    };
    if changed {
        store_state(&tx, version, &request.state)?;
        insert_snapshot(
            &tx,
            &current_project_id,
            &request.access.operation_namespace,
            &request.operation_id,
            version,
            &payload_hash,
            &request.state,
        )?;
    }
    let snapshot = workshop_snapshot(version, request.state.clone())?;
    tx.execute(
        "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
        params![request.access.operation_namespace, request.operation_id, "saveWorkshop", payload_hash, serde_json::to_string(&snapshot)?],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(snapshot)
}

pub fn workshop_history(
    host: &impl WorkshopHost,
    access: ProjectAccess,
) -> CoreResult<Vec<WorkshopSnapshot>> {
    host.check_access(&access)?;
    let db = host.db()?;
    let mut statement = db.prepare(
        "SELECT project_id,operation_namespace,operation_id,version,payload_hash,state_json,state_hash FROM workshop_snapshots ORDER BY version DESC,id DESC",
    )?;
    let mut output = Vec::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })? {
        let (project_id, namespace, operation, version, payload, state, hash) = row?;
        let parsed = validate_snapshot_authority(
            db,
            &|_c, o, s, p| host.validate_chat_workshop_snapshot(o, s, p),
            WorkshopSnapshotOrigin {
                project_id: &project_id,
                namespace: &namespace,
                operation: &operation,
                version,
                payload_hash: &payload,
            },
            &state,
            &hash,
        )?;
        output.push(workshop_snapshot(version, parsed)?);
    }
    Ok(output)
}

pub fn preview_workshop_adoption<H: WorkshopHost>(
    host: &mut H,
    request: PreviewWorkshopAdoption,
) -> CoreResult<WorkshopAdoptionPreview> {
    host.check_access(&request.access)?;
    check_id(&request.session_id)?;
    let expected = parse_version(&request.expected_version)?;
    validate_id_list(&request.candidate_ids, "adoption candidates")?;
    validate_text(&request.rationale, "adoption rationale", MAX_TEXT_BYTES)?;
    for text in &request.protected_text {
        validate_text(text, "protected text", MAX_DETAIL_BYTES)?;
    }
    let (version, state) = read_state(host.db()?)?;
    if version != expected {
        return Err(CoreError::new(
            "VersionConflict",
            "The workshop changed; reload before previewing adoption.",
        ));
    }
    if !state
        .sessions
        .iter()
        .any(|session| session.id == request.session_id)
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The adoption session does not exist.",
        ));
    }
    let mut targets = request.targets.clone();
    let before = validate_adoption_targets(host.db()?, &state, &request.session_id, &mut targets)?;
    let target_ids = targets
        .iter()
        .map(|target| target.document_id.clone())
        .collect::<HashSet<_>>();
    validate_state_references(host.db()?, &state, None, &target_ids)?;
    let source_epoch = current_context_epoch(host.db()?)?;
    validate_candidate_provenance(
        CandidateReadContext::new::<H>(host.db()?),
        &state,
        &host.info().project_id,
        &request.access.operation_namespace,
        &source_epoch,
        Some(&request.session_id),
        &request.candidate_ids,
    )?;
    let relationships =
        validate_relationship_drafts(host.db()?, &state, &targets, &request.relationships)?;
    let impacts = build_adoption_impacts(
        CandidateReadContext::new::<H>(host.db()?),
        &host.info().project_id,
        &request.access.operation_namespace,
        &source_epoch,
        &request.candidate_ids,
        &targets,
        &request.impact_drafts,
    )?;
    let endpoint_sources = relationship_endpoint_sources(host.db()?, &request.relationships)?;
    validate_protected_texts_for_targets(
        host.db()?,
        &state,
        &request.session_id,
        &before,
        &targets,
        &request.protected_text,
    )?;
    validate_target_dependencies(&state, &targets, host.db()?)?;
    let request_json = serde_json::to_string(&wns_kernel::canonicalize_value(
        serde_json::to_value(&request)?,
    ))?;
    let preview = WorkshopAdoptionPreview {
        id: new_id(),
        session_id: request.session_id,
        expected_version: request.expected_version,
        targets,
        before,
        rationale: request.rationale,
        protected_text: request.protected_text,
        candidate_ids: request.candidate_ids.clone(),
        relationships,
        endpoint_sources,
        impacts,
    };
    let preview_json = serde_json::to_string(&wns_kernel::canonicalize_value(
        serde_json::to_value(&preview)?,
    ))?;
    let preview_hash = sha256_hex(preview_json.as_bytes());
    let current_project_id = host.info().project_id.clone();
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO workshop_adoption_previews(id,project_id,operation_namespace,session_id,expected_version,payload_hash,request_json,preview_json,preview_hash) VALUES(?,?,?,?,?,?,?,?,?)",
        params![preview.id, current_project_id, request.access.operation_namespace, preview.session_id, expected, sha256_hex(request_json.as_bytes()), request_json, preview_json, preview_hash],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(preview)
}

pub fn adopt_workshop<H: WorkshopHost>(
    host: &mut H,
    access: ProjectAccess,
    operation_id: String,
    preview_id: String,
) -> CoreResult<WorkshopAdoptionAck> {
    host.check_access(&access)?;
    check_id(&operation_id)?;
    check_id(&preview_id)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AdoptRequest<'a> {
        access: &'a ProjectAccess,
        operation_id: &'a str,
        preview_id: &'a str,
    }
    let payload_hash = operation_payload(&AdoptRequest {
        access: &access,
        operation_id: &operation_id,
        preview_id: &preview_id,
    })?;
    let current_project_id = host.info().project_id.clone();
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(result) = existing_workshop_receipt(
        &tx,
        &access.operation_namespace,
        &operation_id,
        "adoptWorkshop",
        &payload_hash,
    )? {
        let ack: WorkshopAdoptionAck = serde_json::from_value(result)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(ack);
    }
    let preview_row: Option<WorkshopAdoptionPreviewRecord> = tx
        .query_row(
            "SELECT project_id,operation_namespace,session_id,expected_version,payload_hash,request_json,preview_json,preview_hash FROM workshop_adoption_previews WHERE id=?",
            [preview_id.as_str()],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?)),
        )
        .optional()?;
    let (
        preview_project_id,
        namespace,
        session_id,
        expected,
        request_payload_hash,
        request_json,
        preview_json,
        preview_hash,
    ) = preview_row.ok_or_else(|| {
        CoreError::new("PreviewNotFound", "The adoption preview no longer exists.")
    })?;
    if preview_project_id != current_project_id || namespace != access.operation_namespace {
        return Err(CoreError::new(
            "WrongProjectSession",
            "This adoption preview belongs to another project identity.",
        ));
    }
    let preview: WorkshopAdoptionPreview = serde_json::from_str(&preview_json)?;
    let canonical_preview = serde_json::to_string(&wns_kernel::canonicalize_value(
        serde_json::to_value(&preview)?,
    ))?;
    if preview.id != preview_id || sha256_hex(canonical_preview.as_bytes()) != preview_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview failed its fingerprint check.",
        ));
    }
    let stored_request: PreviewWorkshopAdoption = serde_json::from_str(&request_json)?;
    let canonical_request = serde_json::to_string(&wns_kernel::canonicalize_value(
        serde_json::to_value(&stored_request)?,
    ))?;
    if sha256_hex(canonical_request.as_bytes()) != request_payload_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview request failed its fingerprint check.",
        ));
    }
    if stored_request.session_id != session_id
        || stored_request.expected_version != expected.to_string()
        || stored_request.access.project_id != preview_project_id
        || stored_request.access.operation_namespace != namespace
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview provenance is invalid.",
        ));
    }
    let (current_version, mut state) = read_state(&tx)?;
    if current_version != expected {
        return Err(CoreError::new(
            "VersionConflict",
            "The workshop changed after this preview was created.",
        ));
    }
    if preview.expected_version != expected.to_string() || preview.session_id != session_id {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview version is invalid.",
        ));
    }
    let mut request_targets = stored_request.targets.clone();
    validate_adoption_targets(
        &tx,
        &state,
        &stored_request.session_id,
        &mut request_targets,
    )?;
    if request_targets != preview.targets {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview targets no longer match its frozen request.",
        ));
    }
    let mut targets = preview.targets.clone();
    let target_ids = targets
        .iter()
        .map(|target| target.document_id.clone())
        .collect::<HashSet<_>>();
    let before = validate_adoption_targets(&tx, &state, &preview.session_id, &mut targets)?;
    validate_state_references(&tx, &state, None, &target_ids)?;
    let source_epoch = current_context_epoch(&tx)?;
    validate_candidate_provenance(
        CandidateReadContext::new::<H>(&tx),
        &state,
        &current_project_id,
        &access.operation_namespace,
        &source_epoch,
        Some(&preview.session_id),
        &preview.candidate_ids,
    )?;
    let expected_relationships =
        validate_relationship_drafts(&tx, &state, &targets, &stored_request.relationships)?;
    let expected_impacts = build_adoption_impacts(
        CandidateReadContext::new::<H>(&tx),
        &current_project_id,
        &access.operation_namespace,
        &source_epoch,
        &preview.candidate_ids,
        &targets,
        &stored_request.impact_drafts,
    )?;
    if expected_relationships != preview.relationships || expected_impacts != preview.impacts {
        return Err(CoreError::new(
            "InvalidProject",
            "The adoption preview relationship or impact provenance changed.",
        ));
    }
    if !same_document_records(&before, &preview.before) {
        return Err(CoreError::new(
            "VersionConflict",
            "A preview source changed before adoption.",
        ));
    }
    validate_protected_texts_for_targets(
        &tx,
        &state,
        &preview.session_id,
        &before,
        &targets,
        &preview.protected_text,
    )?;
    validate_target_dependencies(&state, &targets, &tx)?;

    let mut resulting_documents = Vec::new();
    let mut decisions = Vec::new();
    let mut changed_heads: HashMap<String, Head> = HashMap::new();
    for target in &targets {
        let material_target = wns_documents::material_adoption::MaterialTarget {
            document_id: target.document_id.clone(),
            title: target.title.clone(),
            kind: target.kind.clone(),
            body: target.body.clone(),
            expected: target.expected.clone(),
        };
        let record = wns_documents::material_adoption::ApprovedMaterialWrite::new(
            &tx,
            &material_target,
            wns_documents::material_adoption::MaterialAdoptionFlow::Workshop,
        )
        .apply()?;
        changed_heads.insert(target.document_id.clone(), record.head.clone());
        let supersedes = state
            .decisions
            .iter()
            .rev()
            .find(|decision| {
                decision.document_id == target.document_id
                    && decision.status == WorkshopDecisionStatus::Chosen
            })
            .map(|decision| decision.id.clone());
        let decision = WorkshopDecision {
            id: new_id(),
            session_id: preview.session_id.clone(),
            title: target.title.clone(),
            document_id: target.document_id.clone(),
            revision_id: record.last_checkpoint_id.clone().ok_or_else(|| {
                CoreError::new(
                    "PersistenceUnavailable",
                    "Adoption revision was not created.",
                )
            })?,
            head: record.head.clone(),
            candidate_ids: preview.candidate_ids.clone(),
            rationale: preview.rationale.clone(),
            status: WorkshopDecisionStatus::Chosen,
            fixed: !preview.protected_text.is_empty(),
            protected_text: preview.protected_text.clone(),
            access: "authorRoom".into(),
            supersedes_id: supersedes,
        };
        decisions.push(decision);
        resulting_documents.push(record);
    }
    let mut committed_heads = changed_heads.clone();
    for relationship in &expected_relationships {
        for document_id in [&relationship.from_document_id, &relationship.to_document_id] {
            if !committed_heads.contains_key(document_id) {
                committed_heads.insert(document_id.clone(), read_document(&tx, document_id)?.head);
            }
        }
    }
    let committed_relationships =
        materialize_relationship_drafts(&stored_request.relationships, &committed_heads)?;
    if committed_relationships != expected_relationships {
        return Err(CoreError::new(
            "VersionConflict",
            "A relationship endpoint changed while the adoption was being committed.",
        ));
    }
    if !changed_heads.is_empty() {
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
    }
    let decision_ids: Vec<String> = decisions
        .iter()
        .map(|decision| decision.id.clone())
        .collect();
    // A document has one current chosen workshop decision. Preserve the
    // immutable history, but retire the prior projection before adding
    // the new adoption so Story Bible consumers cannot see two competing
    // chosen revisions for the same target.
    for previous in &mut state.decisions {
        if previous.status == WorkshopDecisionStatus::Chosen
            && decisions
                .iter()
                .any(|next| next.document_id == previous.document_id)
        {
            previous.status = WorkshopDecisionStatus::Superseded;
        }
    }
    state.current_session_id = Some(preview.session_id.clone());
    state.decisions.extend(decisions.clone());
    let existing_relationships = state.relationships.clone();
    state.relationships.extend(committed_relationships);
    for relationship in &existing_relationships {
        for source in relationship
            .source_heads
            .iter()
            .filter(|head| changed_heads.contains_key(&head.document_id))
        {
            let decision_id = decisions
                .iter()
                .find(|decision| decision.document_id == source.document_id)
                .map(|decision| decision.id.clone())
                .ok_or_else(|| {
                    CoreError::new(
                        "InvalidRequest",
                        "A relationship impact has no adoption decision for its changed source.",
                    )
                })?;
            state.impacts.push(WorkshopImpact {
                id: new_id(),
                decision_id,
                document_id: source.document_id.clone(),
                kind: WorkshopImpactKind::PossibleTension,
                reason: format!(
                    "Relationship {} uses changed source {}; review whether it still holds.",
                    relationship.id, source.document_id
                ),
                status: WorkshopImpactStatus::NeedsReview,
                candidate_id: None,
                relationship_id: Some(relationship.id.clone()),
            });
        }
    }
    for impact in expected_impacts {
        let decision_id = decisions
            .iter()
            .find(|decision| decision.document_id == impact.document_id)
            .or_else(|| decisions.first())
            .map(|decision| decision.id.clone())
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidRequest",
                    "A candidate impact has no adoption decision.",
                )
            })?;
        state.impacts.push(WorkshopImpact {
            id: new_id(),
            decision_id,
            document_id: impact.document_id,
            kind: impact.kind,
            reason: impact.reason,
            status: WorkshopImpactStatus::NeedsReview,
            candidate_id: Some(impact.candidate_id),
            relationship_id: None,
        });
    }
    let next_version = expected
        .checked_add(1)
        .ok_or_else(|| CoreError::new("VersionLimit", "The workshop version limit was reached."))?;
    validate_state_shape(&state)?;
    store_state(&tx, next_version, &state)?;
    insert_snapshot(
        &tx,
        &current_project_id,
        &access.operation_namespace,
        &operation_id,
        next_version,
        &payload_hash,
        &state,
    )?;
    let snapshot = workshop_snapshot(next_version, state)?;
    let ack = WorkshopAdoptionAck {
        snapshot,
        documents: resulting_documents,
        decision_ids,
    };
    tx.execute(
        "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
        params![access.operation_namespace, operation_id, "adoptWorkshop", payload_hash, serde_json::to_string(&ack)?],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(ack)
}

fn validate_protected_texts_for_targets(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    before: &[DocumentRecord],
    targets: &[WorkshopAdoptionTarget],
    requested: &[String],
) -> CoreResult<()> {
    let mut before_by_id = before
        .iter()
        .map(|record| (record.head.document_id.as_str(), record))
        .collect::<HashMap<_, _>>();
    for target in targets {
        let mut protected = requested.to_vec();
        if let Some(old) = before_by_id.remove(target.document_id.as_str()) {
            let previous_fixed = target_fixed_text(connection, state, session_id, old)?;
            protected.extend(previous_fixed.clone());
            validate_protected_text(Some(old), &target.body, &protected, &previous_fixed)?;
        } else {
            validate_protected_text(None, &target.body, &protected, &[])?;
        }
    }
    Ok(())
}

fn same_document_records(left: &[DocumentRecord], right: &[DocumentRecord]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.head == right.head
                && left.title == right.title
                && left.kind == right.kind
                && left.metadata_version == right.metadata_version
                && left.body == right.body
                && left.last_checkpoint_id == right.last_checkpoint_id
        })
}

/// Validate workshop rows at backup/recovery boundaries. Historical previews
/// and receipts may retain an older identity after recovery, but all hashes,
/// JSON contracts, and bounded metadata must remain valid.
pub fn validate_storage(
    connection: &Connection,
    validate_authority: &ChatAuthorityCheck<'_>,
) -> CoreResult<()> {
    if let Some((version, state, hash)) = connection
        .query_row(
            "SELECT version,state_json,state_hash FROM workshop_state WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        if version < 0 {
            return Err(CoreError::new(
                "InvalidBackup",
                "The workshop state version is negative.",
            ));
        }
        let parsed = parse_state(&state, &hash)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        validate_state_shape(&parsed)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        let matching_snapshot: Option<(String, String)> = connection
            .query_row(
                "SELECT state_json,state_hash FROM workshop_snapshots
                 WHERE version=? ORDER BY id DESC LIMIT 1",
                [version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match matching_snapshot {
            Some((snapshot_json, snapshot_hash))
                if snapshot_json == state && snapshot_hash == hash => {}
            Some(_) => {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "The current Workshop state does not match its immutable snapshot.",
                ));
            }
            None if version == 0 && parsed == WorkshopState::default() => {}
            None => {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "The current Workshop state has no immutable snapshot.",
                ));
            }
        }
    }
    let mut snapshots = connection.prepare("SELECT id,project_id,operation_namespace,operation_id,version,payload_hash,state_json,state_hash FROM workshop_snapshots")?;
    for row in snapshots.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })? {
        let (id, project, namespace, operation, version, payload, state, hash) = row?;
        if !storage_valid_id(&id)
            || !storage_valid_id(&project)
            || !storage_valid_id(&namespace)
            || !storage_valid_id(&operation)
            || version < 0
            || !valid_hash(&payload)
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop snapshot has invalid identity or hash metadata.",
            ));
        }
        validate_snapshot_authority(
            connection,
            validate_authority,
            WorkshopSnapshotOrigin {
                project_id: &project,
                namespace: &namespace,
                operation: &operation,
                version,
                payload_hash: &payload,
            },
            &state,
            &hash,
        )
        .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
    }
    let mut previews = connection.prepare("SELECT id,project_id,operation_namespace,session_id,expected_version,payload_hash,request_json,preview_json,preview_hash FROM workshop_adoption_previews")?;
    for row in previews.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })? {
        let (id, project, namespace, session, version, payload, raw_request_json, preview, hash) =
            row?;
        if !storage_valid_id(&id)
            || !storage_valid_id(&project)
            || !storage_valid_id(&namespace)
            || !storage_valid_id(&session)
            || version < 0
            || !valid_hash(&payload)
            || !valid_hash(&hash)
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop preview has invalid identity or hash metadata.",
            ));
        }
        let request: PreviewWorkshopAdoption =
            serde_json::from_str(&raw_request_json).map_err(|_| {
                CoreError::new("InvalidBackup", "A workshop preview request is invalid.")
            })?;
        let parsed: WorkshopAdoptionPreview = serde_json::from_str(&preview)
            .map_err(|_| CoreError::new("InvalidBackup", "A workshop preview is invalid."))?;
        validate_preview_relationships(&request, &parsed)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        let canonical_request = serde_json::to_string(&wns_kernel::canonicalize_value(
            serde_json::to_value(&request)?,
        ))?;
        let mut canonical_targets = request.targets.clone();
        for target in &mut canonical_targets {
            target.body = canonical_body(&target.body)
                .map_err(|_| CoreError::new("InvalidBackup", "A workshop target body is invalid."))?
                .0;
        }
        let canonical = serde_json::to_string(&wns_kernel::canonicalize_value(
            serde_json::to_value(&parsed)?,
        ))?;
        if parsed.id != id
            || parsed.session_id != session
            || parsed.expected_version != version.to_string()
            || request.session_id != session
            || request.access.project_id != project
            || request.access.operation_namespace != namespace
            || request.expected_version != version.to_string()
            || request.candidate_ids != parsed.candidate_ids
            || request.rationale != parsed.rationale
            || request.protected_text != parsed.protected_text
            || canonical_targets != parsed.targets
            || canonical_request != raw_request_json
            || sha256_hex(raw_request_json.as_bytes()) != payload
            || sha256_hex(canonical.as_bytes()) != hash
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop preview provenance or hash is invalid.",
            ));
        }
    }
    let mut receipts = connection.prepare("SELECT operation_namespace,operation_id,operation_kind,payload_hash,result_json FROM workshop_receipts")?;
    for row in receipts.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })? {
        let (namespace, operation, kind, payload, result) = row?;
        if !storage_valid_id(&namespace)
            || !storage_valid_id(&operation)
            || !valid_hash(&payload)
            || !["startWorkshop", "saveWorkshop", "adoptWorkshop"].contains(&kind.as_str())
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop receipt has invalid operation metadata.",
            ));
        }
        if kind == "startWorkshop" {
            let started: DiscussionStart = serde_json::from_str(&result).map_err(|_| {
                CoreError::new("InvalidBackup", "A workshop start receipt is invalid.")
            })?;
            if started.run.owner.operation_namespace != namespace
                || started.run.operation_id != operation
                || started.run.intent
                    != wns_story::discussion_vocabulary::FeedbackIntent::WorkshopExplore
            {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop start receipt has invalid discussion provenance.",
                ));
            }
            let packet = wns_story::context_packets::validated_packet_record(
                connection,
                &started.run.packet_id,
            )?;
            if packet.receipt.packet_id != started.run.packet_id
                || packet.options.provider_binding != started.run.provider_binding
            {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop start receipt does not match its immutable packet.",
                ));
            }
        } else if kind == "saveWorkshop" {
            let receipt_snapshot: WorkshopSnapshot =
                serde_json::from_str(&result).map_err(|_| {
                    CoreError::new("InvalidBackup", "A workshop save receipt is invalid.")
                })?;
            let stored_snapshot: Option<(i64, String, String)> = connection
                .query_row(
                    "SELECT version,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
                    params![namespace, operation],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((version, state_json, state_hash)) = stored_snapshot {
                let persisted = workshop_snapshot(version, parse_state(&state_json, &state_hash)?)?;
                if persisted != receipt_snapshot {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "A workshop save receipt does not match its immutable snapshot.",
                    ));
                }
            }
        } else {
            let receipt_ack: WorkshopAdoptionAck = serde_json::from_str(&result).map_err(|_| {
                CoreError::new("InvalidBackup", "A workshop adoption receipt is invalid.")
            })?;
            let stored_snapshot: Option<(i64, String, String)> = connection
                .query_row(
                    "SELECT version,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
                    params![namespace, operation],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((version, state_json, state_hash)) = stored_snapshot else {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt has no immutable snapshot.",
                ));
            };
            let persisted = workshop_snapshot(version, parse_state(&state_json, &state_hash)?)?;
            if persisted != receipt_ack.snapshot {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt does not match its immutable snapshot.",
                ));
            }
            if receipt_ack.decision_ids.iter().any(|id| {
                !persisted
                    .state
                    .decisions
                    .iter()
                    .any(|decision| &decision.id == id)
            }) {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt names a decision absent from its snapshot.",
                ));
            }
            for document in &receipt_ack.documents {
                let Some(revision_id) = document.last_checkpoint_id.as_deref() else {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "A workshop adoption receipt contains a document without a revision.",
                    ));
                };
                let revision = read_revision(connection, revision_id).map_err(|_| {
                    CoreError::new("InvalidBackup", "An adoption document revision is invalid.")
                })?;
                if revision.head != document.head || revision.body != document.body {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "An adoption document does not match its immutable revision.",
                    ));
                }
            }
        }
    }
    Ok(())
}

// The actor implements the host, beside the module that will become its caller.

mod adoption;
mod state;
use adoption::*;
use state::*;

// Reachable through `workshop` before the split; a glob import is private, so
// the one that was `pub` is named here.
pub use adoption::fixed_decision_is_relevant;
