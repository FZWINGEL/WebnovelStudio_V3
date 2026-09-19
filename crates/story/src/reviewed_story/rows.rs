use super::*;

pub(crate) fn current_epochs(db: &Connection) -> CoreResult<(i64, i64)> {
    db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(CoreError::from)
}

pub(crate) fn active_bundle_id(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<String>> {
    db.query_row(
        "SELECT bundle_id FROM ready_heads WHERE project_id=? AND operation_namespace=? AND document_id=?",
        params![access.project_id, access.operation_namespace, document_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(CoreError::from)
}

pub(crate) fn existing_stage(
    db: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<Option<(String, String)>> {
    db.query_row(
        "SELECT payload_hash,id FROM review_stages WHERE project_id=? AND operation_namespace=? AND operation_id=?",
        params![access.project_id, access.operation_namespace, operation_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

pub(crate) fn existing_bundle(
    db: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<Option<(String, String)>> {
    db.query_row(
        "SELECT payload_hash,id FROM ready_bundles WHERE project_id=? AND operation_namespace=? AND operation_id=?",
        params![access.project_id, access.operation_namespace, operation_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

pub(crate) fn parse_prefix(json: &str, expected_hash: &str) -> CoreResult<Vec<ReviewPrefixItem>> {
    if json.len() > 4 * 1024 * 1024 {
        return Err(CoreError::new(
            "InvalidProject",
            "The review prefix is too large.",
        ));
    }
    let prefix: Vec<ReviewPrefixItem> = serde_json::from_str(json)
        .map_err(|_| CoreError::new("InvalidProject", "The saved review prefix is invalid."))?;
    validate_prefix(&prefix)?;
    if hash_prefix(&prefix)? != expected_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved review prefix fingerprint is invalid.",
        ));
    }
    Ok(prefix)
}

pub(crate) fn validate_prefix(prefix: &[ReviewPrefixItem]) -> CoreResult<()> {
    if prefix.len() > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "ReviewLimitExceeded",
            "The reviewed chapter prefix is too large.",
        ));
    }
    let mut ids = HashSet::new();
    for item in prefix {
        check_id(&item.document_id)?;
        check_id(&item.bundle_id)?;
        check_id(&item.revision_id)?;
        validate_title(&item.title)?;
        parse_version(&item.head.version)?;
        if item.head.document_id != item.document_id || !ids.insert(&item.document_id) {
            return Err(CoreError::new(
                "InvalidProject",
                "The reviewed chapter prefix is not canonical.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn hash_prefix(prefix: &[ReviewPrefixItem]) -> CoreResult<String> {
    Ok(sha256_hex(serde_json::to_string(prefix)?.as_bytes()))
}

pub(crate) fn resolve_summary(
    db: &Connection,
    access: &ProjectAccess,
    target: &Head,
    revision: &Revision,
    prefix: &[ReviewPrefixItem],
    previous_bundle_id: Option<&str>,
    change: Option<&SummaryChange>,
) -> CoreResult<Option<SummaryRevision>> {
    let expected_source = SourceRef {
        project_id: access.project_id.clone(),
        document_id: target.document_id.clone(),
        revision_id: revision.id.clone(),
        body_hash: target.body_hash.clone(),
    };
    match change {
        Some(SummaryChange::Set { text, audience }) => {
            let summary = SummaryRevision {
                id: new_id(),
                text: text.clone(),
                audience: *audience,
                source: expected_source,
                dependencies: prefix.to_vec(),
            };
            validate_summary_binding(&summary, &access.project_id, &summary.source, prefix)?;
            Ok(Some(summary))
        }
        Some(SummaryChange::Clear) => Ok(None),
        None => {
            let Some(previous_bundle_id) = previous_bundle_id else {
                return Ok(None);
            };
            let previous = read_bundle(db, previous_bundle_id)?.ok_or_else(|| {
                CoreError::new("InvalidProject", "The previous reviewed bundle is missing.")
            })?;
            let Some(summary) = previous.summary else {
                return Ok(None);
            };
            if previous.target == *target
                && summary.source == expected_source
                && summary.dependencies == prefix
            {
                return Ok(Some(summary));
            }
            Err(CoreError::new(
                "ReviewSummaryRequired",
                "The reviewed summary basis changed; explicitly set or clear the summary.",
            ))
        }
    }
}

pub(crate) fn parse_record_set(
    records_json: Option<String>,
    records_hash: Option<String>,
    revision: &Revision,
) -> CoreResult<(Option<Vec<PossessionRecord>>, Option<String>)> {
    match (records_json, records_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed evidence JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            let records: Vec<PossessionRecord> = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed evidence is malformed: {error}"),
                )
            })?;
            if records.is_empty() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An empty reviewed evidence set must use the legacy null representation.",
                ));
            }
            let actual = validate_records(&records, revision).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed evidence is invalid: {}", error.detail),
                )
            })?;
            if actual.as_deref() != Some(hash.as_str())
                || canonical_records_json(&records)?.as_deref() != Some(json.as_str())
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed evidence hash or canonical JSON is invalid.",
                ));
            }
            Ok((Some(records), Some(hash)))
        }
    }
}

pub(crate) fn parse_promise_set(
    promises_json: Option<String>,
    promises_hash: Option<String>,
    revision: &Revision,
) -> CoreResult<(Option<Vec<PromiseRecord>>, Option<String>)> {
    match (promises_json, promises_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed promises JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            let promises: Vec<PromiseRecord> = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed promises are malformed: {error}"),
                )
            })?;
            if promises.is_empty() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An empty reviewed promise set must use the legacy null representation.",
                ));
            }
            let actual = validate_promises(&promises, revision).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed promises are invalid: {}", error.detail),
                )
            })?;
            if actual.as_deref() != Some(hash.as_str())
                || canonical_promises_json(&promises)?.as_deref() != Some(json.as_str())
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed promise hash or canonical JSON is invalid.",
                ));
            }
            Ok((Some(promises), Some(hash)))
        }
    }
}

pub(crate) fn parse_knowledge_set(
    knowledge_json: Option<String>,
    knowledge_hash: Option<String>,
    revision: &Revision,
) -> CoreResult<(Option<Vec<KnowledgeRecord>>, Option<String>)> {
    match (knowledge_json, knowledge_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed knowledge JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            let knowledge: Vec<KnowledgeRecord> = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed knowledge is malformed: {error}"),
                )
            })?;
            if knowledge.is_empty() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An empty reviewed knowledge set must use the legacy null representation.",
                ));
            }
            let actual = validate_knowledge(&knowledge, revision).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed knowledge is invalid: {}", error.detail),
                )
            })?;
            if actual.as_deref() != Some(hash.as_str())
                || canonical_knowledge_json(&knowledge)?.as_deref() != Some(json.as_str())
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed knowledge hash or canonical JSON is invalid.",
                ));
            }
            Ok((Some(knowledge), Some(hash)))
        }
    }
}

pub(crate) fn parse_summary_set(
    summary_json: Option<String>,
    expected_hash: Option<String>,
    project_id: &str,
    target: &Head,
    revision_id: &str,
    prefix: &[ReviewPrefixItem],
) -> CoreResult<(Option<SummaryRevision>, Option<String>)> {
    match (summary_json, expected_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed summary JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            if json.len() > 4 * 1024 * 1024 || !valid_hash(&hash) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed summary is too large or has an invalid hash.",
                ));
            }
            let summary: SummaryRevision = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed summary is malformed: {error}"),
                )
            })?;
            if canonical_summary_json(&summary)? != json || summary_hash(&summary)? != hash {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed summary hash or canonical JSON is invalid.",
                ));
            }
            let source = SourceRef {
                project_id: project_id.to_owned(),
                document_id: target.document_id.clone(),
                revision_id: revision_id.to_owned(),
                body_hash: target.body_hash.clone(),
            };
            validate_summary_binding(&summary, project_id, &source, prefix)?;
            Ok((Some(summary), Some(hash)))
        }
    }
}

pub(crate) fn read_stage(
    db: &Connection,
    access: &ProjectAccess,
    stage_id: &str,
) -> CoreResult<Option<StageRow>> {
    let row: Option<StageDbRow> = db
        .query_row(
            "SELECT id,project_id,operation_namespace,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash,created_at FROM review_stages WHERE id=? AND project_id=? AND operation_namespace=?",
            params![stage_id, access.project_id, access.operation_namespace],
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?,
                row.get(13)?, row.get(14)?, row.get(15)?, row.get(16)?, row.get(17)?, row.get(18)?, row.get(19)?, row.get(20)?,
            )),
        )
        .optional()?;
    let Some((
        id,
        project_id,
        operation_namespace,
        document_id,
        target_version,
        target_body_hash,
        target_revision_id,
        source_epoch,
        policy_epoch,
        previous_bundle_id,
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
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
    let revision = read_revision(db, &target_revision_id)?;
    if revision.head.document_id != document_id
        || revision.head.version != target_version.to_string()
        || revision.head.body_hash != target_body_hash
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The review stage revision does not match its target.",
        ));
    }
    let (records, records_hash) = parse_record_set(records_json, records_hash, &revision)?;
    let (promises, promises_hash) = parse_promise_set(promises_json, promises_hash, &revision)?;
    let (knowledge, knowledge_hash) =
        parse_knowledge_set(knowledge_json, knowledge_hash, &revision)?;
    let (summary, summary_hash) = parse_summary_set(
        summary_json,
        summary_hash,
        &project_id,
        &Head {
            document_id: document_id.clone(),
            version: parse_stored_version(target_version)?,
            body_hash: target_body_hash.clone(),
        },
        &target_revision_id,
        &prefix,
    )?;
    Ok(Some(StageRow {
        id,
        project_id,
        operation_namespace,
        document_id: document_id.clone(),
        target: Head {
            document_id,
            version: parse_stored_version(target_version)?,
            body_hash: target_body_hash,
        },
        revision_id: target_revision_id,
        source_epoch,
        policy_epoch,
        previous_bundle_id,
        prefix,
        prefix_hash,
        records,
        records_hash,
        promises,
        promises_hash,
        knowledge,
        knowledge_hash,
        summary,
        summary_hash,
        created_at,
    }))
}

pub(crate) fn stage_to_dto(db: &Connection, stage: StageRow) -> CoreResult<ReviewStage> {
    let revision = read_revision(db, &stage.revision_id)?;
    if revision.head != stage.target {
        return Err(CoreError::new(
            "InvalidProject",
            "The review stage revision does not match its target.",
        ));
    }
    Ok(ReviewStage {
        id: stage.id,
        project_id: stage.project_id,
        operation_namespace: stage.operation_namespace,
        target: stage.target,
        revision,
        previous_bundle_id: stage.previous_bundle_id,
        prefix: stage.prefix,
        records: stage.records,
        records_hash: stage.records_hash,
        promises: stage.promises,
        promises_hash: stage.promises_hash,
        knowledge: stage.knowledge,
        knowledge_hash: stage.knowledge_hash,
        summary: stage.summary,
        summary_hash: stage.summary_hash,
        source_epoch: SourceEpoch::new(parse_stored_version(stage.source_epoch)?),
        policy_epoch: parse_stored_version(stage.policy_epoch)?,
        created_at: stage.created_at,
    })
}

pub(crate) fn read_bundle(db: &Connection, bundle_id: &str) -> CoreResult<Option<BundleRow>> {
    let row: Option<BundleDbRow> = db
        .query_row(
            "SELECT id,project_id,operation_namespace,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,knowledge_json,knowledge_hash,summary_json,summary_hash,created_at FROM ready_bundles WHERE id=?",
            [bundle_id],
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?,
                row.get(15)?, row.get(16)?, row.get(17)?, row.get(18)?, row.get(19)?, row.get(20)?, row.get(21)?, row.get(22)?,
            )),
        )
        .optional()?;
    let Some((
        id,
        project_id,
        operation_namespace,
        stage_id,
        document_id,
        target_version,
        target_body_hash,
        target_revision_id,
        _source_epoch,
        policy_epoch,
        _previous_bundle_id,
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
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    let revision = read_revision(db, &target_revision_id)?;
    if revision.head.document_id != document_id
        || revision.head.version != target_version.to_string()
        || revision.head.body_hash != target_body_hash
    {
        return Err(CoreError::new(
            "InvalidProject",
            "A ready bundle revision does not match its target.",
        ));
    }
    let (records, records_hash) = parse_record_set(records_json, records_hash, &revision)?;
    let (promises, promises_hash) = parse_promise_set(promises_json, promises_hash, &revision)?;
    let (knowledge, knowledge_hash) =
        parse_knowledge_set(knowledge_json, knowledge_hash, &revision)?;
    let target = Head {
        document_id: document_id.clone(),
        version: parse_stored_version(target_version)?,
        body_hash: target_body_hash.clone(),
    };
    let (summary, summary_hash) = parse_summary_set(
        summary_json,
        summary_hash,
        &project_id,
        &target,
        &target_revision_id,
        &parse_prefix(&prefix_json, &prefix_hash)?,
    )?;
    Ok(Some(BundleRow {
        id,
        project_id,
        operation_namespace,
        stage_id,
        document_id: document_id.clone(),
        target,
        revision_id: target_revision_id,
        policy_epoch,
        prefix: parse_prefix(&prefix_json, &prefix_hash)?,
        coverage,
        records,
        records_hash,
        promises,
        promises_hash,
        knowledge,
        knowledge_hash,
        summary,
        summary_hash,
        created_at,
    }))
}

pub(crate) fn bundle_to_dto(bundle: BundleRow) -> ReadyBundle {
    ReadyBundle {
        id: bundle.id,
        project_id: bundle.project_id,
        operation_namespace: bundle.operation_namespace,
        stage_id: bundle.stage_id,
        target: bundle.target,
        records: bundle.records,
        records_hash: bundle.records_hash,
        promises: bundle.promises,
        promises_hash: bundle.promises_hash,
        knowledge: bundle.knowledge,
        knowledge_hash: bundle.knowledge_hash,
        summary: bundle.summary,
        summary_hash: bundle.summary_hash,
        created_at: bundle.created_at,
    }
}
