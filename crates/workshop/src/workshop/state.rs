//! Reading the workshop back out of storage: the snapshot a session started
//! from and the candidate projections the six lenses are assembled from.
//!
//! §3.4 named three concerns for `workshop.rs` — six-lens domain logic,
//! persistence, and generation orchestration — and this is the persistence
//! half. Generation is in `workshop_generation.rs`; the adoption domain is in
//! `workshop/adoption.rs`.

use super::*;

pub(super) fn workshop_snapshot(
    version: i64,
    state: WorkshopState,
) -> CoreResult<WorkshopSnapshot> {
    Ok(WorkshopSnapshot {
        version: parse_stored_version(version)?,
        state,
    })
}

/// Validate the authority for one immutable Workshop snapshot.
///
/// Normal Workshop writes have a matching `workshop_receipts` row and retain
/// that exact payload-hash check.  Chat-origin relationship adoption uses a
/// separate command authority rather than a `workshop_receipts` row: the
/// immutable project-chat command/preview/decision chain, validated by the helper in
/// `project_chat::adoption`.  Keeping the branch here makes both history reads
/// and backup validation use the same rule.
/// The chat-origin authority check, injected rather than called.
///
/// Its owner is `project_chat`, on the far side of the boundary this module is
/// about to cross. Injecting it keeps `validate_storage` — a backup validator
/// taking only a `&Connection`, one of twelve chained in `transfer.rs` — from
/// carrying a host it has no use for.
/// The chat-authority check, injected rather than called.
///
/// Its owner is `project_chat`, on the far side of the boundary this module
/// crossed. Injecting it kept `validate_storage` -- a backup validator taking
/// only a `&Connection`, one of twelve chained in `transfer.rs` -- from
/// carrying a host it has no use for. The lifetime is the caller's, because
/// the actor-side caller passes a closure that borrows the host.
pub(super) type ChatAuthorityCheck<'h> = dyn for<'a> Fn(
        &Connection,
        WorkshopSnapshotOrigin<'a>,
        &WorkshopState,
        &WorkshopState,
    ) -> CoreResult<()>
    + 'h;

pub(super) fn validate_snapshot_authority(
    connection: &Connection,
    validate_authority: &ChatAuthorityCheck<'_>,
    origin: WorkshopSnapshotOrigin<'_>,
    state_json: &str,
    state_hash: &str,
) -> CoreResult<WorkshopState> {
    let WorkshopSnapshotOrigin {
        project_id,
        namespace,
        operation,
        version,
        payload_hash,
    } = origin;
    if !storage_valid_id(project_id)
        || !storage_valid_id(namespace)
        || !storage_valid_id(operation)
        || version < 0
        || !valid_hash(payload_hash)
    {
        return Err(CoreError::new(
            "InvalidProject",
            "A workshop snapshot has invalid identity or hash metadata.",
        ));
    }
    let parsed = parse_state(state_json, state_hash)?;
    let receipt: Option<(String, String, String)> = connection
        .query_row(
            "SELECT operation_kind,payload_hash,result_json
             FROM workshop_receipts
             WHERE operation_namespace=? AND operation_id=?",
            params![namespace, operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match receipt {
        Some((kind, receipt_payload, result_json)) if receipt_payload == payload_hash => {
            let expected = workshop_snapshot(version, parsed.clone())?;
            match kind.as_str() {
                "saveWorkshop" => {
                    let saved: WorkshopSnapshot =
                        serde_json::from_str(&result_json).map_err(|error| {
                            CoreError::new(
                                "InvalidProject",
                                &format!("A workshop save receipt is invalid: {error}"),
                            )
                        })?;
                    if saved != expected {
                        return Err(CoreError::new(
                            "InvalidProject",
                            "A workshop snapshot does not match its save receipt result.",
                        ));
                    }
                }
                "adoptWorkshop" => {
                    let adopted: WorkshopAdoptionAck =
                        serde_json::from_str(&result_json).map_err(|error| {
                            CoreError::new(
                                "InvalidProject",
                                &format!("A workshop adoption receipt is invalid: {error}"),
                            )
                        })?;
                    if adopted.snapshot != expected {
                        return Err(CoreError::new(
                            "InvalidProject",
                            "A workshop snapshot does not match its adoption receipt result.",
                        ));
                    }
                }
                "startWorkshop" => {
                    return Err(CoreError::new(
                        "InvalidProject",
                        "A workshop start receipt cannot authorize a snapshot.",
                    ));
                }
                _ => {
                    return Err(CoreError::new(
                        "InvalidProject",
                        "A workshop snapshot has an unknown receipt kind.",
                    ));
                }
            }
            return Ok(parsed);
        }
        Some(_) => {
            return Err(CoreError::new(
                "InvalidProject",
                "A workshop history snapshot has a mismatched immutable receipt.",
            ));
        }
        None => {}
    }

    let previous: Option<(String, String)> = connection
        .query_row(
            "SELECT state_json,state_hash FROM workshop_snapshots
             WHERE version=? ORDER BY id DESC LIMIT 1",
            [version - 1],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let previous_state = match previous {
        Some((json, hash)) => parse_state(&json, &hash)?,
        None if version == 1 => WorkshopState::default(),
        None => {
            return Err(CoreError::new(
                "InvalidProject",
                "A chat-origin workshop snapshot has no preceding immutable state.",
            ));
        }
    };
    validate_authority(connection, origin, &parsed, &previous_state)?;
    Ok(parsed)
}

pub(super) fn validate_kind(kind: &str) -> CoreResult<()> {
    if !["note", "character", "world", "theme", "hook", "scene"].contains(&kind) {
        return Err(CoreError::new(
            "InvalidDocument",
            "Workshop adoption can target only nonchapter documents.",
        ));
    }
    Ok(())
}

pub(super) fn current_context_epoch(connection: &Connection) -> CoreResult<String> {
    let epoch: i64 = connection.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(epoch)
}

/// Return candidate IDs only from completed, integrity-checked workshop runs.
/// A candidate remains usable for a child what-if session through its parent
/// chain; its frozen source epoch must still be current at the mutation
/// boundary.
pub(super) fn workshop_candidate_sessions(
    source: CandidateReadContext<'_>,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, (String, Option<WorkshopRelationship>)>> {
    Ok(
        workshop_candidate_records(source, project_id, operation_namespace, source_epoch)?
            .into_iter()
            .map(|(candidate_id, (session_id, _content, relationship))| {
                (candidate_id, (session_id, relationship))
            })
            .collect(),
    )
}

pub(super) fn workshop_candidate_records(
    source: CandidateReadContext<'_>,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateRecord>> {
    workshop_candidate_records_with_filter(
        source,
        Some(project_id),
        Some(operation_namespace),
        source_epoch,
    )
}

pub(super) fn historical_workshop_candidate_records(
    source: CandidateReadContext<'_>,
) -> CoreResult<HashMap<String, (String, String)>> {
    Ok(
        workshop_candidate_records_with_filter(source, None, None, None)?
            .into_iter()
            .map(|(candidate_id, (session_id, content, _relationship))| {
                (candidate_id, (session_id, content))
            })
            .collect(),
    )
}

pub(super) fn workshop_candidate_records_with_filter(
    source: CandidateReadContext<'_>,
    project_id: Option<&str>,
    operation_namespace: Option<&str>,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateRecord>> {
    Ok(workshop_candidate_outputs_with_filter(
        source,
        project_id,
        operation_namespace,
        source_epoch,
    )?
    .into_iter()
    .map(|(candidate_id, (session_id, candidate, relationship))| {
        (candidate_id, (session_id, candidate.content, relationship))
    })
    .collect())
}

pub(super) fn workshop_candidate_outputs_with_filter(
    source: CandidateReadContext<'_>,
    project_id: Option<&str>,
    operation_namespace: Option<&str>,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateOutput>> {
    let connection = source.connection;
    let rows = (source.completed_outputs)(connection)?;
    let mut candidates = HashMap::new();
    for CompletedDiscussionOutput {
        run_id,
        packet_id,
        output_text,
    } in rows
    {
        let packet =
            match wns_story::context_packets::validated_packet_record(connection, &packet_id) {
                Ok(packet) => packet,
                Err(_) => continue,
            };
        let Some(instruction) = packet
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
        else {
            continue;
        };
        let metadata = match crate::workshop_generation::metadata_from_instruction(instruction) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let (frozen, namespace) = match wns_story::story_context::validated_snapshot_record(
            connection,
            &packet.receipt.snapshot_id,
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        if project_id.is_some_and(|project| frozen.snapshot.project_id != project)
            || operation_namespace.is_some_and(|expected| namespace != expected)
            || source_epoch.is_some_and(|epoch| frozen.snapshot.context_source_epoch != epoch)
        {
            continue;
        }
        let output = match crate::workshop_generation::validate_workshop_output(
            &output_text,
            &metadata,
            &run_id,
        ) {
            Ok(output) => output,
            Err(_) => continue,
        };
        for candidate in output.candidates {
            let candidate_id = candidate.id.clone();
            if candidates
                .insert(
                    candidate_id,
                    (
                        metadata.exploration.session_id.clone(),
                        candidate,
                        metadata.relationship.clone(),
                    ),
                )
                .is_some()
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "Two workshop results contain the same candidate identity.",
                ));
            }
        }
    }
    Ok(candidates)
}
