//! Part of `packet`, split out of the file §3.4 measured at 4,149 lines.
//!
//! The items are `pub(super)` rather than private because a child module's
//! private items are not visible to its parent, and this module exists to be
//! called by it.

use super::*;

/// Validate every frozen view against the exact resolved source before any
/// budget branch is attempted. A generated candidate is never accepted merely
/// because its durable reference appears in the frozen context.
pub(super) fn validate_lookup_evidence(
    request: &PacketRequest,
    reads: &[CanonicalRead],
) -> Result<(), PacketError> {
    let Some(lookup) = &request.lookup else {
        return Ok(());
    };
    let invalid = |message: &str| source_binding("InvalidLookupEvidence", message, None);
    lookup
        .validate_capability()
        .map_err(|error| invalid(&error.to_string()))?;
    lookup
        .allowance
        .validate()
        .map_err(|error| invalid(&error.to_string()))?;
    if lookup.completed_invocations > lookup.allowance.max_additional_invocations
        || lookup.completed_invocations.to_string() != request.invocation_ordinal
        || (lookup.completed_invocations == 0) != lookup.exchanges.is_empty()
        || lookup.exchanges.len() > usize::from(lookup.completed_invocations) * 8
    {
        return Err(invalid(
            "Lookup evidence does not match its authorized invocation.",
        ));
    }
    let source = |handle: &str| -> Result<&CanonicalRead, PacketError> {
        let read = reads
            .iter()
            .find(|read| read.read.descriptor.handle == handle)
            .ok_or_else(|| invalid("A lookup source is outside the frozen story."))?;
        let target_handle = reads
            .iter()
            .find(|candidate| candidate.read.descriptor.source == request.frozen.snapshot.target)
            .expect("the packet target was validated before lookup evidence")
            .read
            .descriptor
            .handle
            .clone();
        let mut selected = vec![target_handle];
        if !selected.iter().any(|selected| selected == handle) {
            selected.push(handle.to_owned());
        }
        let eligible = evaluate_sources(
            &request.frozen.snapshot,
            &request.frozen.policy,
            request.frozen.purpose,
            &selected,
        )
        .map_err(PacketError::Eligibility)?;
        if !eligible
            .all_dependency_handles
            .iter()
            .any(|candidate| candidate == handle)
        {
            return Err(invalid(
                "A lookup source is unavailable under this request's policy.",
            ));
        }
        Ok(read)
    };
    let mut read_ids = HashSet::new();
    for exchange in &lookup.exchanges {
        lookup
            .authorize_read(&exchange.request)
            .map_err(|error| invalid(&error.to_string()))?;
        if !read_ids.insert(exchange.request.id()) {
            return Err(invalid("A lookup read appears more than once."));
        }
        if exchange.request.is_memory() {
            let expected =
                crate::memory_lookup::execute_memory_lookup(&request.frozen, &exchange.request)
                    .map_err(|error| invalid(&error.to_string()))?
                    .ok_or_else(|| invalid("A reviewed-memory read must have a typed result."))?;
            if exchange.result != expected {
                return Err(invalid(
                    "Reviewed-memory results do not match the exact frozen request and records.",
                ));
            }
            // Complete reviewed payloads were checked against canonical reads
            // before this function. Also fence each returned reference through
            // the same source-eligibility boundary used by text reads.
            let projection = LookupSourceProjection::from_exchanges(
                &request.frozen,
                std::slice::from_ref(exchange),
            )
            .map_err(|error| invalid(&error.to_string()))?;
            for returned in &projection.sources {
                if source(&returned.handle)?.read.descriptor.source != returned.source {
                    return Err(invalid("A reviewed-memory source identity changed."));
                }
            }
            continue;
        }
        match (&exchange.request, &exchange.result) {
            (
                LookupReadRequest::Read {
                    handle, block_ids, ..
                },
                LookupReadResult::Read {
                    handle: returned_handle,
                    source: returned_source,
                    passages,
                    complete,
                },
            ) => {
                let original = source(handle)?;
                if returned_handle != handle || returned_source != &original.read.descriptor.source
                {
                    return Err(invalid("Lookup source identity changed."));
                }
                let expected: Vec<_> = original
                    .passages
                    .iter()
                    .filter(|passage| {
                        block_ids
                            .as_ref()
                            .is_none_or(|ids| ids.contains(&passage.block_id))
                    })
                    .cloned()
                    .collect();
                if block_ids
                    .as_ref()
                    .is_some_and(|ids| ids.len() != expected.len())
                    || &expected != passages
                    || *complete != (expected == original.passages)
                {
                    return Err(invalid(
                        "Lookup passages do not exactly match their saved source and requested blocks.",
                    ));
                }
            }
            (
                LookupReadRequest::Search {
                    query, mode, limit, ..
                },
                LookupReadResult::Search { result },
            ) => {
                if result.snapshot_id != request.frozen.snapshot.snapshot_id
                    || result.searched_sources as usize != reads.len()
                    || result.hits.len() > *limit as usize
                    || result.source_matches.len() > *limit as usize
                    || result.coverage.is_empty()
                    || result.coverage.len() > 512
                    || result.coverage.chars().any(char::is_control)
                {
                    return Err(invalid(
                        "Lookup search coverage does not match its frozen request.",
                    ));
                }
                let expected = crate::frozen::search_saved_passages(
                    &request.frozen,
                    query.trim(),
                    *mode,
                    *limit,
                    |handle| {
                        reads
                            .iter()
                            .find(|read| read.read.descriptor.handle == handle)
                            .map(|read| read.passages.clone())
                            .ok_or_else(|| {
                                wns_kernel::CoreError::new(
                                    "InvalidLookupEvidence",
                                    "A frozen search source is missing.",
                                )
                            })
                    },
                )
                .map_err(|_| invalid("The search could not be reproduced from frozen sources."))?;
                if result.hits != expected.hits
                    || result.source_matches != expected.source_matches
                    || result.has_more != expected.has_more
                {
                    return Err(invalid(
                        "Lookup search results do not match the exact query and frozen sources.",
                    ));
                }
                let mut hits = HashSet::new();
                for hit in &result.hits {
                    let original = source(&hit.passage.handle)?;
                    if !original.passages.contains(&hit.passage)
                        || hit.start_utf16 >= hit.end_utf16
                        || !utf16_boundary(&hit.passage.text, hit.start_utf16)
                        || !utf16_boundary(&hit.passage.text, hit.end_utf16)
                        || !hits.insert((
                            &hit.passage.handle,
                            &hit.passage.block_id,
                            hit.start_utf16,
                            hit.end_utf16,
                        ))
                    {
                        return Err(invalid(
                            "A lookup search hit does not match its exact saved passage.",
                        ));
                    }
                }
                let mut matches = HashSet::new();
                for descriptor in &result.source_matches {
                    if descriptor != &source(&descriptor.handle)?.read.descriptor
                        || !matches.insert(&descriptor.handle)
                    {
                        return Err(invalid(
                            "A lookup source match does not match its frozen descriptor.",
                        ));
                    }
                }
            }
            (_, LookupReadResult::Unavailable { code, detail }) => {
                if code.is_empty()
                    || code.len() > 64
                    || !code.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    || detail.trim().is_empty()
                    || detail.len() > 512
                    || detail.chars().any(char::is_control)
                {
                    return Err(invalid("A lookup gap requires a bounded explanation."));
                }
            }
            _ => {
                return Err(invalid(
                    "The lookup response does not match its requested operation.",
                ));
            }
        }
    }
    if lookup.completed_invocations == 0 && lookup.source_projection.is_some() {
        return Err(invalid(
            "The initial lookup packet cannot contain a source projection.",
        ));
    }
    if let Some(projection) = &lookup.source_projection {
        if request.frozen.purpose != ContextPurpose::Discuss
            || request.frozen.policy.audience != Audience::AuthorRoom
            || request.frozen.snapshot.basis != crate::BasisKind::Working
        {
            return Err(invalid(
                "Lookup source labels require a working author-room discussion.",
            ));
        }
        validate_lookup_source_projection(request, lookup, projection)?;
    }
    Ok(())
}

pub(super) fn validate_lookup_source_projection(
    request: &PacketRequest,
    lookup: &LookupPacketInput,
    projection: &LookupSourceProjection,
) -> Result<(), PacketError> {
    let invalid = |message: &str| source_binding("InvalidLookupEvidence", message, None);
    if projection.schema_version != LOOKUP_SOURCE_PROJECTION_SCHEMA {
        return Err(invalid("The lookup source projection schema is unknown."));
    }
    let expected = LookupSourceProjection::from_exchanges(&request.frozen, &lookup.exchanges)
        .map_err(|error| invalid(&error.to_string()))?;
    if projection != &expected {
        return Err(invalid(
            "The lookup source projection does not match its frozen evidence.",
        ));
    }
    Ok(())
}

pub(super) fn utf16_boundary(text: &str, target: u32) -> bool {
    let mut offset = 0;
    for character in text.chars() {
        if offset == target {
            return true;
        }
        offset += character.len_utf16() as u32;
    }
    offset == target
}

pub(super) fn validate_navigation_views(
    request: &PacketRequest,
    reads: &[CanonicalRead],
) -> Result<Vec<ValidatedNavigationView>, PacketError> {
    let mut validated = Vec::with_capacity(request.frozen.navigation_views.len());
    for view in &request.frozen.navigation_views {
        let source = reads
            .iter()
            .find(|read| read.read.descriptor.source == view.candidate.source)
            .ok_or_else(|| {
                source_binding(
                    "NavigationSourceReadMissing",
                    "Every frozen navigation view must have its exact original source read.",
                    Some(view.reference.view_id.clone()),
                )
            })?;
        validate_navigation_view_payload(view, &source.read).map_err(|error| {
            PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(view.reference.view_id.clone()),
            }
        })?;
        let derived = DerivedView {
            reference: view.reference.clone(),
            dependencies: view.dependencies.clone(),
            candidate: view.candidate.clone(),
        };
        let representation_bytes =
            serde_json::to_vec(&derived).map_err(|error| PacketError::InvalidRequest {
                message: format!("failed to serialize frozen navigation view: {error}"),
            })?;
        let original_bytes =
            serde_json::to_vec(&source.body).map_err(|error| PacketError::InvalidRequest {
                message: format!("failed to serialize original source body: {error}"),
            })?;
        validated.push(ValidatedNavigationView {
            view: view.clone(),
            source_handle: source.read.descriptor.handle.clone(),
            representation_bytes: representation_bytes.len(),
            original_bytes: original_bytes.len(),
        });
    }
    Ok(validated)
}

/// Validate each authenticated frozen set against the exact source read before
/// choosing any representation or entering a budget path. Restricted writing
/// receives only reader-approved records; the frozen set and its original hash
/// remain complete so private records are never silently rewritten.
pub(super) fn validate_reviewed_evidence(
    request: &PacketRequest,
    reads: &[CanonicalRead],
) -> Result<Vec<PackedReviewedEvidence>, PacketError> {
    let mut validated = Vec::with_capacity(request.frozen.reviewed_evidence.len());
    for set in &request.frozen.reviewed_evidence {
        validate_frozen_evidence_set(
            set,
            &request.frozen.snapshot,
            &request.frozen.policy,
            request.frozen.purpose,
        )
        .map_err(|error| PacketError::SourceBinding {
            code: error.code,
            message: error.detail,
            handle: Some(set.source_handle.clone()),
        })?;
        let source = reads
            .iter()
            .find(|read| read.read.descriptor.handle == set.source_handle)
            .ok_or_else(|| {
                source_binding(
                    "ReviewedEvidenceSourceReadMissing",
                    "Every frozen reviewed evidence set needs its exact source read.",
                    Some(set.source_handle.clone()),
                )
            })?;
        validate_evidence_payload(set, &source.read).map_err(|error| {
            PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            }
        })?;
        let records: Vec<crate::story_records::PossessionRecord> =
            eligible_records(&set.records, request.frozen.policy.audience)
                .into_iter()
                .cloned()
                .collect();
        let projection_hash =
            records_hash(&records).map_err(|error| PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            })?;
        validated.push(PackedReviewedEvidence {
            set: set.clone(),
            records,
            projection_hash,
        });
    }
    Ok(validated)
}

/// Validate every frozen promise set and resolve its exact source read before
/// any budget branch.  The complete set remains authenticated; restricted
/// writing receives only its reader-approved projection.
pub(super) fn validate_reviewed_promises(
    request: &PacketRequest,
    reads: &[CanonicalRead],
) -> Result<Vec<PackedReviewedPromises>, PacketError> {
    let mut validated = Vec::with_capacity(request.frozen.reviewed_promises.len());
    for set in &request.frozen.reviewed_promises {
        validate_frozen_promise_set(
            set,
            &request.frozen.snapshot,
            &request.frozen.policy,
            request.frozen.purpose,
        )
        .map_err(|error| PacketError::SourceBinding {
            code: error.code,
            message: error.detail,
            handle: Some(set.source_handle.clone()),
        })?;
        let source = reads
            .iter()
            .find(|read| read.read.descriptor.handle == set.source_handle)
            .ok_or_else(|| {
                source_binding(
                    "ReviewedPromiseSourceReadMissing",
                    "Every frozen reviewed promise set needs its exact source read.",
                    Some(set.source_handle.clone()),
                )
            })?;
        validate_promise_payload(set, &source.read).map_err(|error| {
            PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            }
        })?;
        let records = eligible_promise_records(&set.records, request.frozen.policy.audience)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let projection_hash =
            promise_records_hash(&records).map_err(|error| PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            })?;
        validated.push(PackedReviewedPromises {
            set: set.clone(),
            records,
            projection_hash,
        });
    }
    Ok(validated)
}

/// Validate every frozen knowledge set and resolve its exact source read before
/// any budget branch.  The complete set remains authenticated; restricted
/// writing receives only its reader-approved projection.
pub(super) fn validate_reviewed_knowledge(
    request: &PacketRequest,
    reads: &[CanonicalRead],
) -> Result<Vec<PackedReviewedKnowledge>, PacketError> {
    let mut validated = Vec::with_capacity(request.frozen.reviewed_knowledge.len());
    let mut handles = HashSet::new();
    for set in &request.frozen.reviewed_knowledge {
        if !handles.insert(&set.source_handle) {
            return Err(source_binding(
                "InvalidReviewedKnowledge",
                "A reviewed knowledge source may appear only once in a frozen snapshot.",
                Some(set.source_handle.clone()),
            ));
        }
        validate_frozen_knowledge_set(
            set,
            &request.frozen.snapshot,
            &request.frozen.policy,
            request.frozen.purpose,
        )
        .map_err(|error| PacketError::SourceBinding {
            code: error.code,
            message: error.detail,
            handle: Some(set.source_handle.clone()),
        })?;
        let source = reads
            .iter()
            .find(|read| read.read.descriptor.handle == set.source_handle)
            .ok_or_else(|| {
                source_binding(
                    "ReviewedKnowledgeSourceReadMissing",
                    "Every frozen reviewed knowledge set needs its exact source read.",
                    Some(set.source_handle.clone()),
                )
            })?;
        validate_knowledge_payload(set, &source.read).map_err(|error| {
            PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            }
        })?;
        let records = eligible_knowledge_records(&set.records, request.frozen.policy.audience)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let projection_hash =
            knowledge_records_hash(&records).map_err(|error| PacketError::SourceBinding {
                code: error.code,
                message: error.detail,
                handle: Some(set.source_handle.clone()),
            })?;
        validated.push(PackedReviewedKnowledge {
            set: set.clone(),
            records,
            projection_hash,
        });
    }
    Ok(validated)
}

pub(super) fn reviewed_evidence_omissions(
    all: &[PackedReviewedEvidence],
    delivered: &[PackedReviewedEvidence],
) -> Vec<ReviewedEvidenceOmission> {
    let mut omissions = Vec::new();
    for set in all {
        let delivered_ids: HashSet<&str> = delivered
            .iter()
            .filter(|item| {
                item.set.source_handle == set.set.source_handle
                    && item.set.bundle_id == set.set.bundle_id
                    && item.set.records_hash == set.set.records_hash
            })
            .flat_map(|item| item.records.iter().map(record_id))
            .collect();
        let mut budget_count = 0;
        let mut disclosure_count = 0;
        for record in &set.set.records {
            if delivered_ids.contains(record.id.as_str()) {
                continue;
            }
            if set.records.iter().any(|item| item.id == record.id) {
                budget_count += 1;
            } else {
                disclosure_count += 1;
            }
        }
        for (reason, count) in [
            (ReviewedEvidenceOmissionReason::Budget, budget_count),
            (ReviewedEvidenceOmissionReason::Disclosure, disclosure_count),
        ] {
            if count != 0 {
                omissions.push(ReviewedEvidenceOmission {
                    source_handle: set.set.source_handle.clone(),
                    bundle_id: set.set.bundle_id.clone(),
                    records_hash: set.set.records_hash.clone(),
                    reason,
                    count,
                });
            }
        }
    }
    omissions
}

pub(super) fn reviewed_promise_omissions(
    all: &[PackedReviewedPromises],
    delivered: &[PackedReviewedPromises],
) -> Vec<ReviewedPromiseOmission> {
    let mut omissions = Vec::new();
    for set in all {
        let delivered_ids: HashSet<&str> = delivered
            .iter()
            .filter(|item| {
                item.set.source_handle == set.set.source_handle
                    && item.set.bundle_id == set.set.bundle_id
                    && item.set.records_hash == set.set.records_hash
            })
            .flat_map(|item| item.records.iter().map(|record| record.id.as_str()))
            .collect();
        let mut budget_count = 0;
        let mut disclosure_count = 0;
        for record in &set.set.records {
            if delivered_ids.contains(record.id.as_str()) {
                continue;
            }
            if set.records.iter().any(|item| item.id == record.id) {
                budget_count += 1;
            } else {
                disclosure_count += 1;
            }
        }
        for (reason, count) in [
            (ReviewedPromiseOmissionReason::Budget, budget_count),
            (ReviewedPromiseOmissionReason::Disclosure, disclosure_count),
        ] {
            if count != 0 {
                omissions.push(ReviewedPromiseOmission {
                    source_handle: set.set.source_handle.clone(),
                    bundle_id: set.set.bundle_id.clone(),
                    records_hash: set.set.records_hash.clone(),
                    reason,
                    count,
                });
            }
        }
    }
    omissions
}

pub(super) fn reviewed_knowledge_omissions(
    all: &[PackedReviewedKnowledge],
    delivered: &[PackedReviewedKnowledge],
) -> Vec<ReviewedKnowledgeOmission> {
    let mut omissions = Vec::new();
    for set in all {
        let delivered_ids: HashSet<&str> = delivered
            .iter()
            .filter(|item| {
                item.set.source_handle == set.set.source_handle
                    && item.set.bundle_id == set.set.bundle_id
                    && item.set.records_hash == set.set.records_hash
            })
            .flat_map(|item| item.records.iter().map(|record| record.id.as_str()))
            .collect();
        let mut budget_count = 0;
        let mut disclosure_count = 0;
        for record in &set.set.records {
            if delivered_ids.contains(record.id.as_str()) {
                continue;
            }
            if set.records.iter().any(|item| item.id == record.id) {
                budget_count += 1;
            } else {
                disclosure_count += 1;
            }
        }
        for (reason, count) in [
            (ReviewedKnowledgeOmissionReason::Budget, budget_count),
            (
                ReviewedKnowledgeOmissionReason::Disclosure,
                disclosure_count,
            ),
        ] {
            if count != 0 {
                omissions.push(ReviewedKnowledgeOmission {
                    source_handle: set.set.source_handle.clone(),
                    bundle_id: set.set.bundle_id.clone(),
                    records_hash: set.set.records_hash.clone(),
                    reason,
                    count,
                });
            }
        }
    }
    omissions
}

pub(super) fn optional_handles_without_views(
    optional_handles: &[String],
    delivered_views: &[FrozenNavigationView],
    navigation_by_handle: &HashMap<String, ValidatedNavigationView>,
) -> Vec<String> {
    let delivered_ids: HashSet<&str> = delivered_views
        .iter()
        .map(|view| view.reference.view_id.as_str())
        .collect();
    optional_handles
        .iter()
        .filter(|handle| {
            navigation_by_handle
                .get(handle.as_str())
                .is_none_or(|view| !delivered_ids.contains(view.view.reference.view_id.as_str()))
        })
        .cloned()
        .collect()
}

pub(super) fn navigation_source_omissions(
    delivered_views: &[FrozenNavigationView],
    navigation_by_handle: &HashMap<String, ValidatedNavigationView>,
) -> Vec<String> {
    delivered_views
        .iter()
        .filter_map(|view| {
            navigation_by_handle
                .values()
                .find(|validated| validated.view.reference.view_id == view.reference.view_id)
                .map(|validated| {
                    omission(
                        &validated.source_handle,
                        "navigation view delivered;original source omitted",
                    )
                })
        })
        .collect()
}

pub(super) fn navigation_omissions(
    views: &[ValidatedNavigationView],
    delivered_views: &[FrozenNavigationView],
    full_text_handles: HashSet<&str>,
) -> Vec<NavigationViewOmission> {
    let delivered_ids: HashSet<&str> = delivered_views
        .iter()
        .map(|view| view.reference.view_id.as_str())
        .collect();
    views
        .iter()
        .filter(|view| !delivered_ids.contains(view.view.reference.view_id.as_str()))
        .map(|view| NavigationViewOmission {
            view_id: view.view.reference.view_id.clone(),
            reason: if full_text_handles.contains(view.source_handle.as_str()) {
                NavigationOmissionReason::OriginalTextIncluded
            } else if view.representation_bytes >= view.original_bytes {
                NavigationOmissionReason::NotSmaller
            } else {
                NavigationOmissionReason::Budget
            },
        })
        .collect()
}

pub(super) fn validate_response_contract(request: &PacketRequest) -> Result<(), PacketError> {
    // Provider capability restrictions apply to every response contract,
    // including the early-return author-room contracts below. Keep this
    // check before those branches so a maintenance-only HTTP memory binding
    // cannot be reused for project chat or chapter discussion.
    validate_provider_capability(request)?;

    let is_project_chat = request.frozen.project_chat.is_some();
    let has_project_chat_contract =
        request.response_contract.as_deref() == Some(PROJECT_CHAT_RESPONSE_CONTRACT);
    if is_project_chat || has_project_chat_contract {
        if !is_project_chat
            || !has_project_chat_contract
            || request.frozen.snapshot.basis != crate::BasisKind::Working
            || request.frozen.purpose != ContextPurpose::Discuss
            || request.frozen.policy.audience != Audience::AuthorRoom
            || request.scope.is_some()
            || request.safe_brief.is_some()
            || request.lookup.is_some()
        {
            return Err(PacketError::InvalidRequest {
                message: "Project-chat packets require explicit project-chat metadata, the versioned response contract, and a Working author-room discussion without a writing scope.".to_owned(),
            });
        }
        let prompt_recipe_version = request
            .frozen
            .project_chat
            .as_ref()
            .and_then(|chat| chat.prompt_recipe_version.as_deref());
        project_chat_response_instruction(prompt_recipe_version)
            .map_err(|message| PacketError::InvalidRequest { message })?;
        return Ok(());
    }
    if request.response_contract.as_deref() == Some(CHAPTER_DISCUSSION_RESPONSE_CONTRACT) {
        if request.frozen.snapshot.basis != crate::BasisKind::Working
            || request.frozen.purpose != ContextPurpose::Discuss
            || request.frozen.policy.audience != Audience::AuthorRoom
            || request.scope.is_some()
            || request.safe_brief.is_some()
            || request.lookup.is_some()
        {
            return Err(PacketError::InvalidRequest {
                message: "The chapter discussion response contract requires a Working author-room discussion without a writing scope, brief, or lookup.".to_owned(),
            });
        }
        return Ok(());
    }
    if request.lookup.is_some()
        || request.response_contract.as_deref() == Some(LOOKUP_RESPONSE_CONTRACT)
    {
        if request.lookup.is_none()
            || request.response_contract.as_deref() != Some(LOOKUP_RESPONSE_CONTRACT)
            || request.frozen.purpose != ContextPurpose::Discuss
            || request.frozen.policy.audience != Audience::AuthorRoom
            || request.frozen.snapshot.basis != crate::BasisKind::Working
            || request.safe_brief.is_some()
        {
            return Err(PacketError::InvalidRequest {
                message:
                    "Story lookups require an explicitly authorized working author-room discussion."
                        .into(),
            });
        }
        return Ok(());
    }
    if request.frozen.purpose == ContextPurpose::MemoryAnalysis {
        if request.response_contract.as_deref() != Some(MEMORY_RESPONSE_CONTRACT)
            || request.scope.is_some()
            || request.safe_brief.is_some()
            || !request.mandatory_handles.is_empty()
            || !request.frozen.guidance.is_empty()
            || request.frozen.conversation.is_some()
            || !request.frozen.aliases.is_empty()
        {
            return Err(PacketError::InvalidRequest {
                message: "Chapter memory requires its dedicated response contract and exact chapter without additional instructions or edit scope.".to_owned(),
            });
        }
        return Ok(());
    }
    if request.response_contract.as_deref() == Some(WORKSHOP_RESPONSE_CONTRACT) {
        if request.frozen.purpose != ContextPurpose::StoryQuestion
            || request.frozen.policy.audience != Audience::AuthorRoom
            || request.frozen.snapshot.basis != crate::BasisKind::Working
            || request.scope.is_some()
            || request.safe_brief.is_some()
            || request.lookup.is_some()
        {
            return Err(PacketError::InvalidRequest {
                message: "The workshop response contract requires a Working AuthorRoom story question without an edit scope.".to_owned(),
            });
        }
        // Presence, not a re-parse: the builder owns parsing and validation of
        // its own instruction, and a workshop contract that arrives without
        // parsed metadata was not built by the workshop path.
        if request.workshop_metadata.is_none() {
            return Err(PacketError::InvalidRequest {
                message: "The workshop response contract requires parsed packet metadata.".to_owned(),
            });
        }
        return Ok(());
    }
    let Some(contract) = request.response_contract.as_deref() else {
        return Ok(());
    };
    if contract == CONTINUATION_RESPONSE_CONTRACT {
        if request.frozen.purpose != ContextPurpose::Continue
            || request.frozen.policy.audience != Audience::RestrictedWriting
            || request
                .scope
                .as_ref()
                .is_none_or(|scope| scope.kind != ScopeKind::Append)
        {
            return Err(PacketError::InvalidRequest {
                message: "the continuation response contract requires a restricted append request"
                    .to_owned(),
            });
        }
        return Ok(());
    }
    if contract == STRUCTURED_PROPOSAL_RESPONSE_CONTRACT {
        let author_room_development = author_room_structured_revision_allowed(
            &request.frozen.snapshot,
            &request.frozen.policy,
            request.frozen.purpose,
        );
        if request.provider_binding.is_none()
            || request.frozen.purpose != ContextPurpose::Revise
            || (!author_room_development
                && request.frozen.policy.audience != Audience::RestrictedWriting)
            || request.scope.as_ref().is_none_or(|scope| {
                !matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
            })
        {
            return Err(PacketError::InvalidRequest {
                message: "the structured proposal response contract requires a restricted block or whole-document revision request".to_owned(),
            });
        }
        return Ok(());
    }
    if contract != PROPOSAL_RESPONSE_CONTRACT {
        return Err(PacketError::InvalidRequest {
            message: "the response contract is unknown".to_owned(),
        });
    }
    if request.provider_binding.is_none()
        || request.frozen.purpose != ContextPurpose::Revise
        || request.scope.is_none()
    {
        return Err(PacketError::InvalidRequest {
            message: "the proposal response contract requires a live scoped revision request"
                .to_owned(),
        });
    }
    Ok(())
}

pub(super) fn validate_provider_capability(request: &PacketRequest) -> Result<(), PacketError> {
    if request
        .provider_binding
        .as_ref()
        .is_some_and(ProviderBinding::is_claude)
        && (request.lookup.is_some() || request.frozen.purpose == ContextPurpose::MemoryAnalysis)
    {
        return Err(PacketError::InvalidRequest {
            message: if request.lookup.is_some() {
                "Claude author requests are not qualified for story lookup.".to_owned()
            } else {
                "Claude author requests are not qualified for chapter memory analysis.".to_owned()
            },
        });
    }
    if let Some(binding) = request
        .provider_binding
        .as_ref()
        .filter(|binding| binding.is_http())
        && (request.lookup.is_some()
            || (request.frozen.purpose == ContextPurpose::MemoryAnalysis
                && !binding.is_http_memory())
            || (request.frozen.purpose != ContextPurpose::MemoryAnalysis
                && binding.is_http_memory()))
    {
        return Err(PacketError::InvalidRequest {
            message: if request.lookup.is_some() {
                "OpenAI-compatible HTTP is not qualified for story lookup.".to_owned()
            } else if binding.is_http_memory() {
                "The OpenAI-compatible chapter-memory profile is only valid for chapter memory analysis.".to_owned()
            } else {
                "The ordinary OpenAI-compatible profile is not qualified for chapter memory analysis.".to_owned()
            },
        });
    }
    Ok(())
}
