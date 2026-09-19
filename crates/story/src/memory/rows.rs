use super::*;

#[derive(Debug, Clone)]

pub(crate) struct MemoryJobRow {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) operation_namespace: String,
    pub(crate) operation_id: String,
    pub(crate) payload_hash: String,
    pub(crate) request_json: String,
    pub(crate) target_document_id: String,
    pub(crate) target_version: i64,
    pub(crate) target_body_hash: String,
    pub(crate) source_document_id: String,
    pub(crate) source_revision_id: String,
    pub(crate) source_body_hash: String,
    pub(crate) snapshot_id: String,
    pub(crate) packet_id: String,
    pub(crate) context_source_epoch: i64,
    pub(crate) disclosure_policy_epoch: i64,
    pub(crate) status: String,
    pub(crate) dispatch_state: String,
    pub(crate) stop_reason: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) fn validate_start_memory(request: &StartMemory) -> CoreResult<()> {
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    parse_version(&request.expected.version)?;
    if request.expected.body_hash.len() != 64
        || !request
            .expected
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The expected chapter head has an invalid body hash.",
        ));
    }
    if request.budget.model_id != wns_context::packet::MOCK_MODEL_ID {
        return Err(CoreError::new(
            "InvalidBudget",
            "The chapter memory recipe uses the deterministic mock budget profile.",
        ));
    }
    if let Some(binding) = &request.provider_binding {
        binding
            .validate()
            .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
        if binding.is_http() && !binding.is_http_memory() {
            return Err(CoreError::new(
                "UnsupportedProviderFeature",
                "Chapter memory only accepts the fixed OpenAI-compatible memory profile.",
            ));
        }
        if binding.is_claude()
            || binding.profile_version == wns_providers::codex_profile::CODEX_AUTHOR_PROFILE_VERSION
            || binding.profile_version == wns_providers::codex_app_server::AUTHOR_PROFILE
        {
            return Err(CoreError::new(
                "UnsupportedProviderFeature",
                "Chapter memory uses the fixed maintenance profile and does not accept author CLI bindings.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_complete_memory(request: &CompleteMemory) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.job_id)?;
    check_id(&request.event_id)?;
    if request.raw_output.len() > MAX_RAW_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The memory terminal output exceeds the 64 KiB application cap.",
        ));
    }
    if let Some(value) = &request.confirmed_stdin_bytes {
        parse_decimal_u64(value)?;
    }
    if request.delivery.is_some() && request.confirmed_stdin_bytes.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "An HTTP memory result cannot include local stdin delivery proof.",
        ));
    }
    if request.delivery.is_some() && request.usage.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "An HTTP memory result records provider usage inside its HTTP delivery receipt.",
        ));
    }
    validate_optional_text(request.error.as_deref(), "provider error")?;
    validate_optional_text(request.effective_identity.as_deref(), "provider identity")?;
    Ok(())
}

pub(crate) fn parse_decimal_u64(value: &str) -> CoreResult<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
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

pub(crate) fn validate_optional_text(value: Option<&str>, label: &str) -> CoreResult<()> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > MAX_ERROR_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(CoreError::new(
            "InvalidRequest",
            &format!("The {label} is empty, too large, or contains control characters."),
        ));
    }
    Ok(())
}

pub(crate) fn current_memory_policy(tx: &Connection) -> CoreResult<InformationPolicy> {
    Ok(InformationPolicy {
        version: current_policy_version(tx)?,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    })
}

pub(crate) fn current_policy_version(db: &Connection) -> CoreResult<String> {
    let value: i64 = db.query_row(
        "SELECT disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(value)
}

pub(crate) fn current_source_epoch(db: &Connection) -> CoreResult<String> {
    let value: i64 = db.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(value)
}

pub(crate) fn ensure_current_policy(db: &Connection, frozen: &FrozenContext) -> CoreResult<()> {
    if frozen.policy.version != current_policy_version(db)? {
        return Err(CoreError::new(
            "ContextPolicyChanged",
            "Source permissions changed. The memory job cannot expose its frozen source.",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_memory_basis_current(db: &Connection, row: &MemoryJobRow) -> CoreResult<()> {
    let document = read_document(db, &row.target_document_id)?;
    if document.head.version != row.target_version.to_string()
        || document.head.body_hash != row.target_body_hash
        || current_source_epoch(db)? != row.context_source_epoch.to_string()
        || current_policy_version(db)? != row.disclosure_policy_epoch.to_string()
    {
        Err(CoreError::new(
            "MemoryBasisChanged",
            "The saved chapter or story-context basis changed before memory dispatch.",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn read_memory_job_row(db: &Connection, id: &str) -> CoreResult<MemoryJobRow> {
    check_id(id)?;
    db.query_row(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state,stop_reason,created_at,updated_at FROM memory_jobs WHERE id=?",
        [id],
        |row| {
            Ok(MemoryJobRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                operation_namespace: row.get(2)?,
                operation_id: row.get(3)?,
                payload_hash: row.get(4)?,
                request_json: row.get(5)?,
                target_document_id: row.get(6)?,
                target_version: row.get(7)?,
                target_body_hash: row.get(8)?,
                source_document_id: row.get(9)?,
                source_revision_id: row.get(10)?,
                source_body_hash: row.get(11)?,
                snapshot_id: row.get(12)?,
                packet_id: row.get(13)?,
                context_source_epoch: row.get(14)?,
                disclosure_policy_epoch: row.get(15)?,
                status: row.get(16)?,
                dispatch_state: row.get(17)?,
                stop_reason: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::new("MemoryJobNotFound", "The memory job is not available."))
}

pub(crate) fn validate_job_owner(row: &MemoryJobRow, owner: &MemoryOwner) -> CoreResult<()> {
    if row.project_id != owner.project_id
        || row.operation_namespace != owner.operation_namespace
        || row.id != owner.job_id
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory owner does not match the persisted project namespace and job.",
        ));
    }
    Ok(())
}

pub(crate) fn invalid_memory_lifecycle() -> CoreError {
    CoreError::new(
        "InvalidMemoryLifecycle",
        "The memory job status, dispatch state, stop reason, and result do not form a legal lifecycle.",
    )
}

/// Validate lifecycle-only invariants shared by request reads and transfer
/// validation.  Source/candidate integrity is checked separately by the
/// historical validator; policy-redacted reads still need these state checks.
pub(crate) fn validate_memory_lifecycle(
    row: &MemoryJobRow,
    result: Option<&MemoryResult>,
    provider_bound: bool,
) -> CoreResult<()> {
    let status = MemoryJobStatus::parse(&row.status)?;
    let dispatch = MemoryDispatchState::parse(&row.dispatch_state)?;
    let reason = row.stop_reason.as_deref();
    let valid = match status {
        MemoryJobStatus::Queued => {
            dispatch == MemoryDispatchState::Pending && result.is_none() && reason.is_none()
        }
        MemoryJobStatus::Running => {
            dispatch == MemoryDispatchState::Dispatched && result.is_none() && reason.is_none()
        }
        MemoryJobStatus::Stopping => {
            dispatch == MemoryDispatchState::Dispatched
                && result.is_none()
                && reason == Some("author_stopped")
        }
        MemoryJobStatus::Stopped => {
            matches!(reason, Some("author_stopped") | Some("provider_stopped"))
                && match (dispatch, result) {
                    (MemoryDispatchState::Pending, None) => reason == Some("author_stopped"),
                    (MemoryDispatchState::Dispatched, Some(result)) => {
                        result.cleanup != Some(ProviderCleanup::Unresolved)
                            && (!provider_bound
                                || result.outcome != ProviderOutcomeStatus::Completed
                                || result.cleanup == Some(ProviderCleanup::Settled))
                    }
                    _ => false,
                }
        }
        MemoryJobStatus::Completed => {
            dispatch == MemoryDispatchState::Dispatched
                && reason.is_none()
                && result.is_some_and(|result| {
                    result.outcome == ProviderOutcomeStatus::Completed
                        && result.cleanup != Some(ProviderCleanup::Unresolved)
                        && (!provider_bound || result.cleanup == Some(ProviderCleanup::Settled))
                })
        }
        MemoryJobStatus::Failed => {
            dispatch == MemoryDispatchState::Dispatched
                && reason.is_none()
                && result.is_some_and(|result| {
                    result.cleanup != Some(ProviderCleanup::Unresolved)
                        && (!provider_bound
                            || result.outcome != ProviderOutcomeStatus::Completed
                            || result.cleanup == Some(ProviderCleanup::Settled))
                })
        }
        MemoryJobStatus::Interrupted => match result {
            None => {
                matches!(
                    dispatch,
                    MemoryDispatchState::Pending | MemoryDispatchState::Dispatched
                ) && matches!(
                    reason,
                    Some("recovered_unknown_external_outcome") | Some("dispatch_outcome_unknown")
                )
            }
            Some(result) => {
                dispatch == MemoryDispatchState::Dispatched
                    && (!provider_bound
                        || result.outcome != ProviderOutcomeStatus::Completed
                        || result.cleanup == Some(ProviderCleanup::Settled))
                    && ((result.cleanup == Some(ProviderCleanup::Unresolved)
                        && matches!(reason, Some("author_stopped") | Some("cleanup_unresolved")))
                        || reason == Some("recovered_unknown_external_outcome"))
            }
        },
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_memory_lifecycle())
    }
}

pub(crate) fn validate_runtime_owner(info: &ProjectInfo, owner: &MemoryOwner) -> CoreResult<()> {
    if owner.project_id != info.project_id || owner.operation_namespace != info.operation_namespace
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory job belongs to another project or recovered project identity.",
        ));
    }
    Ok(())
}

pub(crate) fn read_memory_job(db: &Connection, id: &str, reveal: bool) -> CoreResult<MemoryJob> {
    let row = read_memory_job_row(db, id)?;
    let historical: bool = db.query_row(
        "SELECT id<>? OR operation_namespace<>? FROM project WHERE singleton=1",
        params![row.project_id, row.operation_namespace],
        |record| record.get(0),
    )?;
    let packet = validated_packet_record(db, &row.packet_id)?;
    let source = SourceRef {
        project_id: row.project_id.clone(),
        document_id: row.source_document_id.clone(),
        revision_id: row.source_revision_id.clone(),
        body_hash: row.source_body_hash.clone(),
    };
    let result = read_memory_result(db, id, reveal)?;
    validate_memory_lifecycle(
        &row,
        result.as_ref(),
        packet.options.provider_binding.is_some(),
    )?;
    let view = read_memory_view(db, id, reveal)?;
    Ok(MemoryJob {
        id: row.id.clone(),
        owner: MemoryOwner {
            project_id: row.project_id,
            operation_namespace: row.operation_namespace,
            job_id: row.id,
        },
        operation_id: row.operation_id,
        payload_hash: row.payload_hash,
        target: Head {
            document_id: row.target_document_id,
            version: row.target_version.to_string(),
            body_hash: row.target_body_hash,
        },
        source,
        snapshot_id: row.snapshot_id,
        packet_id: row.packet_id,
        context_source_epoch: SourceEpoch::new(row.context_source_epoch.to_string()),
        disclosure_policy_version: row.disclosure_policy_epoch.to_string(),
        provider_binding: packet.options.provider_binding,
        status: MemoryJobStatus::parse(&row.status)?,
        dispatch_state: MemoryDispatchState::parse(&row.dispatch_state)?,
        historical,
        stop_reason: row.stop_reason,
        result,
        view,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub(crate) fn read_memory_job_with_policy(
    db: &Connection,
    id: &str,
    policy_version: &str,
) -> CoreResult<MemoryJob> {
    let row = read_memory_job_row(db, id)?;
    let mut job = read_memory_job(
        db,
        id,
        row.disclosure_policy_epoch.to_string() == policy_version,
    )?;
    if let Some(view) = job.view.take() {
        job.view = Some(resolve_view_current(db, view)?);
    }
    Ok(job)
}

pub(crate) fn validate_delivery(
    db: &Connection,
    request: &CompleteMemory,
    packet: &CompiledPacket,
) -> CoreResult<()> {
    if packet
        .options
        .provider_binding
        .as_ref()
        .is_some_and(wns_providers::codex_app_server::is_app_server)
    {
        return app_server::validate_delivery(db, request, packet);
    }
    if request.app_server.is_some() {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "App-server evidence is only valid for its frozen transport.",
        ));
    }
    if packet
        .options
        .provider_binding
        .as_ref()
        .is_some_and(ProviderBinding::is_http_memory)
    {
        return validate_http_memory_delivery(request, packet);
    }
    if request.delivery.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "HTTP delivery evidence is only valid for the fixed HTTP memory profile.",
        ));
    }
    if packet.options.provider_binding.is_some()
        && request.outcome == ProviderOutcomeStatus::Completed
        && request.cleanup != Some(ProviderCleanup::Settled)
    {
        return Err(CoreError::new(
            "ProviderCleanupUnknown",
            "A completed live memory result needs confirmed settled provider cleanup.",
        ));
    }
    let serialized = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let delivered = request
        .confirmed_stdin_bytes
        .as_deref()
        .map(parse_decimal_u64)
        .transpose()?;
    if packet.options.provider_binding.is_some() {
        match (request.outcome, delivered) {
            (ProviderOutcomeStatus::Completed, Some(delivered))
                if delivered == serialized.len() as u64 => {}
            (ProviderOutcomeStatus::Completed, _) => {
                return Err(CoreError::new(
                    "ProviderInputUnknown",
                    "A completed live memory result needs exact local stdin delivery proof.",
                ));
            }
            (_, Some(delivered)) if delivered > serialized.len() as u64 => {
                return Err(CoreError::new(
                    "ProviderInputMismatch",
                    "The provider delivery count exceeds the frozen memory packet.",
                ));
            }
            _ => {}
        }
    } else if let Some(delivered) = delivered
        && delivered > serialized.len() as u64
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The reported local delivery exceeds the frozen memory packet.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_http_memory_delivery(
    request: &CompleteMemory,
    packet: &CompiledPacket,
) -> CoreResult<()> {
    if request.outcome == ProviderOutcomeStatus::Completed
        && request.cleanup != Some(ProviderCleanup::Settled)
    {
        return Err(CoreError::new(
            "ProviderCleanupUnknown",
            "A completed HTTP memory result needs confirmed settled provider cleanup.",
        ));
    }
    if request.confirmed_stdin_bytes.is_some() || request.usage.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "HTTP memory delivery cannot include Codex stdin or usage fields.",
        ));
    }
    let delivery = request.delivery.as_ref().ok_or_else(|| {
        CoreError::new(
            "ProviderInputUnknown",
            "An OpenAI-compatible memory result needs HTTP delivery evidence.",
        )
    })?;
    validate_http_memory_delivery_shape(delivery, request.outcome)?;
    if delivery.submission == HttpDeliverySubmission::NotSent
        && (!request.raw_output.is_empty() || delivery.usage.is_some())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "An HTTP memory request marked not sent cannot retain output or usage.",
        ));
    }
    if delivery.usage.is_some() && delivery.submission != HttpDeliverySubmission::ResponseReceived {
        return Err(CoreError::new(
            "InvalidRequest",
            "HTTP memory usage is only known after a response is received.",
        ));
    }
    let prepared = prepare_http_request(&packet.messages, &packet.options)?;
    if delivery.body_hash != prepared.body_hash || delivery.body_bytes != prepared.body_bytes {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The HTTP memory delivery receipt does not match the immutable request body.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_http_memory_delivery_shape(
    delivery: &ProviderDeliveryReceipt,
    status: ProviderOutcomeStatus,
) -> CoreResult<()> {
    if delivery.body_hash.len() != 64
        || !delivery
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP memory request body hash is invalid.",
        ));
    }
    let body_bytes = parse_decimal_u64(&delivery.body_bytes)?;
    if body_bytes == 0 || body_bytes > wns_context::packet::HTTP_MEMORY_INPUT_LIMIT_BYTES as u64 {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP memory request body byte count is outside the application limit.",
        ));
    }
    if status == ProviderOutcomeStatus::Completed
        && delivery.submission != HttpDeliverySubmission::ResponseReceived
    {
        return Err(CoreError::new(
            "ProviderInputUnknown",
            "A completed HTTP memory result needs a fully received response.",
        ));
    }
    Ok(())
}

pub(crate) fn terminal_state_with_cleanup(
    prior: MemoryJobStatus,
    outcome: ProviderOutcomeStatus,
    candidate_valid: bool,
    cleanup: Option<ProviderCleanup>,
) -> (MemoryJobStatus, Option<&'static str>) {
    if cleanup == Some(ProviderCleanup::Unresolved) {
        return (
            MemoryJobStatus::Interrupted,
            Some(if prior == MemoryJobStatus::Stopping {
                "author_stopped"
            } else {
                "cleanup_unresolved"
            }),
        );
    }
    if prior == MemoryJobStatus::Stopping {
        return (MemoryJobStatus::Stopped, Some("author_stopped"));
    }
    if outcome == ProviderOutcomeStatus::Stopped {
        return (MemoryJobStatus::Stopped, Some("provider_stopped"));
    }
    (
        match outcome {
            ProviderOutcomeStatus::Completed if candidate_valid => MemoryJobStatus::Completed,
            ProviderOutcomeStatus::Completed
            | ProviderOutcomeStatus::TimedOut
            | ProviderOutcomeStatus::OutputLimit
            | ProviderOutcomeStatus::Failed
            | ProviderOutcomeStatus::Stopped => MemoryJobStatus::Failed,
        },
        None,
    )
}

pub(crate) fn provider_outcome_as_str(outcome: ProviderOutcomeStatus) -> &'static str {
    match outcome {
        ProviderOutcomeStatus::Completed => "completed",
        ProviderOutcomeStatus::Stopped => "stopped",
        ProviderOutcomeStatus::TimedOut => "timed_out",
        ProviderOutcomeStatus::OutputLimit => "output_limit",
        ProviderOutcomeStatus::Failed => "failed",
    }
}

pub(crate) fn parse_provider_outcome(value: &str) -> CoreResult<ProviderOutcomeStatus> {
    match value {
        "completed" => Ok(ProviderOutcomeStatus::Completed),
        "stopped" => Ok(ProviderOutcomeStatus::Stopped),
        "timed_out" => Ok(ProviderOutcomeStatus::TimedOut),
        "output_limit" => Ok(ProviderOutcomeStatus::OutputLimit),
        "failed" => Ok(ProviderOutcomeStatus::Failed),
        _ => Err(CoreError::new(
            "InvalidProject",
            "The saved memory result has an unknown provider outcome.",
        )),
    }
}

pub(crate) fn provider_cleanup_as_str(cleanup: ProviderCleanup) -> &'static str {
    match cleanup {
        ProviderCleanup::Settled => "settled",
        ProviderCleanup::Unresolved => "unresolved",
    }
}

pub(crate) fn parse_provider_cleanup(value: &str) -> CoreResult<ProviderCleanup> {
    match value {
        "settled" => Ok(ProviderCleanup::Settled),
        "unresolved" => Ok(ProviderCleanup::Unresolved),
        _ => Err(CoreError::new(
            "InvalidProject",
            "The saved memory result has an unknown cleanup state.",
        )),
    }
}

pub(crate) fn result_matches(saved: &MemoryResult, request: &CompleteMemory) -> bool {
    saved.event_id == request.event_id
        && saved.raw_output.as_deref() == Some(request.raw_output.as_str())
        && saved.outcome == request.outcome
        && saved.confirmed_stdin_bytes == request.confirmed_stdin_bytes
        && saved.usage == request.usage
        && saved.cleanup == request.cleanup
        && saved.error == request.error
        && saved.effective_identity == request.effective_identity
        && saved.delivery == request.delivery
        && saved.app_server == request.app_server
}

pub(crate) type MemoryResultColumns = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
);

pub(crate) fn read_memory_result(
    db: &Connection,
    job_id: &str,
    reveal: bool,
) -> CoreResult<Option<MemoryResult>> {
    let row: Option<MemoryResultColumns> = db
        .query_row(
            "SELECT job_id,event_id,raw_output,raw_output_hash,candidate_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,validation_error,effective_identity,delivery_json,created_at,app_server_delivery_json FROM memory_results WHERE job_id=?",
            [job_id],
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
                ))
            },
        )
        .optional()?;
    let Some((
        saved_job_id,
        event_id,
        raw_output,
        raw_output_hash,
        candidate_json,
        outcome,
        confirmed_stdin_bytes,
        usage_json,
        cleanup,
        error,
        validation_error,
        effective_identity,
        delivery_json,
        created_at,
        app_server_delivery_json,
    )) = row
    else {
        return Ok(None);
    };
    if saved_job_id != job_id || sha256_hex(raw_output.as_bytes()) != raw_output_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved memory result failed its raw-output fingerprint check.",
        ));
    }
    if raw_output.len() > MAX_RAW_BYTES {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved memory result exceeds the bounded output contract.",
        ));
    }
    check_id(&saved_job_id)?;
    check_id(&event_id)?;
    if reveal {
        validate_optional_text(error.as_deref(), "provider error")?;
        validate_optional_text(validation_error.as_deref(), "memory validation error")?;
        validate_optional_text(effective_identity.as_deref(), "provider identity")?;
    }
    let candidate = if reveal {
        candidate_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
    } else {
        None
    };
    let usage = usage_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let delivery = delivery_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let confirmed_stdin_bytes = confirmed_stdin_bytes
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| {
                    CoreError::new("InvalidProject", "The saved delivery count is negative.")
                })
                .map(|value| value.to_string())
        })
        .transpose()?;
    let (error, validation_error, effective_identity) = if reveal {
        (error, validation_error, effective_identity)
    } else {
        // Diagnostics are provider supplied text and may echo the frozen
        // chapter.  A policy-revoked read must redact every such field, not
        // only the raw output and candidate JSON.
        (None, None, None)
    };
    let result = MemoryResult {
        app_server: app_server_delivery_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        job_id: saved_job_id,
        event_id,
        raw_output: reveal.then_some(raw_output),
        outcome: parse_provider_outcome(&outcome)?,
        confirmed_stdin_bytes,
        usage,
        cleanup: cleanup.as_deref().map(parse_provider_cleanup).transpose()?,
        error,
        validation_error,
        candidate: if reveal { candidate } else { None },
        effective_identity,
        delivery,
        created_at,
    };
    Ok(Some(result))
}

pub(crate) type MemoryViewColumns = (
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    String,
    i64,
    String,
);
