use super::*;

/// Authenticate a complete reviewed evidence array retained in a frozen
/// historical packet.  This intentionally does not consult today's selected
/// head, policy epoch, or ordered prefix.
#[allow(dead_code)]
pub fn validate_reviewed_records(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    source: &SourceRef,
    records_hash: &str,
    records: &[PossessionRecord],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_records(
        project_id,
        operation_namespace,
        bundle_id,
        source,
        records_hash,
        records,
    )?;
    Ok(())
}

/// Authenticate a complete reviewed promise array retained in a frozen
/// historical packet without consulting current selection or policy state.
#[allow(dead_code)]
pub fn validate_reviewed_promises(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    source: &SourceRef,
    promises_hash: &str,
    promises: &[PromiseRecord],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_promises(
        project_id,
        operation_namespace,
        bundle_id,
        source,
        promises_hash,
        promises,
    )?;
    Ok(())
}

/// Authenticate a complete reviewed knowledge array retained in a frozen
/// historical packet without consulting current selection or policy state.
#[allow(dead_code)]
pub fn validate_reviewed_knowledge(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    source: &SourceRef,
    knowledge_hash: &str,
    knowledge: &[KnowledgeRecord],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_knowledge(
        project_id,
        operation_namespace,
        bundle_id,
        source,
        knowledge_hash,
        knowledge,
    )?;
    Ok(())
}

/// Validate the immutable bundle provenance retained by a frozen reviewed
/// snapshot. This intentionally does not consult `ready_heads`: old
/// snapshots remain readable evidence after a later review supersedes a
/// selected head. New continuation requests use `selected_prefix` instead.
#[allow(dead_code)]
pub fn validate_reviewed_snapshot_manifest(
    db: &Connection,
    snapshot_project_id: &str,
    snapshot_namespace: &str,
    snapshot_policy_epoch: &str,
    manifest: &ReviewedBasisManifest,
    sources: &[SourceDescriptor],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_snapshot_manifest(
        snapshot_project_id,
        snapshot_namespace,
        snapshot_policy_epoch,
        manifest,
        sources,
    )?;
    Ok(())
}

pub(crate) fn same_manifest_prefix(
    actual: &[ReviewPrefixItem],
    expected: &[ReviewedBasisMember],
) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.document_id == expected.document_id
                && actual.bundle_id == expected.bundle_id
                && actual.revision_id == expected.revision_id
                && actual.head.version == expected.version
                && actual.head.body_hash == expected.body_hash
        })
}

pub(crate) fn same_prefix_basis(left: &[ReviewPrefixItem], right: &[ReviewPrefixItem]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.document_id == right.document_id
                && left.bundle_id == right.bundle_id
                && left.revision_id == right.revision_id
                && left.head == right.head
        })
}

pub(crate) fn stage_capability(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
    policy_epoch: i64,
) -> CoreResult<(bool, Option<String>)> {
    match selected_prefix(db, access, document_id, policy_epoch) {
        Ok(_) => Ok((true, None)),
        Err(error) if error.code == "ReviewBasisUnavailable" => Ok((false, Some(error.detail))),
        Err(error) => Err(error),
    }
}

pub(crate) fn validate_prefix_evidence(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    owner_document_id: &str,
    prefix: &[ReviewPrefixItem],
) -> CoreResult<()> {
    let mut validation = ReviewValidationContext::new(db);
    validation.validate_prefix_evidence_from_prefix(
        project_id,
        operation_namespace,
        owner_document_id,
        prefix,
    )?;
    Ok(())
}

pub(crate) fn validate_previous_bundle(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    owner_document_id: &str,
    previous_bundle_id: Option<&str>,
) -> CoreResult<()> {
    let Some(previous_bundle_id) = previous_bundle_id else {
        return Ok(());
    };
    let previous = read_bundle(db, previous_bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "A review record points to a missing previous bundle.",
        )
    })?;
    if previous.project_id != project_id
        || previous.operation_namespace != operation_namespace
        || previous.document_id != owner_document_id
    {
        return Err(CoreError::new(
            "InvalidProject",
            "A review record points to a previous bundle from another chapter or identity.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_ordinary_document_role(
    db: &Connection,
    document_id: &str,
) -> CoreResult<()> {
    let role: Option<String> = db
        .query_row(
            "SELECT role FROM documents WHERE id=?",
            [document_id],
            |row| row.get(0),
        )
        .optional()?;
    if role.as_deref() != Some(DocumentRole::Ordinary.storage_name()) {
        return Err(CoreError::new(
            "InvalidProject",
            "Reviewed story evidence points at a non-ordinary document.",
        ));
    }
    Ok(())
}

pub(crate) fn status_needs_review(
    document: &DocumentRecord,
    bundle_id: String,
    reason: &str,
    can_stage: bool,
    pending_stage_id: Option<String>,
) -> ReviewStatus {
    ReviewStatus {
        document_id: document.head.document_id.clone(),
        title: document.title.clone(),
        head: document.head.clone(),
        state: ReviewState::ReviewNeeded,
        active_bundle_id: Some(bundle_id),
        pending_stage_id,
        reason: Some(reason.to_owned()),
        can_stage,
    }
}

pub(crate) fn pending_stage_id(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<String>> {
    db.query_row(
        "SELECT s.id FROM review_stages s
         WHERE s.project_id=? AND s.operation_namespace=? AND s.document_id=?
         AND NOT EXISTS (SELECT 1 FROM ready_bundles b WHERE b.stage_id=s.id)
         ORDER BY s.rowid DESC LIMIT 1",
        params![access.project_id, access.operation_namespace, document_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(CoreError::from)
}

/// Validate the durable review tables without rejecting historical bundles
/// retained after recovery. Stale prose and stale prefixes are ordinary
/// historical states; only a current selected head can authorize future work.
pub fn validate_review_storage(db: &Connection) -> CoreResult<()> {
    let (project_id, namespace): (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let count: i64 = db.query_row("SELECT COUNT(*) FROM review_stages", [], |row| row.get(0))?;
    if count < 0 || count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical review stages.",
        ));
    }
    let count: i64 = db.query_row("SELECT COUNT(*) FROM ready_bundles", [], |row| row.get(0))?;
    if count < 0 || count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical ready bundles.",
        ));
    }
    let count: i64 = db.query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row.get(0))?;
    if count < 0 || count as usize > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many selected reviewed heads.",
        ));
    }
    let mut stages = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash FROM review_stages ORDER BY id",
    )?;
    let stage_rows = stages.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, Option<String>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
        ))
    })?;
    for row in stage_rows {
        let (
            id,
            stage_project,
            stage_namespace,
            operation_id,
            document_id,
            version,
            body_hash,
            revision_id,
            source_epoch,
            policy_epoch,
            previous,
            prefix_json,
            prefix_hash,
            records_json,
            records_hash,
            promises_json,
            promises_hash,
            knowledge_json,
            knowledge_hash,
            summary_json,
            summary_hash,
        ) = row?;
        check_id(&id)?;
        check_id(&stage_project)?;
        check_id(&stage_namespace)?;
        check_id(&operation_id)?;
        check_id(&document_id)?;
        check_id(&revision_id)?;
        validate_ordinary_document_role(db, &document_id)?;
        let _ = (
            parse_stored_version(version)?,
            parse_stored_version(source_epoch)?,
            parse_stored_version(policy_epoch)?,
        );
        if let Some(previous) = previous.as_deref() {
            check_id(previous)?;
            validate_previous_bundle(
                db,
                &stage_project,
                &stage_namespace,
                &document_id,
                Some(previous),
            )?;
        }
        let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
        validate_prefix_evidence(db, &stage_project, &stage_namespace, &document_id, &prefix)?;
        let revision = read_revision(db, &revision_id)?;
        if revision.head.document_id != document_id
            || revision.head.version != version.to_string()
            || revision.head.body_hash != body_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review stage revision does not match its target.",
            ));
        }
        let _ = parse_record_set(records_json, records_hash, &revision)?;
        let _ = parse_promise_set(promises_json, promises_hash, &revision)?;
        let _ = parse_knowledge_set(knowledge_json, knowledge_hash, &revision)?;
        let _ = parse_summary_set(
            summary_json,
            summary_hash,
            &stage_project,
            &Head {
                document_id: document_id.clone(),
                version: parse_stored_version(version)?,
                body_hash: body_hash.clone(),
            },
            &revision_id,
            &prefix,
        )?;
    }
    let mut bundles = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash FROM ready_bundles ORDER BY id",
    )?;
    let bundle_rows = bundles.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
            row.get::<_, Option<String>>(21)?,
            row.get::<_, Option<String>>(22)?,
            row.get::<_, Option<String>>(23)?,
        ))
    })?;
    for row in bundle_rows {
        let (
            id,
            bundle_project,
            bundle_namespace,
            operation_id,
            payload_hash,
            stage_id,
            document_id,
            version,
            body_hash,
            revision_id,
            source_epoch,
            policy_epoch,
            previous,
            prefix_json,
            prefix_hash,
            coverage,
            records_json,
            records_hash,
            promises_json,
            promises_hash,
            knowledge_json,
            knowledge_hash,
            summary_json,
            summary_hash,
        ) = row?;
        for id in [
            &id,
            &bundle_project,
            &bundle_namespace,
            &operation_id,
            &stage_id,
            &document_id,
            &revision_id,
        ] {
            check_id(id)?;
        }
        validate_ordinary_document_role(db, &document_id)?;
        if payload_hash.is_empty() || coverage != "authorOnly" {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle has invalid metadata.",
            ));
        }
        let _ = (
            parse_stored_version(version)?,
            parse_stored_version(source_epoch)?,
            parse_stored_version(policy_epoch)?,
        );
        if let Some(previous) = previous.as_deref() {
            check_id(previous)?;
            validate_previous_bundle(
                db,
                &bundle_project,
                &bundle_namespace,
                &document_id,
                Some(previous),
            )?;
        }
        let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
        validate_prefix_evidence(
            db,
            &bundle_project,
            &bundle_namespace,
            &document_id,
            &prefix,
        )?;
        let revision = read_revision(db, &revision_id)?;
        if revision.head.document_id != document_id
            || revision.head.version != version.to_string()
            || revision.head.body_hash != body_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle revision does not match its target.",
            ));
        }
        let (bundle_records, bundle_records_hash) =
            parse_record_set(records_json, records_hash, &revision)?;
        let (bundle_promises, bundle_promises_hash) =
            parse_promise_set(promises_json, promises_hash, &revision)?;
        let (bundle_knowledge, bundle_knowledge_hash) =
            parse_knowledge_set(knowledge_json, knowledge_hash, &revision)?;
        let (bundle_summary, bundle_summary_hash) = parse_summary_set(
            summary_json,
            summary_hash,
            &bundle_project,
            &Head {
                document_id: document_id.clone(),
                version: parse_stored_version(version)?,
                body_hash: body_hash.clone(),
            },
            &revision_id,
            &prefix,
        )?;
        let stage_identity: Option<(String, String)> = db
            .query_row(
                "SELECT project_id,operation_namespace FROM review_stages WHERE id=?",
                [&stage_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if stage_identity.as_ref() != Some(&(bundle_project.clone(), bundle_namespace.clone())) {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle references a foreign review stage.",
            ));
        }
        let stage_access = ProjectAccess {
            project_id: bundle_project.clone(),
            operation_namespace: bundle_namespace.clone(),
            session: "validation".into(),
            writer_lease: "validation".into(),
        };
        let stage = read_stage(db, &stage_access, &stage_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A ready bundle references a missing review stage.",
            )
        })?;
        if stage.target.document_id != document_id
            || stage.target.version != version.to_string()
            || stage.target.body_hash != body_hash
            || stage.revision_id != revision_id
            || stage.source_epoch != source_epoch
            || stage.policy_epoch != policy_epoch
            || stage.previous_bundle_id != previous
            || !same_prefix_basis(&stage.prefix, &prefix)
            || stage.prefix_hash != prefix_hash
            || stage.records != bundle_records
            || stage.records_hash != bundle_records_hash
            || stage.promises != bundle_promises
            || stage.promises_hash != bundle_promises_hash
            || stage.knowledge != bundle_knowledge
            || stage.knowledge_hash != bundle_knowledge_hash
            || stage.summary != bundle_summary
            || stage.summary_hash != bundle_summary_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle does not match its immutable review stage.",
            ));
        }
    }
    let mut heads = db.prepare("SELECT project_id,operation_namespace,document_id,bundle_id FROM ready_heads ORDER BY document_id")?;
    let head_rows = heads.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in head_rows {
        let (head_project, head_namespace, document_id, bundle_id) = row?;
        if head_project != project_id || head_namespace != namespace {
            return Err(CoreError::new(
                "InvalidProject",
                "A selected reviewed head belongs to a retired identity.",
            ));
        }
        check_id(&document_id)?;
        check_id(&bundle_id)?;
        validate_ordinary_document_role(db, &document_id)?;
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A selected reviewed head points to a missing bundle.",
            )
        })?;
        if bundle.project_id != project_id
            || bundle.operation_namespace != namespace
            || bundle.document_id != document_id
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A selected reviewed head crosses project identity.",
            ));
        }
    }
    let fence_count: i64 =
        db.query_row("SELECT COUNT(*) FROM review_fences", [], |row| row.get(0))?;
    if fence_count < 0 || fence_count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical review fences.",
        ));
    }
    let mut fences = db.prepare("SELECT id,project_id,operation_namespace,affected_bundle_id,changed_document_id,changed_version,changed_body_hash,superseding_bundle_id FROM review_fences ORDER BY id")?;
    let fence_rows = fences.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    for row in fence_rows {
        let (
            id,
            fence_project,
            fence_namespace,
            affected,
            changed_document,
            changed_version,
            changed_hash,
            superseding,
        ) = row?;
        for value in [
            &id,
            &fence_project,
            &fence_namespace,
            &affected,
            &changed_document,
            &superseding,
        ] {
            check_id(value)?;
        }
        let _ = parse_stored_version(changed_version)?;
        if changed_hash.is_empty()
            || read_bundle(db, &affected)?.is_none()
            || read_bundle(db, &superseding)?.is_none()
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review fence references invalid bundle evidence.",
            ));
        }
        let affected_bundle = read_bundle(db, &affected)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A review fence references a missing affected bundle.",
            )
        })?;
        let superseding_bundle = read_bundle(db, &superseding)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A review fence references a missing superseding bundle.",
            )
        })?;
        if affected_bundle.project_id != fence_project
            || affected_bundle.operation_namespace != fence_namespace
            || superseding_bundle.project_id != fence_project
            || superseding_bundle.operation_namespace != fence_namespace
            || superseding_bundle.target.document_id != changed_document
            || superseding_bundle.target.version != changed_version.to_string()
            || superseding_bundle.target.body_hash != changed_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review fence does not match its bundle evidence.",
            ));
        }
    }
    Ok(())
}
