use super::*;
use crate::context_packets;
use crate::story_context;
use wns_kernel::{CoreError, CoreResult};
use wns_providers::codex_app_server::{is_app_server, valid_identifier};
// Actor-side logic, as free functions over `StoryHost`.

pub fn claim_memory_app_server_dispatch(
    host: &mut impl StoryHost,
    owner: MemoryOwner,
    dispatch: AppServerDispatch,
) -> CoreResult<()> {
    validate_runtime_owner(host.info(), &owner)?;
    dispatch.validate()?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let job = read_memory_job_row(&tx, &owner.job_id)?;
    validate_job_owner(&job, &owner)?;
    if job.status != "running" || job.dispatch_state != "dispatched" {
        return Err(CoreError::new(
            "MemoryJobNotRunning",
            "This memory job no longer permits external dispatch.",
        ));
    }
    let packet = context_packets::validated_packet_record(&tx, &job.packet_id)?;
    let (frozen, _) = story_context::validated_snapshot_record(&tx, &job.snapshot_id)?;
    ensure_current_policy(&tx, &frozen)?;
    ensure_memory_basis_current(&tx, &job)?;
    if !packet
        .options
        .provider_binding
        .as_ref()
        .is_some_and(is_app_server)
        || !dispatch_matches_packet(&dispatch, &packet)?
    {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The app-server dispatch does not match the frozen memory packet.",
        ));
    }
    let inserted = tx.execute(
        "INSERT INTO codex_app_server_dispatches(job_kind,job_id,packet_id,dispatch_json) VALUES('memory',?,?,?) ON CONFLICT(job_kind,job_id) DO NOTHING",
        params![job.id, job.packet_id, serde_json::to_string(&dispatch)?],
    )?;
    if inserted != 1 {
        return Err(CoreError::new(
            "DispatchAlreadyClaimed",
            "This memory job already claimed external submission. It will not be sent again.",
        ));
    }
    tx.commit().map_err(CoreError::uncertain)
}

pub fn acknowledge_memory_app_server_turn(
    host: &mut impl StoryHost,
    owner: MemoryOwner,
    dispatch: AppServerDispatch,
    turn_id: String,
) -> CoreResult<()> {
    validate_runtime_owner(host.info(), &owner)?;
    dispatch.validate()?;
    if !valid_identifier(&turn_id) {
        return Err(CoreError::new(
            "InvalidRequest",
            "The app-server turn identity is invalid.",
        ));
    }
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let job = read_memory_job_row(&tx, &owner.job_id)?;
    validate_job_owner(&job, &owner)?;
    let saved = stored_dispatch(&tx, &job.id)?;
    if saved.as_ref().is_none_or(|(identity, saved_turn)| {
        identity != &dispatch || saved_turn.as_ref().is_some_and(|value| value != &turn_id)
    }) {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The acknowledged turn does not match this memory job's dispatch.",
        ));
    }
    tx.execute("UPDATE codex_app_server_dispatches SET turn_id=? WHERE job_kind='memory' AND job_id=? AND turn_id IS NULL", params![turn_id, job.id])?;
    tx.commit().map_err(CoreError::uncertain)
}

fn stored_dispatch(
    db: &Connection,
    job_id: &str,
) -> CoreResult<Option<(AppServerDispatch, Option<String>)>> {
    let saved: Option<(String, Option<String>)> = db.query_row(
        "SELECT dispatch_json,turn_id FROM codex_app_server_dispatches WHERE job_kind='memory' AND job_id=?",
        [job_id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    saved
        .map(|(json, turn)| Ok((serde_json::from_str(&json)?, turn)))
        .transpose()
}

fn dispatch_matches_packet(
    dispatch: &AppServerDispatch,
    packet: &CompiledPacket,
) -> CoreResult<bool> {
    let Some(binding) = packet.options.provider_binding.as_ref() else {
        return Ok(false);
    };
    let text = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let request = wns_providers::codex_app_server::turn_request(dispatch, binding, &text);
    Ok(dispatch.packet_hash == sha256_hex(text.as_bytes())
        && dispatch.request_hash == sha256_hex(&serde_json::to_vec(&request)?))
}

pub fn validate_delivery(
    db: &Connection,
    request: &CompleteMemory,
    packet: &CompiledPacket,
) -> CoreResult<()> {
    if request.confirmed_stdin_bytes.is_some() || request.delivery.is_some() {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "App-server memory cannot use exec or HTTP delivery evidence.",
        ));
    }
    let receipt = request.app_server.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidAppServerDelivery",
            "App-server memory requires its own delivery evidence.",
        )
    })?;
    receipt.validate()?;
    let saved = stored_dispatch(db, &request.owner.job_id)?;
    if let Some(dispatch) = &receipt.dispatch {
        if !dispatch_matches_packet(dispatch, packet)?
            || saved.as_ref().is_none_or(|(identity, turn)| {
                identity != dispatch
                    || turn
                        .as_ref()
                        .is_some_and(|id| receipt.turn_id.as_ref() != Some(id))
            })
        {
            return Err(CoreError::new(
                "InvalidAppServerDelivery",
                "App-server memory delivery belongs to a different dispatch or packet.",
            ));
        }
    } else if saved.is_some() {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "The result omitted this memory job's durable app-server dispatch.",
        ));
    }
    if request.cleanup.is_none()
        || (request.cleanup == Some(ProviderCleanup::Settled)) != receipt.request_settled
        || (request.outcome == ProviderOutcomeStatus::Completed && !receipt.completed())
    {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "The memory result is not supported by terminal app-server evidence.",
        ));
    }
    Ok(())
}

pub fn validate_dispatches(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare("SELECT job_id,packet_id,dispatch_json,turn_id FROM codex_app_server_dispatches WHERE job_kind='memory'")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (job_id, packet_id, json, turn_id) in rows {
        let dispatch: AppServerDispatch = serde_json::from_str(&json)?;
        dispatch.validate()?;
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        let job_packet: String = db.query_row(
            "SELECT packet_id FROM memory_jobs WHERE id=? AND dispatch_state='dispatched'",
            [job_id],
            |row| row.get(0),
        )?;
        if job_packet != packet_id
            || !packet
                .options
                .provider_binding
                .as_ref()
                .is_some_and(is_app_server)
            || !dispatch_matches_packet(&dispatch, &packet)?
            || turn_id.as_ref().is_some_and(|id| !valid_identifier(id))
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The saved app-server dispatch is not bound to its original memory job.",
            ));
        }
    }
    Ok(())
}
