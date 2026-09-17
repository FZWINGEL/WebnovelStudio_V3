use super::*;

impl StartMemory {
    fn identity(&self) -> MemoryRequestIdentity<'_> {
        MemoryRequestIdentity {
            project_id: &self.access.project_id,
            operation_namespace: &self.access.operation_namespace,
            operation_id: &self.operation_id,
            expected: &self.expected,
            budget: &self.budget,
            provider_binding: &self.provider_binding,
        }
    }

    fn stored_request(&self) -> StoredMemoryRequest {
        StoredMemoryRequest {
            project_id: self.access.project_id.clone(),
            operation_namespace: self.access.operation_namespace.clone(),
            operation_id: self.operation_id.clone(),
            expected: self.expected.clone(),
            budget: self.budget.clone(),
            provider_binding: self.provider_binding.clone(),
        }
    }
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn handle_memory(host: &mut impl StoryHost, command: MemoryCommand) {
    macro_rules! mutate {
        ($reply:expr, $operation:expr) => {{
            let result = $operation;
            host.fence_uncertain(&result);
            let _ = $reply.send(result);
        }};
    }
    match command {
        MemoryCommand::ClaimAppServer(owner, dispatch, reply) => mutate!(
            reply,
            app_server::claim_memory_app_server_dispatch(host, owner, dispatch)
        ),
        MemoryCommand::AckAppServer(owner, dispatch, turn_id, reply) => mutate!(
            reply,
            app_server::acknowledge_memory_app_server_turn(host, owner, dispatch, turn_id)
        ),
        MemoryCommand::Start(request, reply) => mutate!(reply, start_memory(host, request)),
        MemoryCommand::Begin(owner, reply) => mutate!(reply, begin_memory(host, owner)),
        MemoryCommand::Stop(access, job_id, reply) => {
            mutate!(reply, stop_memory(host, access, job_id))
        }
        MemoryCommand::Complete(request, reply) => {
            mutate!(reply, complete_memory(host, request))
        }
        MemoryCommand::Install(owner, reply) => {
            mutate!(reply, install_memory(host, owner))
        }
        MemoryCommand::Read(access, document_id, reply) => {
            let _ = reply.send(read_memory(host, access, &document_id));
        }
        MemoryCommand::List(access, reply) => {
            let _ = reply.send(list_memory(host, access));
        }
        MemoryCommand::ReadJob(owner, reply) => {
            let result = validate_runtime_owner(host.info(), &owner)
                .and_then(|()| {
                    let policy = current_policy_version(host.db()?)?;
                    read_memory_job_with_policy(host.db()?, &owner.job_id, &policy)
                })
                .and_then(|job| {
                    if job.owner == owner {
                        Ok(job)
                    } else {
                        Err(CoreError::new(
                            "MemoryProjectMismatch",
                            "The memory owner does not match the persisted job.",
                        ))
                    }
                });
            let _ = reply.send(result);
        }
        MemoryCommand::ReadViewSource(access, view_id, reply) => {
            let _ = reply.send(read_memory_source(host, access, &view_id));
        }
        MemoryCommand::InterruptClaim(owner, reply) => {
            mutate!(reply, interrupt_memory_claim(host, owner))
        }
    }
}

pub fn start_memory(host: &mut impl StoryHost, request: StartMemory) -> CoreResult<MemoryJob> {
    host.check_access(&request.access)?;
    validate_start_memory(&request)?;
    let payload_hash = sha256_hex(
        serde_json::to_string(&request.identity())
            .map_err(CoreError::from)?
            .as_bytes(),
    );
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT id,payload_hash FROM memory_jobs WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((job_id, previous_hash)) = existing {
        if previous_hash != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This memory refresh operation was already used for a different request.",
            ));
        }
        let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(job);
    }

    let policy = current_memory_policy(&tx)?;
    let freeze = FreezeStory {
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        expected: request.expected.clone(),
        basis: BasisKind::Working,
        purpose: ContextPurpose::MemoryAnalysis,
        policy,
    };
    let frozen = story_context::freeze_memory_story_at(&tx, &freeze, &payload_hash)?;
    let target_handle = frozen.snapshot.target.revision_id.clone();
    let source = story_context::read_source(&tx, &frozen, &target_handle)?;
    let prepare = PrepareContext {
        lookup: None,
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        snapshot_id: frozen.snapshot.snapshot_id.clone(),
        instruction: MEMORY_INSTRUCTION.to_owned(),
        mandatory_handles: Vec::new(),
        transient_mandatory_handles: None,
        safe_brief: None,
        scope: None,
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        response_contract: Some(MEMORY_RESPONSE_CONTRACT.to_owned()),
    };
    let packet_request = PacketRequest {
        lookup: None,
        packet_id: new_id(),
        session_id: new_id(),
        invocation_ordinal: "0".to_owned(),
        frozen: frozen.clone(),
        instruction: MEMORY_INSTRUCTION.to_owned(),
        sources: vec![source.clone()],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        response_contract: Some(MEMORY_RESPONSE_CONTRACT.to_owned()),
        workshop_metadata: None,
    };
    let packet = compile_packet(&packet_request).map_err(packet_error)?;
    context_packets::persist_compiled_packet_at(&tx, &prepare, &packet)?;
    let job_id = new_id();
    let stored_request = request.stored_request();
    tx.execute(
        "INSERT INTO memory_jobs(id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            job_id,
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            payload_hash,
            serde_json::to_string(&stored_request)?,
            request.expected.document_id,
            parse_version(&request.expected.version)?,
            request.expected.body_hash,
            source.descriptor.source.document_id,
            source.descriptor.source.revision_id,
            source.descriptor.source.body_hash,
            frozen.snapshot.snapshot_id,
            packet.receipt.packet_id,
            parse_version(&frozen.snapshot.context_source_epoch)?,
            parse_version(&frozen.policy.version)?,
            MemoryJobStatus::Queued.as_str(),
            MemoryDispatchState::Pending.as_str(),
        ],
    )?;
    let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(job)
}

pub fn begin_memory(host: &mut impl StoryHost, owner: MemoryOwner) -> CoreResult<MemoryDispatch> {
    validate_runtime_owner(host.info(), &owner)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_memory_job_row(&tx, &owner.job_id)?;
    validate_job_owner(&current, &owner)?;
    let status = MemoryJobStatus::parse(&current.status)?;
    if matches!(status, MemoryJobStatus::Queued) {
        if MemoryDispatchState::parse(&current.dispatch_state)? != MemoryDispatchState::Pending {
            return Err(CoreError::new(
                "MemoryDispatchConflict",
                "The queued memory job has already been claimed.",
            ));
        }
        let (frozen, namespace) =
            story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
        if namespace != owner.operation_namespace || frozen.snapshot.project_id != owner.project_id
        {
            return Err(CoreError::new(
                "MemoryProjectMismatch",
                "The memory snapshot belongs to another project namespace.",
            ));
        }
        ensure_current_policy(&tx, &frozen)?;
        ensure_memory_basis_current(&tx, &current)?;
        let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
        let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
        tx.execute(
            "UPDATE memory_jobs SET status='running',dispatch_state='dispatched',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status='queued' AND dispatch_state='pending'",
            [&owner.job_id],
        )?;
        let job = read_memory_job(&tx, &owner.job_id, true)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(MemoryDispatch {
            job,
            packet,
            source,
            newly_dispatched: true,
        });
    }
    if matches!(status, MemoryJobStatus::Running) {
        let (frozen, namespace) =
            story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
        if namespace != owner.operation_namespace || frozen.snapshot.project_id != owner.project_id
        {
            return Err(CoreError::new(
                "MemoryProjectMismatch",
                "The memory snapshot belongs to another project namespace.",
            ));
        }
        ensure_current_policy(&tx, &frozen)?;
        let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
        let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
        let job = read_memory_job(&tx, &owner.job_id, true)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(MemoryDispatch {
            job,
            packet,
            source,
            newly_dispatched: false,
        });
    }
    Err(CoreError::new(
        "MemoryJobSealed",
        "This memory job cannot be dispatched in its current state.",
    ))
}

/// Resolve an uncertain durable dispatch claim without submitting a
/// provider request.  This is intentionally owner based so recovery can
/// run without a renderer lease.
pub fn interrupt_memory_claim(
    host: &mut impl StoryHost,
    owner: MemoryOwner,
) -> CoreResult<MemoryJob> {
    validate_runtime_owner(host.info(), &owner)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_memory_job_row(&tx, &owner.job_id)?;
    validate_job_owner(&current, &owner)?;
    let status = MemoryJobStatus::parse(&current.status)?;
    let result = read_memory_result(&tx, &owner.job_id, true)?;
    if !matches!(
        status,
        MemoryJobStatus::Queued | MemoryJobStatus::Running | MemoryJobStatus::Stopping
    ) {
        // Terminal and already-interrupted claims are idempotent reads.
        let job = read_memory_job_with_policy(&tx, &owner.job_id, &current_policy_version(&tx)?)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(job);
    }
    if result.is_some() {
        return Err(CoreError::new(
            "InvalidMemoryLifecycle",
            "An active memory claim already has a terminal result.",
        ));
    }
    validate_memory_lifecycle(&current, None, false)?;
    tx.execute(
        "UPDATE memory_jobs SET status='interrupted',stop_reason='dispatch_outcome_unknown',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status IN ('queued','running','stopping')",
        [&owner.job_id],
    )?;
    let job = read_memory_job_with_policy(&tx, &owner.job_id, &current_policy_version(&tx)?)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(job)
}

pub fn stop_memory(
    host: &mut impl StoryHost,
    access: ProjectAccess,
    job_id: String,
) -> CoreResult<MemoryJob> {
    host.check_access(&access)?;
    check_id(&job_id)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_memory_job_row(&tx, &job_id)?;
    if current.project_id != access.project_id
        || current.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory job belongs to another project namespace.",
        ));
    }
    let status = MemoryJobStatus::parse(&current.status)?;
    let next = match status {
        MemoryJobStatus::Queued => Some(MemoryJobStatus::Stopped),
        MemoryJobStatus::Running => Some(MemoryJobStatus::Stopping),
        _ => None,
    };
    if let Some(next) = next {
        tx.execute(
            "UPDATE memory_jobs SET status=?,stop_reason='author_stopped',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status=?",
            params![next.as_str(), job_id, status.as_str()],
        )?;
    }
    let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(job)
}

pub fn complete_memory(
    host: &mut impl StoryHost,
    request: CompleteMemory,
) -> CoreResult<MemoryCompletion> {
    validate_runtime_owner(host.info(), &request.owner)?;
    validate_complete_memory(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_memory_job_row(&tx, &request.owner.job_id)?;
    validate_job_owner(&current, &request.owner)?;
    if let Some(saved) = read_memory_result(&tx, &request.owner.job_id, true)? {
        if result_matches(&saved, &request) {
            let policy = current_policy_version(&tx)?;
            let reveal = current.disclosure_policy_epoch.to_string() == policy;
            let result = if reveal {
                saved
            } else {
                read_memory_result(&tx, &request.owner.job_id, false)?.ok_or_else(|| {
                    CoreError::new(
                        "PersistenceUnavailable",
                        "The memory result could not be read.",
                    )
                })?
            };
            let job = read_memory_job_with_policy(&tx, &request.owner.job_id, &policy)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(MemoryCompletion { job, result });
        }
        return Err(CoreError::new(
            "MemoryResultConflict",
            "A terminal memory result is already durably recorded for this job.",
        ));
    }
    let status = MemoryJobStatus::parse(&current.status)?;
    let recovered_dispatch_claim = status == MemoryJobStatus::Interrupted
        && current.dispatch_state == MemoryDispatchState::Dispatched.as_str()
        && current.stop_reason.as_deref() == Some("recovered_unknown_external_outcome");
    if !matches!(status, MemoryJobStatus::Running | MemoryJobStatus::Stopping)
        && !recovered_dispatch_claim
    {
        return Err(CoreError::new(
            "MemoryJobNotRunning",
            "Claim the queued memory job before recording its terminal result.",
        ));
    }
    let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
    validate_delivery(&tx, &request, &packet)?;
    let (frozen, namespace) = story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
    if namespace != request.owner.operation_namespace
        || frozen.snapshot.project_id != request.owner.project_id
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory snapshot belongs to another project namespace.",
        ));
    }
    let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
    if source.descriptor.source.project_id != current.project_id
        || source.descriptor.source.document_id != current.source_document_id
        || source.descriptor.source.revision_id != current.source_revision_id
        || source.descriptor.source.body_hash != current.source_body_hash
    {
        return Err(CoreError::new(
            "InvalidMemoryJob",
            "The memory job source binding does not match its frozen snapshot.",
        ));
    }
    let (candidate, validation_error) =
        match validate_navigation_digest(request.raw_output.as_bytes(), &source) {
            Ok(candidate) => (Some(candidate), None),
            Err(error) => (None, Some(format!("{}: {}", error.code, error.detail))),
        };
    let (lifecycle, stop_reason) = if recovered_dispatch_claim {
        (
            MemoryJobStatus::Interrupted,
            Some("recovered_unknown_external_outcome"),
        )
    } else {
        terminal_state_with_cleanup(
            status,
            request.outcome,
            candidate.is_some(),
            request.cleanup,
        )
    };
    let candidate_json = candidate.as_ref().map(serde_json::to_string).transpose()?;
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
    tx.execute(
        "INSERT INTO memory_results(job_id,event_id,raw_output,raw_output_hash,candidate_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,validation_error,effective_identity,delivery_json,app_server_delivery_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            request.owner.job_id,
            request.event_id,
            request.raw_output,
            sha256_hex(request.raw_output.as_bytes()),
            candidate_json,
            provider_outcome_as_str(request.outcome),
            request.confirmed_stdin_bytes.as_deref().map(parse_decimal_u64).transpose()?.map(i64::try_from).transpose().map_err(|_| CoreError::new("InvalidRequest", "The provider stdin byte count is too large."))?,
            usage_json,
            request.cleanup.map(provider_cleanup_as_str),
            request.error,
            validation_error,
            request.effective_identity,
            delivery_json,
            request.app_server.as_ref().map(serde_json::to_string).transpose()?,
        ],
    )?;
    tx.execute(
        "UPDATE memory_jobs SET status=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND (status IN ('running','stopping') OR (status='interrupted' AND dispatch_state='dispatched' AND stop_reason='recovered_unknown_external_outcome'))",
        params![lifecycle.as_str(), stop_reason, request.owner.job_id,],
    )?;
    let policy = current_policy_version(&tx)?;
    let reveal = current.disclosure_policy_epoch.to_string() == policy;
    let result = read_memory_result(&tx, &request.owner.job_id, reveal)?.ok_or_else(|| {
        CoreError::new(
            "PersistenceUnavailable",
            "The memory result could not be read.",
        )
    })?;
    let job = read_memory_job_with_policy(&tx, &request.owner.job_id, &policy)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(MemoryCompletion { job, result })
}

pub fn install_memory(host: &mut impl StoryHost, owner: MemoryOwner) -> CoreResult<MemoryView> {
    validate_runtime_owner(host.info(), &owner)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_memory_job_row(&tx, &owner.job_id)?;
    validate_job_owner(&current, &owner)?;
    if let Some(view) = read_memory_view(&tx, &owner.job_id, true)? {
        let resolved = resolve_view_current(&tx, view)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(resolved);
    }
    if MemoryJobStatus::parse(&current.status)? != MemoryJobStatus::Completed {
        return Err(CoreError::new(
            "MemoryInstallBlocked",
            "Only a completed memory candidate can be installed.",
        ));
    }
    let result = read_memory_result(&tx, &owner.job_id, true)?.ok_or_else(|| {
        CoreError::new(
            "MemoryInstallBlocked",
            "The completed memory job has no result.",
        )
    })?;
    let candidate_json = result
        .candidate
        .as_ref()
        .ok_or_else(|| {
            CoreError::new(
                "MemoryCandidateInvalid",
                "The terminal memory output did not validate.",
            )
        })
        .and_then(|candidate| serde_json::to_string(candidate).map_err(CoreError::from))?;
    // The immutable frozen packet derives its view fingerprint from this
    // canonical candidate.  Compute it at installation time as a bounded
    // integrity check without adding mutable state to the view row.
    let candidate: DigestCandidate = serde_json::from_str(&candidate_json)?;
    navigation_content_hash(&candidate)?;
    let (frozen, namespace) = story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
    if namespace != owner.operation_namespace || frozen.snapshot.project_id != owner.project_id {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory snapshot belongs to another project namespace.",
        ));
    }
    ensure_current_policy(&tx, &frozen)?;
    let current_policy = current_policy_version(&tx)?;
    let current_epoch = current_source_epoch(&tx)?;
    let document = read_document(&tx, &current.target_document_id)?;
    let current_basis = document.head.version == current.target_version.to_string()
        && document.head.body_hash == current.target_body_hash
        && current_epoch == current.context_source_epoch.to_string()
        && current_policy == current.disclosure_policy_epoch.to_string();
    let view_id = new_id();
    tx.execute(
        "INSERT INTO memory_views(id,job_id,project_id,operation_namespace,document_id,target_version,target_body_hash,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,candidate_json,installed_current) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            view_id,
            owner.job_id,
            current.project_id,
            current.operation_namespace,
            current.target_document_id,
            current.target_version,
            current.target_body_hash,
            current.source_revision_id,
            current.source_body_hash,
            current.snapshot_id,
            current.packet_id,
            current.context_source_epoch,
            current.disclosure_policy_epoch,
            candidate_json,
            current_basis,
        ],
    )?;
    tx.execute(
        "INSERT INTO memory_view_sources(view_id,document_id,revision_id,body_hash) VALUES(?,?,?,?)",
        params![
            view_id,
            current.source_document_id,
            current.source_revision_id,
            current.source_body_hash
        ],
    )?;
    let view = read_memory_view(&tx, &owner.job_id, true)?.ok_or_else(|| {
        CoreError::new(
            "PersistenceUnavailable",
            "The generated memory view could not be read.",
        )
    })?;
    let view = resolve_view_current(&tx, view)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(view)
}

pub fn read_memory(
    host: &impl StoryHost,
    access: ProjectAccess,
    document_id: &str,
) -> CoreResult<MemoryRead> {
    host.check_access(&access)?;
    check_id(document_id)?;
    let policy = current_policy_version(host.db()?)?;
    // A recovered project rotates its live identity while retaining the
    // old memory rows as read-only history.  Read/list are therefore
    // document scoped; every mutating path still validates the live
    // project/namespace owner before it can act on a job.
    let mut jobs = host
        .db()?
        .prepare("SELECT id FROM memory_jobs WHERE target_document_id=? ORDER BY created_at,id")?;
    let ids = jobs
        .query_map([document_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let jobs = ids
        .iter()
        .map(|id| read_memory_job_with_policy(host.db()?, id, policy.as_str()))
        .collect::<CoreResult<Vec<_>>>()?;
    let views = read_memory_views_for_document(host.db()?, document_id, policy.as_str())?;
    let pending_job_ids: Vec<String> = jobs
        .iter()
        .filter(|job| {
            job.status == MemoryJobStatus::Completed
                && job.view.is_none()
                && job
                    .result
                    .as_ref()
                    .is_some_and(|result| result.candidate.is_some())
        })
        .map(|job| job.id.clone())
        .collect();
    Ok(MemoryRead {
        document_id: document_id.to_owned(),
        jobs,
        views,
        pending_save: !pending_job_ids.is_empty(),
        pending_job_ids,
    })
}

pub fn read_memory_source(
    host: &impl StoryHost,
    access: ProjectAccess,
    view_id: &str,
) -> CoreResult<SourceRead> {
    host.check_access(&access)?;
    check_id(view_id)?;
    let db = host.db()?;
    // A view must be present in this opened database. Its original owner
    // is retained on recovery; this read grants no operation authority.
    let job_id: String = db
        .query_row(
            "SELECT job_id FROM memory_views WHERE id=?",
            [view_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "MemoryViewNotFound",
                "The memory view is not available in this project.",
            )
        })?;
    let job = read_memory_job_row(db, &job_id)?;
    validate_memory_job_record(db, &job)?;
    let (frozen, _) = validated_snapshot_record(db, &job.snapshot_id)?;
    ensure_current_policy(db, &frozen)?;
    read_source(db, &frozen, &job.source_revision_id)
}

pub fn list_memory(host: &impl StoryHost, access: ProjectAccess) -> CoreResult<MemoryList> {
    host.check_access(&access)?;
    let policy = current_policy_version(host.db()?)?;
    let mut jobs = host
        .db()?
        .prepare("SELECT id FROM memory_jobs ORDER BY created_at,id")?;
    let ids = jobs
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let jobs = ids
        .iter()
        .map(|id| read_memory_job_with_policy(host.db()?, id, policy.as_str()))
        .collect::<CoreResult<Vec<_>>>()?;
    let views = read_memory_views(host.db()?, policy.as_str())?;
    Ok(MemoryList { jobs, views })
}

/// Reopening never submits a provider request.  Active jobs become
/// interrupted with unknown external outcome and remain historical.
pub fn recover_interrupted_memory(host: &mut impl StoryHost) -> CoreResult<u32> {
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = tx.execute(
        "UPDATE memory_jobs SET status='interrupted',stop_reason='recovered_unknown_external_outcome',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('queued','running','stopping')",
        [],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(changed as u32)
}
