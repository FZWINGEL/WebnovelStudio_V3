use super::*;

// ---------------------------------------------------------------------
// Actor-side logic, as free functions over the host.
// ---------------------------------------------------------------------

/// Resolve the current author-reviewed revision for a chapter export.
///
/// The selected head and current review status are authoritative here;
/// historical bundle validation alone is intentionally insufficient for a
/// new export.  The returned revision is the exact immutable checkpoint
/// recorded by the ready bundle.
pub fn resolve_reviewed_export_source(
    host: &impl StoryHost,
    access: &ProjectAccess,
    expected: &Head,
) -> CoreResult<(String, Revision)> {
    host.check_access(access)?;
    check_id(&expected.document_id)?;
    parse_version(&expected.version)?;

    let db = host.db()?;
    let document = read_document(db, &expected.document_id)?;
    if document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidDocument",
            "Reviewed export applies to chapter documents.",
        ));
    }
    require_head(&document.head, expected)?;

    let status = chapter_review_status(host, access.clone(), expected.document_id.as_str())?;
    if status.state == ReviewState::NoReview {
        return Err(CoreError::new(
            "ReviewRequired",
            "Mark this chapter reviewed before exporting the reviewed snapshot.",
        ));
    }
    if status.state != ReviewState::Ready {
        return Err(CoreError::new(
            "ReviewStale",
            "The reviewed chapter is no longer current; review it again before exporting.",
        ));
    }
    let bundle_id = status.active_bundle_id.ok_or_else(|| {
        CoreError::new(
            "ReviewRequired",
            "Mark this chapter reviewed before exporting the reviewed snapshot.",
        )
    })?;
    let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "ReviewSourceMismatch",
            "The current reviewed bundle is unavailable.",
        )
    })?;
    validate_reviewed_export_source(
        db,
        &access.project_id,
        &access.operation_namespace,
        &bundle_id,
        expected,
        &bundle.revision_id,
    )?;
    let revision = read_revision(db, &bundle.revision_id)?;
    Ok((bundle_id, revision))
}

pub fn read_reviewed_record_set_internal(
    host: &impl StoryHost,
    access: ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<ReviewedRecordSet>> {
    host.check_access(&access)?;
    check_id(document_id)?;
    let db = host.db()?;
    let document = read_document(db, document_id)?;
    if document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidDocument",
            "Reviewed evidence applies to chapter documents.",
        ));
    }
    let Some(bundle_id) = active_bundle_id(db, &access, document_id)? else {
        return Ok(None);
    };
    let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
        CoreError::new("InvalidProject", "The selected reviewed bundle is missing.")
    })?;
    if bundle.project_id != access.project_id
        || bundle.operation_namespace != access.operation_namespace
        || bundle.document_id != document_id
        || bundle.coverage != "authorOnly"
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The selected reviewed bundle crosses project identity.",
        ));
    }
    let revision = read_revision(db, &bundle.revision_id)?;
    let status = chapter_review_status(host, access, document_id)?;
    Ok(Some(ReviewedRecordSet {
        bundle_id,
        project_id: bundle.project_id,
        operation_namespace: bundle.operation_namespace,
        target: bundle.target,
        revision,
        records: bundle.records.unwrap_or_default(),
        records_hash: bundle.records_hash,
        promises: bundle.promises,
        promises_hash: bundle.promises_hash,
        knowledge: bundle.knowledge,
        knowledge_hash: bundle.knowledge_hash,
        summary: bundle.summary,
        summary_hash: bundle.summary_hash,
        current: status.state == ReviewState::Ready,
    }))
}

pub fn chapter_review_status(
    host: &impl StoryHost,
    access: ProjectAccess,
    document_id: &str,
) -> CoreResult<ReviewStatus> {
    host.check_access(&access)?;
    check_id(document_id)?;
    let db = host.db()?;
    let document = read_document(db, document_id)?;
    if document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidDocument",
            "Author review applies to chapter documents.",
        ));
    }
    let pending_stage_id = pending_stage_id(db, &access, document_id)?;
    let current_policy = current_epochs(db)?.1;
    let active_id = active_bundle_id(db, &access, document_id)?;
    let Some(active_id) = active_id.clone() else {
        let (can_stage, reason) = stage_capability(db, &access, document_id, current_policy)?;
        return Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::NoReview,
            active_bundle_id: None,
            pending_stage_id,
            reason,
            can_stage,
        });
    };
    let Some(bundle) = read_bundle(db, &active_id)? else {
        let (can_stage, _) = stage_capability(db, &access, document_id, current_policy)?;
        return Ok(status_needs_review(
            &document,
            active_id,
            "The selected reviewed bundle is missing.",
            can_stage,
            pending_stage_id.clone(),
        ));
    };
    if bundle.project_id != access.project_id
        || bundle.operation_namespace != access.operation_namespace
        || bundle.document_id != document_id
    {
        let (can_stage, _) = stage_capability(db, &access, document_id, current_policy)?;
        return Ok(status_needs_review(
            &document,
            active_id,
            "The selected reviewed bundle belongs to another project namespace.",
            can_stage,
            pending_stage_id.clone(),
        ));
    }
    let (can_stage, _stage_reason) = stage_capability(db, &access, document_id, current_policy)?;
    if bundle.target != document.head {
        return Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::ChangedProse,
            active_bundle_id: Some(active_id),
            pending_stage_id: pending_stage_id.clone(),
            reason: Some("The chapter changed after it was marked reviewed.".into()),
            can_stage,
        });
    }
    if bundle.policy_epoch != current_policy {
        return Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::ReviewNeeded,
            active_bundle_id: Some(active_id),
            pending_stage_id: pending_stage_id.clone(),
            reason: Some("The disclosure policy changed; review this chapter again.".into()),
            can_stage,
        });
    }
    match selected_prefix(db, &access, document_id, current_policy) {
        Ok(prefix) if same_prefix_basis(&prefix, &bundle.prefix) => Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::Ready,
            active_bundle_id: Some(active_id),
            pending_stage_id: None,
            reason: None,
            can_stage,
        }),
        Ok(_) => Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::EarlierBasisChanged,
            active_bundle_id: Some(active_id),
            pending_stage_id: pending_stage_id.clone(),
            reason: Some("An earlier reviewed chapter changed or needs review.".into()),
            can_stage,
        }),
        Err(error) if error.code == "ReviewBasisUnavailable" => Ok(ReviewStatus {
            document_id: document_id.to_owned(),
            title: document.title,
            head: document.head,
            state: ReviewState::EarlierBasisChanged,
            active_bundle_id: Some(active_id),
            pending_stage_id,
            reason: Some(error.detail),
            can_stage: false,
        }),
        Err(error) => Err(error),
    }
}

pub fn read_review_stage(
    host: &impl StoryHost,
    access: ProjectAccess,
    stage_id: &str,
) -> CoreResult<ReviewStage> {
    host.check_access(&access)?;
    check_id(stage_id)?;
    let db = host.db()?;
    let stage = read_stage(db, &access, stage_id)?.ok_or_else(|| {
        CoreError::new("ReviewStageNotFound", "This review stage is not available.")
    })?;
    stage_to_dto(db, stage)
}

pub fn stage_author_review(
    host: &mut impl StoryHost,
    request: StageAuthorReview,
) -> CoreResult<ReviewStage> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    parse_version(&request.expected.version)?;
    let payload_hash = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(existing) = existing_stage(&tx, &request.access, &request.operation_id)? {
        if existing.0 != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This review operation was already used for another request.",
            ));
        }
        let stage = read_stage(&tx, &request.access, &existing.1)?.ok_or_else(|| {
            CoreError::new("InvalidProject", "The saved review stage is missing.")
        })?;
        let result = stage_to_dto(&tx, stage)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(result);
    }
    let document = read_document(&tx, &request.expected.document_id)?;
    if document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidDocument",
            "Author review applies to chapter documents.",
        ));
    }
    require_head(&document.head, &request.expected)?;
    let revision = checkpoint_at(&tx, &document, "authorReview")?;
    let (source_epoch, policy_epoch) = current_epochs(&tx)?;
    let prefix = selected_prefix(
        &tx,
        &request.access,
        &document.head.document_id,
        policy_epoch,
    )?;
    let previous_bundle_id = active_bundle_id(&tx, &request.access, &document.head.document_id)?;
    let records = match request.records {
        Some(records) => Some(records),
        None => match previous_bundle_id.as_deref() {
            Some(id) => read_bundle(&tx, id)?.and_then(|bundle| bundle.records),
            None => None,
        },
    };
    let records_hash =
        validate_records(&records.clone().unwrap_or_default(), &revision).map_err(|error| {
            CoreError::new(
                "InvalidReviewedRecords",
                &format!("The reviewed evidence is invalid: {}", error.detail),
            )
        })?;
    let records_json = canonical_records_json(&records.clone().unwrap_or_default())?;
    let promises = match request.promises {
        Some(promises) => Some(promises),
        None => match previous_bundle_id.as_deref() {
            Some(id) => read_bundle(&tx, id)?.and_then(|bundle| bundle.promises),
            None => None,
        },
    };
    let promises_hash = validate_promises(&promises.clone().unwrap_or_default(), &revision)
        .map_err(|error| {
            CoreError::new(
                "InvalidReviewedPromises",
                &format!("The reviewed promises are invalid: {}", error.detail),
            )
        })?;
    let promises_json = canonical_promises_json(&promises.clone().unwrap_or_default())?;
    let knowledge = match request.knowledge {
        Some(knowledge) => Some(knowledge),
        None => match previous_bundle_id.as_deref() {
            Some(id) => read_bundle(&tx, id)?.and_then(|bundle| bundle.knowledge),
            None => None,
        },
    };
    let knowledge_hash = validate_knowledge(&knowledge.clone().unwrap_or_default(), &revision)
        .map_err(|error| {
            CoreError::new(
                "InvalidReviewedKnowledge",
                &format!("The reviewed knowledge is invalid: {}", error.detail),
            )
        })?;
    let knowledge_json = canonical_knowledge_json(&knowledge.clone().unwrap_or_default())?;
    let summary = resolve_summary(
        &tx,
        &request.access,
        &document.head,
        &revision,
        &prefix,
        previous_bundle_id.as_deref(),
        request.summary.as_ref(),
    )?;
    let summary_json = summary.as_ref().map(canonical_summary_json).transpose()?;
    let summary_hash = summary.as_ref().map(summary_hash).transpose()?;
    let prefix_hash = hash_prefix(&prefix)?;
    let stage_id = new_id();
    tx.execute(
        "INSERT INTO review_stages(id,project_id,operation_namespace,operation_id,payload_hash,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            stage_id,
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            payload_hash,
            document.head.document_id,
            parse_version(&document.head.version)?,
            document.head.body_hash,
            revision.id,
            source_epoch,
            policy_epoch,
            previous_bundle_id,
            serde_json::to_string(&prefix)?,
            prefix_hash,
            records_json,
            records_hash,
            promises_json,
            promises_hash,
            knowledge_json,
            knowledge_hash,
            summary_json,
            summary_hash,
        ],
    )?;
    let stage = read_stage(&tx, &request.access, &stage_id)?.ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "The review stage was not readable after insert.",
        )
    })?;
    let result = stage_to_dto(&tx, stage)?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

pub fn mark_ready(host: &mut impl StoryHost, request: MarkReady) -> CoreResult<ReadyBundle> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.stage_id)?;
    let payload_hash = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(existing) = existing_bundle(&tx, &request.access, &request.operation_id)? {
        if existing.0 != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This ready operation was already used for another request.",
            ));
        }
        let bundle = read_bundle(&tx, &existing.1)?.ok_or_else(|| {
            CoreError::new("InvalidProject", "The saved ready bundle is missing.")
        })?;
        let result = bundle_to_dto(bundle);
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(result);
    }
    let stage = read_stage(&tx, &request.access, &request.stage_id)?.ok_or_else(|| {
        CoreError::new("ReviewStageNotFound", "This review stage is not available.")
    })?;
    let (source_epoch, policy_epoch) = current_epochs(&tx)?;
    if stage.source_epoch != source_epoch || stage.policy_epoch != policy_epoch {
        return Err(CoreError::new(
            "ReviewStageStale",
            "The story changed while this review was staged. Prepare it again.",
        ));
    }
    let document = read_document(&tx, &stage.document_id)?;
    require_head(&document.head, &stage.target)?;
    if document.last_checkpoint_id.as_deref() != Some(stage.revision_id.as_str()) {
        return Err(CoreError::new(
            "ReviewStageStale",
            "The staged revision is no longer the current saved revision.",
        ));
    }
    let stage_revision = read_revision(&tx, &stage.revision_id)?;
    validate_records(&stage.records.clone().unwrap_or_default(), &stage_revision).map_err(
        |error| {
            CoreError::new(
                "InvalidReviewedRecords",
                &format!("The staged reviewed evidence is invalid: {}", error.detail),
            )
        },
    )?;
    validate_promises(&stage.promises.clone().unwrap_or_default(), &stage_revision).map_err(
        |error| {
            CoreError::new(
                "InvalidReviewedPromises",
                &format!("The staged reviewed promises are invalid: {}", error.detail),
            )
        },
    )?;
    validate_knowledge(
        &stage.knowledge.clone().unwrap_or_default(),
        &stage_revision,
    )
    .map_err(|error| {
        CoreError::new(
            "InvalidReviewedKnowledge",
            &format!("The staged reviewed knowledge is invalid: {}", error.detail),
        )
    })?;
    let prefix = selected_prefix(&tx, &request.access, &stage.document_id, policy_epoch)?;
    if !same_prefix_basis(&prefix, &stage.prefix) {
        return Err(CoreError::new(
            "ReviewStageStale",
            "An earlier reviewed chapter changed while this review was staged.",
        ));
    }
    let current_previous = active_bundle_id(&tx, &request.access, &stage.document_id)?;
    if current_previous != stage.previous_bundle_id {
        return Err(CoreError::new(
            "ReviewStageStale",
            "The selected reviewed head changed while this review was staged.",
        ));
    }
    let bundle_id = new_id();
    let prefix_json = serde_json::to_string(&stage.prefix)?;
    let summary_json = stage
        .summary
        .as_ref()
        .map(canonical_summary_json)
        .transpose()?;
    tx.execute(
        "INSERT INTO ready_bundles(id,project_id,operation_namespace,operation_id,payload_hash,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'authorOnly',?,?,?,?,?,?,?,?)",
        params![
            bundle_id,
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            payload_hash,
            stage.id,
            stage.document_id,
            parse_version(&stage.target.version)?,
            stage.target.body_hash,
            stage.revision_id,
            stage.source_epoch,
            stage.policy_epoch,
            stage.previous_bundle_id,
            prefix_json,
            stage.prefix_hash,
            canonical_records_json(&stage.records.clone().unwrap_or_default())?,
            stage.records_hash,
            canonical_promises_json(&stage.promises.clone().unwrap_or_default())?,
            stage.promises_hash,
            canonical_knowledge_json(&stage.knowledge.clone().unwrap_or_default())?,
            stage.knowledge_hash,
            summary_json,
            stage.summary_hash,
        ],
    )?;
    let target_position: i64 = tx.query_row(
        "SELECT position FROM documents WHERE id=? AND trashed=0 AND role='ordinary'",
        [&stage.document_id],
        |row| row.get(0),
    )?;
    let mut later = tx.prepare(
        "SELECT h.bundle_id FROM ready_heads h JOIN documents d ON d.id=h.document_id
         WHERE h.project_id=? AND h.operation_namespace=? AND d.kind='chapter' AND d.trashed=0 AND d.role='ordinary'
         AND (d.position>? OR (d.position=? AND d.id>?)) ORDER BY d.position,d.id LIMIT ?",
    )?;
    let later_ids = later
        .query_map(
            params![
                &request.access.project_id,
                &request.access.operation_namespace,
                target_position,
                target_position,
                &stage.document_id,
                (MAX_REVIEW_CHAPTERS + 1) as i64,
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if later_ids.len() > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "ReviewLimitExceeded",
            "There are too many later reviewed chapters to fence safely.",
        ));
    }
    drop(later);
    for affected_bundle_id in later_ids {
        tx.execute(
            "INSERT INTO review_fences(id,project_id,operation_namespace,affected_bundle_id,changed_document_id,changed_version,changed_body_hash,superseding_bundle_id,reason)
             VALUES(?,?,?,?,?,?,?,?,?)",
            params![
                new_id(),
                &request.access.project_id,
                &request.access.operation_namespace,
                affected_bundle_id,
                &stage.document_id,
                parse_version(&stage.target.version)?,
                &stage.target.body_hash,
                &bundle_id,
                "An earlier chapter was superseded; reaffirm the later chapter.",
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO ready_heads(project_id,operation_namespace,document_id,bundle_id)
         VALUES(?,?,?,?)
         ON CONFLICT(project_id,operation_namespace,document_id) DO UPDATE SET bundle_id=excluded.bundle_id",
        params![
            &request.access.project_id,
            &request.access.operation_namespace,
            &stage.document_id,
            &bundle_id,
        ],
    )?;
    // Selecting a new authority bundle is a source change for future
    // stages. Existing bundles do not become stale merely because this
    // project epoch advances; their exact target and prefix decide that.
    tx.execute(
        "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
        [],
    )?;
    let result = read_bundle(&tx, &bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "The ready bundle was not readable after insert.",
        )
    })?;
    let result = bundle_to_dto(result);
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}

/// The actor's dispatch for this concern.
///
/// This stayed behind in `webnovel-core` as a twelve-line delegation until the
/// move; it belongs here with the vocabulary it dispatches, because its arms
/// name `ReviewCommand` and call the five functions above.
pub fn handle_review(host: &mut impl StoryHost, command: ReviewCommand) {
    match command {
        ReviewCommand::Status(access, document_id, reply) => {
            let _ = reply.send(chapter_review_status(host, access, &document_id));
        }
        ReviewCommand::ReadStage(access, stage_id, reply) => {
            let _ = reply.send(read_review_stage(host, access, &stage_id));
        }
        ReviewCommand::Stage(request, reply) => {
            let result = stage_author_review(host, request);
            host.fence_uncertain(&result);
            let _ = reply.send(result);
        }
        ReviewCommand::Mark(request, reply) => {
            let result = mark_ready(host, request);
            host.fence_uncertain(&result);
            let _ = reply.send(result);
        }
        ReviewCommand::ReadRecords(access, document_id, reply) => {
            let _ = reply.send(read_reviewed_record_set_internal(
                host,
                access,
                &document_id,
            ));
        }
    }
}

// ---------------------------------------------------------------------
// Helpers. Verbatim: there is no `self.` below the impl block, so these
// travel unchanged.
// ---------------------------------------------------------------------
