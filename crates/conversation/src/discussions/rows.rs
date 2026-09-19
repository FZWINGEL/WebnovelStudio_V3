use super::*;

pub(crate) fn ensure_thread(
    tx: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<String> {
    tx.execute("INSERT INTO discussion_threads(id,project_id,operation_namespace,document_id) VALUES(?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO NOTHING", params![new_id(), access.project_id, access.operation_namespace, document_id])?;
    tx.query_row("SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?", params![access.project_id,access.operation_namespace,document_id], |row| row.get(0)).map_err(CoreError::from)
}

pub(crate) fn validate_previous_run(
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

pub(crate) type ProviderResultRow = (
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

pub(crate) fn read_provider_result(
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

pub(crate) fn read_message(db: &Connection, message_id: &str) -> CoreResult<DiscussionMessage> {
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

pub(crate) fn read_draft(
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

pub(crate) fn existing_event(
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

pub(crate) fn validate_owner(run: &DiscussionRun, owner: &RunOwner) -> CoreResult<()> {
    if &run.owner != owner {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The output owner does not match the persisted run.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_runtime_owner(info: &ProjectInfo, owner: &RunOwner) -> CoreResult<()> {
    if owner.project_id != info.project_id || owner.operation_namespace != info.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The discussion run belongs to another project or recovered project identity.",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_run_started(status: DiscussionRunStatus) -> CoreResult<()> {
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

pub(crate) fn seal_run(
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

pub(crate) fn append_text(existing: &str, chunk: &str, limit: usize) -> CoreResult<String> {
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
