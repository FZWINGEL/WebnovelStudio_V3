//! The packet-assembly glue: what a discussion contributes to a compiled
//! context packet, and what it reads back out of one.
//!
//! §3.4 named this concern specifically — "the packet glue is what moves down
//! to `wns-context`" — and the compilation itself did move, leaving here only
//! the discussion's own half: choosing the policy, resolving pinned handles,
//! capturing the scope, and inserting the compiled packet in the same
//! transaction that queues the run.

use super::*;

pub(super) fn packet_error(error: PacketError) -> CoreError {
    CoreError::new("ContextPreparationFailed", &error.to_string())
}

/// Discussion intent is part of the immutable context contract. Keeping it
/// there means a recovered database and old run rows do not need a second,
/// mutable intent column whose value could drift from the packet.
pub(super) fn intent_for_packet(
    db: &Connection,
    packet_id: &str,
) -> CoreResult<(FeedbackIntent, Option<BasisKind>)> {
    let (snapshot_id, manifest, manifest_hash): (String, String, String) = db
        .query_row(
            "SELECT p.snapshot_id,s.manifest_json,s.manifest_hash
             FROM context_packets p JOIN story_snapshots s ON s.id=p.snapshot_id
             WHERE p.id=?",
            [packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "InvalidContext",
                "The discussion packet has no frozen story context.",
            )
        })?;
    let frozen = story_context::decode_snapshot(&manifest, &manifest_hash)?;
    if frozen.snapshot.snapshot_id != snapshot_id {
        return Err(CoreError::new(
            "InvalidContext",
            "The discussion packet points to a different story snapshot.",
        ));
    }
    let intent = FeedbackIntent::from_purpose(frozen.purpose)?;
    Ok((
        intent,
        (intent == FeedbackIntent::Continue).then_some(frozen.snapshot.basis),
    ))
}

pub(super) fn discussion_context_policy(
    tx: &Connection,
    request: &StartDiscussion,
) -> CoreResult<(ContextPurpose, InformationPolicy)> {
    let policy_epoch: i64 = tx.query_row(
        "SELECT disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    let version = policy_epoch.to_string();
    let purpose = request.intent.purpose();
    match request.intent {
        FeedbackIntent::Discuss | FeedbackIntent::WorkshopExplore => Ok((
            purpose,
            InformationPolicy {
                version,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        )),
        FeedbackIntent::ProposeEdits | FeedbackIntent::Continue => {
            if request.intent == FeedbackIntent::ProposeEdits
                && request.scope.as_ref().is_none_or(|scope| {
                    !matches!(
                        scope.kind,
                        ScopeKind::Passage | ScopeKind::Blocks | ScopeKind::WholeDocument
                    )
                })
            {
                return Err(CoreError::new(
                    "InvalidScope",
                    "Propose edits requires an explicit passage selection.",
                ));
            }
            let target: Option<(String, i64)> = tx
                .query_row(
                    "SELECT kind,position FROM documents WHERE id=? AND trashed=0",
                    [&request.expected.document_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((kind, position)) = target else {
                return Err(CoreError::new(
                    "DocumentNotFound",
                    "The selected chapter is not available.",
                ));
            };
            if request.intent == FeedbackIntent::Continue && kind != "chapter" {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Only chapter documents can continue prose at the end.",
                ));
            }
            if request.intent == FeedbackIntent::ProposeEdits
                && kind != "chapter"
                && request.scope.as_ref().is_none_or(|scope| {
                    !matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                })
            {
                return Err(CoreError::new(
                    "InvalidScope",
                    "Document development requires an explicit block or whole-document scope.",
                ));
            }
            if kind == "chapter" && position < 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The selected chapter has an invalid reader position.",
                ));
            }
            if kind != "chapter" {
                return Ok((
                    purpose,
                    InformationPolicy {
                        version,
                        audience: Audience::AuthorRoom,
                        reader_frontier: None,
                        character_id: None,
                        character_grants: Vec::new(),
                        allow_alternatives: false,
                        allow_historical: false,
                    },
                ));
            }
            Ok((
                purpose,
                InformationPolicy {
                    version,
                    audience: Audience::RestrictedWriting,
                    reader_frontier: Some(position.to_string()),
                    character_id: None,
                    character_grants: Vec::new(),
                    allow_alternatives: false,
                    allow_historical: false,
                },
            ))
        }
    }
}

pub(super) fn resolve_pinned_handles(
    frozen: &FrozenContext,
    pinned_document_ids: &[String],
) -> CoreResult<Vec<String>> {
    let mut handles = Vec::with_capacity(pinned_document_ids.len());
    for document_id in pinned_document_ids {
        let handle = frozen
            .snapshot
            .sources
            .iter()
            .find(|source| source.source.document_id == *document_id)
            .map(|source| source.handle.clone())
            .ok_or_else(|| {
                CoreError::new(
                    "SourceNotFound",
                    "A pinned document is not in the frozen context.",
                )
            })?;
        if handles.iter().any(|existing| existing == &handle) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A document was pinned more than once.",
            ));
        }
        handles.push(handle);
    }
    Ok(handles)
}

pub(super) fn merge_pinned_document_ids(
    persistent: &[String],
    transient: &[String],
) -> CoreResult<Vec<String>> {
    let mut transient_seen = std::collections::HashSet::new();
    for id in transient {
        if !transient_seen.insert(id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A transient source document was pinned more than once.",
            ));
        }
    }
    let mut merged = BTreeSet::new();
    merged.extend(persistent.iter().cloned());
    merged.extend(transient.iter().cloned());
    if merged.len() > MAX_PINNED_DOCUMENTS {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion may use at most 64 source documents after persistent pins are merged.",
        ));
    }
    Ok(merged.into_iter().collect())
}

pub(super) fn capture_discussion_scope(
    input: Option<&DiscussionScopeInput>,
    target: &Value,
) -> CoreResult<Option<ScopeGrant>> {
    let Some(input) = input else {
        return Ok(None);
    };
    let captured = capture_scope(
        target,
        ScopeGrant {
            kind: input.kind,
            start: input.start.clone(),
            end: input.end.clone(),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .map_err(|message| CoreError::new("InvalidScope", &message))?;
    if captured.source_hash != input.source_body_hash || captured.quote != input.quote {
        return Err(CoreError::new(
            "InvalidScope",
            "The scope quote or source hash does not match the exact target revision.",
        ));
    }
    validate_scope(&ScopeValidationRequest {
        source_snapshot: target.clone(),
        result_snapshot: target.clone(),
        scope: captured.clone(),
    })
    .map_err(|message| CoreError::new("InvalidScope", &message))?;
    Ok(Some(captured))
}

pub(super) fn insert_packet(
    tx: &Connection,
    packet: &CompiledPacket,
    request: &StartDiscussion,
    mandatory_handles: &[String],
    transient_handles: Option<Vec<String>>,
    scope: Option<&ScopeGrant>,
    response_contract: Option<&str>,
) -> CoreResult<()> {
    // context_packets is validated on packet reads and transfers. Persist its
    // canonical PrepareContext envelope rather than the larger discussion
    // request so the packet remains readable through the shared C2 contract.
    let prepared = PrepareContext {
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        snapshot_id: packet.receipt.snapshot_id.clone(),
        instruction: packet_instruction(request, response_contract)?,
        mandatory_handles: mandatory_handles.to_vec(),
        transient_mandatory_handles: transient_handles,
        safe_brief: request.safe_brief.clone(),
        scope: scope.cloned(),
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        lookup: request
            .lookup
            .clone()
            .map(|allowance| wns_context::lookup::LookupPacketInput {
                allowance,
                completed_invocations: 0,
                exchanges: Vec::new(),
                source_projection: None,
                reviewed_memory: Some(wns_context::lookup::REVIEWED_MEMORY_CAPABILITY.to_owned()),
            }),
        response_contract: response_contract
            .map(str::to_owned)
            .or_else(|| match request.intent {
                FeedbackIntent::Discuss if request.lookup.is_some() => {
                    Some(LOOKUP_RESPONSE_CONTRACT.to_owned())
                }
                FeedbackIntent::Continue => Some(CONTINUATION_RESPONSE_CONTRACT.to_owned()),
                FeedbackIntent::ProposeEdits if packet.options.provider_binding.is_some() => Some(
                    if scope.is_some_and(|scope| {
                        matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                    }) {
                        STRUCTURED_PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    } else {
                        PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    },
                ),
                FeedbackIntent::WorkshopExplore => Some(WORKSHOP_RESPONSE_CONTRACT.to_owned()),
                _ => None,
            }),
    };
    let payload_hash = logical_hash(&prepared)?;
    let request_json = serde_json::to_string(&prepared)?;
    let packet_json = serde_json::to_string(packet)?;
    tx.execute("INSERT INTO context_packets(id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![packet.receipt.packet_id, request.access.project_id, request.access.operation_namespace, request.operation_id, payload_hash, request_json, packet.receipt.snapshot_id, packet.receipt.session_id, parse_version(&packet.receipt.invocation_ordinal)?, packet_json, sha256_hex(packet_json.as_bytes()), packet.receipt.input_hash])?;
    Ok(())
}

pub(super) fn packet_instruction(
    request: &StartDiscussion,
    response_contract: Option<&str>,
) -> CoreResult<String> {
    if response_contract == Some(project_chat_output::CHAPTER_DISCUSSION_RESPONSE_CONTRACT) {
        let target = serde_json::to_string(&request.expected)?;
        return Ok(format!(
            "{}\n\n{}\n{}",
            request.instruction,
            project_chat_output::CHAPTER_TARGET_HEAD_MARKER,
            target
        ));
    }
    Ok(request.instruction.clone())
}
