//! Local packet preparation and exact receipts. Dispatch belongs to the job
//! supervisor; preparing a packet never invokes a provider or edits prose.
use crate::host::StoryHost;
use crate::story_context;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use wns_context::BudgetError;
use wns_context::packet::{
    CONTEXT_PACKET_SCHEMA_V1, CONTEXT_PACKET_SCHEMA_V2, CompiledPacket, MOCK_MODEL_ID,
    MOCK_TOKEN_ACCOUNTING_METHOD, MockContextBudget, PacketError, PacketRequest, ProviderBinding,
    compile_packet, compile_packet_legacy, packet_input_hash, serialized_input,
};
use wns_documents::ScopeGrant;
use wns_kernel::{
    CoreError, CoreResult, ProjectAccess, Reply, check_id, logical_hash, new_id,
    parse_stored_version, parse_version, sha256_hex,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareContext {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub snapshot_id: String,
    pub instruction: String,
    pub mandatory_handles: Vec<String>,
    /// The original caller-selected handles for discussion retries. Generic
    /// preparation leaves this absent; persistent AuthorRoom pins are loaded
    /// again when a linked retry starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transient_mandatory_handles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<wns_context::SafeBriefInput>,
    pub scope: Option<ScopeGrant>,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<wns_context::lookup::LookupPacketInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub enum PreparationResult {
    #[specta(rename_all = "camelCase")]
    Prepared {
        packet: Box<CompiledPacket>,
        current: bool,
    },
    #[specta(rename_all = "camelCase")]
    BudgetRejected { error: BudgetError },
}

pub enum PacketCommand {
    Prepare(Box<PrepareContext>, Reply<PreparationResult>),
    Read(ProjectAccess, String, Reply<CompiledPacket>),
    Current(ProjectAccess, String, Reply<bool>),
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn handle_packet(host: &mut impl StoryHost, command: PacketCommand) {
    match command {
        PacketCommand::Prepare(request, reply) => {
            let result = prepare_context_packet(host, *request);
            host.fence_uncertain(&result);
            let _ = reply.send(result);
        }
        PacketCommand::Read(access, id, reply) => {
            let _ = reply.send(read_context_packet(host, &access, &id));
        }
        PacketCommand::Current(access, id, reply) => {
            let result = read_context_packet(host, &access, &id).and_then(|packet| {
                let snapshot =
                    story_context::load_snapshot(host.db()?, &access, &packet.receipt.snapshot_id)?;
                Ok(snapshot.snapshot.context_source_epoch == host.context_source_epoch()?)
            });
            let _ = reply.send(result);
        }
    }
}

pub fn prepare_context_packet(
    host: &mut impl StoryHost,
    request: PrepareContext,
) -> CoreResult<PreparationResult> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    if request.lookup.is_some() {
        return Err(CoreError::new(
            "LookupRequiresDiscussion",
            "Story lookups require an explicitly authorized discussion job.",
        ));
    }
    if request.safe_brief.is_some() {
        return Err(CoreError::new(
            "SafeBriefRequiresDiscussion",
            "An approved writing brief must be committed by a restricted discussion start.",
        ));
    }
    if request.response_contract.is_some() {
        return Err(CoreError::new(
            "ResponseContractRequiresDiscussion",
            "A provider response contract is reserved for an internal live discussion start.",
        ));
    }
    let payload = logical_hash(&request)?;
    let existing: Option<(String, String)> = host.db()?.query_row(
        "SELECT id,payload_hash FROM context_packets WHERE operation_namespace=? AND operation_id=?",
        params![request.access.operation_namespace,request.operation_id],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    if let Some((id, previous)) = existing {
        if previous != payload {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This preparation was already used for another request.",
            ));
        }
        let packet = read_context_packet(host, &request.access, &id)?;
        let frozen =
            story_context::load_snapshot(host.db()?, &request.access, &packet.receipt.snapshot_id)?;
        return Ok(PreparationResult::Prepared {
            packet: Box::new(packet),
            current: frozen.snapshot.context_source_epoch == host.context_source_epoch()?,
        });
    }
    let frozen = story_context::load_snapshot(host.db()?, &request.access, &request.snapshot_id)?;
    if frozen
        .guidance
        .iter()
        .any(|record| record.version.scope == wns_context::guidance::GuidanceScope::Request)
    {
        return Err(CoreError::new(
            "RequestGuidanceRequiresDiscussion",
            "Request-scoped author guidance is reserved for an explicit discussion and cannot be replayed by generic packet preparation.",
        ));
    }
    if frozen.snapshot.context_source_epoch != host.context_source_epoch()? {
        return Err(CoreError::new(
            "ContextChanged",
            "The story changed. Prepare a fresh snapshot before compiling a new request.",
        ));
    }
    let sources = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| story_context::read_source(host.db()?, &frozen, &source.handle))
        .collect::<CoreResult<Vec<_>>>()?;
    let compile_request = PacketRequest {
        packet_id: new_id(),
        session_id: new_id(),
        invocation_ordinal: "0".into(),
        frozen,
        instruction: request.instruction.clone(),
        sources,
        mandatory_handles: request.mandatory_handles.clone(),
        safe_brief: request.safe_brief.clone(),
        scope: request.scope.clone(),
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        response_contract: request.response_contract.clone(),
        // Parsed here, where the instruction is authored, and passed down.
        // The compiler used to parse it out of `instruction` itself, which
        // made it reach up into this crate for the workshop vocabulary and
        // its validation cluster. The builder is the only code that can
        // build a valid workshop instruction, so it owns proving this.
        workshop_metadata: if request.response_contract.as_deref()
            == Some(wns_context::response_contracts::WORKSHOP_RESPONSE_CONTRACT)
        {
            let metadata =
                crate::workshop_metadata::metadata_from_instruction(&request.instruction)?;
            Some(crate::workshop_metadata::metadata_value(&metadata)?)
        } else {
            None
        },
        lookup: request.lookup.clone(),
    };
    let packet = match compile_packet(&compile_request) {
        Ok(packet) => packet,
        Err(PacketError::Budget(error)) => {
            return Ok(PreparationResult::BudgetRejected { error });
        }
        Err(error) => return Err(packet_error(error)),
    };
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    persist_compiled_packet_at(&tx, &request, &packet)?;
    tx.commit().map_err(CoreError::uncertain)?;
    // Keep the test-only process-loss barrier aligned with the other
    // durable author operations.  The barrier runs after COMMIT and
    // before the caller receives the packet acknowledgment, so recovery
    // must discover the exact operation by retrying its immutable request.
    host.hold_context_after_commit_before_ack(&request.operation_id);
    Ok(PreparationResult::Prepared {
        packet: Box::new(packet),
        current: true,
    })
}

pub fn read_context_packet(
    host: &impl StoryHost,
    access: &ProjectAccess,
    id: &str,
) -> CoreResult<CompiledPacket> {
    host.check_access(access)?;
    let stored = read_packet_row(host.db()?, id)?;
    if stored.project_id != access.project_id || stored.namespace != access.operation_namespace {
        return Err(CoreError::new(
            "ContextProjectMismatch",
            "This prepared request belongs to another project or an independent recovered copy.",
        ));
    }
    let packet = validate_packet_row(host.db()?, &stored)?;
    story_context::load_snapshot(host.db()?, access, &packet.receipt.snapshot_id)?;
    Ok(packet)
}

/// Internal request owners compose this insert with their job acceptance.
/// The caller must compile and validate the exact request before persisting it.
pub fn persist_compiled_packet_at(
    tx: &Connection,
    request: &PrepareContext,
    packet: &CompiledPacket,
) -> CoreResult<()> {
    let payload = logical_hash(request)?;
    let json = serde_json::to_string(packet)?;
    tx.execute("INSERT INTO context_packets(id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![packet.receipt.packet_id,request.access.project_id,request.access.operation_namespace,request.operation_id,payload,serde_json::to_string(request)?,request.snapshot_id,packet.receipt.session_id,parse_version(&packet.receipt.invocation_ordinal)?,json,sha256_hex(json.as_bytes()),packet.receipt.input_hash])?;
    Ok(())
}

/// Read and validate a packet from an actor transaction. The packet's immutable
/// receipt, frozen source owner, and exact provider input are checked here;
/// transfer validation applies the same checks to every stored packet.
pub fn read_context_packet_at(
    db: &Connection,
    access: &ProjectAccess,
    id: &str,
) -> CoreResult<CompiledPacket> {
    let stored = read_packet_row(db, id)?;
    if stored.project_id != access.project_id || stored.namespace != access.operation_namespace {
        return Err(CoreError::new(
            "ContextProjectMismatch",
            "This prepared request belongs to another project or an independent recovered copy.",
        ));
    }
    let packet = validate_packet_row(db, &stored)?;
    story_context::load_snapshot(db, access, &packet.receipt.snapshot_id)?;
    Ok(packet)
}

struct PacketRow {
    id: String,
    project_id: String,
    namespace: String,
    operation: String,
    payload: String,
    request_json: String,
    snapshot_id: String,
    session_id: String,
    ordinal: i64,
    json: String,
    hash: String,
    input_hash: String,
}

/// Historical integrity without current permission. For durable terminal
/// results and backup validation only; request-facing reads use the owner and
/// current-policy checks in `read_context_packet_at`.
pub fn validated_packet_record(db: &Connection, id: &str) -> CoreResult<CompiledPacket> {
    validate_packet_row(db, &read_packet_row(db, id)?)
}

fn read_packet_row(db: &Connection, id: &str) -> CoreResult<PacketRow> {
    check_id(id)?;
    db.query_row("SELECT id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash FROM context_packets WHERE id=?", [id], |row| Ok(PacketRow { id:row.get(0)?,project_id:row.get(1)?,namespace:row.get(2)?,operation:row.get(3)?,payload:row.get(4)?,request_json:row.get(5)?,snapshot_id:row.get(6)?,session_id:row.get(7)?,ordinal:row.get(8)?,json:row.get(9)?,hash:row.get(10)?,input_hash:row.get(11)? })).optional()?.ok_or_else(|| CoreError::new("ContextPacketNotFound", "The prepared request is not available in this project."))
}

fn validate_packet_row(db: &Connection, stored: &PacketRow) -> CoreResult<CompiledPacket> {
    for id in [
        &stored.id,
        &stored.project_id,
        &stored.namespace,
        &stored.operation,
        &stored.snapshot_id,
        &stored.session_id,
    ] {
        check_id(id)?;
    }
    if sha256_hex(stored.json.as_bytes()) != stored.hash {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The prepared request failed its fingerprint check.",
        ));
    }
    let request: PrepareContext = serde_json::from_str(&stored.request_json)?;
    if logical_hash(&request)? != stored.payload
        || request.operation_id != stored.operation
        || request.snapshot_id != stored.snapshot_id
        || request.access.project_id != stored.project_id
        || request.access.operation_namespace != stored.namespace
    {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The prepared request does not match its operation receipt.",
        ));
    }
    let packet_json: serde_json::Value = serde_json::from_str(&stored.json)?;
    let has_mandatory_annotation = packet_json["receipt"]
        .as_object()
        .is_some_and(|receipt| receipt.contains_key("mandatorySourceHandles"));
    let packet: CompiledPacket = serde_json::from_value(packet_json)?;
    let receipt = &packet.receipt;
    let input = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let expected_model = request
        .provider_binding
        .as_ref()
        .map_or(MOCK_MODEL_ID, |binding| binding.model_id.as_str());
    let expected_accounting = request
        .provider_binding
        .as_ref()
        .map_or(MOCK_TOKEN_ACCOUNTING_METHOD, |binding| {
            binding.accounting_method.as_str()
        });
    if receipt.packet_id != stored.id
        || receipt.snapshot_id != stored.snapshot_id
        || receipt.session_id != stored.session_id
        || receipt.invocation_ordinal != parse_stored_version(stored.ordinal)?
        || receipt.input_hash != stored.input_hash
        || packet_input_hash(&packet.messages, &packet.options).map_err(packet_error)?
            != stored.input_hash
        || receipt.input_tokens != input.len().to_string()
        || receipt.token_accounting_method != expected_accounting
        || packet.options.model_id != expected_model
        || packet.options.token_accounting_method != expected_accounting
        || packet.options.provider_binding != request.provider_binding
    {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The exact request input does not match its receipt.",
        ));
    }
    let envelope_schema = packet
        .messages
        .get(1)
        .and_then(|message| serde_json::from_str::<serde_json::Value>(&message.content).ok())
        .and_then(|envelope| {
            envelope
                .get("schema")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            CoreError::new(
                "InvalidContextPacket",
                "The context envelope is missing its schema identifier.",
            )
        })?;
    let compile = match envelope_schema.as_str() {
        CONTEXT_PACKET_SCHEMA_V1 => compile_packet_legacy,
        CONTEXT_PACKET_SCHEMA_V2 => compile_packet,
        _ => {
            return Err(CoreError::new(
                "InvalidContextPacket",
                "The context envelope schema is unknown.",
            ));
        }
    };
    // Validate retained history without granting present-day access. Revoked
    // packets must remain backup-able; request-facing reads separately enforce
    // current project ownership and policy through load_snapshot.
    let (frozen, owner_namespace) =
        story_context::validated_snapshot_record(db, &stored.snapshot_id)?;
    if (frozen.snapshot.project_id.clone(), owner_namespace)
        != (stored.project_id.clone(), stored.namespace.clone())
    {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The request and snapshot have different owners.",
        ));
    }
    let sources = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| story_context::read_source(db, &frozen, &source.handle))
        .collect::<CoreResult<Vec<_>>>()?;
    // Reproduction must supply what the original compile was given, or it
    // verifies a packet against a request that no longer matches it.
    let workshop_metadata = if request.response_contract.as_deref()
        == Some(wns_context::response_contracts::WORKSHOP_RESPONSE_CONTRACT)
    {
        let metadata = crate::workshop_metadata::metadata_from_instruction(&request.instruction)?;
        Some(crate::workshop_metadata::metadata_value(&metadata)?)
    } else {
        None
    };
    let mut expected = compile(&PacketRequest {
        packet_id: receipt.packet_id.clone(),
        session_id: receipt.session_id.clone(),
        invocation_ordinal: receipt.invocation_ordinal.clone(),
        frozen,
        instruction: request.instruction,
        sources,
        mandatory_handles: request.mandatory_handles,
        scope: request.scope,
        safe_brief: request.safe_brief,
        budget: request.budget,
        provider_binding: request.provider_binding,
        response_contract: request.response_contract,
        workshop_metadata,
        lookup: request.lookup,
    })
    .map_err(|error| {
        CoreError::new(
            "InvalidContextPacket",
            &format!("The stored request cannot reproduce its packet: {error}"),
        )
    })?;
    // Older exact packets predate this optional inspector annotation. Rebuild
    // from the original immutable request, never from receipt metadata, then
    // compare the historical representation without inventing a new receipt.
    if !has_mandatory_annotation && request.transient_mandatory_handles.is_none() {
        expected.receipt.mandatory_source_handles.clear();
    }
    if expected != packet {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The delivered packet does not match the stored request and frozen source revisions.",
        ));
    }
    Ok(packet)
}

fn packet_error(error: PacketError) -> CoreError {
    CoreError::new("ContextPreparationFailed", &error.to_string())
}

pub fn validate_context_packets(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare("SELECT id FROM context_packets")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        validate_packet_row(db, &read_packet_row(db, &id)?)?;
    }
    Ok(())
}

// The actor implements the host. One impl for the whole story-context cluster,
// declared beside the first module to convert.
