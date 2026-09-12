//! Durable author-room chapter discussions.
//!
//! A discussion start is one actor-owned transaction: the target is checked,
//! its immutable context revision is frozen, the exact C2 packet is compiled,
//! and the user message plus queued run are inserted before the transaction
//! commits. Provider execution is intentionally outside this module. The
//! output methods below are the small durable boundary a later supervisor can
//! drive with deterministic or live events.
use crate::{guidance, proposals};
use wns_context::project_chat_output;
use wns_kernel::{
    CoreError, CoreResult, Head, ProjectAccess, ProjectInfo, Reply, check_id, logical_hash, new_id,
    parse_stored_version, parse_version, sha256_hex,
};
use wns_storage::{read_document, read_revision};
pub use wns_story::discussion_vocabulary::{
    DiscussionScopeInput, FeedbackIntent, StartDiscussion, skip_default_feedback_intent,
};
use wns_story::host::StoryHost;
pub use wns_story::run_vocabulary::*;
use wns_story::{context_packets, source_pins, story_context};
// The provider delivery vocabulary moved to `wns-providers::vocabulary` (L1),
// below both this module (bound for `wns-conversation`, L5) and `memory`
// (`wns-story`, L4). Re-exported at the historical path so
// `discussions::ProviderCleanup` and its siblings resolve unchanged for
// `discussion_lookup` and `memory`.
use crate::discussion_lookup;
use wns_context::continuation::CONTINUATION_RESPONSE_CONTRACT;
use wns_context::lookup::LookupAllowance;
use wns_context::packet::{
    CODEX_INPUT_LIMIT_BYTES, CODEX_OUTPUT_LIMIT_BYTES, CompiledPacket, LOOKUP_RESPONSE_CONTRACT,
    PROPOSAL_RESPONSE_CONTRACT, PacketError, PacketRequest, ProviderBinding,
    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT, compile_packet, serialized_input,
};
use wns_context::{Audience, BasisKind, ContextPurpose, InformationPolicy, MAX_SAFE_BRIEF_BYTES};
use wns_documents::{
    ScopeGrant, ScopeKind, ScopeValidationRequest, capture_append_scope, capture_scope,
    validate_scope,
};
pub use wns_providers::vocabulary::{
    HttpDeliverySubmission, HttpProviderUsage, ProviderCleanup, ProviderDeliveryReceipt,
    ProviderOutcomeStatus, ProviderUsage,
};
use wns_story::context_packets::PrepareContext;
use wns_story::story_context::{FreezeReviewedContinuation, FreezeStory, FrozenContext};
// The workshop contract and its parser both live below this module now — the
// contract at L3, the parser in `wns-story` beside the metadata types. Naming
// them here rather than through `workshop_generation` is what removes the
// L5→L5 edge this module used to have with `wns-workshop`.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use wns_context::response_contracts::WORKSHOP_RESPONSE_CONTRACT;
use wns_providers::http_request::prepare_request as prepare_http_request;
use wns_story::workshop_metadata::{metadata_from_instruction, metadata_value};

pub use wns_context::SafeBriefInput;

mod app_server;
pub mod queries;
mod retry;

const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;
const MAX_SCOPE_QUOTE_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 128 * 1024;
const MAX_PINNED_DOCUMENTS: usize = 64;
const STOP_SETTLED_MESSAGE: &str =
    "You stopped this response. Any partial text shown here is saved.";
const STOP_UNRESOLVED_MESSAGE: &str =
    "This response was interrupted. Any partial text shown here is saved.";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionDispatch {
    pub run: DiscussionRun,
    pub packet: CompiledPacket,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFail {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionDraft {
    pub document_id: String,
    pub version: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_run_id: Option<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionRetry {
    pub text: String,
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    pub previous_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveDiscussionDraft {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub document_id: String,
    pub expected_version: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionView {
    pub document_id: String,
    pub thread_id: Option<String>,
    pub messages: Vec<DiscussionMessage>,
    pub runs: Vec<DiscussionRun>,
    pub draft: Option<DiscussionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionOutputAppend {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFinish {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionStopCleanup {
    Settled,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionStopSettled {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
    pub cleanup: DiscussionStopCleanup,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionStop {
    pub run: DiscussionRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionBegin {
    pub owner: RunOwner,
}

#[allow(clippy::large_enum_variant)]
pub enum DiscussionCommand {
    ClaimAppServer(
        RunOwner,
        wns_providers::codex_app_server::AppServerDispatch,
        Reply<()>,
    ),
    AckAppServer(
        RunOwner,
        wns_providers::codex_app_server::AppServerDispatch,
        String,
        Reply<()>,
    ),
    Start(StartDiscussion, Reply<DiscussionStart>),
    Begin(DiscussionBegin, Reply<DiscussionDispatch>),
    MarkDelivered(RunOwner, Reply<DiscussionRun>),
    Append(DiscussionOutputAppend, Reply<DiscussionRun>),
    Finish(DiscussionFinish, Reply<DiscussionRun>),
    Fail(DiscussionFail, Reply<DiscussionRun>),
    Stop(ProjectAccess, String, Reply<DiscussionStop>),
    SettleStop(DiscussionStopSettled, Reply<DiscussionRun>),
    SettleProvider(ProviderTerminalReport, Reply<ProviderDiscussionSettlement>),
    ClaimLookup(
        RunOwner,
        String,
        Reply<crate::discussion_lookup::LookupDispatch>,
    ),
    SettleLookup(
        crate::discussion_lookup::LookupInvocationReport,
        Reply<DiscussionRun>,
    ),
    AdvanceLookup(
        crate::discussion_lookup::LookupAdvanceRequest,
        Reply<crate::discussion_lookup::LookupAdvance>,
    ),
    HaltLookup(
        crate::discussion_lookup::LookupHaltRequest,
        Reply<DiscussionRun>,
    ),
    ReadRun(RunOwner, Reply<DiscussionRun>),
    Read(ProjectAccess, String, Reply<DiscussionView>),
    Retry(ProjectAccess, String, Reply<DiscussionRetry>),
    SaveDraft(SaveDiscussionDraft, Reply<DiscussionDraft>),
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn handle_discussion(host: &mut impl StoryHost, command: DiscussionCommand) {
    macro_rules! mutate {
        ($reply:expr, $operation:expr) => {{
            let result = $operation;
            host.fence_uncertain(&result);
            let _ = $reply.send(result);
        }};
    }
    match command {
        DiscussionCommand::ClaimAppServer(owner, dispatch, reply) => {
            mutate!(
                reply,
                app_server::claim_app_server_dispatch(host, owner, dispatch)
            );
        }
        DiscussionCommand::AckAppServer(owner, dispatch, turn_id, reply) => {
            mutate!(
                reply,
                app_server::acknowledge_app_server_turn(host, owner, dispatch, turn_id)
            );
        }
        DiscussionCommand::Start(request, reply) => {
            mutate!(reply, start_discussion(host, request));
        }
        DiscussionCommand::Begin(request, reply) => {
            mutate!(reply, begin_discussion_run(host, request));
        }
        DiscussionCommand::MarkDelivered(owner, reply) => {
            mutate!(reply, mark_discussion_delivered(host, owner));
        }
        DiscussionCommand::Append(request, reply) => {
            mutate!(reply, append_discussion_output(host, request));
        }
        DiscussionCommand::Finish(request, reply) => {
            mutate!(reply, finish_discussion(host, request));
        }
        DiscussionCommand::Fail(request, reply) => {
            mutate!(reply, fail_discussion_run(host, request));
        }
        DiscussionCommand::Stop(access, run_id, reply) => {
            mutate!(reply, stop_discussion(host, access, run_id));
        }
        DiscussionCommand::SettleStop(request, reply) => {
            mutate!(reply, settle_discussion_stop(host, request));
        }
        DiscussionCommand::SettleProvider(request, reply) => {
            mutate!(reply, settle_provider_discussion(host, request));
        }
        DiscussionCommand::ClaimLookup(owner, ordinal, reply) => {
            mutate!(reply, claim_lookup_invocation(host, owner, &ordinal));
        }
        DiscussionCommand::SettleLookup(request, reply) => {
            mutate!(reply, settle_lookup_invocation(host, request));
        }
        DiscussionCommand::AdvanceLookup(request, reply) => {
            mutate!(reply, advance_lookup(host, request));
        }
        DiscussionCommand::HaltLookup(request, reply) => {
            mutate!(reply, halt_lookup(host, request));
        }
        DiscussionCommand::ReadRun(owner, reply) => {
            let result = validate_runtime_owner(host.info(), &owner).and_then(|()| {
                read_run(host.db()?, &owner.run_id).and_then(|run| {
                    validate_owner(&run, &owner)?;
                    Ok(run)
                })
            });
            let _ = reply.send(result);
        }
        DiscussionCommand::Read(access, document_id, reply) => {
            let _ = reply.send(read_discussion(host, access, document_id));
        }
        DiscussionCommand::Retry(access, run_id, reply) => {
            let _ = reply.send(
                host.check_access(&access)
                    .and_then(|()| retry::draft(host.db()?, &access, &run_id)),
            );
        }
        DiscussionCommand::SaveDraft(request, reply) => {
            mutate!(reply, save_discussion_draft(host, request));
        }
    }
}

/// Start is intentionally an actor method. The parent `Command` enum can
/// wire this method after the C2 persistence checkpoint without nesting a
/// freeze transaction or a packet transaction.
pub fn start_discussion(
    host: &mut impl StoryHost,
    request: StartDiscussion,
) -> CoreResult<DiscussionStart> {
    host.check_access(&request.access)?;
    validate_start(&request)?;
    let payload_hash = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = start_discussion_at(&tx, &request, &payload_hash, None, false)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

/// Start a discussion using an already-open actor transaction. Project
/// chat uses this seam to persist its conversation reference atomically
/// with the ordinary discussion run, packet, and message. It performs no
/// access check or commit and therefore cannot nest actor transactions.
pub fn start_discussion_at(
    tx: &Connection,
    request: &StartDiscussion,
    payload_hash: &str,
    chat: Option<&crate::project_chat_context::ProjectChatFreeze>,
    chapter_range: bool,
) -> CoreResult<DiscussionStart> {
    if let Some(chat) = chat {
        if request.intent != FeedbackIntent::Discuss
            || request.scope.is_some()
            || request.safe_brief.is_some()
            || request.lookup.is_some()
            || request.basis.is_some()
        {
            return Err(CoreError::new(
                "InvalidProjectChatRequest",
                "Project chat uses a plain author-room discussion without a writing scope, lookup, or brief.",
            ));
        }
        check_id(&chat.conversation_id)?;
    }
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT id,payload_hash FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",
            params![request.access.project_id, request.access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((run_id, previous_payload)) = existing {
        if previous_payload != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This discussion operation was already used for a different request.",
            ));
        }
        let result = read_start(tx, &run_id)?;
        return Ok(result);
    }
    validate_safe_brief_origin(tx, request)?;

    let retry_guidance = retry::guidance(tx, request)?;
    let (purpose, policy) = discussion_context_policy(tx, request)?;
    let context_request = FreezeStory {
        access: request.access.clone(),
        operation_id: new_id(),
        expected: request.expected.clone(),
        basis: BasisKind::Working,
        purpose,
        policy,
    };
    let context_payload = logical_hash(&context_request)?;
    let frozen_context = if request.basis == Some(BasisKind::Reviewed) {
        let reviewed = FreezeReviewedContinuation {
            access: request.access.clone(),
            operation_id: context_request.operation_id.clone(),
            expected: request.expected.clone(),
            policy: context_request.policy.clone(),
        };
        let payload = logical_hash(&reviewed)?;
        story_context::freeze_reviewed_continuation_at(tx, &reviewed, &payload)?
    } else if let Some(chat) = chat {
        crate::project_chat_context::freeze_project_chat_at(
            tx,
            &context_request,
            &context_payload,
            chat,
        )?
    } else {
        story_context::freeze_discussion_story_at(
            tx,
            &context_request,
            &context_payload,
            retry_guidance.as_deref(),
        )?
    };
    let target = read_revision(tx, &frozen_context.snapshot.target.revision_id)?;
    if chapter_range {
        let target_kind: Option<String> = tx
            .query_row(
                "SELECT kind FROM documents WHERE id=? AND trashed=0",
                [&request.expected.document_id],
                |row| row.get(0),
            )
            .optional()?;
        if request.intent != FeedbackIntent::Discuss
            || request.scope.is_some()
            || target_kind.as_deref() != Some("chapter")
        {
            return Err(CoreError::new(
                "InvalidChapterRequest",
                "The chapter range response contract requires an unscoped Discuss request targeting an ordinary chapter.",
            ));
        }
    }
    // Project chat is rooted at its blank control anchor. Persistent
    // document pins are a legacy document-discussion feature and cannot
    // be read through that control identity; project-chat source refs are
    // authenticated by its dedicated freeze path instead.
    let persistent_ids = if chat.is_some() {
        Vec::new()
    } else {
        source_pins::persistent_for_discussion(
            tx,
            &request.access,
            &request.expected.document_id,
            request.intent.is_discuss(),
        )?
    };
    let merged_document_ids =
        merge_pinned_document_ids(&persistent_ids, &request.pinned_document_ids)?;
    let transient_handles = resolve_pinned_handles(&frozen_context, &request.pinned_document_ids)?;
    let mut all_mandatory_handles = resolve_pinned_handles(&frozen_context, &merged_document_ids)?;
    if let Some(chat) = chat {
        // Explicit project-chat source refs are author-selected evidence;
        // they are mandatory packet inputs and may not disappear under
        // layered budget packing. The blank control target remains
        // mandatory through the shared target rule.
        for head in chat
            .source_refs
            .iter()
            .chain(chat.task_draft_refs.iter().map(|draft| &draft.head))
        {
            let handle = frozen_context
                .snapshot
                .sources
                .iter()
                .find(|source| {
                    source.source.document_id == head.document_id
                        && source.source.body_hash == head.body_hash
                })
                .map(|source| source.handle.clone())
                .ok_or_else(|| {
                    CoreError::new(
                        "SourceOutsideFrozenContext",
                        "A project-chat source ref has no frozen source handle.",
                    )
                })?;
            if !all_mandatory_handles.contains(&handle) {
                all_mandatory_handles.push(handle);
            }
        }
    }
    let target_handle = frozen_context
        .snapshot
        .sources
        .iter()
        .find(|source| source.source == frozen_context.snapshot.target)
        .map(|source| source.handle.clone())
        .ok_or_else(|| {
            CoreError::new(
                "InvalidContext",
                "The frozen discussion target is missing from its source manifest.",
            )
        })?;
    // A transient target selection keeps the existing compiler refusal.
    // A persistent target selection needs no additional source entry:
    // the compiler already reserves the complete target itself.
    let mandatory_handles = if transient_handles
        .iter()
        .any(|handle| handle == &target_handle)
    {
        all_mandatory_handles.clone()
    } else {
        all_mandatory_handles
            .iter()
            .filter(|handle| *handle != &target_handle)
            .cloned()
            .collect()
    };
    let source_reads = frozen_context
        .snapshot
        .sources
        .iter()
        .map(|source| story_context::read_source(tx, &frozen_context, &source.handle))
        .collect::<CoreResult<Vec<_>>>()?;
    let scope = if request.intent == FeedbackIntent::Continue {
        Some(
            capture_append_scope(&target.body)
                .map_err(|message| CoreError::new("InvalidScope", &message))?,
        )
    } else {
        capture_discussion_scope(request.scope.as_ref(), &target.body)?
    };
    // The response contract is derived here from trusted intent and the
    // immutable live binding. It is never accepted from the renderer, so
    // old mock packets and old live packets remain contract-free.
    if request.intent == FeedbackIntent::WorkshopExplore {
        metadata_from_instruction(&request.instruction)?;
    }
    let response_contract = if chat.is_some() {
        Some(project_chat_output::PROJECT_CHAT_RESPONSE_CONTRACT.to_owned())
    } else if chapter_range {
        Some(project_chat_output::CHAPTER_DISCUSSION_RESPONSE_CONTRACT.to_owned())
    } else {
        match request.intent {
            FeedbackIntent::Discuss if request.lookup.is_some() => {
                Some(LOOKUP_RESPONSE_CONTRACT.to_owned())
            }
            FeedbackIntent::Continue => Some(CONTINUATION_RESPONSE_CONTRACT.to_owned()),
            FeedbackIntent::ProposeEdits if request.provider_binding.is_some() => Some(
                if scope.as_ref().is_some_and(|scope| {
                    matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                }) {
                    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT.to_owned()
                } else {
                    PROPOSAL_RESPONSE_CONTRACT.to_owned()
                },
            ),
            FeedbackIntent::WorkshopExplore => Some(WORKSHOP_RESPONSE_CONTRACT.to_owned()),
            _ => None,
        }
    };
    let instruction = packet_instruction(request, response_contract.as_deref())?;
    // Parsed here, where the instruction is authored, and passed down. The
    // compiler used to parse it out of `instruction` itself, which made it
    // reach up into this crate for the workshop vocabulary and its
    // validation cluster.
    let workshop_metadata = if response_contract.as_deref() == Some(WORKSHOP_RESPONSE_CONTRACT) {
        let metadata = metadata_from_instruction(&instruction)?;
        Some(metadata_value(&metadata)?)
    } else {
        None
    };
    let packet = compile_packet(&PacketRequest {
        packet_id: new_id(),
        session_id: new_id(),
        invocation_ordinal: "0".into(),
        frozen: frozen_context.clone(),
        instruction,
        sources: source_reads,
        mandatory_handles: mandatory_handles.clone(),
        scope: scope.clone(),
        safe_brief: request.safe_brief.clone(),
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        lookup: request
            .lookup
            .clone()
            .map(|allowance| wns_context::lookup::LookupPacketInput {
                allowance,
                completed_invocations: 0,
                exchanges: Vec::new(),
                source_projection: None,
                reviewed_memory: Some(wns_context::lookup::REVIEWED_MEMORY_CAPABILITY.to_owned()),
            }),
        response_contract: response_contract.clone(),
        workshop_metadata,
    })
    .map_err(packet_error)?;
    insert_packet(
        tx,
        &packet,
        request,
        &mandatory_handles,
        Some(transient_handles),
        scope.as_ref(),
        response_contract.as_deref(),
    )?;
    if retry_guidance.is_none() {
        guidance::consume_request_guidance_at(
            tx,
            &frozen_context.snapshot.snapshot_id,
            &frozen_context.guidance,
        )?;
    }

    let thread_id = ensure_thread(tx, &request.access, &request.expected.document_id)?;
    let run_id = new_id();
    tx.execute(
        "INSERT INTO discussion_runs(id,thread_id,project_id,operation_namespace,operation_id,payload_hash,target_document_id,target_version,target_body_hash,packet_id,previous_run_id,status,sequence,output_text) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,0,'')",
        params![
            run_id,
            thread_id,
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            payload_hash,
            request.expected.document_id,
            parse_version(&request.expected.version)?,
            request.expected.body_hash,
            packet.receipt.packet_id,
            request.previous_run_id,
            DiscussionRunStatus::Queued.as_str(),
        ],
    )?;
    if let Some(lookup_allowance) = request.lookup.as_ref() {
        discussion_lookup::insert_initial(
            tx,
            &run_id,
            &packet,
            &frozen_context,
            &request.access.operation_namespace,
            lookup_allowance,
        )?;
    }
    let user_message_id = new_id();
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,scope_json,packet_id) VALUES(?,?,?,?,?,?,?)",
        params![
            user_message_id,
            thread_id,
            run_id,
            DiscussionMessageRole::User.as_str(),
            request.instruction,
            scope.as_ref().map(serde_json::to_string).transpose()?,
            packet.receipt.packet_id,
        ],
    )?;
    let run = read_run(tx, &run_id)?;
    let user_message = read_message(tx, &user_message_id)?;
    Ok(DiscussionStart {
        thread_id,
        run,
        user_message,
        packet,
    })
}

pub fn append_discussion_output(
    host: &mut impl StoryHost,
    request: DiscussionOutputAppend,
) -> CoreResult<DiscussionRun> {
    validate_output_event(&request.owner, &request.event_id, &request.chunk)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let expected = parse_version(&request.expected_sequence)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    reject_legacy_lookup_path(&current)?;
    if let Some((kind, chunk, event_sequence)) =
        existing_event(&tx, &request.owner.run_id, &request.event_id)?
    {
        if kind == "chunk"
            && chunk == request.chunk
            && expected.checked_add(1) == Some(event_sequence)
        {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        return Err(CoreError::new(
            "EventIdReused",
            "An output event ID was reused with different content or sequence.",
        ));
    }
    ensure_run_started(current.status)?;
    let sequence = parse_version(&current.sequence)?;
    if expected != sequence {
        return Err(CoreError::new(
            "SequenceConflict",
            "The output sequence is stale; reconcile the run before retrying.",
        ));
    }
    let next_output = append_text(&current.output_text, &request.chunk, MAX_OUTPUT_BYTES)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![request.owner.run_id, next, request.event_id, "chunk", request.chunk],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status='running',sequence=?,output_text=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
        params![next, next_output, request.owner.run_id, request.owner.project_id, request.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The run changed before the output event was committed.",
        ));
    }
    let result = read_run(&tx, &request.owner.run_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

/// Claim a queued run for one dispatcher. The claim is separate from the
/// persisted run state, so a later provider supervisor can mark delivery
/// without confusing a durable queued job with model execution.
pub fn begin_discussion_run(
    host: &mut impl StoryHost,
    request: DiscussionBegin,
) -> CoreResult<DiscussionDispatch> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    match current.status {
        DiscussionRunStatus::Queued => {
            let (snapshot_id, snapshot_source_epoch, snapshot_policy_epoch): (String, i64, i64) = tx.query_row(
                "SELECT cp.snapshot_id,ss.context_source_epoch,ss.disclosure_policy_epoch FROM discussion_runs dr JOIN context_packets cp ON cp.id=dr.packet_id JOIN story_snapshots ss ON ss.id=cp.snapshot_id WHERE dr.id=?",
                [&request.owner.run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let (source_epoch, policy_epoch): (i64, i64) = tx.query_row(
                "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let project_chat_current =
                if snapshot_source_epoch == source_epoch && snapshot_policy_epoch == policy_epoch {
                    let (frozen, _) = story_context::validated_snapshot_record(&tx, &snapshot_id)?;
                    !frozen.project_chat.is_some()
                        || crate::project_chat_context::project_chat_basis_is_current(&tx, &frozen)?
                } else {
                    false
                };
            if !project_chat_current {
                let stale_message = "The story changed before this discussion started; the saved response was not dispatched.";
                let _ = seal_run(
                    &tx,
                    &current,
                    DiscussionRunStatus::Failed,
                    "context_stale",
                    &format!("system-stale-{}", current.id),
                    stale_message,
                )?;
                tx.commit().map_err(CoreError::uncertain)?;
                return Err(CoreError::new(
                    "ContextChanged",
                    "The queued discussion is historical because its frozen context is no longer current.",
                ));
            }
            let changed = tx.execute(
                "UPDATE discussion_runs SET status='running',dispatch_state='claimed',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='queued' AND dispatch_state='pending'",
                params![request.owner.run_id, request.owner.project_id, request.owner.operation_namespace],
            )?;
            if changed != 1 {
                return Err(CoreError::new(
                    "RunAlreadyStarted",
                    "Another dispatcher already claimed this discussion run.",
                ));
            }
        }
        DiscussionRunStatus::Running => {
            return Err(CoreError::new(
                "RunAlreadyStarted",
                "This discussion run is already claimed by a dispatcher.",
            ));
        }
        _ => {
            return Err(CoreError::new(
                "RunSealed",
                "This discussion run is no longer dispatchable.",
            ));
        }
    }
    let run = read_run(&tx, &request.owner.run_id)?;
    let packet_access = ProjectAccess {
        project_id: request.owner.project_id.clone(),
        operation_namespace: request.owner.operation_namespace.clone(),
        session: String::new(),
        writer_lease: String::new(),
    };
    let packet = context_packets::read_context_packet_at(&tx, &packet_access, &run.packet_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(DiscussionDispatch { run, packet })
}

pub fn mark_discussion_delivered(
    host: &mut impl StoryHost,
    owner: RunOwner,
) -> CoreResult<DiscussionRun> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    validate_runtime_owner(host.info(), &owner)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &owner.run_id)?;
    validate_owner(&current, &owner)?;
    if current.provider_binding.is_some() {
        return Err(CoreError::new(
            "ProviderResultRequired",
            "A live discussion can be delivered only through its typed provider result.",
        ));
    }
    if current.status == DiscussionRunStatus::Stopping {
        return Err(CoreError::new(
            "RunStopping",
            "The discussion is stopping and cannot be marked delivered before cleanup settles.",
        ));
    }
    if current.status != DiscussionRunStatus::Running {
        return Err(CoreError::new(
            "RunSealed",
            "Only a running discussion can be marked delivered.",
        ));
    }
    tx.execute("UPDATE discussion_runs SET dispatch_state='delivered',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running'", params![owner.run_id,owner.project_id,owner.operation_namespace])?;
    let run = read_run(&tx, &owner.run_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(run)
}

pub fn finish_discussion(
    host: &mut impl StoryHost,
    request: DiscussionFinish,
) -> CoreResult<DiscussionRun> {
    validate_finish_request(&request.owner, &request.event_id, &request.assistant_text)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let expected = parse_version(&request.expected_sequence)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    reject_legacy_lookup_path(&current)?;
    if current.provider_binding.is_some() {
        return Err(CoreError::new(
            "ProviderResultRequired",
            "A live discussion can be completed only through its typed provider result.",
        ));
    }
    if let Some((kind, text, event_sequence)) =
        existing_event(&tx, &request.owner.run_id, &request.event_id)?
    {
        if kind == "terminal"
            && text == request.assistant_text
            && expected.checked_add(1) == Some(event_sequence)
            && current.status == DiscussionRunStatus::Completed
        {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        return Err(CoreError::new(
            "EventIdReused",
            "A terminal event ID was reused with a different outcome, content, or sequence.",
        ));
    }
    ensure_run_started(current.status)?;
    validate_final_output(&current.output_text, &request.assistant_text, false)?;
    let sequence = parse_version(&current.sequence)?;
    if expected != sequence {
        return Err(CoreError::new(
            "SequenceConflict",
            "The output sequence is stale; reconcile the run before retrying.",
        ));
    }
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![request.owner.run_id, next, request.event_id, "terminal", request.assistant_text],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status='completed',sequence=?,output_text=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
        params![next, request.assistant_text, request.owner.run_id, request.owner.project_id, request.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The run changed before completion was committed.",
        ));
    }
    let message_id = new_id();
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content) VALUES(?,?,?,?,?)",
        params![
            message_id,
            current.thread_id,
            request.owner.run_id,
            DiscussionMessageRole::Assistant.as_str(),
            request.assistant_text
        ],
    )?;
    // Propose-edits runs retain their bounded candidate set in the same
    // transaction as the terminal assistant message. A malformed or
    // non-proposal response remains a normal completed discussion; the
    // proposal store simply retains no candidates for it.
    proposals::retain_candidates_at(&tx, &current, &request.assistant_text)?;
    let result = read_run(&tx, &request.owner.run_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

/// Seal a provider or supervisor failure while preserving every chunk
/// already accepted. The failure explanation is an immutable assistant
/// message in the same transaction as the terminal run event.
pub fn fail_discussion_run(
    host: &mut impl StoryHost,
    request: DiscussionFail,
) -> CoreResult<DiscussionRun> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    check_id(&request.event_id)?;
    check_id(&request.owner.run_id)?;
    let expected = parse_version(&request.expected_sequence)?;
    let reason = request.reason.trim();
    if reason.is_empty() || reason.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion failure reason must be nonempty and at most 128 KiB.",
        ));
    }
    let terminal_text = format!("Discussion failed: {reason}");
    if terminal_text.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "The discussion failure message exceeds the durable event limit.",
        ));
    }
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    reject_legacy_lookup_path(&current)?;
    if current.status == DiscussionRunStatus::Stopping {
        return Err(CoreError::new(
            "RunStopping",
            "The discussion is stopping and cannot be failed before cleanup settles.",
        ));
    }
    if let Some((kind, text, event_sequence)) =
        existing_event(&tx, &request.owner.run_id, &request.event_id)?
    {
        if kind == "terminal"
            && text == terminal_text
            && expected.checked_add(1) == Some(event_sequence)
            && current.status == DiscussionRunStatus::Failed
        {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        return Err(CoreError::new(
            "EventIdReused",
            "A terminal event ID was reused with a different outcome, content, or sequence.",
        ));
    }
    let sequence = parse_version(&current.sequence)?;
    if expected != sequence {
        return Err(CoreError::new(
            "SequenceConflict",
            "The output sequence changed before failure was recorded.",
        ));
    }
    let result = seal_run(
        &tx,
        &current,
        DiscussionRunStatus::Failed,
        reason,
        &request.event_id,
        &terminal_text,
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

pub fn stop_discussion(
    host: &mut impl StoryHost,
    access: ProjectAccess,
    run_id: String,
) -> CoreResult<DiscussionStop> {
    host.check_access(&access)?;
    check_id(&run_id)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &run_id)?;
    if current.owner.project_id != access.project_id
        || current.owner.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "This run belongs to another project session.",
        ));
    }
    let run = match current.status {
        DiscussionRunStatus::Queued => seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Stopped,
            "author_stopped",
            &format!("system-stop-{}", current.id),
            STOP_SETTLED_MESSAGE,
        )?,
        DiscussionRunStatus::Running => {
            tx.execute(
                "UPDATE discussion_runs SET status='stopping',stop_reason='author_stopped',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running'",
                params![run_id, access.project_id, access.operation_namespace],
            )?;
            read_run(&tx, &run_id)?
        }
        DiscussionRunStatus::Stopping
        | DiscussionRunStatus::Completed
        | DiscussionRunStatus::Stopped
        | DiscussionRunStatus::Failed
        | DiscussionRunStatus::Interrupted => current,
    };
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(DiscussionStop { run })
}

pub fn settle_discussion_stop(
    host: &mut impl StoryHost,
    request: DiscussionStopSettled,
) -> CoreResult<DiscussionRun> {
    validate_settlement_request(&request)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let expected = parse_version(&request.expected_sequence)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    let status = match request.cleanup {
        DiscussionStopCleanup::Settled => DiscussionRunStatus::Stopped,
        DiscussionStopCleanup::Unresolved => DiscussionRunStatus::Interrupted,
    };
    if let Some((kind, text, event_sequence)) =
        existing_event(&tx, &request.owner.run_id, &request.event_id)?
    {
        if kind == "terminal"
            && text == request.assistant_text
            && expected.checked_add(1) == Some(event_sequence)
            && current.status == status
            && current.output_text == request.assistant_text
        {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        return Err(CoreError::new(
            "EventIdReused",
            "A stop settlement event ID was reused with a different outcome, content, or sequence.",
        ));
    }
    if current.status != DiscussionRunStatus::Stopping {
        return Err(CoreError::new(
            "RunSealed",
            "Only a stopping discussion can be settled.",
        ));
    }
    let sequence = parse_version(&current.sequence)?;
    if expected != sequence {
        return Err(CoreError::new(
            "SequenceConflict",
            "The stop settlement sequence is stale; reconcile the run before retrying.",
        ));
    }
    validate_final_output(&current.output_text, &request.assistant_text, true)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![
            request.owner.run_id,
            next,
            request.event_id,
            "terminal",
            request.assistant_text
        ],
    )?;
    let reason = match request.cleanup {
        DiscussionStopCleanup::Settled => "author_stopped",
        DiscussionStopCleanup::Unresolved => "stop_cleanup_unresolved",
    };
    let changed = tx.execute(
        "UPDATE discussion_runs SET status=?,sequence=?,output_text=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='stopping' AND sequence=?",
        params![
            status.as_str(),
            next,
            request.assistant_text,
            reason,
            request.owner.run_id,
            request.owner.project_id,
            request.owner.operation_namespace,
            sequence
        ],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The stop settlement changed before its terminal state was committed.",
        ));
    }
    let explanation = match request.cleanup {
        DiscussionStopCleanup::Settled => STOP_SETTLED_MESSAGE,
        DiscussionStopCleanup::Unresolved => STOP_UNRESOLVED_MESSAGE,
    };
    let message = if request.assistant_text.is_empty() {
        explanation.to_owned()
    } else {
        format!("{}\n\n[{}]", request.assistant_text, explanation)
    };
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![
            new_id(),
            current.thread_id,
            request.owner.run_id,
            DiscussionMessageRole::Assistant.as_str(),
            message,
            current.packet_id
        ],
    )?;
    let result = read_run(&tx, &request.owner.run_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

/// Persist the terminal result returned by the bounded provider worker.
/// The packet, output sequence, terminal event, assistant message, and
/// provider receipt commit together. A receipt already present for the run
/// is treated as the reconciliation authority after a lost acknowledgment.
pub fn settle_provider_discussion(
    host: &mut impl StoryHost,
    request: ProviderTerminalReport,
) -> CoreResult<ProviderDiscussionSettlement> {
    validate_provider_report_shape(&request)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let expected = parse_version(&request.expected_sequence)?;
    let confirmed_stdin_bytes = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    reject_legacy_lookup_path(&current)?;
    if let Some(saved) = read_provider_result(&tx, &current.id, &current.packet_id)? {
        if provider_result_matches_report(&saved, &request) {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(ProviderDiscussionSettlement {
                run: current,
                provider_result: saved,
            });
        }
        return Err(CoreError::new(
            "ProviderResultConflict",
            "A provider result is already durably recorded for this run.",
        ));
    }
    if !matches!(
        current.status,
        DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    ) {
        return Err(CoreError::new(
            "RunNotStarted",
            "Claim the queued discussion before settling a provider result.",
        ));
    }
    let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
    let binding = packet.options.provider_binding.as_ref().ok_or_else(|| {
        CoreError::new(
            "ProviderBindingMissing",
            "This discussion was prepared for the local mock and cannot accept a live result.",
        )
    })?;
    if binding != &request.binding {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The provider result does not match the immutable packet binding.",
        ));
    }
    let serialized = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let delivered = if wns_providers::codex_app_server::is_app_server(binding) {
        app_server::validate_delivery(
            &tx,
            &current.id,
            &packet,
            request.app_server.as_ref(),
            request.status,
            request.cleanup,
        )?
    } else if binding.is_http() {
        validate_http_delivery(&packet, &request)?;
        matches!(
            request.delivery.as_ref().map(|receipt| receipt.submission),
            Some(HttpDeliverySubmission::ResponseReceived)
        )
    } else {
        if request.delivery.is_some()
            || confirmed_stdin_bytes > serialized.len() as u64
            || (request.status == ProviderOutcomeStatus::Completed
                && confirmed_stdin_bytes != serialized.len() as u64)
        {
            return Err(CoreError::new(
                "ProviderInputMismatch",
                "The Codex provider reported invalid stdin delivery evidence.",
            ));
        }
        confirmed_stdin_bytes == serialized.len() as u64
    };
    let output_limit = binding
        .output_limit()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    if request.assistant_text.len() > output_limit {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The provider output exceeds the application byte cap.",
        ));
    }
    validate_final_output(&current.output_text, &request.assistant_text, true)?;
    if request.status == ProviderOutcomeStatus::Completed && request.assistant_text.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed provider result must include assistant output.",
        ));
    }
    if request.status == ProviderOutcomeStatus::Completed && request.error.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed provider result cannot include a provider error.",
        ));
    }
    let sequence = parse_version(&current.sequence)?;
    if expected != sequence {
        return Err(CoreError::new(
            "SequenceConflict",
            "The provider result sequence is stale; reconcile the run before retrying.",
        ));
    }
    if existing_event(&tx, &current.id, &request.event_id)?.is_some() {
        return Err(CoreError::new(
            "EventIdReused",
            "The provider terminal event ID is already used by another event.",
        ));
    }
    let status = provider_discussion_status(current.status, request.status, request.cleanup);
    let reason = provider_stop_reason(current.status, request.status, request.cleanup);
    let terminal_message = provider_terminal_message(&request, status)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![current.id, next, request.event_id, "terminal", terminal_message],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status=?,sequence=?,output_text=?,stop_reason=?,dispatch_state=CASE WHEN ? THEN 'delivered' ELSE dispatch_state END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status IN ('queued','running','stopping') AND sequence=?",
        params![
            status.as_str(),
            next,
            request.assistant_text,
            reason,
            delivered,
            current.id,
            current.owner.project_id,
            current.owner.operation_namespace,
            sequence
        ],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The discussion changed before its provider result was committed.",
        ));
    }
    let message = if status == DiscussionRunStatus::Completed {
        request.assistant_text.clone()
    } else if request.assistant_text.is_empty() {
        terminal_message.clone()
    } else {
        format!("{}\n\n[{}]", request.assistant_text, terminal_message)
    };
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![
            new_id(),
            current.thread_id,
            current.id,
            DiscussionMessageRole::Assistant.as_str(),
            message,
            current.packet_id
        ],
    )?;
    let binding_json = serde_json::to_string(binding)?;
    let usage_json = request
        .usage
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let delivery_json = request
        .delivery
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let app_server_json = request
        .app_server
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    tx.execute(
        "INSERT INTO provider_results(run_id,packet_id,terminal_event_id,expected_sequence,binding_json,assistant_text,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,effective_identity,reported_model,delivery_json,app_server_delivery_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            current.id,
            current.packet_id,
            request.event_id,
            expected,
            binding_json,
            request.assistant_text,
            request.status.as_str(),
            i64::try_from(confirmed_stdin_bytes).map_err(|_| {
                CoreError::new("InvalidRequest", "The provider stdin byte count is too large.")
            })?,
            usage_json,
            request.cleanup.as_str(),
            request.error,
            request.effective_identity,
            request.reported_model,
            delivery_json,
            app_server_json,
        ],
    )?;
    if status == DiscussionRunStatus::Completed {
        proposals::retain_candidates_at(&tx, &current, &request.assistant_text)?;
    }
    let result = read_provider_result(&tx, &current.id, &current.packet_id)?.ok_or_else(|| {
        CoreError::new(
            "PersistenceUnavailable",
            "The provider receipt could not be read.",
        )
    })?;
    let run = read_run(&tx, &current.id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(ProviderDiscussionSettlement {
        run,
        provider_result: result,
    })
}

/// The open path calls this once after migration. Active jobs are not
/// replayed; they become inspectable interrupted history.
pub fn recover_interrupted_discussions(host: &mut impl StoryHost) -> CoreResult<u32> {
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut statement = tx.prepare(
        "SELECT id FROM discussion_runs WHERE status IN ('queued','running','stopping') ORDER BY created_at,id",
    )?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for id in &ids {
        let current = read_run(&tx, id)?;
        // A reopened project must seal the whole bounded lookup chain in
        // the same transaction as the discussion run. Otherwise a
        // claimed invocation (or a prepared child) can look dispatchable
        // after recovery even though its owning run is interrupted.
        discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
        seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Interrupted,
            "project_reopened",
            &format!("system-interrupted-{}", current.id),
            "The project was reopened before this discussion produced a complete response.",
        )?;
    }
    tx.commit().map_err(CoreError::uncertain)?;
    u32::try_from(ids.len())
        .map_err(|_| CoreError::new("InvalidProject", "Too many discussion jobs."))
}

/// Interrupt one exact active run after its local worker has already
/// been fenced. This is intentionally owner/ID based rather than a
/// rediscovery sweep, so a later run cannot be settled by an earlier
/// close census. `seal_run` retains output text, output events, and any
/// provider receipt already committed for the run.
pub fn interrupt_discussion(
    host: &mut impl StoryHost,
    access: ProjectAccess,
    run_id: String,
) -> CoreResult<DiscussionRun> {
    host.check_access(&access)?;
    check_id(&run_id)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &run_id)?;
    if current.owner.project_id != access.project_id
        || current.owner.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "This run belongs to another project session.",
        ));
    }
    let run = match current.status {
        DiscussionRunStatus::Queued
        | DiscussionRunStatus::Running
        | DiscussionRunStatus::Stopping => {
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Interrupted,
                "project_close_cleanup",
                &format!("system-interrupted-{}", current.id),
                STOP_UNRESOLVED_MESSAGE,
            )?
        }
        DiscussionRunStatus::Completed
        | DiscussionRunStatus::Stopped
        | DiscussionRunStatus::Failed
        | DiscussionRunStatus::Interrupted => current,
    };
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(run)
}

pub fn read_discussion(
    host: &impl StoryHost,
    access: ProjectAccess,
    document_id: String,
) -> CoreResult<DiscussionView> {
    host.check_access(&access)?;
    check_id(&document_id)?;
    let db = host.db()?;
    let thread_id: Option<String> = db
        .query_row(
            "SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?",
            params![access.project_id, access.operation_namespace, document_id],
            |row| row.get(0),
        )
        .optional()?;
    // Copies retain readable history. Run mutations still require the
    // current project and operation namespace, never a historical owner.
    read_document(db, &document_id)?;
    let mut runs_statement = db.prepare("SELECT dr.id FROM discussion_runs dr JOIN discussion_threads dt ON dt.id=dr.thread_id WHERE dt.document_id=? ORDER BY dr.rowid")?;
    let run_ids = runs_statement
        .query_map([&document_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let runs = run_ids
        .iter()
        .map(|id| read_run(db, id))
        .collect::<CoreResult<Vec<_>>>()?;
    let mut message_statement = db.prepare(
        "SELECT dm.id FROM discussion_messages dm JOIN discussion_threads dt ON dt.id=dm.thread_id WHERE dt.document_id=? ORDER BY dm.rowid",
    )?;
    let message_ids = message_statement
        .query_map([&document_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let messages = message_ids
        .iter()
        .map(|id| read_message(db, id))
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(DiscussionView {
        document_id: document_id.clone(),
        thread_id,
        messages,
        runs,
        draft: read_draft(db, &access, &document_id)?,
    })
}

pub fn save_discussion_draft(
    host: &mut impl StoryHost,
    request: SaveDiscussionDraft,
) -> CoreResult<DiscussionDraft> {
    host.check_access(&request.access)?;
    check_id(&request.document_id)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_version)?;
    if request.text.len() > MAX_OUTPUT_BYTES
        || request.pinned_document_ids.len() > MAX_PINNED_DOCUMENTS
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The discussion draft is too large or has too many pinned documents.",
        ));
    }
    for id in &request.pinned_document_ids {
        check_id(id)?;
    }
    if let Some(scope) = &request.scope
        && scope.quote.len() > MAX_SCOPE_QUOTE_BYTES
    {
        return Err(CoreError::new(
            "InvalidScope",
            "The discussion draft scope quote is too large.",
        ));
    }
    validate_feedback_basis(request.intent, request.basis, request.scope.as_ref())?;
    validate_lookup_request(request.intent, request.basis, request.lookup.as_ref())?;
    validate_safe_brief_draft(request.safe_brief.as_ref())?;
    let payload_hash = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<(String,String)> = tx.query_row("SELECT payload_hash,result_json FROM discussion_draft_receipts WHERE project_id=? AND operation_namespace=? AND operation_id=?", params![request.access.project_id,request.access.operation_namespace,request.operation_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    if let Some((previous, result)) = existing {
        if previous != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This draft operation was already used for different content.",
            ));
        }
        let draft: DiscussionDraft = serde_json::from_str(&result)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(draft);
    }
    read_document(&tx, &request.document_id)?;
    validate_previous_run(
        &tx,
        request.previous_run_id.as_deref(),
        &request.access,
        &request.document_id,
    )?;
    if let Some(previous) = request.previous_run_id.as_deref() {
        let previous_run = read_run(&tx, previous)?;
        if previous_run.intent != request.intent {
            return Err(CoreError::new(
                "RetryRequestChanged",
                "The saved retry intent changed. Start a new discussion instead.",
            ));
        }
    }
    let current: Option<i64> = tx.query_row("SELECT version FROM discussion_drafts WHERE project_id=? AND operation_namespace=? AND document_id=?", params![request.access.project_id,request.access.operation_namespace,request.document_id], |row| row.get(0)).optional()?;
    let current = current.unwrap_or(0);
    if current != expected {
        return Err(CoreError::new(
            "DraftVersionConflict",
            "The composer draft changed; reload it before saving.",
        ));
    }
    let next = current
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The draft version is exhausted."))?;
    tx.execute("INSERT INTO discussion_drafts(project_id,operation_namespace,document_id,version,text,intent,scope_json,pinned_document_ids_json,previous_run_id,safe_brief_json,basis,lookup_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO UPDATE SET version=excluded.version,text=excluded.text,intent=excluded.intent,scope_json=excluded.scope_json,pinned_document_ids_json=excluded.pinned_document_ids_json,previous_run_id=excluded.previous_run_id,safe_brief_json=excluded.safe_brief_json,basis=excluded.basis,lookup_json=excluded.lookup_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![request.access.project_id,request.access.operation_namespace,request.document_id,next,request.text,request.intent.as_str(),request.scope.as_ref().map(serde_json::to_string).transpose()?,serde_json::to_string(&request.pinned_document_ids)?,request.previous_run_id,request.safe_brief.as_ref().map(serde_json::to_string).transpose()?,request.basis.map(basis_label),request.lookup.as_ref().map(serde_json::to_string).transpose()?])?;
    let draft = read_draft(&tx, &request.access, &request.document_id)?.ok_or_else(|| {
        CoreError::new(
            "PersistenceUnavailable",
            "The saved composer draft could not be read.",
        )
    })?;
    tx.execute("INSERT INTO discussion_draft_receipts(project_id,operation_namespace,operation_id,document_id,expected_version,payload_hash,result_json) VALUES(?,?,?,?,?,?,?)", params![request.access.project_id,request.access.operation_namespace,request.operation_id,request.document_id,expected,payload_hash,serde_json::to_string(&draft)?])?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(draft)
}

fn ensure_thread(tx: &Connection, access: &ProjectAccess, document_id: &str) -> CoreResult<String> {
    tx.execute("INSERT INTO discussion_threads(id,project_id,operation_namespace,document_id) VALUES(?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO NOTHING", params![new_id(), access.project_id, access.operation_namespace, document_id])?;
    tx.query_row("SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?", params![access.project_id,access.operation_namespace,document_id], |row| row.get(0)).map_err(CoreError::from)
}

fn validate_previous_run(
    tx: &Connection,
    previous: Option<&str>,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let row: Option<(String,String,String,String)> = tx.query_row("SELECT project_id,operation_namespace,target_document_id,status FROM discussion_runs WHERE id=?", [previous], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    let Some((project, namespace, target, status)) = row else {
        return Err(CoreError::new(
            "PreviousRunNotFound",
            "The previous discussion run is not available.",
        ));
    };
    if project != access.project_id
        || namespace != access.operation_namespace
        || target != document_id
    {
        return Err(CoreError::new(
            "PreviousRunMismatch",
            "A previous discussion must belong to the same project and target document.",
        ));
    }
    if !DiscussionRunStatus::parse(&status)?.terminal() {
        return Err(CoreError::new(
            "PreviousRunActive",
            "The previous discussion must be terminal before it can be used as context.",
        ));
    }
    Ok(())
}

pub fn read_start(db: &Connection, run_id: &str) -> CoreResult<DiscussionStart> {
    let run = read_run(db, run_id)?;
    let user_message = db.query_row("SELECT id FROM discussion_messages WHERE run_id=? AND role='user' ORDER BY created_at,id LIMIT 1", [run_id], |row| row.get::<_,String>(0)).map_err(CoreError::from).and_then(|id| read_message(db, &id))?;
    // Safe-brief starts are explicit author actions whose receipt must remain
    // replayable after a later policy bump. Ordinary discussion receipts keep
    // the existing current-policy read boundary.
    let retained = context_packets::validated_packet_record(db, &run.packet_id)?;
    let packet = if retained.receipt.safe_brief.is_some() {
        retained
    } else {
        context_packets::read_context_packet_at(
            db,
            &ProjectAccess {
                project_id: run.owner.project_id.clone(),
                operation_namespace: run.owner.operation_namespace.clone(),
                session: String::new(),
                writer_lease: String::new(),
            },
            &run.packet_id,
        )?
    };
    Ok(DiscussionStart {
        thread_id: run.thread_id.clone(),
        run,
        user_message,
        packet,
    })
}

pub fn read_run(db: &Connection, run_id: &str) -> CoreResult<DiscussionRun> {
    check_id(run_id)?;
    type RunRow = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
        String,
        i64,
        String,
        Option<String>,
        String,
        String,
    );
    let row: RunRow = db.query_row("SELECT id,thread_id,project_id,operation_namespace,operation_id,payload_hash,target_document_id,target_version,target_body_hash,packet_id,previous_run_id,status,dispatch_state,sequence,output_text,stop_reason,created_at,updated_at FROM discussion_runs WHERE id=?", [run_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?,row.get(11)?,row.get(12)?,row.get(13)?,row.get(14)?,row.get(15)?,row.get(16)?,row.get(17)?))).optional()?.ok_or_else(|| CoreError::new("DiscussionRunNotFound", "The discussion run is not available."))?;
    let (intent, basis) = intent_for_packet(db, &row.9)?;
    let packet = context_packets::validated_packet_record(db, &row.9)?;
    let provider_binding = packet.options.provider_binding;
    let provider_result = read_provider_result(db, &row.0, &row.9)?;
    let lookup = discussion_lookup::read_summary(db, &row.0)?;
    if let Some(result) = &provider_result {
        let packet_binding = provider_binding.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A provider result exists for a packet without a provider binding.",
            )
        })?;
        if &result.binding != packet_binding || result.packet_id != row.9 {
            return Err(CoreError::new(
                "InvalidProject",
                "The saved provider result does not match its immutable packet binding.",
            ));
        }
    }
    Ok(DiscussionRun {
        id: row.0.clone(),
        thread_id: row.1,
        owner: RunOwner {
            project_id: row.2,
            operation_namespace: row.3,
            run_id: row.0,
        },
        operation_id: row.4,
        intent,
        basis,
        payload_hash: row.5,
        target: Head {
            document_id: row.6,
            version: row.7.to_string(),
            body_hash: row.8,
        },
        packet_id: row.9,
        provider_binding,
        provider_result,
        lookup,
        previous_run_id: row.10,
        status: DiscussionRunStatus::parse(&row.11)?,
        dispatch_state: row.12,
        sequence: row.13.to_string(),
        output_text: row.14,
        stop_reason: row.15,
        created_at: row.16,
        updated_at: row.17,
    })
}

type ProviderResultRow = (
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    i64,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
);

fn read_provider_result(
    db: &Connection,
    run_id: &str,
    packet_id: &str,
) -> CoreResult<Option<ProviderResult>> {
    let row: Option<ProviderResultRow> = db
        .query_row(
            "SELECT run_id,packet_id,terminal_event_id,expected_sequence,binding_json,assistant_text,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,effective_identity,reported_model,created_at,delivery_json,app_server_delivery_json FROM provider_results WHERE run_id=?",
            [run_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                ))
            },
        )
        .optional()?;
    let Some((
        saved_run_id,
        saved_packet_id,
        event_id,
        expected_sequence,
        binding_json,
        assistant_text,
        outcome,
        confirmed_stdin_bytes,
        usage_json,
        cleanup,
        error,
        effective_identity,
        reported_model,
        created_at,
        delivery_json,
        app_server_json,
    )) = row
    else {
        return Ok(None);
    };
    if saved_run_id != run_id || saved_packet_id != packet_id {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved provider result belongs to a different run or packet.",
        ));
    }
    let binding: ProviderBinding = serde_json::from_str(&binding_json)?;
    binding
        .validate()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    let confirmed_stdin_bytes = u64::try_from(confirmed_stdin_bytes).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved provider stdin byte count is negative.",
        )
    })?;
    let expected_sequence = parse_stored_version(expected_sequence)?;
    let usage = usage_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let delivery = delivery_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let app_server = app_server_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let status = ProviderOutcomeStatus::parse(&outcome)?;
    validate_reported_model(&binding, status, reported_model.as_deref(), true)?;
    let cleanup = ProviderCleanup::parse(&cleanup)?;
    let input_limit = binding
        .input_limit()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    if wns_providers::codex_app_server::is_app_server(&binding) {
        if confirmed_stdin_bytes != 0 || delivery.is_some() {
            return Err(CoreError::new(
                "InvalidProject",
                "App-server results cannot claim exec or HTTP delivery.",
            ));
        }
        let packet = context_packets::validated_packet_record(db, packet_id)?;
        app_server::validate_delivery(db, run_id, &packet, app_server.as_ref(), status, cleanup)?;
    } else if app_server.is_some() {
        return Err(CoreError::new(
            "InvalidProject",
            "Only app-server results can retain app-server delivery.",
        ));
    } else if binding.is_http() {
        if confirmed_stdin_bytes != 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "An HTTP provider result must retain a zero Codex stdin count.",
            ));
        }
        let packet = context_packets::validated_packet_record(db, packet_id)?;
        validate_stored_http_delivery(&packet, &binding, delivery.as_ref())?;
        if status == ProviderOutcomeStatus::Completed
            && !matches!(
                delivery.as_ref().map(|receipt| receipt.submission),
                Some(HttpDeliverySubmission::ResponseReceived)
            )
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed HTTP provider result needs complete response evidence.",
            ));
        }
    } else if delivery.is_some()
        || assistant_text.len() > CODEX_OUTPUT_LIMIT_BYTES
        || confirmed_stdin_bytes > input_limit as u64
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved Codex provider result has invalid transport evidence.",
        ));
    }
    let output_limit = binding
        .output_limit()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    if assistant_text.len() > output_limit
        || (status == ProviderOutcomeStatus::Completed && assistant_text.is_empty())
        || (status == ProviderOutcomeStatus::Completed && error.is_some())
        || error.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control)
        })
        || effective_identity.is_some()
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved provider result violates the bounded terminal contract.",
        ));
    }
    Ok(Some(ProviderResult {
        run_id: saved_run_id,
        packet_id: saved_packet_id,
        event_id,
        expected_sequence,
        assistant_text,
        binding,
        status,
        confirmed_stdin_bytes: confirmed_stdin_bytes.to_string(),
        usage,
        cleanup,
        error,
        effective_identity,
        reported_model,
        created_at,
        delivery,
        app_server,
    }))
}

/// Validate immutable provider receipts when opening or transferring a
/// project. This checks the receipt's local fences and packet binding; it does
/// not claim that the external process itself can be reconstructed.
pub fn validate_provider_results(db: &Connection) -> CoreResult<()> {
    app_server::validate_dispatches(db)?;
    let mut statement = db.prepare("SELECT run_id,packet_id FROM provider_results")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (run_id, packet_id) in rows {
        let result = read_provider_result(db, &run_id, &packet_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A provider result disappeared during validation.",
            )
        })?;
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        if packet.options.provider_binding.as_ref() != Some(&result.binding) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result does not match its immutable packet binding.",
            ));
        }
        let input_len = serialized_input(&packet.messages, &packet.options)
            .map_err(packet_error)?
            .len() as u64;
        let confirmed = parse_decimal_u64(&result.confirmed_stdin_bytes)?;
        if wns_providers::codex_app_server::is_app_server(&result.binding) {
            if confirmed != 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "App-server receipts cannot claim exec stdin delivery.",
                ));
            }
            app_server::validate_delivery(
                db,
                &run_id,
                &packet,
                result.app_server.as_ref(),
                result.status,
                result.cleanup,
            )?;
        } else if result.binding.is_http() {
            if confirmed != 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An HTTP provider result must retain a zero Codex stdin count.",
                ));
            }
            validate_stored_http_delivery(&packet, &result.binding, result.delivery.as_ref())?;
        } else if confirmed > input_len
            || (result.status == ProviderOutcomeStatus::Completed && confirmed != input_len)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has an invalid frozen-packet byte count.",
            ));
        }
        let (status, sequence, output_text): (String, i64, String) = db.query_row(
            "SELECT status,sequence,output_text FROM discussion_runs WHERE id=? AND packet_id=?",
            params![run_id, packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let status = DiscussionRunStatus::parse(&status)?;
        if status.active() || output_text != result.assistant_text {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result does not match its terminal discussion run.",
            ));
        }
        let expected_sequence = parse_version(&result.expected_sequence)?;
        if expected_sequence.checked_add(1) != Some(sequence) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has an invalid terminal sequence.",
            ));
        }
        let event: Option<(String, i64)> = db
            .query_row(
                "SELECT kind,sequence FROM discussion_output_events WHERE run_id=? AND event_id=?",
                params![run_id, result.event_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if event != Some(("terminal".to_owned(), sequence)) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has no matching immutable terminal event.",
            ));
        }
        let allowed = if result.cleanup == ProviderCleanup::Unresolved {
            status == DiscussionRunStatus::Interrupted
        } else {
            match result.status {
                ProviderOutcomeStatus::Completed => {
                    matches!(
                        status,
                        DiscussionRunStatus::Completed | DiscussionRunStatus::Stopped
                    )
                }
                ProviderOutcomeStatus::Stopped => status == DiscussionRunStatus::Stopped,
                ProviderOutcomeStatus::TimedOut
                | ProviderOutcomeStatus::OutputLimit
                | ProviderOutcomeStatus::Failed => {
                    matches!(
                        status,
                        DiscussionRunStatus::Failed | DiscussionRunStatus::Stopped
                    )
                }
            }
        };
        if !allowed {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result outcome does not match its terminal run status.",
            ));
        }
    }
    let mut live_statement = db.prepare(
        "SELECT dr.id,dr.packet_id,dr.status
         FROM discussion_runs dr
         ORDER BY dr.id",
    )?;
    let live_runs = live_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (run_id, packet_id, status) in live_runs {
        if status != DiscussionRunStatus::Completed.as_str() {
            continue;
        }
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        if packet.options.provider_binding.is_some() {
            let receipt: Option<String> = db
                .query_row(
                    "SELECT run_id FROM provider_results WHERE run_id=?",
                    [&run_id],
                    |row| row.get(0),
                )
                .optional()?;
            if receipt.is_none() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A completed live discussion is missing its immutable provider result.",
                ));
            }
        }
    }
    Ok(())
}

fn read_message(db: &Connection, message_id: &str) -> CoreResult<DiscussionMessage> {
    type MessageRow = (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    );
    let row: MessageRow = db.query_row("SELECT id,thread_id,run_id,role,content,scope_json,packet_id,created_at FROM discussion_messages WHERE id=?", [message_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?))).optional()?.ok_or_else(|| CoreError::new("DiscussionMessageNotFound", "The discussion message is not available."))?;
    Ok(DiscussionMessage {
        id: row.0,
        thread_id: row.1,
        run_id: row.2,
        role: DiscussionMessageRole::parse(&row.3)?,
        content: row.4,
        scope: row.5.map(|json| serde_json::from_str(&json)).transpose()?,
        packet_id: row.6,
        created_at: row.7,
    })
}

fn read_draft(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<DiscussionDraft>> {
    type DraftRow = (
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let row: Option<DraftRow> = db
        .query_row(
            "SELECT document_id,version,text,intent,scope_json,pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,basis,lookup_json FROM discussion_drafts WHERE project_id=? AND operation_namespace=? AND document_id=?",
            params![access.project_id, access.operation_namespace, document_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        document_id,
        version,
        text,
        intent,
        scope_json,
        pins_json,
        updated_at,
        previous_run_id,
        safe_brief_json,
        basis,
        lookup_json,
    )) = row
    else {
        return Ok(None);
    };
    let intent = FeedbackIntent::parse(&intent)?;
    let basis = basis
        .map(|label| serde_json::from_value::<BasisKind>(Value::String(label)))
        .transpose()?;
    let scope: Option<DiscussionScopeInput> = scope_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    validate_feedback_basis(intent, basis, scope.as_ref())?;
    Ok(Some(DiscussionDraft {
        document_id,
        version: parse_stored_version(version)?,
        text,
        intent,
        basis,
        scope,
        pinned_document_ids: serde_json::from_str(&pins_json)?,
        safe_brief: safe_brief_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        previous_run_id,
        updated_at,
        lookup: lookup_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
    }))
}

fn existing_event(
    db: &Connection,
    run_id: &str,
    event_id: &str,
) -> CoreResult<Option<(String, String, i64)>> {
    db.query_row(
        "SELECT kind,chunk,sequence FROM discussion_output_events WHERE run_id=? AND event_id=?",
        params![run_id, event_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

fn validate_owner(run: &DiscussionRun, owner: &RunOwner) -> CoreResult<()> {
    if &run.owner != owner {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The output owner does not match the persisted run.",
        ));
    }
    Ok(())
}

fn validate_runtime_owner(info: &ProjectInfo, owner: &RunOwner) -> CoreResult<()> {
    if owner.project_id != info.project_id || owner.operation_namespace != info.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The discussion run belongs to another project or recovered project identity.",
        ));
    }
    Ok(())
}

fn ensure_run_started(status: DiscussionRunStatus) -> CoreResult<()> {
    match status {
        DiscussionRunStatus::Running => Ok(()),
        DiscussionRunStatus::Queued => Err(CoreError::new(
            "RunNotStarted",
            "Claim the queued discussion before accepting provider output.",
        )),
        DiscussionRunStatus::Stopping => Err(CoreError::new(
            "RunStopping",
            "The discussion is stopping and no further output is accepted.",
        )),
        _ => Err(CoreError::new(
            "RunSealed",
            "This discussion run is already sealed.",
        )),
    }
}

fn seal_run(
    tx: &Connection,
    current: &DiscussionRun,
    status: DiscussionRunStatus,
    reason: &str,
    event_id: &str,
    message: &str,
) -> CoreResult<DiscussionRun> {
    if !current.status.active() || !status.terminal() {
        return Err(CoreError::new(
            "RunSealed",
            "This discussion run is already sealed.",
        ));
    }
    check_id(event_id)?;
    if reason.is_empty() || message.is_empty() || message.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A terminal discussion event must include a bounded reason and message.",
        ));
    }
    let sequence = parse_version(&current.sequence)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![current.id, next, event_id, "terminal", message],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status=?,sequence=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status IN ('queued','running','stopping') AND sequence=?",
        params![status.as_str(), next, reason, current.id, current.owner.project_id, current.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The discussion changed before its terminal state was committed.",
        ));
    }
    let content = if current.output_text.is_empty() {
        message.to_owned()
    } else {
        format!("{}\n\n[{}]", current.output_text, message)
    };
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![new_id(), current.thread_id, current.id, DiscussionMessageRole::Assistant.as_str(), content, current.packet_id],
    )?;
    read_run(tx, &current.id)
}

fn append_text(existing: &str, chunk: &str, limit: usize) -> CoreResult<String> {
    if existing
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > limit)
    {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    let mut result = String::with_capacity(existing.len() + chunk.len());
    result.push_str(existing);
    result.push_str(chunk);
    Ok(result)
}

mod lookup;
mod packet;
mod validation;
use lookup::*;
use packet::*;
use validation::*;

// These were reachable through `discussions` before the split; a glob import is
// private, so the ones that were `pub` are named here.
pub use lookup::{advance_lookup, claim_lookup_invocation, halt_lookup, settle_lookup_invocation};
pub use validation::validate_start;
