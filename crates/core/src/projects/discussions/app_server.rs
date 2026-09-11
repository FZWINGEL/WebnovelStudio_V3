use super::*;
use crate::providers::codex_app_server::{
    AppServerDelivery, AppServerDispatch, AppServerSubmission, is_app_server, valid_identifier,
};

impl ProjectSession {
    /// One durable authorization, committed after thread creation and before
    /// turn/start. An existing claim is never permission to repeat submission.
    pub fn claim_app_server_dispatch(
        &self,
        owner: RunOwner,
        dispatch: AppServerDispatch,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ClaimAppServer(
                owner, dispatch, reply,
            )))
        })
    }

    pub fn acknowledge_app_server_turn(
        &self,
        owner: RunOwner,
        dispatch: AppServerDispatch,
        turn_id: String,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::AckAppServer(
                owner, dispatch, turn_id, reply,
            )))
        })
    }
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn claim_app_server_dispatch(
    host: &mut impl StoryHost,
    owner: RunOwner,
    dispatch: AppServerDispatch,
) -> CoreResult<()> {
    validate_runtime_owner(&host.info(), &owner)?;
    dispatch.validate()?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let run = read_run(&tx, &owner.run_id)?;
    validate_owner(&run, &owner)?;
    reject_legacy_lookup_path(&run)?;
    if run.status != DiscussionRunStatus::Running || run.dispatch_state != "claimed" {
        return Err(CoreError::new(
            "RunNotStarted",
            "This discussion no longer permits external dispatch.",
        ));
    }
    let packet = context_packets::validated_packet_record(&tx, &run.packet_id)?;
    let current_basis: bool = tx.query_row(
        "SELECT ss.context_source_epoch=p.context_source_epoch AND ss.disclosure_policy_epoch=p.disclosure_policy_epoch FROM context_packets cp JOIN story_snapshots ss ON ss.id=cp.snapshot_id JOIN project p ON p.singleton=1 WHERE cp.id=?",
        [&run.packet_id], |row| row.get(0),
    )?;
    if !current_basis {
        return Err(CoreError::new(
            "ContextChanged",
            "The story or its permissions changed before app-server submission. No turn was sent.",
        ));
    }
    if !packet
        .options
        .provider_binding
        .as_ref()
        .is_some_and(is_app_server)
        || !dispatch_matches_packet(&dispatch, &packet)?
    {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The app-server dispatch does not match the frozen packet.",
        ));
    }
    let inserted = tx.execute(
        "INSERT INTO codex_app_server_dispatches(job_kind,job_id,packet_id,dispatch_json) VALUES('discussion',?,?,?) ON CONFLICT(job_kind,job_id) DO NOTHING",
        params![run.id, run.packet_id, serde_json::to_string(&dispatch)?],
    )?;
    if inserted != 1 {
        return Err(CoreError::new(
            "DispatchAlreadyClaimed",
            "This request already claimed external submission. It will not be sent again.",
        ));
    }
    tx.commit().map_err(CoreError::uncertain)
}

pub fn acknowledge_app_server_turn(
    host: &mut impl StoryHost,
    owner: RunOwner,
    dispatch: AppServerDispatch,
    turn_id: String,
) -> CoreResult<()> {
    validate_runtime_owner(&host.info(), &owner)?;
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
    let run = read_run(&tx, &owner.run_id)?;
    validate_owner(&run, &owner)?;
    let saved = stored_dispatch(&tx, &run.id)?;
    if saved.as_ref().is_none_or(|(identity, saved_turn)| {
        identity != &dispatch || saved_turn.as_ref().is_some_and(|value| value != &turn_id)
    }) {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The acknowledged turn does not match this request's dispatch.",
        ));
    }
    tx.execute("UPDATE codex_app_server_dispatches SET turn_id=? WHERE job_kind='discussion' AND job_id=? AND turn_id IS NULL", params![turn_id, run.id])?;
    tx.commit().map_err(CoreError::uncertain)
}

fn stored_dispatch(
    db: &Connection,
    run_id: &str,
) -> CoreResult<Option<(AppServerDispatch, Option<String>)>> {
    let saved: Option<(String, Option<String>)> = db.query_row(
        "SELECT dispatch_json,turn_id FROM codex_app_server_dispatches WHERE job_kind='discussion' AND job_id=?",
        [run_id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    saved
        .map(|(json, turn)| Ok((serde_json::from_str(&json)?, turn)))
        .transpose()
}

pub(super) fn packet_hash(packet: &CompiledPacket) -> CoreResult<String> {
    Ok(crate::sha256_hex(
        serialized_input(&packet.messages, &packet.options)
            .map_err(packet_error)?
            .as_bytes(),
    ))
}

fn dispatch_matches_packet(
    dispatch: &AppServerDispatch,
    packet: &CompiledPacket,
) -> CoreResult<bool> {
    let Some(binding) = packet.options.provider_binding.as_ref() else {
        return Ok(false);
    };
    let text = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let request = crate::providers::codex_app_server::turn_request(dispatch, binding, &text);
    Ok(dispatch.packet_hash == packet_hash(packet)?
        && dispatch.request_hash == crate::sha256_hex(&serde_json::to_vec(&request)?))
}

pub(super) fn validate_delivery(
    db: &Connection,
    run_id: &str,
    packet: &CompiledPacket,
    delivery: Option<&AppServerDelivery>,
    status: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> CoreResult<bool> {
    let receipt = delivery.ok_or_else(|| {
        CoreError::new(
            "InvalidAppServerDelivery",
            "App-server results require their own delivery evidence.",
        )
    })?;
    receipt.validate()?;
    let saved = stored_dispatch(db, run_id)?;
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
                "App-server delivery belongs to a different dispatch or packet.",
            ));
        }
    } else if saved.is_some() {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "The result omitted this request's durable app-server dispatch.",
        ));
    }
    if (cleanup == ProviderCleanup::Settled) != receipt.request_settled
        || (status == ProviderOutcomeStatus::Completed && !receipt.completed())
    {
        return Err(CoreError::new(
            "InvalidAppServerDelivery",
            "The result is not supported by terminal app-server evidence.",
        ));
    }
    Ok(receipt.submission == AppServerSubmission::Acknowledged)
}

pub(super) fn validate_dispatches(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare("SELECT job_id,packet_id,dispatch_json,turn_id FROM codex_app_server_dispatches WHERE job_kind='discussion'")?;
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
    for (run_id, packet_id, json, turn_id) in rows {
        let dispatch: AppServerDispatch = serde_json::from_str(&json)?;
        dispatch.validate()?;
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        let run_packet: String = db.query_row("SELECT packet_id FROM discussion_runs WHERE id=? AND dispatch_state IN ('claimed','delivered')", [run_id], |row| row.get(0))?;
        if run_packet != packet_id
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
                "The saved app-server dispatch is not bound to its original request.",
            ));
        }
    }
    Ok(())
}
