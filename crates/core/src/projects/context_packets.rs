//! Local packet preparation and exact receipts. Dispatch belongs to the job
//! supervisor; preparing a packet never invokes a provider or edits prose.
use super::*;
use crate::context::BudgetError;
use crate::context::packet::{
    CompiledPacket, MOCK_MODEL_ID, MOCK_TOKEN_ACCOUNTING_METHOD, MockContextBudget, PacketError,
    PacketRequest, compile_packet, packet_input_hash, serialized_input,
};
use crate::documents::ScopeGrant;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareContext {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub snapshot_id: String,
    pub instruction: String,
    pub mandatory_handles: Vec<String>,
    pub scope: Option<ScopeGrant>,
    pub budget: MockContextBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub enum PreparationResult {
    Prepared {
        packet: Box<CompiledPacket>,
        current: bool,
    },
    BudgetRejected {
        error: BudgetError,
    },
}

pub(super) enum PacketCommand {
    Prepare(Box<PrepareContext>, Reply<PreparationResult>),
    Read(ProjectAccess, String, Reply<CompiledPacket>),
    Current(ProjectAccess, String, Reply<bool>),
}

impl ProjectSession {
    pub fn prepare_context(&self, request: PrepareContext) -> CoreResult<PreparationResult> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Prepare(Box::new(request), reply)))
        })
    }
    pub fn prepared_context(
        &self,
        access: ProjectAccess,
        packet_id: String,
    ) -> CoreResult<CompiledPacket> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Read(access, packet_id, reply)))
        })
    }
    pub fn prepared_context_is_current(
        &self,
        access: ProjectAccess,
        packet_id: String,
    ) -> CoreResult<bool> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Current(access, packet_id, reply)))
        })
    }
}

impl OwnedProject {
    pub(super) fn handle_packet(&mut self, command: PacketCommand) {
        match command {
            PacketCommand::Prepare(request, reply) => {
                let result = self.prepare_context_packet(*request);
                self.fence_uncertain(&result);
                let _ = reply.send(result);
            }
            PacketCommand::Read(access, id, reply) => {
                let _ = reply.send(self.read_context_packet(&access, &id));
            }
            PacketCommand::Current(access, id, reply) => {
                let result = self.read_context_packet(&access, &id).and_then(|packet| {
                    let snapshot = story_context::load_snapshot(
                        self.db()?,
                        &access,
                        &packet.receipt.snapshot_id,
                    )?;
                    Ok(snapshot.snapshot.context_source_epoch == self.context_source_epoch()?)
                });
                let _ = reply.send(result);
            }
        }
    }

    fn prepare_context_packet(&mut self, request: PrepareContext) -> CoreResult<PreparationResult> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        let payload = logical_hash(&request)?;
        let existing: Option<(String, String)> = self.db()?.query_row(
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
            let packet = self.read_context_packet(&request.access, &id)?;
            let frozen = story_context::load_snapshot(
                self.db()?,
                &request.access,
                &packet.receipt.snapshot_id,
            )?;
            return Ok(PreparationResult::Prepared {
                packet: Box::new(packet),
                current: frozen.snapshot.context_source_epoch == self.context_source_epoch()?,
            });
        }
        let frozen =
            story_context::load_snapshot(self.db()?, &request.access, &request.snapshot_id)?;
        if frozen
            .guidance
            .iter()
            .any(|record| record.version.scope == crate::context::guidance::GuidanceScope::Request)
        {
            return Err(CoreError::new(
                "RequestGuidanceRequiresDiscussion",
                "Request-scoped author guidance is reserved for an explicit discussion and cannot be replayed by generic packet preparation.",
            ));
        }
        if frozen.snapshot.context_source_epoch != self.context_source_epoch()? {
            return Err(CoreError::new(
                "ContextChanged",
                "The story changed. Prepare a fresh snapshot before compiling a new request.",
            ));
        }
        let sources = frozen
            .snapshot
            .sources
            .iter()
            .map(|source| story_context::read_source(self.db()?, &frozen, &source.handle))
            .collect::<CoreResult<Vec<_>>>()?;
        let compile_request = PacketRequest {
            packet_id: new_id(),
            session_id: new_id(),
            invocation_ordinal: "0".into(),
            frozen,
            instruction: request.instruction.clone(),
            sources,
            mandatory_handles: request.mandatory_handles.clone(),
            scope: request.scope.clone(),
            budget: request.budget.clone(),
        };
        let packet = match compile_packet(&compile_request) {
            Ok(packet) => packet,
            Err(PacketError::Budget(error)) => {
                return Ok(PreparationResult::BudgetRejected { error });
            }
            Err(error) => return Err(packet_error(error)),
        };
        let json = serde_json::to_string(&packet)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO context_packets(id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![packet.receipt.packet_id,request.access.project_id,request.access.operation_namespace,request.operation_id,payload,serde_json::to_string(&request)?,request.snapshot_id,packet.receipt.session_id,parse_version(&packet.receipt.invocation_ordinal)?,json,sha256_hex(json.as_bytes()),packet.receipt.input_hash])?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(PreparationResult::Prepared {
            packet: Box::new(packet),
            current: true,
        })
    }

    fn read_context_packet(&self, access: &ProjectAccess, id: &str) -> CoreResult<CompiledPacket> {
        self.check_access(access)?;
        let stored = read_packet_row(self.db()?, id)?;
        if stored.project_id != access.project_id || stored.namespace != access.operation_namespace
        {
            return Err(CoreError::new(
                "ContextProjectMismatch",
                "This prepared request belongs to another project or an independent recovered copy.",
            ));
        }
        let packet = validate_packet_row(self.db()?, &stored)?;
        story_context::load_snapshot(self.db()?, access, &packet.receipt.snapshot_id)?;
        Ok(packet)
    }
}

/// Read and validate a packet from an actor transaction. The packet's immutable
/// receipt, frozen source owner, and exact provider input are checked here;
/// transfer validation applies the same checks to every stored packet.
pub(super) fn read_context_packet_at(
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
    let packet: CompiledPacket = serde_json::from_str(&stored.json)?;
    let receipt = &packet.receipt;
    let input = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    if receipt.packet_id != stored.id
        || receipt.snapshot_id != stored.snapshot_id
        || receipt.session_id != stored.session_id
        || receipt.invocation_ordinal != parse_stored_version(stored.ordinal)?
        || receipt.input_hash != stored.input_hash
        || packet_input_hash(&packet.messages, &packet.options).map_err(packet_error)?
            != stored.input_hash
        || receipt.input_tokens != input.len().to_string()
        || receipt.token_accounting_method != MOCK_TOKEN_ACCOUNTING_METHOD
        || packet.options.model_id != MOCK_MODEL_ID
        || packet.options.token_accounting_method != MOCK_TOKEN_ACCOUNTING_METHOD
    {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The exact request input does not match its receipt.",
        ));
    }
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
    let expected = compile_packet(&PacketRequest {
        packet_id: receipt.packet_id.clone(),
        session_id: receipt.session_id.clone(),
        invocation_ordinal: receipt.invocation_ordinal.clone(),
        frozen,
        instruction: request.instruction,
        sources,
        mandatory_handles: request.mandatory_handles,
        scope: request.scope,
        budget: request.budget,
    })
    .map_err(|error| {
        CoreError::new(
            "InvalidContextPacket",
            &format!("The stored request cannot reproduce its packet: {error}"),
        )
    })?;
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

pub(crate) fn validate_context_packets(db: &Connection) -> CoreResult<()> {
    let mut statement = db.prepare("SELECT id FROM context_packets")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        validate_packet_row(db, &read_packet_row(db, &id)?)?;
    }
    Ok(())
}
