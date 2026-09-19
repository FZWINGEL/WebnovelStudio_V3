use super::*;

pub(crate) fn read_memory_view(
    db: &Connection,
    job_id: &str,
    reveal: bool,
) -> CoreResult<Option<MemoryView>> {
    let row: Option<MemoryViewColumns> = db
        .query_row(
            "SELECT id,job_id,project_id,operation_namespace,document_id,target_version,target_body_hash,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,candidate_json,installed_current,created_at FROM memory_views WHERE job_id=?",
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
                    row.get(15)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        saved_job_id,
        project_id,
        operation_namespace,
        document_id,
        target_version,
        target_body_hash,
        source_revision_id,
        source_body_hash,
        snapshot_id,
        packet_id,
        context_source_epoch,
        disclosure_policy_epoch,
        candidate_json,
        installed_current,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    check_id(&id)?;
    let candidate = if reveal {
        Some(serde_json::from_str(&candidate_json).map_err(|error| {
            CoreError::new(
                "InvalidProject",
                &format!("The saved memory view is invalid: {error}"),
            )
        })?)
    } else {
        None
    };
    Ok(Some(MemoryView {
        id,
        job_id: saved_job_id,
        project_id: project_id.clone(),
        operation_namespace,
        document_id: document_id.clone(),
        target: Head {
            document_id: document_id.clone(),
            version: target_version.to_string(),
            body_hash: target_body_hash,
        },
        source: SourceRef {
            project_id,
            document_id: document_id.clone(),
            revision_id: source_revision_id,
            body_hash: source_body_hash,
        },
        snapshot_id,
        packet_id,
        context_source_epoch: SourceEpoch::new(context_source_epoch.to_string()),
        disclosure_policy_version: disclosure_policy_epoch.to_string(),
        candidate,
        current: installed_current != 0,
        source_changed: false,
        policy_available: reveal,
        historical: false,
        created_at,
    }))
}

/// Validate one generated view through its owning immutable memory job.  The
/// caller supplies the enclosing story snapshot so a malformed database
/// cannot make a frozen navigation view validate itself recursively.
pub fn validate_navigation_view_record(
    db: &Connection,
    view_id: &str,
    enclosing_snapshot_id: Option<&str>,
) -> CoreResult<MemoryView> {
    check_id(view_id)?;
    let job_id: Option<String> = db
        .query_row(
            "SELECT job_id FROM memory_views WHERE id=?",
            [view_id],
            |row| row.get(0),
        )
        .optional()?;
    let job_id = job_id.ok_or_else(|| {
        CoreError::new(
            "InvalidMemoryStorage",
            "The frozen navigation view refers to a missing generated view.",
        )
    })?;
    let row = read_memory_job_row(db, &job_id)?;
    if enclosing_snapshot_id.is_some_and(|id| row.snapshot_id == id) {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A generated navigation view recursively refers to its enclosing snapshot.",
        ));
    }
    // Close the recursion boundary before the general memory validator asks
    // the context owner to validate this snapshot's immutable pins.
    let (snapshot_project, snapshot_namespace, snapshot_json, snapshot_hash): (
        String,
        String,
        String,
        String,
    ) = db.query_row(
        "SELECT project_id,operation_namespace,manifest_json,manifest_hash
         FROM story_snapshots WHERE id=?",
        [&row.snapshot_id],
        |query| Ok((query.get(0)?, query.get(1)?, query.get(2)?, query.get(3)?)),
    )?;
    let snapshot = crate::story_context::decode_snapshot(&snapshot_json, &snapshot_hash)?;
    if snapshot_project != row.project_id
        || snapshot_namespace != row.operation_namespace
        || snapshot.purpose != ContextPurpose::MemoryAnalysis
        || !snapshot.navigation_views.is_empty()
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A generated navigation view needs a closed MemoryAnalysis snapshot.",
        ));
    }
    validate_memory_job_record(db, &row)?;
    let view = read_memory_view(db, &job_id, true)?.ok_or_else(|| {
        CoreError::new(
            "InvalidMemoryStorage",
            "The generated navigation view could not be read after validation.",
        )
    })?;
    if view.id != view_id {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "The generated navigation view identity does not match its memory job.",
        ));
    }
    Ok(view)
}

pub(crate) fn resolve_view_current(
    db: &Connection,
    mut view: MemoryView,
) -> CoreResult<MemoryView> {
    let (current_project_id, current_namespace): (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let owner_current =
        view.project_id == current_project_id && view.operation_namespace == current_namespace;
    view.historical = !owner_current;
    let current_policy = current_policy_version(db)?;
    if view.disclosure_policy_version != current_policy {
        view.current = false;
        view.source_changed = false;
        view.policy_available = false;
        view.candidate = None;
        return Ok(view);
    }
    let current_epoch = current_source_epoch(db)?;
    let current = read_document(db, &view.document_id)?;
    if !owner_current {
        // Rotated/recovered rows remain visible as bounded history, but can
        // never look current or authorize work in the new project identity.
        view.current = false;
        view.source_changed = false;
    } else {
        let source_current = current.head == view.target;
        let generation_epoch = parse_version(&view.context_source_epoch)?;
        let current_epoch = parse_version(&current_epoch)?;
        // C4-C narrows freshness to the closed one-chapter MemoryAnalysis
        // chain.  A later unrelated source epoch does not stale an aid whose
        // exact source revision is still current; a future epoch is never
        // accepted as current.
        view.current = source_current
            && generation_epoch <= current_epoch
            && validate_navigation_view_record(db, &view.id, None).is_ok();
        view.source_changed = !source_current;
    }
    Ok(view)
}

pub(crate) fn read_memory_views_for_document(
    db: &Connection,
    document_id: &str,
    policy_version: &str,
) -> CoreResult<Vec<MemoryView>> {
    let mut statement = db.prepare(
        "SELECT v.job_id FROM memory_views v WHERE v.document_id=? ORDER BY v.created_at,v.id",
    )?;
    let ids = statement
        .query_map([document_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| {
            read_memory_view(db, id, policy_version == current_policy_version(db)?)?
                .ok_or_else(|| {
                    CoreError::new("MemoryViewNotFound", "The memory view is not available.")
                })
                .and_then(|view| resolve_view_current(db, view))
        })
        .collect()
}

pub(crate) fn read_memory_views(
    db: &Connection,
    policy_version: &str,
) -> CoreResult<Vec<MemoryView>> {
    let mut statement =
        db.prepare("SELECT v.job_id FROM memory_views v ORDER BY v.created_at,v.id")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| {
            read_memory_view(db, id, policy_version == current_policy_version(db)?)?
                .ok_or_else(|| {
                    CoreError::new("MemoryViewNotFound", "The memory view is not available.")
                })
                .and_then(|view| resolve_view_current(db, view))
        })
        .collect()
}

/// Validate every retained memory row without consulting today's current
/// document head or disclosure policy.  Transfer/backup validation uses this
/// historical path; request-facing reads add the current project/lease/policy
/// checks above.
pub fn validate_memory_storage(db: &Connection) -> CoreResult<()> {
    app_server::validate_dispatches(db)?;
    for table in [
        "memory_jobs",
        "memory_results",
        "memory_views",
        "memory_view_sources",
    ] {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table],
            |row| row.get(0),
        )?;
        if present != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                &format!("The memory backup is missing its {table} table."),
            ));
        }
    }
    // Prepare each table up front so a schema that happens to contain no jobs
    // still proves that all immutable memory tables are present and readable.
    db.prepare("SELECT job_id FROM memory_results")?;
    db.prepare("SELECT job_id FROM memory_views")?;
    db.prepare("SELECT view_id FROM memory_view_sources")?;
    let mut statement = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state,stop_reason FROM memory_jobs ORDER BY id",
    )?;
    let jobs = statement
        .query_map([], |row| {
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
                created_at: String::new(),
                updated_at: String::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for row in jobs {
        validate_memory_job_record(db, &row)?;
    }

    // Foreign-key enforcement is a connection setting.  Explicitly reject
    // orphan rows as well, so backup validation remains sound on a detached
    // connection that did not enable PRAGMA foreign_keys.
    let mut results = db.prepare("SELECT job_id FROM memory_results")?;
    let result_ids = results
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for job_id in result_ids {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_jobs WHERE id=?", [&job_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A memory result has no owning memory job.",
            ));
        }
    }
    let mut views = db.prepare("SELECT id,job_id FROM memory_views")?;
    let view_rows = views
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (view_id, job_id) in view_rows {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_jobs WHERE id=?", [&job_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view has no owning memory job.",
            ));
        }
        let source_rows: i64 = db.query_row(
            "SELECT COUNT(*) FROM memory_view_sources WHERE view_id=?",
            [&view_id],
            |row| row.get(0),
        )?;
        if source_rows != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view must have exactly one source dependency.",
            ));
        }
    }
    let mut source_rows = db.prepare("SELECT view_id FROM memory_view_sources")?;
    let source_view_ids = source_rows
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for view_id in source_view_ids {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_views WHERE id=?", [&view_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A memory view source has no owning generated view.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_memory_job_record(db: &Connection, row: &MemoryJobRow) -> CoreResult<()> {
    for value in [
        &row.id,
        &row.project_id,
        &row.operation_namespace,
        &row.operation_id,
        &row.target_document_id,
        &row.source_document_id,
        &row.source_revision_id,
        &row.snapshot_id,
        &row.packet_id,
    ] {
        check_id(value)?;
    }
    if !is_sha256(&row.payload_hash)
        || !is_sha256(&row.target_body_hash)
        || !is_sha256(&row.source_body_hash)
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job has an invalid fingerprint.",
        ));
    }
    let stored: StoredMemoryRequest = serde_json::from_str(&row.request_json).map_err(|error| {
        CoreError::new(
            "InvalidMemoryStorage",
            &format!("A memory request is malformed: {error}"),
        )
    })?;
    if stored.project_id != row.project_id
        || stored.operation_namespace != row.operation_namespace
        || stored.operation_id != row.operation_id
        || stored.expected.document_id != row.target_document_id
        || stored.expected.version != row.target_version.to_string()
        || stored.expected.body_hash != row.target_body_hash
        || sha256_hex(serde_json::to_string(&stored)?.as_bytes()) != row.payload_hash
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory request does not match its immutable job identity.",
        ));
    }
    let (frozen, namespace) = validated_snapshot_record(db, &row.snapshot_id)?;
    if namespace != row.operation_namespace
        || frozen.snapshot.project_id != row.project_id
        || frozen.purpose != ContextPurpose::MemoryAnalysis
        || frozen.snapshot.target.document_id != row.target_document_id
        || frozen.snapshot.target.revision_id != row.source_revision_id
        || frozen.snapshot.target.body_hash != row.source_body_hash
        || frozen.snapshot.context_source_epoch
            != SourceEpoch::new(row.context_source_epoch.to_string())
        || frozen.policy.version != row.disclosure_policy_epoch.to_string()
        || frozen.snapshot.sources.len() != 1
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job does not match its exact one-chapter snapshot.",
        ));
    }
    let source = read_source(db, &frozen, &row.source_revision_id)?;
    if source.descriptor.source.project_id != row.project_id
        || source.descriptor.source.document_id != row.source_document_id
        || source.descriptor.source.revision_id != row.source_revision_id
        || source.descriptor.source.body_hash != row.source_body_hash
        || row.target_body_hash != row.source_body_hash
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job source is not the frozen chapter revision.",
        ));
    }
    let source_version: i64 = db.query_row(
        "SELECT source_working_version FROM revisions WHERE document_id=? AND id=?",
        params![row.source_document_id, row.source_revision_id],
        |query| query.get(0),
    )?;
    if source_version != row.target_version {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory source revision does not match the frozen target version.",
        ));
    }
    let packet = validated_packet_record(db, &row.packet_id)?;
    if packet.receipt.snapshot_id != row.snapshot_id
        || packet.receipt.source_handles.len() != 1
        || packet.receipt.source_handles[0] != row.source_revision_id
        || packet.options.provider_binding != stored.provider_binding
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory packet does not match its job, source, or producer binding.",
        ));
    }
    let status = MemoryJobStatus::parse(&row.status)?;
    let result = read_memory_result(db, &row.id, true)?;
    validate_memory_lifecycle(
        row,
        result.as_ref(),
        packet.options.provider_binding.is_some(),
    )
    .map_err(|_| {
        CoreError::new(
            "InvalidMemoryStorage",
            "A memory job has an invalid lifecycle, dispatch state, stop reason, or result.",
        )
    })?;
    if let Some(result) = &result {
        let delivery = CompleteMemory {
            app_server: result.app_server.clone(),
            owner: MemoryOwner {
                project_id: row.project_id.clone(),
                operation_namespace: row.operation_namespace.clone(),
                job_id: row.id.clone(),
            },
            event_id: result.event_id.clone(),
            raw_output: result.raw_output.clone().ok_or_else(|| {
                CoreError::new(
                    "InvalidMemoryStorage",
                    "A retained memory result has no raw output.",
                )
            })?,
            outcome: result.outcome,
            confirmed_stdin_bytes: result.confirmed_stdin_bytes.clone(),
            usage: result.usage.clone(),
            cleanup: result.cleanup,
            error: result.error.clone(),
            effective_identity: result.effective_identity.clone(),
            delivery: result.delivery.clone(),
        };
        validate_delivery(db, &delivery, &packet).map_err(|error| {
            CoreError::new(
                "InvalidMemoryStorage",
                &format!("A retained memory result has invalid delivery proof: {error}"),
            )
        })?;
        if let Some(raw) = result.raw_output.as_deref() {
            let validated = validate_navigation_digest(raw.as_bytes(), &source);
            match (&result.candidate, validated) {
                (Some(saved), Ok(actual)) if saved == &actual => {}
                (None, Err(_)) => {}
                _ => {
                    return Err(CoreError::new(
                        "InvalidMemoryStorage",
                        "A memory result candidate does not match its retained raw output.",
                    ));
                }
            }
        }
        if status == MemoryJobStatus::Completed
            && (result.outcome != ProviderOutcomeStatus::Completed || result.candidate.is_none())
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A completed memory job must retain a completed provider outcome and valid candidate.",
            ));
        }
        if status == MemoryJobStatus::Failed
            && result.outcome == ProviderOutcomeStatus::Completed
            && result.candidate.is_some()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A failed memory job cannot retain a valid completed result as its terminal outcome.",
            ));
        }
    }
    let view = read_memory_view(db, &row.id, true)?;
    if let Some(view) = view {
        if status != MemoryJobStatus::Completed
            || view.project_id != row.project_id
            || view.operation_namespace != row.operation_namespace
            || view.document_id != row.target_document_id
            || view.target.version != row.target_version.to_string()
            || view.target.body_hash != row.target_body_hash
            || view.source.revision_id != row.source_revision_id
            || view.source.body_hash != row.source_body_hash
            || view.snapshot_id != row.snapshot_id
            || view.packet_id != row.packet_id
            || view.context_source_epoch != SourceEpoch::new(row.context_source_epoch.to_string())
            || view.disclosure_policy_version != row.disclosure_policy_epoch.to_string()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view is not bound to its job basis.",
            ));
        }
        let candidate = view.candidate.ok_or_else(|| {
            CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view has no candidate.",
            )
        })?;
        if validate_navigation_digest(serde_json::to_string(&candidate)?.as_bytes(), &source)
            .is_err()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view candidate is not valid for its source.",
            ));
        }
        if result.as_ref().and_then(|result| result.candidate.as_ref()) != Some(&candidate) {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view candidate differs from its terminal result.",
            ));
        }
        let source_count: i64 = db.query_row(
            "SELECT COUNT(*) FROM memory_view_sources WHERE view_id=?",
            [&view.id],
            |query| query.get(0),
        )?;
        if source_count != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view must retain its one exact source dependency.",
            ));
        }
        let linked: (String, String, String) = db.query_row(
            "SELECT document_id,revision_id,body_hash FROM memory_view_sources WHERE view_id=?",
            [&view.id],
            |query| Ok((query.get(0)?, query.get(1)?, query.get(2)?)),
        )?;
        if linked
            != (
                row.source_document_id.clone(),
                row.source_revision_id.clone(),
                row.source_body_hash.clone(),
            )
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view dependency is not source-bound.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn packet_error(error: PacketError) -> CoreError {
    CoreError::new("InvalidContextPacket", &error.to_string())
}
