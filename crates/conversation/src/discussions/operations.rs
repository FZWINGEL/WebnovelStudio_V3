use super::*;

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
