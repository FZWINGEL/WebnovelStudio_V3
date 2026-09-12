use super::*;
use wns_context::guidance::FrozenGuidance;

pub fn draft(db: &Connection, access: &ProjectAccess, run_id: &str) -> CoreResult<DiscussionRetry> {
    let (draft, frozen) = original(db, access, run_id)?;
    guidance::retry_request_guidance_at(db, access, &frozen)?;
    Ok(draft)
}

pub fn guidance(
    db: &Connection,
    request: &StartDiscussion,
) -> CoreResult<Option<Vec<FrozenGuidance>>> {
    let Some(previous) = request.previous_run_id.as_deref() else {
        return Ok(None);
    };
    validate_previous_run(
        db,
        Some(previous),
        &request.access,
        &request.expected.document_id,
    )?;
    let (original, frozen) = original(db, &request.access, previous)?;
    if request.basis != original.basis
        || request.intent != original.intent
        || request.instruction != original.text
        || request.scope != original.scope
        || request.pinned_document_ids != original.pinned_document_ids
        || request.safe_brief != original.safe_brief
        || request.lookup != original.lookup
    {
        return Err(CoreError::new(
            "RetryRequestChanged",
            "The feedback, selection, or included sources changed. Send this as a new request.",
        ));
    }
    Ok(Some(guidance::retry_request_guidance_at(
        db,
        &request.access,
        &frozen,
    )?))
}

fn original(
    db: &Connection,
    access: &ProjectAccess,
    run_id: &str,
) -> CoreResult<(DiscussionRetry, FrozenContext)> {
    let run = read_run(db, run_id)?;
    validate_previous_run(db, Some(run_id), access, &run.target.document_id)?;
    if run.status == DiscussionRunStatus::Completed {
        return Err(CoreError::new(
            "RetryAlreadyCompleted",
            "This discussion already completed. Send follow-up feedback as a new request.",
        ));
    }
    let started = read_start(db, run_id)?;
    // Request-facing reads enforce current permission, while retained packet
    // integrity remains independently verifiable for backup and recovery.
    let frozen = story_context::load_snapshot(db, access, &started.packet.receipt.snapshot_id)?;
    let intent = FeedbackIntent::from_purpose(frozen.purpose)?;
    let request_json: String = db.query_row(
        "SELECT request_json FROM context_packets WHERE id=?",
        [&run.packet_id],
        |row| row.get(0),
    )?;
    let prepared: PrepareContext = serde_json::from_str(&request_json)?;
    let retry_handles = prepared
        .transient_mandatory_handles
        .unwrap_or(prepared.mandatory_handles.clone());
    let pins = retry_handles
        .iter()
        .map(|handle| {
            frozen
                .snapshot
                .sources
                .iter()
                .find(|source| &source.handle == handle)
                .map(|source| source.source.document_id.clone())
                .ok_or_else(|| {
                    CoreError::new("InvalidContext", "An original included source is missing.")
                })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let scope = started
        .user_message
        .scope
        .filter(|_| intent != FeedbackIntent::Continue)
        .map(|scope| DiscussionScopeInput {
            kind: scope.kind,
            start: scope.start,
            end: scope.end,
            quote: scope.quote,
            source_body_hash: scope.source_hash,
        });
    Ok((
        DiscussionRetry {
            text: started.user_message.content,
            intent,
            basis: (intent == FeedbackIntent::Continue).then_some(frozen.snapshot.basis),
            scope,
            pinned_document_ids: pins,
            safe_brief: prepared.safe_brief,
            previous_run_id: run_id.to_owned(),
            lookup: prepared.lookup.map(|lookup| lookup.allowance),
        },
        frozen,
    ))
}
