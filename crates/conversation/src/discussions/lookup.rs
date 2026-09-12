//! The bounded story-lookup concern of a discussion: the four invocation
//! entry points, the request validator, and the reading and sealing helpers
//! that decide whether a provider's lookup report is the one this run asked
//! for.
//!
//! Split out of `discussions.rs`, which §3.4 measured at 4,605 lines and
//! named four concerns for. The items are `pub(super)` because a child
//! module's private items are not visible to the parent that calls them.

use super::*;

pub fn claim_lookup_invocation(
    host: &mut impl StoryHost,
    owner: RunOwner,
    ordinal_text: &str,
) -> CoreResult<discussion_lookup::LookupDispatch> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    validate_runtime_owner(host.info(), &owner)?;
    let ordinal = parse_lookup_ordinal(ordinal_text)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &owner.run_id)?;
    validate_owner(&current, &owner)?;
    if current.lookup.is_none() {
        return Err(CoreError::new(
            "LookupNotEnabled",
            "This discussion did not opt into bounded context lookup.",
        ));
    }
    ensure_run_started(current.status)?;
    let (snapshot_source_epoch, snapshot_policy_epoch): (i64, i64) = tx.query_row(
        "SELECT source_epoch,policy_epoch FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=?",
        params![owner.run_id, i64::from(ordinal)],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (source_epoch, policy_epoch): (i64, i64) = tx.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if snapshot_source_epoch != source_epoch || snapshot_policy_epoch != policy_epoch {
        let stale_message = "The story or disclosure policy changed before this lookup started; the saved response was not dispatched.";
        discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
        let _ = seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Failed,
            "context_stale",
            &format!("lookup-stale-{}-{}", current.id, ordinal),
            stale_message,
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Err(CoreError::new(
            "ContextChanged",
            "The queued lookup is historical because its frozen context is no longer current.",
        ));
    }
    let identity = discussion_lookup::claim(&tx, &owner, ordinal)?;
    let packet = read_lookup_packet(&tx, &owner, &identity.packet_id)?;
    let run = read_run(&tx, &owner.run_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(discussion_lookup::LookupDispatch {
        run,
        ordinal: ordinal.to_string(),
        packet,
    })
}

pub fn settle_lookup_invocation(
    host: &mut impl StoryHost,
    request: discussion_lookup::LookupInvocationReport,
) -> CoreResult<DiscussionRun> {
    validate_lookup_report_shape(&request)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let ordinal = parse_lookup_ordinal(&request.ordinal)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    if current.lookup.is_none() {
        return Err(CoreError::new(
            "LookupNotEnabled",
            "This discussion did not opt into bounded context lookup.",
        ));
    }
    if let Some(saved) = lookup_result_record(&tx, &request.owner.run_id, ordinal)? {
        if lookup_result_matches(&saved, &request) {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        return Err(CoreError::new(
            "LookupResultConflict",
            "A different immutable result is already recorded for this invocation.",
        ));
    }
    if !matches!(
        current.status,
        DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    ) {
        return Err(CoreError::new(
            "RunNotStarted",
            "Claim the lookup invocation before settling its provider result.",
        ));
    }
    let identity = discussion_lookup::read_identity(&tx, &request.owner, ordinal)?;
    let packet = context_packets::validated_packet_record(&tx, &identity.packet_id)?;
    if identity.state != discussion_lookup::LookupInvocationState::Claimed {
        return Err(CoreError::new(
            "LookupInvocationNotClaimed",
            "A provider result requires a claimed lookup invocation.",
        ));
    }
    if packet.options.provider_binding != request.binding {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The lookup result does not match the immutable packet binding.",
        ));
    }
    validate_lookup_provider_bytes(&tx, &identity, &packet, &request)?;
    let mut response_error = None;
    let parsed = if request.status == ProviderOutcomeStatus::Completed {
        match discussion_lookup::parse_response(&request.assistant_text) {
            Ok(parsed) => match discussion_lookup::authorize_envelope(&packet, &parsed.envelope) {
                Ok(()) => Some(parsed),
                Err(error) => {
                    response_error = Some(error.detail);
                    None
                }
            },
            Err(error) => {
                response_error = Some(error.detail);
                None
            }
        }
    } else {
        None
    };
    let report_error = request.error.clone().or(response_error);
    let stored_status = if parsed.is_none() && request.status == ProviderOutcomeStatus::Completed {
        ProviderOutcomeStatus::Failed
    } else {
        request.status
    };
    let state = discussion_lookup::store_outcome(
        &tx,
        &identity,
        &discussion_lookup::ProviderReport {
            event_id: request.event_id.clone(),
            assistant_text: request.assistant_text.clone(),
            binding: request.binding.clone(),
            status: stored_status,
            confirmed_stdin_bytes: request.confirmed_stdin_bytes.clone(),
            usage: request.usage.clone(),
            cleanup: request.cleanup,
            error: report_error.clone(),
        },
        parsed.as_ref(),
    )?;
    let result = match parsed {
        Some(_) if current.status == DiscussionRunStatus::Stopping => {
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            seal_run(
                &tx,
                &current,
                if request.cleanup == ProviderCleanup::Unresolved {
                    DiscussionRunStatus::Interrupted
                } else {
                    DiscussionRunStatus::Stopped
                },
                if request.cleanup == ProviderCleanup::Unresolved {
                    "stop_cleanup_unresolved"
                } else {
                    "author_stopped"
                },
                &request.event_id,
                if request.cleanup == ProviderCleanup::Unresolved {
                    STOP_UNRESOLVED_MESSAGE
                } else {
                    STOP_SETTLED_MESSAGE
                },
            )?
        }
        Some(parsed) if parsed.kind == discussion_lookup::ResponseKind::Discussion => {
            let discussion_text = match &parsed.envelope {
                wns_context::lookup::LookupEnvelope::Discussion { text, .. } => text,
                wns_context::lookup::LookupEnvelope::NeedsContext { .. } => unreachable!(),
            };
            seal_lookup_discussion(
                &tx,
                &current,
                &identity.packet_id,
                &request.event_id,
                discussion_text,
            )?
        }
        Some(_) => read_run(&tx, &current.id)?,
        None => {
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            let reason = lookup_failure_reason(request.status, request.cleanup);
            let message = report_error
                .as_deref()
                .filter(|error| !error.is_empty())
                .unwrap_or(reason);
            seal_run(
                &tx,
                &current,
                if request.cleanup == ProviderCleanup::Unresolved {
                    DiscussionRunStatus::Interrupted
                } else if request.status == ProviderOutcomeStatus::Stopped {
                    DiscussionRunStatus::Stopped
                } else {
                    DiscussionRunStatus::Failed
                },
                reason,
                &request.event_id,
                message,
            )?
        }
    };
    if state == discussion_lookup::LookupInvocationState::NeedsContext {
        // The root run intentionally remains Running until a later final
        // envelope or an explicit halt settles the chain.
    }
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

pub fn advance_lookup(
    host: &mut impl StoryHost,
    request: discussion_lookup::LookupAdvanceRequest,
) -> CoreResult<discussion_lookup::LookupAdvance> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    let completed = parse_lookup_ordinal(&request.completed_ordinal)?;
    let current_source_epoch = host.context_source_epoch()?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    let Some(lookup_summary) = current.lookup.as_ref() else {
        return Err(CoreError::new(
            "LookupNotEnabled",
            "This discussion did not opt into bounded context lookup.",
        ));
    };
    if current.status != DiscussionRunStatus::Running {
        return Err(CoreError::new(
            "RunSealed",
            "Only a running lookup discussion can be expanded.",
        ));
    }
    let identity = discussion_lookup::read_identity(&tx, &request.owner, completed)?;
    if identity.state != discussion_lookup::LookupInvocationState::NeedsContext {
        return Err(CoreError::new(
            "LookupExpansionUnavailable",
            "Only a needs-context invocation can be expanded.",
        ));
    }
    if completed
        >= lookup_summary
            .allowance
            .max_additional_invocations
            .saturating_add(1)
        || completed >= 2
    {
        discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
        let finished = seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Failed,
            "lookup_limit",
            &format!("lookup-limit-{}", completed),
            "This lookup request reached its bounded context-expansion limit.",
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(discussion_lookup::LookupAdvance::Finished { run: finished });
    }
    if identity.source_epoch != current_source_epoch {
        discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
        let finished = seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Failed,
            "context_stale",
            &format!("lookup-stale-{}", completed),
            "The story changed before the next lookup request; no new provider call was made.",
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(discussion_lookup::LookupAdvance::Finished { run: finished });
    }
    let packet = context_packets::validated_packet_record(&tx, &identity.packet_id)?;
    let prepare_json: String = tx.query_row(
        "SELECT request_json FROM context_packets WHERE id=?",
        [&identity.packet_id],
        |row| row.get(0),
    )?;
    let mut prepare: PrepareContext = serde_json::from_str(&prepare_json)?;
    let frozen = story_context::load_snapshot(&tx, &prepare.access, &identity.snapshot_id)?;
    let reads = discussion_lookup::pending_reads(&tx, &current.id, completed)?;
    let lookup_packet = packet.receipt.lookup.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidLookupCapability",
            "The lookup packet is missing its authorized lookup receipt.",
        )
    })?;
    lookup_packet
        .validate_capability()
        .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
    let mut round_exchanges = Vec::with_capacity(reads.len());
    for read in &reads {
        lookup_packet
            .authorize_read(read)
            .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
        let (result, truncated) = discussion_lookup::execute_read(&tx, &frozen, read);
        let value = serde_json::to_value(&result)?;
        discussion_lookup::store_read(
            &tx,
            &request.owner,
            completed,
            read.id(),
            read,
            &value,
            truncated,
        )?;
        round_exchanges.push(wns_context::lookup::LookupExchange {
            request: read.clone(),
            result,
        });
    }
    let mut exchanges = if completed == 0 {
        Vec::new()
    } else {
        discussion_lookup::read_exchanges(&tx, &current.id, completed - 1)?
    };
    exchanges.extend(round_exchanges);
    let next = completed
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidLookupCounter", "The lookup ordinal overflowed."))?;
    let source_projection = Some(wns_context::lookup::LookupSourceProjection::from_exchanges(
        &frozen, &exchanges,
    )?);
    prepare.operation_id = new_id();
    prepare.snapshot_id = frozen.snapshot.snapshot_id.clone();
    prepare.lookup = Some(wns_context::lookup::LookupPacketInput {
        allowance: lookup_summary.allowance.clone(),
        completed_invocations: next,
        exchanges,
        source_projection,
        reviewed_memory: lookup_packet.reviewed_memory.clone(),
    });
    let sources = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| story_context::read_source(&tx, &frozen, &source.handle))
        .collect::<CoreResult<Vec<_>>>()?;
    let packet_request = PacketRequest {
        packet_id: new_id(),
        session_id: packet.receipt.session_id.clone(),
        invocation_ordinal: next.to_string(),
        frozen: frozen.clone(),
        instruction: prepare.instruction.clone(),
        sources,
        mandatory_handles: prepare.mandatory_handles.clone(),
        safe_brief: prepare.safe_brief.clone(),
        scope: prepare.scope.clone(),
        budget: prepare.budget.clone(),
        provider_binding: prepare.provider_binding.clone(),
        response_contract: Some(LOOKUP_RESPONSE_CONTRACT.to_owned()),
        workshop_metadata: None,
        lookup: prepare.lookup.clone(),
    };
    let child_packet = compile_packet(&packet_request).map_err(packet_error)?;
    context_packets::persist_compiled_packet_at(&tx, &prepare, &child_packet)?;
    discussion_lookup::insert_child(
        &tx,
        &current.id,
        next,
        &child_packet,
        &frozen,
        &request.owner.operation_namespace,
        &lookup_summary.allowance,
    )?;
    let run = read_run(&tx, &current.id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(discussion_lookup::LookupAdvance::Prepared {
        dispatch: Box::new(discussion_lookup::LookupDispatch {
            run,
            ordinal: next.to_string(),
            packet: child_packet,
        }),
    })
}

pub fn halt_lookup(
    host: &mut impl StoryHost,
    request: discussion_lookup::LookupHaltRequest,
) -> CoreResult<DiscussionRun> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    validate_runtime_owner(host.info(), &request.owner)?;
    if request.reason.trim().is_empty() || request.reason.len() > 4096 {
        return Err(CoreError::new(
            "InvalidRequest",
            "A lookup halt reason must be nonblank and at most 4 KiB.",
        ));
    }
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_run(&tx, &request.owner.run_id)?;
    validate_owner(&current, &request.owner)?;
    if current.lookup.is_none() {
        return Err(CoreError::new(
            "LookupNotEnabled",
            "This discussion did not opt into bounded context lookup.",
        ));
    }
    if current.status.terminal() {
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(current);
    }
    discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
    let result = seal_run(
        &tx,
        &current,
        DiscussionRunStatus::Interrupted,
        "lookup_halted",
        &format!("lookup-halt-{}", current.sequence),
        &request.reason,
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

pub(super) fn validate_lookup_request(
    intent: FeedbackIntent,
    basis: Option<BasisKind>,
    lookup: Option<&LookupAllowance>,
) -> CoreResult<()> {
    let Some(lookup) = lookup else {
        return Ok(());
    };
    if intent != FeedbackIntent::Discuss || basis.is_some_and(|basis| basis != BasisKind::Working) {
        return Err(CoreError::new(
            "UnsupportedContextTools",
            "Bounded story lookup is available only for Working AuthorRoom discussions.",
        ));
    }
    discussion_lookup::validate_allowance(lookup)
}

pub(super) fn reject_legacy_lookup_path(run: &DiscussionRun) -> CoreResult<()> {
    if run.lookup.is_some() {
        return Err(CoreError::new(
            "LookupInvocationRequired",
            "This discussion opted into bounded lookup and must use its lookup invocation protocol.",
        ));
    }
    Ok(())
}

pub(super) fn parse_lookup_ordinal(value: &str) -> CoreResult<u8> {
    let ordinal = parse_version(value)?;
    u8::try_from(ordinal)
        .ok()
        .filter(|ordinal| *ordinal < discussion_lookup::MAX_LOOKUP_INVOCATIONS)
        .ok_or_else(|| {
            CoreError::new(
                "InvalidLookupCounter",
                "A lookup invocation ordinal must be 0, 1, or 2.",
            )
        })
}

pub(super) fn validate_lookup_report_shape(
    request: &discussion_lookup::LookupInvocationReport,
) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    parse_lookup_ordinal(&request.ordinal)?;
    if request.assistant_text.len() > MAX_OUTPUT_BYTES
        || request
            .assistant_text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The lookup provider response is too large or contains control characters.",
        ));
    }
    if let Some(error) = &request.error
        && (error.is_empty() || error.len() > 4096 || error.chars().any(char::is_control))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The lookup provider error must be sanitized and at most 4 KiB.",
        ));
    }
    if request.status == ProviderOutcomeStatus::Completed {
        if request.assistant_text.is_empty() || request.error.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed lookup invocation must include only a response envelope.",
            ));
        }
        if request.cleanup == ProviderCleanup::Unresolved {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed lookup invocation must have settled cleanup.",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_lookup_provider_bytes(
    tx: &Connection,
    identity: &discussion_lookup::InvocationIdentity,
    packet: &CompiledPacket,
    request: &discussion_lookup::LookupInvocationReport,
) -> CoreResult<()> {
    let confirmed = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
    let serialized = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    if confirmed > serialized.len() as u64
        || (request.status == ProviderOutcomeStatus::Completed
            && confirmed != serialized.len() as u64)
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The lookup provider did not consume the exact frozen packet input.",
        ));
    }
    let output_limit = packet
        .options
        .provider_binding
        .as_ref()
        .map(|binding| binding.output_limit())
        .transpose()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?
        .unwrap_or(MAX_OUTPUT_BYTES);
    if request.assistant_text.len() > output_limit {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The lookup provider response exceeds the application output cap.",
        ));
    }
    let (previous_input, previous_output): (i64, i64) = tx.query_row(
        "SELECT COALESCE(SUM(confirmed_stdin_bytes),0),COALESCE(SUM(length(CAST(assistant_text AS BLOB))),0) FROM discussion_lookup_results WHERE run_id=?",
        [&identity.run_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let total_input = u128::try_from(previous_input)
        .unwrap_or(u128::MAX)
        .saturating_add(u128::from(confirmed));
    let total_output = u128::try_from(previous_output)
        .unwrap_or(u128::MAX)
        .saturating_add(request.assistant_text.len() as u128);
    let input_limit = parse_decimal_u64(&identity.allowance.total_input_bytes)? as u128;
    let output_limit = parse_decimal_u64(&identity.allowance.total_output_bytes)? as u128;
    if total_input > input_limit || total_output > output_limit {
        return Err(CoreError::new(
            "LookupAllowanceExceeded",
            "The lookup invocation would exceed its application byte allowance.",
        ));
    }
    Ok(())
}

pub(super) type LookupResultIdentity = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub(super) fn lookup_result_record(
    tx: &Connection,
    run_id: &str,
    ordinal: u8,
) -> CoreResult<Option<LookupResultIdentity>> {
    tx.query_row(
        "SELECT event_id,assistant_text,outcome,confirmed_stdin_bytes,cleanup,error,binding_json,usage_json FROM discussion_lookup_results WHERE run_id=? AND ordinal=?",
        params![run_id, i64::from(ordinal)],
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
            ))
        },
    )
    .optional()
    .map_err(CoreError::from)
}

pub(super) fn lookup_result_matches(
    saved: &LookupResultIdentity,
    request: &discussion_lookup::LookupInvocationReport,
) -> bool {
    let binding_json = request
        .binding
        .as_ref()
        .and_then(|binding| serde_json::to_string(binding).ok());
    let usage_json = request
        .usage
        .as_ref()
        .and_then(|usage| serde_json::to_string(usage).ok());
    let expected_outcome = request.status.as_str();
    let confirmed_stdin_matches = parse_decimal_u64(&request.confirmed_stdin_bytes)
        .ok()
        .and_then(|value| i64::try_from(value).ok())
        == Some(saved.3);
    let error_matches = saved.5 == request.error
        || (request.status == ProviderOutcomeStatus::Completed
            && saved.2 == "failed"
            && request.error.is_none()
            && saved.5.is_some());
    saved.0 == request.event_id
        && saved.1 == request.assistant_text
        && (saved.2 == expected_outcome
            || (request.status == ProviderOutcomeStatus::Completed && saved.2 == "failed"))
        && confirmed_stdin_matches
        && saved.4 == request.cleanup.as_str()
        && error_matches
        && saved.6 == binding_json
        && saved.7 == usage_json
}

pub(super) fn read_lookup_packet(
    tx: &Connection,
    owner: &RunOwner,
    packet_id: &str,
) -> CoreResult<CompiledPacket> {
    context_packets::read_context_packet_at(
        tx,
        &ProjectAccess {
            project_id: owner.project_id.clone(),
            operation_namespace: owner.operation_namespace.clone(),
            session: String::new(),
            writer_lease: String::new(),
        },
        packet_id,
    )
}

pub(super) fn seal_lookup_discussion(
    tx: &Connection,
    current: &DiscussionRun,
    packet_id: &str,
    event_id: &str,
    text: &str,
) -> CoreResult<DiscussionRun> {
    if current.status != DiscussionRunStatus::Running {
        return Err(CoreError::new(
            "RunSealed",
            "A lookup discussion can finish only while running.",
        ));
    }
    validate_finish_request(&current.owner, event_id, text)?;
    let sequence = parse_version(&current.sequence)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![current.id, next, event_id, "terminal", text],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status='completed',sequence=?,output_text=?,stop_reason=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
        params![next, text, current.id, current.owner.project_id, current.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The lookup discussion changed before its final response was committed.",
        ));
    }
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![new_id(), current.thread_id, current.id, DiscussionMessageRole::Assistant.as_str(), text, packet_id],
    )?;
    read_run(tx, &current.id)
}

pub(super) fn lookup_failure_reason(
    status: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> &'static str {
    if cleanup == ProviderCleanup::Unresolved {
        "lookup_cleanup_unresolved"
    } else {
        match status {
            ProviderOutcomeStatus::Completed => "lookup_invalid_response",
            ProviderOutcomeStatus::Stopped => "lookup_stopped",
            ProviderOutcomeStatus::TimedOut => "lookup_timed_out",
            ProviderOutcomeStatus::OutputLimit => "lookup_output_limit",
            ProviderOutcomeStatus::Failed => "lookup_failed",
        }
    }
}
