use super::*;

pub fn selected_prefix(
    db: &Connection,
    access: &ProjectAccess,
    target_document_id: &str,
    policy_epoch: i64,
) -> CoreResult<Vec<ReviewPrefixItem>> {
    let target_position: i64 = db.query_row(
        "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0 AND role='ordinary'",
        [target_document_id],
        |row| row.get(0),
    )?;
    let mut statement = db.prepare(
        "SELECT id,title,position FROM documents WHERE kind='chapter' AND trashed=0 AND role='ordinary'
         AND (position<? OR (position=? AND id<?)) ORDER BY position,id LIMIT ?",
    )?;
    let rows = statement
        .query_map(
            params![
                target_position,
                target_position,
                target_document_id,
                (MAX_REVIEW_CHAPTERS + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "ReviewLimitExceeded",
            "There are too many earlier chapters to review safely.",
        ));
    }
    let mut result = Vec::with_capacity(rows.len());
    for (document_id, title, _) in rows {
        let bundle_id = active_bundle_id(db, access, &document_id)?.ok_or_else(|| {
            CoreError::new(
                "ReviewBasisUnavailable",
                "Every earlier chapter must be marked reviewed first.",
            )
        })?;
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier selected reviewed bundle is missing.",
            )
        })?;
        let current = read_document(db, &document_id)?;
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.target != current.head
            || bundle.policy_epoch != policy_epoch
        {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter needs review before this chapter can be marked reviewed.",
            ));
        }
        if !same_prefix_basis(&bundle.prefix, &result) {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter's reviewed basis is no longer valid.",
            ));
        }
        result.push(ReviewPrefixItem {
            document_id,
            title,
            bundle_id,
            revision_id: bundle.revision_id,
            head: current.head,
        });
    }
    Ok(result)
}

/// Validate immutable reviewed provenance for an export or historical read.
///
/// This deliberately does not consult `ready_heads`, current policy, or the
/// current ordered prefix. Those are required by
/// `resolve_reviewed_export_source` for a new export, while an already-recorded
/// export must remain independently verifiable after a later review replaces
/// the selected head.
pub fn validate_reviewed_export_source(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    expected: &Head,
    revision_id: &str,
) -> CoreResult<()> {
    check_id(project_id)?;
    check_id(operation_namespace)?;
    check_id(bundle_id)?;
    check_id(&expected.document_id)?;
    check_id(revision_id)?;
    parse_version(&expected.version)?;
    if !valid_hash(&expected.body_hash) {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export target has an invalid body fingerprint.",
        ));
    }

    let bundle = read_bundle(connection, bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "ReviewBundleNotFound",
            "The reviewed export bundle is not available.",
        )
    })?;
    if bundle.coverage != "authorOnly"
        || bundle.project_id != project_id
        || bundle.operation_namespace != operation_namespace
        || bundle.document_id != expected.document_id
        || bundle.target != *expected
        || bundle.revision_id != revision_id
    {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export bundle does not match its immutable source.",
        ));
    }

    validate_prefix_evidence(
        connection,
        project_id,
        operation_namespace,
        &expected.document_id,
        &bundle.prefix,
    )
    .map_err(|error| {
        CoreError::new(
            "ReviewSourceMismatch",
            &format!("The reviewed export prefix is invalid: {}", error.detail),
        )
    })?;

    let revision = read_revision(connection, revision_id).map_err(|error| {
        CoreError::new(
            "ReviewSourceMismatch",
            &format!("The reviewed export revision is invalid: {}", error.detail),
        )
    })?;
    if revision.id != revision_id || revision.head != *expected {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export revision does not match its immutable source.",
        ));
    }
    Ok(())
}

/// Resolve the explicit reviewed evidence attached to a set of exact sources
/// in one ordered prefix validation pass.
///
/// The single-source helper historically called [`selected_prefix`] for every
/// source.  That made a working-story freeze reread and reparse every growing
/// reviewed prefix.  This batch form reads each ordered chapter's selected
/// bundle once, while retaining the same distinction between an unavailable
/// current basis (`None`) and malformed persistence (`Err`).  Returned sets
/// retain the order of `sources`; sources without a current reviewed record
/// set are omitted.
pub fn current_records_for_sources(
    db: &Connection,
    access: &ProjectAccess,
    sources: &[SourceRef],
) -> CoreResult<Vec<ReviewedRecordSet>> {
    let policy_epoch = current_epochs(db)?.1;
    let mut candidates = Vec::new();
    for source in sources {
        check_id(&source.project_id)?;
        check_id(&source.document_id)?;
        check_id(&source.revision_id)?;
        if !valid_hash(&source.body_hash) || source.project_id != access.project_id {
            return Err(CoreError::new(
                "InvalidReviewedRecords",
                "The reviewed evidence source has invalid project or body identity.",
            ));
        }
        let Some(bundle_id) = active_bundle_id(db, access, &source.document_id)? else {
            continue;
        };
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new("InvalidProject", "The selected reviewed bundle is missing.")
        })?;
        let document = read_document(db, &source.document_id)?;
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != source.document_id
            || bundle.coverage != "authorOnly"
            || bundle.target != document.head
        {
            continue;
        }
        if bundle.policy_epoch != policy_epoch
            || bundle.target.body_hash != source.body_hash
            || bundle.revision_id != source.revision_id
        {
            continue;
        }
        let target_position: i64 = db.query_row(
            "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0 AND role='ordinary'",
            [&source.document_id],
            |row| row.get(0),
        )?;
        candidates.push(CurrentRecordCandidate {
            source: source.clone(),
            bundle,
            target_position,
        });
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let prefixes = batch_selected_prefixes(db, access, policy_epoch, &candidates)?;
    let mut result = Vec::new();
    for candidate in candidates {
        let Some(outcome) = prefixes.get(&candidate.source.document_id) else {
            continue;
        };
        match outcome {
            PrefixOutcome::Valid {
                matches_bundle: true,
            } => {}
            PrefixOutcome::Unavailable => continue,
            PrefixOutcome::Error(error) => return Err(error.clone()),
            PrefixOutcome::Valid {
                matches_bundle: false,
            } => continue,
        };
        let revision = read_revision(db, &candidate.bundle.revision_id)?;
        if revision.head.document_id != candidate.source.document_id
            || revision.head.body_hash != candidate.source.body_hash
            || revision.id != candidate.source.revision_id
        {
            continue;
        }
        if candidate.bundle.records.is_none()
            && candidate.bundle.promises.is_none()
            && candidate.bundle.knowledge.is_none()
            && candidate.bundle.summary.is_none()
        {
            continue;
        }
        result.push(ReviewedRecordSet {
            bundle_id: candidate.bundle.id,
            project_id: candidate.bundle.project_id,
            operation_namespace: candidate.bundle.operation_namespace,
            target: candidate.bundle.target,
            revision,
            records: candidate.bundle.records.unwrap_or_default(),
            records_hash: candidate.bundle.records_hash,
            promises: candidate.bundle.promises,
            promises_hash: candidate.bundle.promises_hash,
            knowledge: candidate.bundle.knowledge,
            knowledge_hash: candidate.bundle.knowledge_hash,
            summary: candidate.bundle.summary,
            summary_hash: candidate.bundle.summary_hash,
            current: true,
        });
    }
    Ok(result)
}

#[derive(Debug)]
pub(crate) struct CurrentRecordCandidate {
    source: SourceRef,
    bundle: BundleRow,
    target_position: i64,
}

#[derive(Debug)]
pub(crate) enum PrefixOutcome {
    Valid { matches_bundle: bool },
    Unavailable,
    Error(CoreError),
}

/// Resolve selected prefixes for all current candidates in one ordered walk.
/// The result is keyed by document ID because a source's exact revision was
/// already checked while building `CurrentRecordCandidate`.
pub(crate) fn batch_selected_prefixes(
    db: &Connection,
    access: &ProjectAccess,
    policy_epoch: i64,
    candidates: &[CurrentRecordCandidate],
) -> CoreResult<HashMap<String, PrefixOutcome>> {
    let max_target = candidates
        .iter()
        .max_by(|left, right| {
            left.target_position
                .cmp(&right.target_position)
                .then_with(|| left.source.document_id.cmp(&right.source.document_id))
        })
        .expect("batch prefix validation requires a candidate");
    let mut statement = db.prepare(
        "SELECT id,title,position FROM documents WHERE kind='chapter' AND trashed=0 AND role='ordinary'
         AND (position<? OR (position=? AND id<?)) ORDER BY position,id LIMIT ?",
    )?;
    let rows = statement
        .query_map(
            params![
                max_target.target_position,
                max_target.target_position,
                max_target.source.document_id,
                (MAX_REVIEW_CHAPTERS + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    let mut target_by_key = HashMap::new();
    let mut candidate_by_document = HashMap::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        candidate_by_document.insert(candidate.source.document_id.clone(), candidate_index);
        target_by_key.insert(
            (
                candidate.target_position,
                candidate.source.document_id.clone(),
            ),
            candidate_index,
        );
    }
    let mut outcomes = HashMap::new();
    for candidate in candidates {
        // `rows` is ordered by position and ID.  Count exact predecessors so
        // same-position chapters retain selected_prefix's lexical ordering.
        let prior_count = rows
            .iter()
            .take_while(|(document_id, _, position)| {
                *position < candidate.target_position
                    || (*position == candidate.target_position
                        && document_id < &candidate.source.document_id)
            })
            .count();
        if prior_count > MAX_REVIEW_CHAPTERS {
            outcomes.insert(
                candidate.source.document_id.clone(),
                PrefixOutcome::Error(CoreError::new(
                    "ReviewLimitExceeded",
                    "There are too many earlier chapters to review safely.",
                )),
            );
        }
    }

    let active_heads = load_active_bundle_ids(db, access)?;
    let mut prefix = Vec::new();
    let mut broken: Option<CoreError> = None;
    for (index, (document_id, title, position)) in rows.iter().enumerate() {
        match target_by_key.get(&(*position, document_id.clone())) {
            Some(candidate_index)
                if !outcomes.contains_key(&candidates[*candidate_index].source.document_id) =>
            {
                let target_id = &candidates[*candidate_index].source.document_id;
                outcomes.insert(
                    target_id.clone(),
                    match broken.as_ref() {
                        Some(error) if error.code == "ReviewBasisUnavailable" => {
                            PrefixOutcome::Unavailable
                        }
                        Some(error) => PrefixOutcome::Error(error.clone()),
                        None => PrefixOutcome::Valid {
                            matches_bundle: same_prefix_basis(
                                &candidates[*candidate_index].bundle.prefix,
                                &prefix,
                            ),
                        },
                    },
                );
            }
            _ => {}
        }
        if index >= MAX_REVIEW_CHAPTERS {
            broken.get_or_insert_with(|| {
                CoreError::new(
                    "ReviewLimitExceeded",
                    "There are too many earlier chapters to review safely.",
                )
            });
            continue;
        }
        if broken.is_some() {
            continue;
        }
        let Some(bundle_id) = active_heads.get(document_id) else {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "Every earlier chapter must be marked reviewed first.",
            ));
            continue;
        };
        let bundle = if let Some(candidate_index) = candidate_by_document.get(document_id) {
            candidates[*candidate_index].bundle.clone()
        } else {
            match read_bundle(db, bundle_id) {
                Ok(Some(bundle)) => bundle,
                Ok(None) => {
                    broken = Some(CoreError::new(
                        "ReviewBasisUnavailable",
                        "An earlier selected reviewed bundle is missing.",
                    ));
                    continue;
                }
                Err(error) => {
                    broken = Some(error);
                    continue;
                }
            }
        };
        let current = match read_document(db, document_id) {
            Ok(current) => current,
            Err(error) => {
                broken = Some(error);
                continue;
            }
        };
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != *document_id
            || bundle.coverage != "authorOnly"
            || bundle.target != current.head
            || bundle.policy_epoch != policy_epoch
        {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter needs review before this chapter can be marked reviewed.",
            ));
            continue;
        }
        if !same_prefix_basis(&bundle.prefix, &prefix) {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter's reviewed basis is no longer valid.",
            ));
            continue;
        }
        prefix.push(ReviewPrefixItem {
            document_id: document_id.clone(),
            title: title.clone(),
            bundle_id: bundle.id,
            revision_id: bundle.revision_id,
            head: current.head,
        });
    }

    for candidate in candidates {
        if outcomes.contains_key(&candidate.source.document_id) {
            continue;
        }
        outcomes.insert(
            candidate.source.document_id.clone(),
            match broken.as_ref() {
                Some(error) if error.code == "ReviewBasisUnavailable" => PrefixOutcome::Unavailable,
                Some(error) => PrefixOutcome::Error(error.clone()),
                None => PrefixOutcome::Valid {
                    matches_bundle: same_prefix_basis(&candidate.bundle.prefix, &prefix),
                },
            },
        );
    }
    Ok(outcomes)
}

pub(crate) fn load_active_bundle_ids(
    db: &Connection,
    access: &ProjectAccess,
) -> CoreResult<HashMap<String, String>> {
    let mut statement = db.prepare(
        "SELECT document_id,bundle_id FROM ready_heads WHERE project_id=? AND operation_namespace=?",
    )?;
    Ok(statement
        .query_map(
            params![access.project_id, access.operation_namespace],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .collect::<Result<HashMap<_, _>, _>>()?)
}
