//! The validators a discussion request passes before it is allowed to touch
//! durable state: the feedback basis, the start request, the safe brief and
//! its project origin, and the provider report that settles a run.
//!
//! Split out of `discussions.rs`, which §3.4 measured at 4,605 lines and named
//! four concerns for. The items are `pub(super)` because a child module's
//! private items are not visible to the parent that calls them.

use super::*;

pub(super) fn validate_output_event(
    owner: &RunOwner,
    event_id: &str,
    text: &str,
) -> CoreResult<()> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    check_id(event_id)?;
    if text.is_empty() || text.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion output event must be nonempty and at most 128 KiB.",
        ));
    }
    Ok(())
}

pub(super) fn validate_finish_request(
    owner: &RunOwner,
    event_id: &str,
    text: &str,
) -> CoreResult<()> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    check_id(event_id)?;
    if text.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed discussion must include assistant output.",
        ));
    }
    if text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    Ok(())
}

pub(super) fn validate_settlement_request(request: &DiscussionStopSettled) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    if request.assistant_text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The stopped discussion output exceeds the durable limit.",
        ));
    }
    Ok(())
}

pub(super) fn parse_decimal_u64(value: &str) -> CoreResult<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Provider counters must be canonical nonnegative decimal strings.",
        ));
    }
    value.parse::<u64>().map_err(|_| {
        CoreError::new(
            "InvalidRequest",
            "Provider counters exceed the supported range.",
        )
    })
}

pub(super) fn validate_provider_report_shape(request: &ProviderTerminalReport) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    request
        .binding
        .validate()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    let bytes = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
    if (request.binding.is_http()
        || wns_providers::codex_app_server::is_app_server(&request.binding))
        && bytes != 0
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "An HTTP provider report must retain a zero Codex stdin count.",
        ));
    }
    if !request.binding.is_http() && bytes > CODEX_INPUT_LIMIT_BYTES as u64 {
        return Err(CoreError::new(
            "InputTooLarge",
            "The provider stdin exceeds the application byte cap.",
        ));
    }
    let output_limit = request
        .binding
        .output_limit()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    if request.assistant_text.len() > output_limit {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The provider output exceeds the application byte cap.",
        ));
    }
    if request.binding.is_http() {
        validate_http_delivery_shape(request.delivery.as_ref(), request.status)?;
    } else if request.delivery.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A Codex provider report cannot contain HTTP delivery evidence.",
        ));
    }
    if wns_providers::codex_app_server::is_app_server(&request.binding) {
        let receipt = request.app_server.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidAppServerDelivery",
                "App-server delivery evidence is required.",
            )
        })?;
        receipt.validate()?;
    } else if request.app_server.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "App-server evidence requires an app-server binding.",
        ));
    }
    if let Some(error) = &request.error
        && (error.is_empty() || error.len() > 4096 || error.chars().any(char::is_control))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The provider error must be sanitized and at most 4 KiB.",
        ));
    }
    if request.effective_identity.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "The current provider boundary cannot confirm an effective identity.",
        ));
    }
    validate_reported_model(
        &request.binding,
        request.status,
        request.reported_model.as_deref(),
        false,
    )?;
    Ok(())
}

/// Claude's stream may claim a terminal model identity.  Keep that claim
/// separate from the immutable requested binding and accept it only for the
/// bounded Claude profile.  A completed Claude result must agree exactly;
/// failed results may retain a bounded, known Claude model for diagnosis.
pub(super) fn validate_reported_model(
    binding: &ProviderBinding,
    status: ProviderOutcomeStatus,
    reported_model: Option<&str>,
    persisted: bool,
) -> CoreResult<()> {
    let Some(reported_model) = reported_model else {
        if binding.is_claude() && status == ProviderOutcomeStatus::Completed {
            return Err(CoreError::new(
                if persisted {
                    "InvalidProject"
                } else {
                    "InvalidRequest"
                },
                "A completed Claude result must retain its reported model identity.",
            ));
        }
        return Ok(());
    };
    if !binding.is_claude()
        || !wns_providers::claude_profile::valid_reported_model_id(reported_model)
    {
        return Err(CoreError::new(
            if persisted {
                "InvalidProject"
            } else {
                "InvalidRequest"
            },
            "Only a bounded Claude result may retain a known reported model identity.",
        ));
    }
    if status == ProviderOutcomeStatus::Completed && reported_model != binding.model_id {
        return Err(CoreError::new(
            if persisted {
                "InvalidProject"
            } else {
                "InvalidRequest"
            },
            "A completed Claude result reported a different model than the request binding.",
        ));
    }
    Ok(())
}

pub(super) fn validate_http_delivery_shape(
    delivery: Option<&ProviderDeliveryReceipt>,
    status: ProviderOutcomeStatus,
) -> CoreResult<()> {
    let Some(delivery) = delivery else {
        return Err(CoreError::new(
            "InvalidRequest",
            "An OpenAI-compatible provider report needs HTTP delivery evidence.",
        ));
    };
    if delivery.body_hash.len() != 64
        || !delivery
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP request body hash is invalid.",
        ));
    }
    let body_bytes = parse_decimal_u64(&delivery.body_bytes)?;
    if body_bytes == 0 || body_bytes > wns_context::packet::HTTP_INPUT_LIMIT_BYTES as u64 {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP request body byte count is outside the application limit.",
        ));
    }
    if status == ProviderOutcomeStatus::Completed
        && delivery.submission != HttpDeliverySubmission::ResponseReceived
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed HTTP provider result needs a fully received response.",
        ));
    }
    Ok(())
}

pub(super) fn validate_http_delivery(
    packet: &CompiledPacket,
    request: &ProviderTerminalReport,
) -> CoreResult<()> {
    validate_http_delivery_shape(request.delivery.as_ref(), request.status)?;
    let delivery = request.delivery.as_ref().expect("validated above");
    let prepared = prepare_http_request(&packet.messages, &packet.options)?;
    if delivery.body_hash != prepared.body_hash
        || delivery.body_bytes != prepared.body_bytes
        || request.status == ProviderOutcomeStatus::Completed
            && delivery.submission != HttpDeliverySubmission::ResponseReceived
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The HTTP delivery receipt does not match the immutable request body.",
        ));
    }
    Ok(())
}

pub(super) fn validate_stored_http_delivery(
    packet: &CompiledPacket,
    binding: &ProviderBinding,
    delivery: Option<&ProviderDeliveryReceipt>,
) -> CoreResult<()> {
    let Some(delivery) = delivery else {
        return Err(CoreError::new(
            "InvalidProject",
            "An HTTP provider result is missing its delivery receipt.",
        ));
    };
    validate_http_delivery_shape(Some(delivery), ProviderOutcomeStatus::Failed)?;
    if packet.options.provider_binding.as_ref() != Some(binding) {
        return Err(CoreError::new(
            "InvalidProject",
            "The HTTP delivery receipt does not match its packet binding.",
        ));
    }
    let prepared = prepare_http_request(&packet.messages, &packet.options)?;
    if delivery.body_hash != prepared.body_hash || delivery.body_bytes != prepared.body_bytes {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved HTTP delivery receipt does not match the immutable request body.",
        ));
    }
    Ok(())
}

pub(super) fn provider_result_matches_report(
    saved: &ProviderResult,
    report: &ProviderTerminalReport,
) -> bool {
    saved.run_id == report.owner.run_id
        && saved.event_id == report.event_id
        && saved.expected_sequence == report.expected_sequence
        && saved.assistant_text == report.assistant_text
        && saved.binding == report.binding
        && saved.status == report.status
        && saved.confirmed_stdin_bytes == report.confirmed_stdin_bytes
        && saved.app_server == report.app_server
        && saved.usage == report.usage
        && saved.cleanup == report.cleanup
        && saved.error == report.error
        && saved.effective_identity == report.effective_identity
        && saved.reported_model == report.reported_model
        && saved.delivery == report.delivery
}

pub(super) fn provider_discussion_status(
    current: DiscussionRunStatus,
    outcome: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> DiscussionRunStatus {
    if cleanup == ProviderCleanup::Unresolved {
        DiscussionRunStatus::Interrupted
    } else if current == DiscussionRunStatus::Stopping || outcome == ProviderOutcomeStatus::Stopped
    {
        DiscussionRunStatus::Stopped
    } else if outcome == ProviderOutcomeStatus::Completed {
        DiscussionRunStatus::Completed
    } else {
        DiscussionRunStatus::Failed
    }
}

pub(super) fn provider_stop_reason(
    current: DiscussionRunStatus,
    outcome: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> Option<&'static str> {
    if cleanup == ProviderCleanup::Unresolved {
        Some("provider_cleanup_unresolved")
    } else if current == DiscussionRunStatus::Stopping {
        Some("author_stopped")
    } else {
        match outcome {
            ProviderOutcomeStatus::Completed => None,
            ProviderOutcomeStatus::Stopped => Some("provider_stopped"),
            ProviderOutcomeStatus::TimedOut => Some("provider_timed_out"),
            ProviderOutcomeStatus::OutputLimit => Some("provider_output_limit"),
            ProviderOutcomeStatus::Failed => Some("provider_failed"),
        }
    }
}

pub(super) fn provider_terminal_message(
    request: &ProviderTerminalReport,
    status: DiscussionRunStatus,
) -> CoreResult<String> {
    if status == DiscussionRunStatus::Completed {
        return Ok(request.assistant_text.clone());
    }
    let message = if let Some(error) = request.error.as_deref() {
        format!("Provider request failed: {error}")
    } else {
        match status {
            DiscussionRunStatus::Stopped => STOP_SETTLED_MESSAGE.to_owned(),
            DiscussionRunStatus::Interrupted => STOP_UNRESOLVED_MESSAGE.to_owned(),
            DiscussionRunStatus::Failed => match request.status {
                ProviderOutcomeStatus::TimedOut => "The provider request timed out.".to_owned(),
                ProviderOutcomeStatus::OutputLimit => {
                    "The provider output reached the application limit.".to_owned()
                }
                _ => "The provider request failed.".to_owned(),
            },
            _ => "The provider request ended without a completed response.".to_owned(),
        }
    };
    if message.is_empty() || message.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "The provider terminal message is too large.",
        ));
    }
    Ok(message)
}

pub(super) fn validate_final_output(
    current: &str,
    final_text: &str,
    allow_empty: bool,
) -> CoreResult<()> {
    if !allow_empty && final_text.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed discussion must include assistant output.",
        ));
    }
    if final_text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    if !final_text.starts_with(current) {
        return Err(CoreError::new(
            "OutputConflict",
            "A terminal discussion result cannot replace persisted output.",
        ));
    }
    Ok(())
}

pub(super) fn validate_feedback_basis(
    intent: FeedbackIntent,
    basis: Option<BasisKind>,
    scope: Option<&DiscussionScopeInput>,
) -> CoreResult<()> {
    let valid = match intent {
        FeedbackIntent::Continue => {
            matches!(basis, Some(BasisKind::Working | BasisKind::Reviewed)) && scope.is_none()
        }
        FeedbackIntent::WorkshopExplore => basis.is_none() && scope.is_none(),
        _ => basis.is_none() && scope.is_none_or(|scope| scope.kind != ScopeKind::Append),
    };
    if !valid {
        return Err(CoreError::new(
            "InvalidContinuationBasis",
            "Continuation needs an explicit Working draft or Reviewed story basis and no passage selection.",
        ));
    }
    Ok(())
}

pub(super) fn basis_label(basis: BasisKind) -> &'static str {
    match basis {
        BasisKind::Working => "working",
        BasisKind::Reviewed => "reviewed",
        BasisKind::ExplicitHistory => "explicitHistory",
    }
}

pub fn validate_start(request: &StartDiscussion) -> CoreResult<()> {
    validate_feedback_basis(request.intent, request.basis, request.scope.as_ref())?;
    validate_lookup_request(request.intent, request.basis, request.lookup.as_ref())?;
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    if request.instruction.trim().is_empty() || request.instruction.len() > MAX_INSTRUCTION_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion instruction must be nonempty and at most 64 KiB.",
        ));
    }
    if request.pinned_document_ids.len() > MAX_PINNED_DOCUMENTS {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion may pin at most 64 source documents.",
        ));
    }
    for id in &request.pinned_document_ids {
        check_id(id)?;
    }
    if let Some(run_id) = &request.previous_run_id {
        check_id(run_id)?;
    }
    if let Some(binding) = &request.provider_binding {
        binding
            .validate()
            .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
        if binding.is_http() && request.lookup.is_some() {
            return Err(CoreError::new(
                "UnsupportedProviderFeature",
                "OpenAI-compatible HTTP discussions do not support bounded story lookup yet.",
            ));
        }
    }
    if let Some(scope) = &request.scope
        && (scope.quote.len() > MAX_SCOPE_QUOTE_BYTES
            || scope.source_body_hash.len() != 64
            || !scope
                .source_body_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(CoreError::new(
            "InvalidScope",
            "A discussion scope quote or source hash is invalid.",
        ));
    }
    validate_safe_brief_start(request)?;
    if request.intent == FeedbackIntent::WorkshopExplore {
        if request.previous_run_id.is_some()
            || request.lookup.is_some()
            || request.safe_brief.is_some()
        {
            return Err(CoreError::new(
                "InvalidWorkshop",
                "Workshop exploration cannot resume or use writing-only request features.",
            ));
        }
        metadata_from_instruction(&request.instruction)?;
    }
    Ok(())
}

pub(super) fn validate_safe_brief_shape(brief: &SafeBriefInput) -> CoreResult<()> {
    if brief.text.len() > MAX_SAFE_BRIEF_BYTES {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief must be at most 16 KiB.",
        ));
    }
    if let Some(origin) = brief.origin_message_id.as_deref()
        && (origin.is_empty()
            || origin.len() > 64
            || !origin
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message ID is invalid.",
        ));
    }
    if let Some(origin) = brief.project_origin.as_ref()
        && (origin.version != "project-conversation-brief.v1"
            || origin.project_id.is_empty()
            || origin.operation_namespace.is_empty()
            || origin.conversation_id.is_empty()
            || origin.message_id.is_empty()
            || origin.scope_hash.len() != 64
            || origin.text_hash.len() != 64
            || !origin
                .scope_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !origin
                .text_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief provenance is malformed.",
        ));
    }
    if let Some(origin_id) = brief.origin_message_id.as_deref()
        && brief
            .project_origin
            .as_ref()
            .is_some_and(|origin| origin.message_id != origin_id)
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The legacy and project brief origins do not identify the same message.",
        ));
    }
    Ok(())
}

pub(super) fn validate_safe_brief_draft(brief: Option<&SafeBriefInput>) -> CoreResult<()> {
    if let Some(brief) = brief {
        validate_safe_brief_shape(brief)?;
    }
    Ok(())
}

pub(super) fn validate_safe_brief_start(request: &StartDiscussion) -> CoreResult<()> {
    let Some(brief) = request.safe_brief.as_ref() else {
        return Ok(());
    };
    validate_safe_brief_shape(brief)?;
    if brief.text.trim().is_empty() || !brief.confirmed {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A writing brief must be nonempty and explicitly confirmed before use.",
        ));
    }
    if !matches!(
        request.intent,
        FeedbackIntent::ProposeEdits | FeedbackIntent::Continue
    ) || (request.intent == FeedbackIntent::ProposeEdits
        && request.scope.as_ref().is_none_or(|scope| {
            !matches!(
                scope.kind,
                ScopeKind::Passage | ScopeKind::Blocks | ScopeKind::WholeDocument
            )
        }))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A writing brief is available only for a passage revision or chapter continuation.",
        ));
    }
    Ok(())
}

pub(super) fn validate_safe_brief_origin(
    tx: &Connection,
    request: &StartDiscussion,
) -> CoreResult<()> {
    let Some(brief) = request.safe_brief.as_ref() else {
        return Ok(());
    };
    if let Some(origin) = brief.project_origin.as_ref() {
        validate_project_brief_origin(tx, request, brief, origin)?;
        // A project-conversation origin is intentionally cross-document: the
        // retained AuthorRoom message belongs to the project conversation's
        // control anchor, while this request targets an ordinary chapter.
        // The legacy origin path below requires the message's discussion
        // thread to be the chapter itself, so running it as well would reject
        // every valid project brief after approval. The project-origin
        // validator already authenticates the exact message, conversation,
        // project identity, target, scope, text, and AuthorRoom packet.
        return Ok(());
    }
    let Some(origin_id) = brief.origin_message_id.as_deref() else {
        return Ok(());
    };
    let row: Option<(String, String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT dt.project_id,dt.operation_namespace,dt.document_id,dm.role,dr.packet_id
             FROM discussion_messages dm
             JOIN discussion_threads dt ON dt.id=dm.thread_id
             LEFT JOIN discussion_runs dr ON dr.id=dm.run_id
             WHERE dm.id=?",
            [origin_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((project, namespace, document, role, packet_id)) = row else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message is not available.",
        ));
    };
    if project != request.access.project_id
        || namespace != request.access.operation_namespace
        || document != request.expected.document_id
        || DiscussionMessageRole::parse(&role)? == DiscussionMessageRole::Assistant
            && packet_id.is_none()
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message does not belong to this chapter discussion.",
        ));
    }
    let packet_id = packet_id.ok_or_else(|| {
        CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message has no readable discussion context.",
        )
    })?;
    let snapshot_id: Option<String> = tx
        .query_row(
            "SELECT snapshot_id FROM context_packets WHERE id=?",
            [&packet_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(snapshot_id) = snapshot_id else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message has no readable discussion context.",
        ));
    };
    let frozen = story_context::load_snapshot(tx, &request.access, &snapshot_id)?;
    if frozen.policy.audience != Audience::AuthorRoom
        || frozen.snapshot.target.document_id != request.expected.document_id
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message is not readable in the current AuthorRoom policy.",
        ));
    }
    Ok(())
}

pub(super) fn validate_project_brief_origin(
    tx: &Connection,
    request: &StartDiscussion,
    brief: &SafeBriefInput,
    origin: &wns_context::ProjectBriefOrigin,
) -> CoreResult<()> {
    if origin.version != "project-conversation-brief.v1"
        || origin.project_id != request.access.project_id
        || origin.operation_namespace != request.access.operation_namespace
        || origin.conversation_id.is_empty()
        || origin.message_id.is_empty()
        || origin.target != request.expected
        || origin.text_hash != sha256_hex(brief.text.as_bytes())
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief is bound to another project, target, or text.",
        ));
    }
    let scope_hash = sha256_hex(&serde_json::to_vec(&request.scope)?);
    if origin.scope_hash != scope_hash {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief is bound to another selection scope.",
        ));
    }
    let source: Option<(String, String, String)> = tx
        .query_row(
            "SELECT ci.kind,r.status,cp.snapshot_id
             FROM conversation_items ci
             JOIN discussion_runs r ON r.id=ci.reference_id
             JOIN context_packets cp ON cp.id=r.packet_id
             JOIN discussion_messages dm ON dm.run_id=r.id
             WHERE ci.conversation_id=? AND ci.project_id=?
               AND ci.operation_namespace=? AND r.project_id=?
               AND r.operation_namespace=?
               AND ci.kind IN ('request','chapterRequest')
               AND dm.id=? AND dm.role IN ('user','assistant')",
            params![
                origin.conversation_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.access.project_id,
                request.access.operation_namespace,
                origin.message_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((item_kind, status, snapshot_id)) = source else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief origin message is not retained in this project.",
        ));
    };
    if item_kind != "request" || status != "completed" {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A project conversation brief must come from a completed author-room request.",
        ));
    }
    let frozen = story_context::load_snapshot(tx, &request.access, &snapshot_id)?;
    if frozen.policy.audience != Audience::AuthorRoom
        || frozen
            .project_chat
            .as_ref()
            .is_none_or(|chat| chat.conversation_id != origin.conversation_id)
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A project conversation brief must come from a completed author-room request.",
        ));
    }
    Ok(())
}
