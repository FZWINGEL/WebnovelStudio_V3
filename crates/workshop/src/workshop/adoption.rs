//! The adoption domain: what a reviewed draft may do to the working story,
//! and the validation that keeps a preview from claiming more than the
//! author approved — relationship provenance and freshness, adoption targets,
//! dependency ordering and protected text.
//!
//! Split out of `workshop.rs` along the lines §3.4 named.

use super::*;

pub(super) fn build_adoption_impacts(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
    candidate_ids: &[String],
    targets: &[WorkshopAdoptionTarget],
    impact_drafts: &[WorkshopImpactDraft],
) -> CoreResult<Vec<WorkshopAdoptionImpact>> {
    if impact_drafts.len() > MAX_LIST {
        return Err(CoreError::new(
            "InvalidRequest",
            "The adoption contains too many impact classifications.",
        ));
    }
    let mut overrides = HashMap::new();
    for draft in impact_drafts {
        // Workshop anchors are blank author-room implementation notes. Older
        // renderers may still send their model annotations as stale drafts;
        // discard them before validating override provenance.
        if is_workshop_anchor_id(&draft.document_id) {
            continue;
        }
        check_id(&draft.document_id)?;
        validate_text(&draft.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if draft.reason.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "An impact classification requires a reason.",
            ));
        }
        if overrides
            .insert(draft.document_id.as_str(), draft)
            .is_some()
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "An impact classification may appear only once per target.",
            ));
        }
    }
    let mut available = existing_document_ids(connection)?;
    available.extend(targets.iter().map(|target| target.document_id.clone()));
    let candidates = workshop_candidate_outputs_with_filter(
        connection,
        Some(project_id),
        Some(operation_namespace),
        Some(source_epoch),
    )?;
    let mut actual_targets = HashSet::new();
    let mut impacts = Vec::new();
    for candidate_id in candidate_ids {
        let Some((_, candidate, _relationship)) = candidates.get(candidate_id) else {
            return Err(CoreError::new(
                "InvalidWorkshopCandidate",
                "A workshop candidate is not a completed validated result from this project.",
            ));
        };
        for affected in &candidate.affected_targets {
            // A model can see the run's internal anchor in its frozen packet,
            // but it is never story material eligible for an adoption review
            // flag. Keep the raw candidate/packet history unchanged.
            if is_workshop_anchor_id(&affected.document_id) {
                continue;
            }
            if !available.contains(&affected.document_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopImpactTarget",
                    "A candidate affected target does not belong to this project or adoption.",
                ));
            }
            actual_targets.insert(affected.document_id.as_str());
            let (kind, reason) =
                if let Some(override_draft) = overrides.get(affected.document_id.as_str()) {
                    (override_draft.kind, override_draft.reason.clone())
                } else {
                    // Model output can identify affected material, but only the
                    // author may classify a contradiction or another stronger
                    // impact category.
                    (WorkshopImpactKind::PossibleTension, affected.reason.clone())
                };
            impacts.push(WorkshopAdoptionImpact {
                candidate_id: candidate_id.clone(),
                document_id: affected.document_id.clone(),
                kind,
                reason,
                status: WorkshopImpactStatus::NeedsReview,
            });
        }
    }
    if overrides
        .keys()
        .any(|document_id| !actual_targets.contains(document_id))
    {
        return Err(CoreError::new(
            "InvalidWorkshopImpactTarget",
            "An impact classification must refer to an actual candidate affected target.",
        ));
    }
    Ok(impacts)
}

pub(super) fn is_workshop_anchor_id(document_id: &str) -> bool {
    document_id.starts_with("workshop-")
}

pub(super) fn session_is_ancestor(
    state: &WorkshopState,
    session_id: &str,
    candidate_session_id: &str,
) -> bool {
    let sessions: HashMap<&str, &WorkshopSession> = state
        .sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect();
    let mut current = Some(session_id);
    for _ in 0..=MAX_SESSIONS {
        let Some(id) = current else { return false };
        if id == candidate_session_id {
            return true;
        }
        current = sessions
            .get(id)
            .and_then(|session| session.parent_session_id.as_deref());
    }
    false
}

/// A fixed decision constrains a request when the request names its source,
/// explicitly includes its source, explores a validated relationship endpoint,
/// or descends from the decision's exploration. Protection remains durable
/// across decision status changes; this predicate only controls packet context.
pub fn fixed_decision_is_relevant(
    state: &WorkshopState,
    session: &WorkshopSession,
    decision: &WorkshopDecision,
    relationship: Option<&WorkshopRelationship>,
) -> bool {
    session.focus_document_id.as_deref() == Some(decision.document_id.as_str())
        || session
            .included_document_ids
            .iter()
            .any(|document_id| document_id == &decision.document_id)
        || relationship.is_some_and(|relationship| {
            relationship.from_document_id == decision.document_id
                || relationship.to_document_id == decision.document_id
        })
        || session_is_ancestor(state, &session.id, &decision.session_id)
}

pub(super) fn candidate_relationship_matches_session(
    state: &WorkshopState,
    session_id: &str,
    candidate_relationship: Option<&WorkshopRelationship>,
) -> bool {
    let Some(session) = state
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    else {
        return false;
    };
    let candidate_id = candidate_relationship.map(|relationship| relationship.id.as_str());
    if session.relationship_id.as_deref() != candidate_id {
        return false;
    }
    let Some(expected) = candidate_relationship else {
        return true;
    };
    state
        .relationships
        .iter()
        .find(|relationship| relationship.id == expected.id)
        .is_some_and(|current| current == expected)
}

pub(super) fn validate_candidate_provenance(
    connection: &Connection,
    state: &WorkshopState,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
    requested_session: Option<&str>,
    candidate_ids: &[String],
) -> CoreResult<()> {
    let candidates = workshop_candidate_sessions(
        connection,
        project_id,
        operation_namespace,
        Some(source_epoch),
    )?;
    let historical_candidates = historical_workshop_candidate_records(connection)?
        .into_iter()
        .map(|(candidate_id, (session_id, _content))| (candidate_id, session_id))
        .collect::<HashMap<_, _>>();
    let validate_for = |session_id: &str, ids: &[String]| -> CoreResult<()> {
        for candidate_id in ids {
            let Some((candidate_session_id, candidate_relationship)) = candidates.get(candidate_id)
            else {
                let detail = if historical_candidates.contains_key(candidate_id) {
                    "A workshop candidate was generated against an older source epoch; generate a fresh result before adoption."
                } else {
                    "A workshop candidate is not a completed validated result from this project."
                };
                return Err(CoreError::new("InvalidWorkshopCandidate", detail));
            };
            if !session_is_ancestor(state, session_id, candidate_session_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate belongs to another exploration branch.",
                ));
            }
            if !candidate_relationship_matches_session(
                state,
                session_id,
                candidate_relationship.as_ref(),
            ) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate was generated for a different relationship scope; generate a fresh result before adoption.",
                ));
            }
        }
        Ok(())
    };
    let validate_historical = |session_id: &str, ids: &[String]| -> CoreResult<()> {
        for candidate_id in ids {
            let Some(candidate_session_id) = historical_candidates.get(candidate_id) else {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate is not a completed validated result from this project.",
                ));
            };
            if !session_is_ancestor(state, session_id, candidate_session_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate belongs to another exploration branch.",
                ));
            }
        }
        Ok(())
    };
    if let Some(session_id) = requested_session {
        validate_for(session_id, candidate_ids)?;
    }
    for session in &state.sessions {
        for detail in &session.selected_details {
            if let Some(candidate_id) = &detail.candidate_id {
                validate_historical(&session.id, std::slice::from_ref(candidate_id))?;
            }
        }
        validate_historical(
            &session.id,
            &session
                .choices
                .iter()
                .map(|choice| choice.candidate_id.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

pub(super) fn validate_relationship_freshness(
    connection: &Connection,
    state: &WorkshopState,
    previous: Option<&WorkshopState>,
) -> CoreResult<()> {
    let previous_relationships: HashMap<&str, &WorkshopRelationship> = previous
        .into_iter()
        .flat_map(|state| state.relationships.iter())
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect();
    for relationship in &state.relationships {
        if previous_relationships
            .get(relationship.id.as_str())
            .is_some_and(|old| *old == relationship)
        {
            // A stale facet remains visible for author review until the
            // author edits or replaces the relationship explicitly.
            continue;
        }
        for head in &relationship.source_heads {
            let current = read_document(connection, &head.document_id)?;
            if current.head != *head {
                return Err(CoreError::new(
                    "StaleRelationship",
                    "A relationship source changed; review the relationship before saving it.",
                ));
            }
        }
    }
    Ok(())
}


pub(super) fn body_blocks(value: &Value) -> CoreResult<&Vec<Value>> {
    value["body"]["content"].as_array().ok_or_else(|| {
        CoreError::new(
            "InvalidDocument",
            "The document body has no content blocks.",
        )
    })
}

pub(super) fn canonical_body(body: &Value) -> CoreResult<(Value, String)> {
    let receipt = validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidDocument", &error))?;
    Ok((receipt.snapshot, receipt.hash))
}


pub(super) fn target_fixed_text(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    document: &DocumentRecord,
) -> CoreResult<Vec<String>> {
    let session = state
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The adoption session does not exist."))?;
    let mut protected = session
        .selected_details
        .iter()
        .filter(|detail| {
            detail.fixed
                && !detail.text.is_empty()
                && body_text(&document.body).contains(&detail.text)
        })
        .map(|detail| detail.text.clone())
        .collect::<Vec<_>>();
    for decision in state.decisions.iter().filter(|decision| {
        // Protection is independent from decision status. Archiving or
        // superseding a decision must not release its protected source; the
        // author must explicitly clear Keep fixed first.
        decision.fixed && decision.document_id == document.head.document_id
    }) {
        if decision.protected_text.is_empty() {
            protected.push(body_text(
                &read_revision(connection, &decision.revision_id)?.body,
            ));
        } else {
            protected.extend(decision.protected_text.iter().cloned());
        }
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}





pub(super) fn validate_adoption_targets(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    targets: &mut [WorkshopAdoptionTarget],
) -> CoreResult<Vec<DocumentRecord>> {
    if targets.is_empty() || targets.len() > 64 {
        return Err(CoreError::new(
            "InvalidRequest",
            "Adoption requires 1..64 targets.",
        ));
    }
    let mut ids = HashSet::new();
    let mut before = Vec::new();
    for target in targets {
        check_id(&target.document_id)?;
        if !ids.insert(&target.document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Adoption target IDs must be unique.",
            ));
        }
        validate_title(&target.title)?;
        validate_kind(&target.kind)?;
        let (canonical, _) = canonical_body(&target.body)?;
        target.body = canonical;
        let current = match read_document(connection, &target.document_id) {
            Ok(document) => Some(document),
            Err(error) if error.code == "DocumentNotFound" => None,
            Err(error) => return Err(error),
        };
        match (&target.expected, current) {
            (None, Some(_)) => {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A new adoption target already exists.",
                ));
            }
            (None, None) if target.mode != AdoptionMode::Add => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New adoption targets must use add mode.",
                ));
            }
            (None, None) => {}
            (Some(expected), Some(current)) => {
                if expected != &current.head {
                    let mut error = CoreError::new(
                        "VersionConflict",
                        "An adoption target has changed since preview.",
                    );
                    error.current_head = Some(current.head.clone());
                    return Err(error);
                }
                if current.kind != target.kind || current.head.document_id != target.document_id {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An adoption target kind or identity changed.",
                    ));
                }
                if current.title != target.title {
                    return Err(CoreError::new(
                        "InvalidAdoption",
                        "An existing document title cannot change during workshop adoption.",
                    ));
                }
                let fixed = target_fixed_text(connection, state, session_id, &current)?;
                validate_protected_text(Some(&current), &target.body, &fixed, &fixed)?;
                if target.mode == AdoptionMode::Add {
                    let source = body_blocks(&current.body)?;
                    let result = body_blocks(&target.body)?;
                    if result.len() <= source.len() || result[..source.len()] != source[..] {
                        return Err(CoreError::new(
                            "InvalidAdoption",
                            "Append adoption must preserve every existing block exactly.",
                        ));
                    }
                }
                before.push(current);
            }
            (Some(_), None) => {
                return Err(CoreError::new(
                    "DocumentNotFound",
                    "An adoption target no longer exists.",
                ));
            }
        }
    }
    Ok(before)
}

pub(super) fn validate_relationship_draft_fields(draft: &WorkshopRelationshipDraft) -> CoreResult<()> {
    check_id(&draft.id)?;
    for (document_id, expected) in [
        (&draft.from_document_id, &draft.from_expected),
        (&draft.to_document_id, &draft.to_expected),
    ] {
        check_id(document_id)?;
        if let Some(expected) = expected {
            if expected.document_id != *document_id || !valid_hash(&expected.body_hash) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship endpoint has an invalid expected head.",
                ));
            }
            parse_version(&expected.version)?;
        }
    }
    if draft.from_document_id == draft.to_document_id {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship cannot connect a document to itself.",
        ));
    }
    for (value, label) in [
        (&draft.relationship_type, "relationship type"),
        (&draft.description, "relationship description"),
        (&draft.uncertainty, "relationship uncertainty"),
    ] {
        validate_text(value, label, MAX_DETAIL_BYTES)?;
    }
    if draft.relationship_type.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship requires a type.",
        ));
    }
    if draft.description.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship requires a description.",
        ));
    }
    Ok(())
}

pub(super) fn projected_target_heads(
    connection: &Connection,
    targets: &[WorkshopAdoptionTarget],
) -> CoreResult<HashMap<String, Head>> {
    let mut heads = HashMap::new();
    for target in targets {
        let (_, body_hash) = canonical_body(&target.body)?;
        let version = match read_document(connection, &target.document_id) {
            Ok(current) => parse_version(&current.head.version)?
                .checked_add(1)
                .ok_or_else(|| {
                    CoreError::new("VersionLimit", "The document version limit was reached.")
                })?,
            Err(error) if error.code == "DocumentNotFound" => 0,
            Err(error) => return Err(error),
        };
        heads.insert(
            target.document_id.clone(),
            Head {
                document_id: target.document_id.clone(),
                version: version.to_string(),
                body_hash,
            },
        );
    }
    Ok(heads)
}

pub(super) fn validate_relationship_drafts(
    connection: &Connection,
    state: &WorkshopState,
    targets: &[WorkshopAdoptionTarget],
    drafts: &[WorkshopRelationshipDraft],
) -> CoreResult<Vec<WorkshopRelationship>> {
    if drafts.len() > MAX_RELATIONSHIPS {
        return Err(CoreError::new(
            "InvalidRequest",
            "The adoption contains too many relationship drafts.",
        ));
    }
    let target_map: HashMap<&str, &WorkshopAdoptionTarget> = targets
        .iter()
        .map(|target| (target.document_id.as_str(), target))
        .collect();
    let mut ids: HashSet<&str> = state
        .relationships
        .iter()
        .map(|relationship| relationship.id.as_str())
        .collect();
    let projected = projected_target_heads(connection, targets)?;
    let mut resolved = Vec::with_capacity(drafts.len());
    for draft in drafts {
        validate_relationship_draft_fields(draft)?;
        if !ids.insert(draft.id.as_str()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A relationship draft ID is already in use.",
            ));
        }
        let mut endpoint_heads = Vec::with_capacity(2);
        for (document_id, expected) in [
            (&draft.from_document_id, &draft.from_expected),
            (&draft.to_document_id, &draft.to_expected),
        ] {
            let current = match read_document(connection, document_id) {
                Ok(current) => Some(current),
                Err(error) if error.code == "DocumentNotFound" => None,
                Err(error) => return Err(error),
            };
            let head = if let Some(current) = current {
                if !["character", "world"].contains(&current.kind.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "Workshop relationships may only connect character or world documents.",
                    ));
                }
                let Some(expected) = expected else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An existing relationship endpoint requires its current head.",
                    ));
                };
                if expected != &current.head {
                    let mut error = CoreError::new(
                        "VersionConflict",
                        "A relationship endpoint changed since this adoption was prepared.",
                    );
                    error.current_head = Some(current.head);
                    return Err(error);
                }
                if let Some(target) = target_map.get(document_id.as_str())
                    && target.kind != current.kind
                {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A relationship endpoint kind does not match its adoption target.",
                    ));
                }
                projected
                    .get(document_id.as_str())
                    .cloned()
                    .unwrap_or(current.head)
            } else {
                if expected.is_some() {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A new relationship endpoint must not include an expected head.",
                    ));
                }
                let Some(target) = target_map.get(document_id.as_str()) else {
                    return Err(CoreError::new(
                        "DocumentNotFound",
                        "A relationship endpoint does not belong to this project or adoption.",
                    ));
                };
                if target.expected.is_some() || target.mode != AdoptionMode::Add {
                    return Err(CoreError::new(
                        "VersionConflict",
                        "A new relationship endpoint must be an added adoption target.",
                    ));
                }
                if !["character", "world"].contains(&target.kind.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "Workshop relationships may only connect character or world documents.",
                    ));
                }
                projected
                    .get(document_id.as_str())
                    .cloned()
                    .ok_or_else(|| {
                        CoreError::new(
                            "InvalidProject",
                            "A relationship endpoint has no projected adoption head.",
                        )
                    })?
            };
            endpoint_heads.push(head);
        }
        resolved.push(WorkshopRelationship {
            id: draft.id.clone(),
            from_document_id: draft.from_document_id.clone(),
            to_document_id: draft.to_document_id.clone(),
            relationship_type: draft.relationship_type.clone(),
            description: draft.description.clone(),
            uncertainty: draft.uncertainty.clone(),
            status: WorkshopRelationshipStatus::Chosen,
            source_heads: endpoint_heads,
        });
    }
    Ok(resolved)
}

pub(super) fn materialize_relationship_drafts(
    drafts: &[WorkshopRelationshipDraft],
    heads: &HashMap<String, Head>,
) -> CoreResult<Vec<WorkshopRelationship>> {
    drafts
        .iter()
        .map(|draft| {
            let from = heads.get(&draft.from_document_id).cloned().ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A committed relationship endpoint head is missing.",
                )
            })?;
            let to = heads.get(&draft.to_document_id).cloned().ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A committed relationship endpoint head is missing.",
                )
            })?;
            Ok(WorkshopRelationship {
                id: draft.id.clone(),
                from_document_id: draft.from_document_id.clone(),
                to_document_id: draft.to_document_id.clone(),
                relationship_type: draft.relationship_type.clone(),
                description: draft.description.clone(),
                uncertainty: draft.uncertainty.clone(),
                status: WorkshopRelationshipStatus::Chosen,
                source_heads: vec![from, to],
            })
        })
        .collect()
}

pub(super) fn relationship_endpoint_sources(
    connection: &Connection,
    drafts: &[WorkshopRelationshipDraft],
) -> CoreResult<Vec<DocumentRecord>> {
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for document_id in drafts
        .iter()
        .flat_map(|draft| [&draft.from_document_id, &draft.to_document_id])
    {
        if !seen.insert(document_id.clone()) {
            continue;
        }
        match read_document(connection, document_id) {
            Ok(document) => sources.push(document),
            Err(error) if error.code == "DocumentNotFound" => {}
            Err(error) => return Err(error),
        }
    }
    Ok(sources)
}

pub(super) fn validate_preview_relationships(
    request: &PreviewWorkshopAdoption,
    preview: &WorkshopAdoptionPreview,
) -> CoreResult<()> {
    if request.relationships.len() != preview.relationships.len() {
        return Err(CoreError::new(
            "InvalidProject",
            "An adoption preview has mismatched relationship provenance.",
        ));
    }
    for (draft, relationship) in request.relationships.iter().zip(&preview.relationships) {
        validate_relationship_draft_fields(draft)?;
        if relationship.id != draft.id
            || relationship.from_document_id != draft.from_document_id
            || relationship.to_document_id != draft.to_document_id
            || relationship.relationship_type != draft.relationship_type
            || relationship.description != draft.description
            || relationship.uncertainty != draft.uncertainty
            || relationship.status != WorkshopRelationshipStatus::Chosen
            || relationship.source_heads.len() != 2
            || relationship.source_heads[0].document_id != draft.from_document_id
            || relationship.source_heads[1].document_id != draft.to_document_id
        {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview relationship is not bound to its request.",
            ));
        }
        for head in &relationship.source_heads {
            parse_version(&head.version)?;
            if !valid_hash(&head.body_hash) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An adoption preview relationship has an invalid source hash.",
                ));
            }
        }
    }
    for impact in &preview.impacts {
        check_id(&impact.candidate_id)?;
        check_id(&impact.document_id)?;
        validate_text(&impact.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if impact.status != WorkshopImpactStatus::NeedsReview
            || !request.candidate_ids.contains(&impact.candidate_id)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview impact has invalid provenance.",
            ));
        }
    }
    let mut relationship_heads = preview
        .relationships
        .iter()
        .flat_map(|relationship| relationship.source_heads.iter())
        .map(|head| {
            (
                head.document_id.as_str(),
                head.version.as_str(),
                head.body_hash.as_str(),
            )
        })
        .collect::<HashSet<_>>();
    relationship_heads.extend(preview.before.iter().map(|record| {
        (
            record.head.document_id.as_str(),
            record.head.version.as_str(),
            record.head.body_hash.as_str(),
        )
    }));
    for source in &preview.endpoint_sources {
        if !relationship_heads.contains(&(
            source.head.document_id.as_str(),
            source.head.version.as_str(),
            source.head.body_hash.as_str(),
        )) {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview endpoint source is not bound to a relationship head.",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_target_dependencies(
    state: &WorkshopState,
    targets: &[WorkshopAdoptionTarget],
    connection: &Connection,
) -> CoreResult<()> {
    let mut available = existing_document_ids(connection)?;
    available.extend(targets.iter().map(|target| target.document_id.clone()));
    for relationship in &state.relationships {
        if !available.contains(&relationship.from_document_id)
            || !available.contains(&relationship.to_document_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Adoption would leave a dangling relationship endpoint.",
            ));
        }
    }
    Ok(())
}
