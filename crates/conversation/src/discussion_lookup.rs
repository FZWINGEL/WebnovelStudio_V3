//! Durable state for the bounded, request-scoped discussion lookup loop.
//!
//! The ordinary provider receipt is deliberately not extended.  It is one
//! terminal row per legacy discussion and its historical packet/result bytes
//! are part of the transfer contract.  Lookup discussions instead retain one
//! mutable invocation state row plus immutable result and local-read rows.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_context::lookup::{
    LOOKUP_SCHEMA_VERSION, LookupAllowance, LookupEnvelope, LookupExchange, LookupRead,
    LookupReadResult, MAX_LOOKUP_ENVELOPE_BYTES, parse_lookup_envelope, validate_lookup_envelope,
    validate_lookup_read,
};
use wns_context::packet::{CompiledPacket, ProviderBinding, packet_input_hash, serialized_input};
use wns_kernel::check_id;
use wns_kernel::sha256_hex;
use wns_kernel::{CoreError, CoreResult, SourceEpoch};
use wns_providers::vocabulary::{ProviderCleanup, ProviderOutcomeStatus, ProviderUsage};
pub use wns_story::run_vocabulary::*;
use wns_story::story_context::{FrozenContext, SearchMode};

pub const MAX_LOOKUP_INVOCATIONS: u8 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupDispatch {
    pub run: wns_story::run_vocabulary::DiscussionRun,
    pub ordinal: String,
    pub packet: CompiledPacket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupInvocationReport {
    pub owner: RunOwner,
    pub ordinal: String,
    pub event_id: String,
    pub assistant_text: String,
    pub binding: Option<ProviderBinding>,
    pub status: ProviderOutcomeStatus,
    pub confirmed_stdin_bytes: String,
    pub usage: Option<ProviderUsage>,
    pub cleanup: ProviderCleanup,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupAdvanceRequest {
    pub owner: RunOwner,
    pub completed_ordinal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupHaltRequest {
    pub owner: RunOwner,
    pub reason: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LookupAdvance {
    Prepared {
        dispatch: Box<LookupDispatch>,
    },
    Finished {
        run: wns_story::run_vocabulary::DiscussionRun,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationIdentity {
    pub run_id: String,
    pub ordinal: u8,
    pub packet_id: String,
    pub snapshot_id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub source_epoch: SourceEpoch,
    pub policy_epoch: String,
    pub allowance: LookupAllowance,
    pub state: LookupInvocationState,
    pub expected_sequence: String,
}

#[derive(Debug, Clone)]
pub struct ProviderReport {
    pub event_id: String,
    pub assistant_text: String,
    pub binding: Option<ProviderBinding>,
    pub status: ProviderOutcomeStatus,
    pub confirmed_stdin_bytes: String,
    pub usage: Option<ProviderUsage>,
    pub cleanup: ProviderCleanup,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    NeedsContext,
    Discussion,
}

impl ResponseKind {
    fn state(self) -> LookupInvocationState {
        match self {
            Self::NeedsContext => LookupInvocationState::NeedsContext,
            Self::Discussion => LookupInvocationState::Completed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoredResult {
    pub kind: ResponseKind,
    pub envelope: LookupEnvelope,
}

pub fn validate_allowance(allowance: &LookupAllowance) -> CoreResult<()> {
    allowance
        .validate()
        .map_err(|error| CoreError::new("InvalidLookupAllowance", &error.to_string()))
}

pub fn parse_response(raw: &str) -> CoreResult<StoredResult> {
    let envelope = parse_lookup_envelope(raw)
        .map_err(|error| CoreError::new("InvalidLookupResponse", &error.to_string()))?;
    if envelope.schema_version() != LOOKUP_SCHEMA_VERSION {
        return Err(CoreError::new(
            "InvalidLookupResponse",
            "The lookup response schema is not supported.",
        ));
    }
    let kind = match envelope {
        LookupEnvelope::NeedsContext { .. } => ResponseKind::NeedsContext,
        LookupEnvelope::Discussion { .. } => ResponseKind::Discussion,
    };
    Ok(StoredResult { kind, envelope })
}

pub fn insert_initial(
    tx: &Connection,
    run_id: &str,
    packet: &CompiledPacket,
    frozen: &FrozenContext,
    operation_namespace: &str,
    allowance: &LookupAllowance,
) -> CoreResult<()> {
    validate_allowance(allowance)?;
    let lookup = packet.receipt.lookup.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidLookupCapability",
            "The initial lookup packet is missing its lookup receipt.",
        )
    })?;
    lookup
        .validate_capability()
        .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
    if lookup.allowance != *allowance || lookup.completed_invocations != 0 {
        return Err(CoreError::new(
            "InvalidLookupCapability",
            "The initial lookup packet does not match its durable lookup allowance.",
        ));
    }
    let allowance_json = serde_json::to_string(allowance)?;
    tx.execute(
        "INSERT INTO discussion_lookup_invocations(run_id,ordinal,packet_id,snapshot_id,project_id,operation_namespace,source_epoch,policy_epoch,allowance_json,state,expected_sequence) VALUES(?,?,?,?,?,?,?,?,?,'prepared',0)",
        params![
            run_id,
            0_i64,
            packet.receipt.packet_id,
            frozen.snapshot.snapshot_id,
            frozen.snapshot.project_id,
            operation_namespace,
            parse_decimal(&frozen.snapshot.context_source_epoch)?,
            parse_decimal(&frozen.policy.version)?,
            allowance_json,
        ],
    )?;
    Ok(())
}

pub fn claim(tx: &Connection, owner: &RunOwner, ordinal: u8) -> CoreResult<InvocationIdentity> {
    let current = read_identity(tx, owner, ordinal)?;
    if current.state != LookupInvocationState::Prepared {
        return Err(CoreError::new(
            "LookupInvocationAlreadyClaimed",
            "This lookup invocation has already been claimed or settled; no paid replay is allowed.",
        ));
    }
    let packet = wns_story::context_packets::validated_packet_record(tx, &current.packet_id)?;
    let serialized_len = serialized_input(&packet.messages, &packet.options)
        .map_err(|error| CoreError::new("InvalidContextPacket", &error.to_string()))?
        .len() as u128;
    let (previous_input, previous_output): (i64, i64) = tx.query_row(
        "SELECT COALESCE(SUM(confirmed_stdin_bytes),0),COALESCE(SUM(length(CAST(assistant_text AS BLOB))),0) FROM discussion_lookup_results WHERE run_id=?",
        [&owner.run_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let input_limit = parse_decimal(&current.allowance.total_input_bytes)? as u128;
    let output_limit = parse_decimal(&current.allowance.total_output_bytes)? as u128;
    if u128::try_from(previous_input).unwrap_or(u128::MAX) + serialized_len > input_limit
        || u128::try_from(previous_output).unwrap_or(u128::MAX) + MAX_LOOKUP_ENVELOPE_BYTES as u128
            > output_limit
    {
        return Err(CoreError::new(
            "LookupAllowanceExceeded",
            "The next lookup call cannot be claimed within its frozen byte allowance.",
        ));
    }
    let changed = tx.execute(
        "UPDATE discussion_lookup_invocations SET state='claimed',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=? AND ordinal=? AND state='prepared'",
        params![owner.run_id, i64::from(ordinal)],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "LookupInvocationConflict",
            "The lookup invocation changed before it could be claimed.",
        ));
    }
    read_identity(tx, owner, ordinal)
}

pub fn read_identity(
    db: &Connection,
    owner: &RunOwner,
    ordinal: u8,
) -> CoreResult<InvocationIdentity> {
    type InvocationRow = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
    );
    let row: Option<InvocationRow> = db
        .query_row(
            "SELECT i.run_id,i.packet_id,i.snapshot_id,i.project_id,i.operation_namespace,i.source_epoch,i.policy_epoch,i.allowance_json,i.state,i.expected_sequence FROM discussion_lookup_invocations i JOIN discussion_runs r ON r.id=i.run_id WHERE i.run_id=? AND i.ordinal=? AND r.project_id=? AND r.operation_namespace=?",
            params![owner.run_id, i64::from(ordinal), owner.project_id, owner.operation_namespace],
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get::<_, i64>(5)?.to_string(), row.get::<_, i64>(6)?.to_string(),
                    row.get(7)?, row.get(8)?, row.get::<_, i64>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        run_id,
        packet_id,
        snapshot_id,
        project_id,
        namespace,
        source_epoch,
        policy_epoch,
        allowance_json,
        state,
        expected_sequence_i64,
    )) = row
    else {
        return Err(CoreError::new(
            "LookupInvocationNotFound",
            "The lookup invocation is not available in this project.",
        ));
    };
    let allowance: LookupAllowance = serde_json::from_str(&allowance_json)?;
    validate_allowance(&allowance)?;
    Ok(InvocationIdentity {
        run_id,
        ordinal,
        packet_id,
        snapshot_id,
        project_id,
        operation_namespace: namespace,
        source_epoch: source_epoch.into(),
        policy_epoch,
        allowance,
        state: LookupInvocationState::parse(&state)?,
        expected_sequence: expected_sequence_i64.to_string(),
    })
}

pub fn store_outcome(
    tx: &Connection,
    identity: &InvocationIdentity,
    report: &ProviderReport,
    parsed: Option<&StoredResult>,
) -> CoreResult<LookupInvocationState> {
    if identity.state != LookupInvocationState::Claimed {
        return Err(CoreError::new(
            "LookupInvocationNotClaimed",
            "A lookup result requires a claimed invocation.",
        ));
    }
    check_id(&report.event_id)?;
    let response_json = parsed
        .map(|result| serde_json::to_string(&result.envelope))
        .transpose()?;
    let binding_json = report
        .binding
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let usage_json = report
        .usage
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let confirmed = parse_decimal(&report.confirmed_stdin_bytes)?;
    let state = match parsed.map(|result| result.kind) {
        Some(kind) => kind.state(),
        None if report.cleanup == ProviderCleanup::Unresolved => LookupInvocationState::Unknown,
        None if report.status == ProviderOutcomeStatus::Stopped => LookupInvocationState::Stopped,
        None => LookupInvocationState::Failed,
    };
    tx.execute(
        "INSERT INTO discussion_lookup_results(run_id,ordinal,packet_id,event_id,expected_sequence,assistant_text,response_json,binding_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            identity.run_id,
            i64::from(identity.ordinal),
            identity.packet_id,
            report.event_id,
            parse_decimal(&identity.expected_sequence)?,
            report.assistant_text,
            response_json,
            binding_json,
            report.status.as_str(),
            confirmed,
            usage_json,
            report.cleanup.as_str(),
            report.error,
        ],
    )?;
    tx.execute(
        "UPDATE discussion_lookup_invocations SET state=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=? AND ordinal=? AND state='claimed'",
        params![state.as_str(), identity.run_id, i64::from(identity.ordinal)],
    )?;
    Ok(state)
}

pub fn mark_chain_stopped(tx: &Connection, run_id: &str) -> CoreResult<()> {
    tx.execute(
        "UPDATE discussion_lookup_invocations SET state=CASE WHEN state='claimed' THEN 'unknown' ELSE 'stopped' END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=? AND state IN ('prepared','claimed')",
        [run_id],
    )?;
    Ok(())
}

pub fn read_exchanges(
    db: &Connection,
    run_id: &str,
    through_ordinal: u8,
) -> CoreResult<Vec<LookupExchange>> {
    let mut statement = db.prepare(
        "SELECT ordinal,response_json FROM discussion_lookup_results WHERE run_id=? AND ordinal<=? ORDER BY ordinal",
    )?;
    let responses = statement
        .query_map(params![run_id, i64::from(through_ordinal)], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut exchanges = Vec::new();
    for (ordinal, response_json) in responses {
        let Some(response_json) = response_json else {
            continue;
        };
        let envelope: LookupEnvelope = serde_json::from_str(&response_json)?;
        let LookupEnvelope::NeedsContext { reads, .. } = envelope else {
            continue;
        };
        for request in reads {
            let result_json: Option<String> = db
                .query_row(
                    "SELECT result_json FROM discussion_lookup_reads WHERE run_id=? AND ordinal=? AND read_id=?",
                    params![run_id, ordinal, request.id()],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(result_json) = result_json else {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A completed lookup invocation is missing an immutable read receipt.",
                ));
            };
            let result: LookupReadResult = serde_json::from_str(&result_json)?;
            exchanges.push(LookupExchange { request, result });
        }
    }
    Ok(exchanges)
}

pub fn pending_reads(db: &Connection, run_id: &str, ordinal: u8) -> CoreResult<Vec<LookupRead>> {
    let response_json: Option<String> = db
        .query_row(
            "SELECT response_json FROM discussion_lookup_results WHERE run_id=? AND ordinal=?",
            params![run_id, i64::from(ordinal)],
            |row| row.get(0),
        )
        .optional()?;
    let Some(response_json) = response_json else {
        return Err(CoreError::new(
            "LookupResponseMissing",
            "The lookup invocation has no saved response to expand.",
        ));
    };
    let envelope: LookupEnvelope = serde_json::from_str(&response_json)?;
    match envelope {
        LookupEnvelope::NeedsContext { reads, .. } => Ok(reads),
        LookupEnvelope::Discussion { .. } => Err(CoreError::new(
            "LookupAlreadyFinished",
            "A final lookup discussion cannot be expanded.",
        )),
    }
}

pub fn insert_child(
    tx: &Connection,
    run_id: &str,
    ordinal: u8,
    packet: &CompiledPacket,
    frozen: &FrozenContext,
    operation_namespace: &str,
    allowance: &LookupAllowance,
) -> CoreResult<()> {
    if ordinal == 0 || ordinal > 2 {
        return Err(CoreError::new(
            "InvalidLookupCounter",
            "A child lookup ordinal must be between 1 and 2.",
        ));
    }
    validate_allowance(allowance)?;
    let lookup = packet.receipt.lookup.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidLookupCapability",
            "The child lookup packet is missing its lookup receipt.",
        )
    })?;
    lookup
        .validate_capability()
        .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
    if lookup.allowance != *allowance || lookup.completed_invocations != ordinal {
        return Err(CoreError::new(
            "InvalidLookupCapability",
            "The child lookup packet does not retain the root lookup capability exactly.",
        ));
    }
    let allowance_json = serde_json::to_string(allowance)?;
    tx.execute(
        "INSERT INTO discussion_lookup_invocations(run_id,ordinal,packet_id,snapshot_id,project_id,operation_namespace,source_epoch,policy_epoch,allowance_json,state,expected_sequence) VALUES(?,?,?,?,?,?,?,?,?,'prepared',0)",
        params![
            run_id,
            i64::from(ordinal),
            packet.receipt.packet_id,
            frozen.snapshot.snapshot_id,
            frozen.snapshot.project_id,
            operation_namespace,
            parse_decimal(&frozen.snapshot.context_source_epoch)?,
            parse_decimal(&frozen.policy.version)?,
            allowance_json,
        ],
    )?;
    Ok(())
}

pub fn execute_read(
    db: &Connection,
    frozen: &FrozenContext,
    request: &LookupRead,
) -> (LookupReadResult, bool) {
    if let Err(error) = validate_lookup_read(request) {
        return (
            LookupReadResult::Unavailable {
                code: "invalidRead".into(),
                detail: error.to_string(),
            },
            false,
        );
    }
    match request {
        LookupRead::Search {
            query, mode, limit, ..
        } => (execute_search(db, frozen, query, *mode, *limit), false),
        LookupRead::Read {
            handle, block_ids, ..
        } => match wns_story::story_context::read_source(db, frozen, handle) {
            Ok(source) => {
                let passages = if let Some(ids) = block_ids {
                    if ids.iter().any(|id| {
                        !source
                            .passages
                            .iter()
                            .any(|passage| &passage.block_id == id)
                    }) {
                        return (
                            LookupReadResult::Unavailable {
                                code: "unknownBlock".into(),
                                detail: "The requested block is not present in the frozen source."
                                    .into(),
                            },
                            false,
                        );
                    }
                    source
                        .passages
                        .iter()
                        .filter(|passage| ids.iter().any(|id| id == &passage.block_id))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    source.passages.clone()
                };
                let complete = passages.len() == source.passages.len()
                    && passages
                        .iter()
                        .zip(&source.passages)
                        .all(|(selected, original)| selected == original);
                (
                    LookupReadResult::Read {
                        handle: handle.clone(),
                        source: source.descriptor.source,
                        passages,
                        complete,
                    },
                    !complete,
                )
            }
            Err(_) => (
                LookupReadResult::Unavailable {
                    code: "SourceUnavailable".into(),
                    detail: "This source could not be read from the frozen story.".into(),
                },
                false,
            ),
        },
        _ if request.is_memory() => {
            match wns_context::memory_lookup::execute_memory_lookup(frozen, request) {
                Ok(Some(result)) => (result, false),
                Ok(None) => (
                    LookupReadResult::Unavailable {
                        code: "invalidRead".into(),
                        detail: "The reviewed-memory read is not recognized.".into(),
                    },
                    false,
                ),
                Err(error) => (
                    LookupReadResult::Unavailable {
                        code: "memoryUnavailable".into(),
                        detail: error.detail,
                    },
                    false,
                ),
            }
        }
        _ => (
            LookupReadResult::Unavailable {
                code: "invalidRead".into(),
                detail: "The lookup read is not recognized.".into(),
            },
            false,
        ),
    }
}

/// Check the Rust-owned capability before a provider response can create a
/// durable read receipt. The provider envelope remains the existing
/// story-lookup.v1 shape; this fence is deliberately packet-owned.
pub fn authorize_envelope(packet: &CompiledPacket, envelope: &LookupEnvelope) -> CoreResult<()> {
    let LookupEnvelope::NeedsContext { reads, .. } = envelope else {
        return Ok(());
    };
    let lookup = packet.receipt.lookup.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidLookupCapability",
            "A lookup response has no authorized lookup packet.",
        )
    })?;
    lookup
        .validate_capability()
        .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
    for read in reads {
        lookup
            .authorize_read(read)
            .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
    }
    Ok(())
}

fn execute_search(
    db: &Connection,
    frozen: &FrozenContext,
    query: &str,
    mode: SearchMode,
    limit: u32,
) -> LookupReadResult {
    match wns_story::story_context::search_frozen(db, frozen, query.trim(), mode, limit) {
        Ok(result) => LookupReadResult::Search { result },
        Err(_) => LookupReadResult::Unavailable {
            code: "SourceUnavailable".into(),
            detail: "The frozen sources could not all be searched. No complete-search result is claimed.".into(),
        },
    }
}

pub fn store_read(
    tx: &Connection,
    owner: &RunOwner,
    ordinal: u8,
    read_id: &str,
    request: &LookupRead,
    result: &Value,
    truncated: bool,
) -> CoreResult<()> {
    let identity = read_identity(tx, owner, ordinal)?;
    if !matches!(identity.state, LookupInvocationState::NeedsContext) {
        return Err(CoreError::new(
            "LookupReadUnavailable",
            "Reads can only be recorded after a needs-context response.",
        ));
    }
    let request_json = serde_json::to_string(request)?;
    let result_json = serde_json::to_string(result)?;
    let request_hash = sha256_hex(request_json.as_bytes());
    let result_hash = sha256_hex(result_json.as_bytes());
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT request_hash,result_hash FROM discussion_lookup_reads WHERE run_id=? AND read_id=?",
            params![owner.run_id, read_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((saved_request, saved_result)) = existing {
        if saved_request == request_hash && saved_result == result_hash {
            return Ok(());
        }
        return Err(CoreError::new(
            "LookupReadConflict",
            "A lookup read ID was reused with different request or result bytes.",
        ));
    }
    tx.execute(
        "INSERT INTO discussion_lookup_reads(run_id,ordinal,read_id,request_hash,request_json,result_hash,result_json,snapshot_id,project_id,operation_namespace,policy_epoch,truncated) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            owner.run_id,
            i64::from(ordinal),
            read_id,
            request_hash,
            request_json,
            result_hash,
            result_json,
            identity.snapshot_id,
            identity.project_id,
            identity.operation_namespace,
            parse_decimal(&identity.policy_epoch)?,
            i64::from(truncated),
        ],
    )?;
    Ok(())
}

pub fn read_summary(db: &Connection, run_id: &str) -> CoreResult<Option<LookupRunSummary>> {
    let allowance_json: Option<String> = db
        .query_row(
            "SELECT allowance_json FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=0",
            [run_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(allowance_json) = allowance_json else {
        return Ok(None);
    };
    let allowance: LookupAllowance = serde_json::from_str(&allowance_json)?;
    let mut statement = db.prepare(
        "SELECT i.ordinal,i.packet_id,i.state,r.response_json,r.error,r.confirmed_stdin_bytes FROM discussion_lookup_invocations i LEFT JOIN discussion_lookup_results r ON r.run_id=i.run_id AND r.ordinal=i.ordinal WHERE i.run_id=? ORDER BY i.ordinal",
    )?;
    let rows = statement
        .query_map([run_id], |row| {
            let ordinal: i64 = row.get(0)?;
            Ok((
                ordinal,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut invocations = Vec::with_capacity(rows.len());
    for (ordinal, packet_id, state, response_json, error, confirmed_stdin_bytes) in rows {
        let ordinal = u8::try_from(ordinal).map_err(|_| {
            CoreError::new("InvalidProject", "A lookup invocation ordinal is invalid.")
        })?;
        let input_delivered =
            input_delivery_matches_saved_packet(db, &packet_id, confirmed_stdin_bytes)?;
        invocations.push(LookupInvocationSummary {
            ordinal: ordinal.to_string(),
            packet_id,
            state: LookupInvocationState::parse(&state)?,
            input_delivered,
            response: response_json
                .map(|json| serde_json::from_str(&json))
                .transpose()?,
            error,
        });
    }
    Ok(Some(LookupRunSummary {
        allowance,
        invocations,
    }))
}

/// Read-only delivery projection for discussion polling. This intentionally
/// validates the retained packet row and its deterministic input bytes without
/// reloading the packet's frozen story sources. Full packet/source validation
/// remains at claim, inspection, and backup validation boundaries.
fn input_delivery_matches_saved_packet(
    db: &Connection,
    packet_id: &str,
    confirmed_stdin_bytes: Option<i64>,
) -> CoreResult<bool> {
    let Some(confirmed_stdin_bytes) = confirmed_stdin_bytes.filter(|bytes| *bytes > 0) else {
        return Ok(false);
    };
    check_id(packet_id)?;
    let row: Option<(String, String, String)> = db
        .query_row(
            "SELECT packet_json,packet_hash,input_hash FROM context_packets WHERE id=?",
            [packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((packet_json, packet_hash, stored_input_hash)) = row else {
        return Err(CoreError::new(
            "ContextPacketNotFound",
            "The prepared request is not available in this project.",
        ));
    };
    if sha256_hex(packet_json.as_bytes()) != packet_hash {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The prepared request failed its fingerprint check.",
        ));
    }
    let packet: CompiledPacket = serde_json::from_str(&packet_json)?;
    if packet.receipt.packet_id != packet_id {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The prepared request receipt does not match its packet ID.",
        ));
    }
    let input = serialized_input(&packet.messages, &packet.options)
        .map_err(|error| CoreError::new("InvalidContextPacket", &error.to_string()))?;
    let computed_input_hash = packet_input_hash(&packet.messages, &packet.options)
        .map_err(|error| CoreError::new("InvalidContextPacket", &error.to_string()))?;
    if packet.receipt.input_hash != stored_input_hash || computed_input_hash != stored_input_hash {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The exact request input does not match its receipt.",
        ));
    }
    Ok(i64::try_from(input.len()).ok() == Some(confirmed_stdin_bytes))
}

pub fn validate_storage(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare(
        "SELECT run_id,ordinal,packet_id,snapshot_id,project_id,operation_namespace,source_epoch,policy_epoch,allowance_json,state,expected_sequence FROM discussion_lookup_invocations ORDER BY run_id,ordinal",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        run_id,
        ordinal,
        packet_id,
        snapshot_id,
        project_id,
        namespace,
        source_epoch,
        policy_epoch,
        allowance_json,
        state,
        expected_sequence,
    ) in rows
    {
        check_id(&run_id)?;
        check_id(&packet_id)?;
        check_id(&snapshot_id)?;
        check_id(&project_id)?;
        check_id(&namespace)?;
        if !(0..=2).contains(&ordinal)
            || source_epoch < 0
            || policy_epoch < 0
            || expected_sequence < 0
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup invocation has invalid counters.",
            ));
        }
        let allowance: LookupAllowance = serde_json::from_str(&allowance_json)?;
        validate_allowance(&allowance)?;
        LookupInvocationState::parse(&state)?;
        let packet_owner: (String, String, String) = db.query_row(
            "SELECT project_id,operation_namespace,snapshot_id FROM context_packets WHERE id=?",
            [&packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if packet_owner != (project_id.clone(), namespace.clone(), snapshot_id.clone()) {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup packet has a mismatched owner or snapshot.",
            ));
        }
        let packet = wns_story::context_packets::validated_packet_record(db, &packet_id)?;
        let (frozen, frozen_namespace) =
            wns_story::story_context::validated_snapshot_record(db, &snapshot_id)?;
        let root: (String, String, String, String) = db.query_row(
            "SELECT project_id,operation_namespace,operation_id,packet_id FROM discussion_runs WHERE id=?",
            [&run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let root_packet = wns_story::context_packets::validated_packet_record(db, &root.3)?;
        let root_request = lookup_packet_request(db, &root.3)?;
        if root_request.operation_id != root.2
            || root_request.access.project_id != root.0
            || root_request.access.operation_namespace != root.1
            || root_packet.receipt.session_id.is_empty()
            || root_packet.options.provider_binding != root_request.provider_binding
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The lookup root packet is not bound to its discussion run.",
            ));
        }
        let lookup = packet.receipt.lookup.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A lookup invocation is missing its authorized allowance.",
            )
        })?;
        lookup
            .validate_capability()
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
        let root_lookup = root_packet.receipt.lookup.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "The lookup root packet is missing its authorized capability.",
            )
        })?;
        root_lookup
            .validate_capability()
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
        if packet.receipt.invocation_ordinal != ordinal.to_string()
            || lookup.completed_invocations != ordinal as u8
            || lookup.allowance != allowance
            || lookup.reviewed_memory != root_lookup.reviewed_memory
            || root.0 != project_id
            || root.1 != namespace
            || frozen_namespace != namespace
            || frozen.snapshot.context_source_epoch != SourceEpoch::new(source_epoch.to_string())
            || frozen.policy.version != policy_epoch.to_string()
            || (ordinal == 0 && root.3 != packet_id)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup invocation packet is missing its exact lookup receipt.",
            ));
        }
        let request = lookup_packet_request(db, &packet_id)?;
        if packet.receipt.session_id != root_packet.receipt.session_id
            || packet.options.provider_binding != root_packet.options.provider_binding
            || request.access.project_id != root.0
            || request.access.operation_namespace != root.1
            || request.instruction != root_request.instruction
            || request.mandatory_handles != root_request.mandatory_handles
            || request.transient_mandatory_handles != root_request.transient_mandatory_handles
            || request.safe_brief != root_request.safe_brief
            || request.scope != root_request.scope
            || request.budget != root_request.budget
            || request.provider_binding != root_request.provider_binding
            || request.response_contract != root_request.response_contract
            || (ordinal > 0 && request.operation_id == root.2)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup packet is not bound to the exact discussion request provenance.",
            ));
        }
        let initial: (String, String) = db.query_row(
            "SELECT snapshot_id,allowance_json FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=0",
            [&run_id], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if initial.0 != snapshot_id
            || serde_json::from_str::<LookupAllowance>(&initial.1)? != allowance
        {
            return Err(CoreError::new(
                "InvalidProject",
                "Lookup invocations must share one frozen story and allowance.",
            ));
        }
        let legacy_results: i64 = db.query_row(
            "SELECT count(*) FROM provider_results WHERE run_id=?",
            [&run_id],
            |row| row.get(0),
        )?;
        if legacy_results != 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "Lookup discussions cannot also carry a legacy provider result.",
            ));
        }
        if ordinal > 0 {
            let previous_state: Option<String> = db
                .query_row(
                    "SELECT state FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=?",
                    params![run_id, ordinal - 1],
                    |row| row.get(0),
                )
                .optional()?;
            if previous_state.as_deref() != Some("needs_context") {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A child lookup invocation has no needs-context predecessor.",
                ));
            }
            if lookup.exchanges != read_exchanges(db, &run_id, (ordinal - 1) as u8)? {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The lookup packet does not contain its exact preceding read receipts.",
                ));
            }
        }
        let saved_result: Option<(String, String)> = db
            .query_row(
                "SELECT cleanup,outcome FROM discussion_lookup_results WHERE run_id=? AND ordinal=?",
                params![run_id, ordinal],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match (state.as_str(), saved_result) {
            ("needs_context" | "completed" | "failed", Some((cleanup, _)))
                if cleanup == "settled" => {}
            ("needs_context" | "completed" | "failed", _) => {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A settled lookup invocation is missing its immutable result receipt.",
                ));
            }
            ("prepared" | "claimed", None) => {}
            ("prepared" | "claimed", Some(_)) => {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An unclaimed lookup invocation has an immutable result receipt.",
                ));
            }
            ("stopped" | "unknown", _) => {}
            _ => {}
        }
    }
    validate_results(db)?;
    validate_reads(db)?;
    let mut runs =
        db.prepare("SELECT DISTINCT run_id FROM discussion_lookup_invocations ORDER BY run_id")?;
    let run_ids = runs
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for run_id in run_ids {
        validate_lookup_terminal_consistency(db, &run_id)?;
    }
    Ok(())
}

fn lookup_packet_request(
    db: &Connection,
    packet_id: &str,
) -> CoreResult<wns_story::context_packets::PrepareContext> {
    let json: String = db.query_row(
        "SELECT request_json FROM context_packets WHERE id=?",
        [packet_id],
        |row| row.get(0),
    )?;
    serde_json::from_str(&json).map_err(CoreError::from)
}

/// Validate the durable run-facing terminal fences which cannot be inferred
/// from one invocation row alone.  Lookup results are provider receipts, while
/// the discussion run, terminal event, and assistant message are the durable
/// user-visible history.  A backup must retain the same relationship between
/// those records, including the stop race where a completed provider envelope
/// is retained but the run is sealed as stopped.
fn validate_lookup_terminal_consistency(db: &Connection, run_id: &str) -> CoreResult<()> {
    let (status, sequence, output_text, run_packet_id): (String, i64, String, String) = db
        .query_row(
            "SELECT status,sequence,output_text,packet_id FROM discussion_runs WHERE id=?",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let terminal = db
        .query_row(
            "SELECT event_id,chunk FROM discussion_output_events WHERE run_id=? AND sequence=? AND kind='terminal'",
            params![run_id, sequence],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let terminal_status = matches!(
        status.as_str(),
        "completed" | "stopped" | "failed" | "interrupted"
    );
    if terminal_status {
        let Some((terminal_event_id, terminal_text)) = terminal else {
            return Err(CoreError::new(
                "InvalidProject",
                "A terminal lookup run has no matching terminal event.",
            ));
        };
        if status == "completed" && terminal_text != output_text {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run output does not match its terminal event.",
            ));
        }

        let terminal_result_events: Vec<(String, String, String)> = db
            .prepare(
                "SELECT r.event_id,i.state,r.outcome FROM discussion_lookup_results r JOIN discussion_lookup_invocations i ON i.run_id=r.run_id AND i.ordinal=r.ordinal WHERE r.run_id=? AND i.state IN ('completed','failed','stopped','unknown') ORDER BY r.ordinal",
            )?
            .query_map([run_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if !terminal_result_events.is_empty()
            && !terminal_result_events
                .iter()
                .any(|(event_id, _, _)| event_id == &terminal_event_id)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A terminal lookup result is not linked to the run terminal event.",
            ));
        }
        if status != "completed" {
            let assistants: Vec<(String, Option<String>)> = db
                .prepare(
                    "SELECT content,packet_id FROM discussion_messages WHERE run_id=? AND role='assistant' ORDER BY created_at,id",
                )?
                .query_map([run_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            let expected = if output_text.is_empty() {
                terminal_text.clone()
            } else {
                format!("{}\n\n[{}]", output_text, terminal_text)
            };
            if assistants != vec![(expected, Some(run_packet_id.clone()))] {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A terminal lookup run has no exact retained terminal assistant message.",
                ));
            }
        }
    }

    let discussions: Vec<(i64, String, String, String, String)> = db
        .prepare(
            "SELECT r.ordinal,r.packet_id,r.assistant_text,r.response_json,r.outcome FROM discussion_lookup_results r WHERE r.run_id=? AND r.response_json IS NOT NULL ORDER BY r.ordinal",
        )?
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(ordinal, packet_id, assistant_text, response_json, outcome)| {
            let envelope = serde_json::from_str::<LookupEnvelope>(&response_json).ok()?;
            if matches!(envelope, LookupEnvelope::Discussion { .. }) {
                Some((ordinal, packet_id, assistant_text, response_json, outcome))
            } else {
                None
            }
        })
        .collect();

    if status == "completed" {
        let Some((ordinal, packet_id, _assistant_raw, response_json, outcome)) =
            discussions.first().filter(|_| discussions.len() == 1)
        else {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run is missing its unique final discussion result.",
            ));
        };
        if outcome != "completed" {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run has a non-completed final result.",
            ));
        }
        let envelope: LookupEnvelope = serde_json::from_str(response_json)?;
        let LookupEnvelope::Discussion { text, .. } = envelope else {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run has a non-discussion final envelope.",
            ));
        };
        let later_invocations: i64 = db.query_row(
            "SELECT count(*) FROM discussion_lookup_invocations WHERE run_id=? AND ordinal>?",
            params![run_id, ordinal],
            |row| row.get(0),
        )?;
        if later_invocations != 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run has invocation history after its final result.",
            ));
        }
        let event: Option<(String, String)> = db
            .query_row(
                "SELECT kind,chunk FROM discussion_output_events WHERE run_id=? AND sequence=(SELECT sequence FROM discussion_runs WHERE id=?)",
                params![run_id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if event != Some(("terminal".to_owned(), text.clone())) {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup result does not match its terminal event.",
            ));
        }
        let assistants: Vec<(String, Option<String>)> = db
            .prepare(
                "SELECT content,packet_id FROM discussion_messages WHERE run_id=? AND role='assistant' ORDER BY created_at,id",
            )?
            .query_map([run_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if assistants != vec![(text, Some(packet_id.clone()))] {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup run has no exact final assistant message.",
            ));
        }
    } else if !discussions.is_empty() {
        // The only legitimate non-completed discussion envelope is the stop
        // race: the provider response was durably retained before the author
        // stop sealed the run. The stop terminal message remains authoritative
        // and no lookup packet is presented as a final assistant message.
        if !matches!(status.as_str(), "stopped" | "interrupted") || discussions.len() != 1 {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup discussion result is attached to an invalid run state.",
            ));
        }
        let packet_id = &discussions[0].1;
        let packet_assistants: i64 = db.query_row(
            "SELECT count(*) FROM discussion_messages WHERE run_id=? AND role='assistant' AND packet_id=?",
            params![run_id, packet_id],
            |row| row.get(0),
        )?;
        if packet_assistants != 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "A stopped lookup race incorrectly published a final assistant message.",
            ));
        }
    }
    Ok(())
}

fn validate_results(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare(
        "SELECT run_id,ordinal,packet_id,event_id,expected_sequence,assistant_text,response_json,binding_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error FROM discussion_lookup_results ORDER BY run_id,ordinal",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        run_id,
        ordinal,
        packet_id,
        event_id,
        expected_sequence,
        assistant_text,
        response_json,
        binding_json,
        outcome,
        confirmed_stdin_bytes,
        usage_json,
        cleanup,
        error,
    ) in rows
    {
        if !(0..=2).contains(&ordinal) || expected_sequence < 0 || confirmed_stdin_bytes < 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup result has invalid counters.",
            ));
        }
        check_id(&run_id)?;
        check_id(&packet_id)?;
        check_id(&event_id)?;
        if assistant_text.len() > MAX_LOOKUP_ENVELOPE_BYTES
            || assistant_text
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup result has invalid assistant bytes.",
            ));
        }
        let state: String = db.query_row(
            "SELECT state FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=? AND packet_id=? AND expected_sequence=?",
            params![run_id, ordinal, packet_id, expected_sequence],
            |row| row.get(0),
        )?;
        let parsed = response_json
            .as_deref()
            .map(|json| {
                let envelope: LookupEnvelope = serde_json::from_str(json)?;
                validate_lookup_envelope(&envelope)
                    .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
                Ok::<_, CoreError>(envelope)
            })
            .transpose()?;
        if let Some(parsed) = &parsed {
            let original = parse_lookup_envelope(&assistant_text)
                .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
            if serde_json::to_value(original)? != serde_json::to_value(parsed)? {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The parsed lookup response differs from its exact retained output.",
                ));
            }
        }
        let expected_state = match parsed.as_ref() {
            Some(LookupEnvelope::NeedsContext { .. }) => LookupInvocationState::NeedsContext,
            Some(LookupEnvelope::Discussion { .. }) => LookupInvocationState::Completed,
            None if cleanup == "unresolved" => LookupInvocationState::Unknown,
            None if outcome == "stopped" => LookupInvocationState::Stopped,
            None => LookupInvocationState::Failed,
        };
        if state != expected_state.as_str() {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup result state does not match its immutable response.",
            ));
        }
        if outcome == "completed" && parsed.is_none() {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed lookup result is missing its response envelope.",
            ));
        }
        if outcome != "completed" && parsed.is_some() {
            return Err(CoreError::new(
                "InvalidProject",
                "A failed lookup result cannot contain a successful response envelope.",
            ));
        }
        let binding = binding_json
            .as_deref()
            .map(serde_json::from_str::<ProviderBinding>)
            .transpose()?;
        let packet = wns_story::context_packets::validated_packet_record(db, &packet_id)?;
        if let Some(parsed) = &parsed {
            authorize_envelope(&packet, parsed)?;
        }
        if packet.options.provider_binding != binding {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup result binding does not match its packet.",
            ));
        }
        let input_size = serialized_input(&packet.messages, &packet.options)
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?
            .len();
        if confirmed_stdin_bytes as u128 > input_size as u128
            || (outcome == "completed"
                && (confirmed_stdin_bytes as u128 != input_size as u128
                    || cleanup != "settled"
                    || error.is_some()))
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The lookup result has invalid delivery or cleanup evidence.",
            ));
        }
        let totals: (i64, i64) = db.query_row(
            "SELECT COALESCE(SUM(confirmed_stdin_bytes),0),COALESCE(SUM(length(CAST(assistant_text AS BLOB))),0) FROM discussion_lookup_results WHERE run_id=?",
            [&run_id], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let allowance = &packet
            .receipt
            .lookup
            .as_ref()
            .expect("lookup packet validated")
            .allowance;
        if totals.0 as u128 > parse_decimal(&allowance.total_input_bytes)? as u128
            || totals.1 as u128 > parse_decimal(&allowance.total_output_bytes)? as u128
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The saved lookup chain exceeds its authorized byte allowance.",
            ));
        }
        if let Some(usage_json) = usage_json {
            let _: ProviderUsage = serde_json::from_str(&usage_json)?;
        }
        if let Some(error) = error
            && (error.is_empty() || error.len() > 4096 || error.chars().any(char::is_control))
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup result error is not sanitized.",
            ));
        }
    }
    Ok(())
}

fn validate_reads(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare(
        "SELECT run_id,ordinal,read_id,request_hash,request_json,result_hash,result_json,snapshot_id,project_id,operation_namespace,policy_epoch,truncated FROM discussion_lookup_reads ORDER BY run_id,ordinal,read_id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        run_id,
        ordinal,
        read_id,
        request_hash,
        request_json,
        result_hash,
        result_json,
        snapshot_id,
        project_id,
        namespace,
        policy_epoch,
        truncated,
    ) in rows
    {
        if !(0..=2).contains(&ordinal) || policy_epoch < 0 || !matches!(truncated, 0 | 1) {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup read has invalid counters.",
            ));
        }
        check_id(&run_id)?;
        check_id(&read_id)?;
        check_id(&snapshot_id)?;
        check_id(&project_id)?;
        check_id(&namespace)?;
        if sha256_hex(request_json.as_bytes()) != request_hash
            || sha256_hex(result_json.as_bytes()) != result_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup read receipt failed its immutable fingerprint.",
            ));
        }
        let request: LookupRead = serde_json::from_str(&request_json)?;
        validate_lookup_read(&request)
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
        let invocation_packet_id: String = db.query_row(
            "SELECT packet_id FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=?",
            params![run_id, ordinal],
            |row| row.get(0),
        )?;
        let invocation_packet =
            wns_story::context_packets::validated_packet_record(db, &invocation_packet_id)?;
        let invocation_lookup = invocation_packet.receipt.lookup.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A lookup read has no authorized packet capability.",
            )
        })?;
        invocation_lookup
            .validate_capability()
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
        invocation_lookup
            .authorize_read(&request)
            .map_err(|error| CoreError::new("InvalidProject", &error.to_string()))?;
        let result: LookupReadResult = serde_json::from_str(&result_json)?;
        if request.id() != read_id {
            return Err(CoreError::new(
                "InvalidProject",
                "The lookup read ID does not match its exact request.",
            ));
        }
        let (frozen, frozen_namespace) =
            wns_story::story_context::validated_snapshot_record(db, &snapshot_id)?;
        if frozen_namespace != namespace
            || frozen.snapshot.project_id != project_id
            || frozen.policy.version != policy_epoch.to_string()
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The lookup read has a different frozen owner or policy.",
            ));
        }
        let requested = pending_reads(db, &run_id, ordinal as u8)?;
        if !requested.contains(&request) {
            return Err(CoreError::new(
                "InvalidProject",
                "The provider did not request this saved lookup read.",
            ));
        }
        if let LookupReadResult::Unavailable { code, detail } = &result
            && !request.is_memory()
        {
            validate_unavailable_lookup_result(code, detail, truncated)?;
        } else {
            let (expected, expected_truncated) = execute_read(db, &frozen, &request);
            if expected != result || truncated != i64::from(expected_truncated) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The lookup result cannot be reproduced from its exact frozen evidence.",
                ));
            }
        }
        let state: String = db.query_row(
            "SELECT state FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=? AND snapshot_id=? AND project_id=? AND operation_namespace=?",
            params![run_id, ordinal, snapshot_id, project_id, namespace],
            |row| row.get(0),
        )?;
        if state != LookupInvocationState::NeedsContext.as_str() {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup read is not attached to a needs-context invocation.",
            ));
        }
        let child_packet_id: String = db.query_row(
            "SELECT packet_id FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=?",
            params![run_id, ordinal + 1],
            |row| row.get(0),
        )?;
        let child_packet =
            wns_story::context_packets::validated_packet_record(db, &child_packet_id)?;
        let exchange = child_packet
            .receipt
            .lookup
            .as_ref()
            .and_then(|lookup| {
                lookup
                    .exchanges
                    .iter()
                    .find(|exchange| exchange.request.id() == read_id)
            })
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A lookup read is missing from its immutable child packet.",
                )
            })?;
        if exchange.request != request || exchange.result != result {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup read receipt does not match its immutable child packet.",
            ));
        }
        if let LookupReadResult::Read { complete, .. } = result
            && truncated != i64::from(!complete)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A lookup read truncation flag does not match its complete coverage.",
            ));
        }
    }
    Ok(())
}

fn validate_unavailable_lookup_result(code: &str, detail: &str, truncated: i64) -> CoreResult<()> {
    if code.is_empty()
        || code.len() > 128
        || detail.is_empty()
        || detail.len() > 4096
        || code
            .chars()
            .chain(detail.chars())
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        || truncated != 0
    {
        return Err(CoreError::new(
            "InvalidProject",
            "An unavailable lookup result has invalid bounded detail or truncation evidence.",
        ));
    }
    Ok(())
}

fn parse_decimal(value: &str) -> CoreResult<i64> {
    value.parse::<i64>().map_err(|_| {
        CoreError::new(
            "InvalidLookupCounter",
            "A lookup counter is not a valid SQLite integer.",
        )
    })
}
