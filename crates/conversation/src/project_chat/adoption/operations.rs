use super::*;

pub(crate) fn prepare(
    host: &mut impl ProjectChatHost,
    request: PrepareChatAdoption,
) -> CoreResult<ChatAdoptionPreview> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.conversation_id)?;
    if request.drafts.is_empty() || request.drafts.len() > MAX_TARGETS {
        return Err(CoreError::new(
            "InvalidRequest",
            "Chat adoption requires one to three nonchapter drafts.",
        ));
    }

    let payload_hash = logical_hash(&request)?;
    let connection = host.db_mut()?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // A request operation is idempotent.  Keep the original immutable preview
    // and reconstruct its bodies from revisions rather than replaying writes.
    if let Some(existing) = operation_item(
        &tx,
        &request.access,
        &request.conversation_id,
        &request.operation_id,
    )? {
        if existing.kind != PREVIEW_KIND {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID already belongs to another project-chat command.",
            ));
        }
        let stored: StoredPreview = serde_json::from_value(existing.payload)?;
        if stored.request_hash != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another chat adoption preview.",
            ));
        }
        let preview = expand_preview(&tx, &request.access, stored.preview)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(preview);
    }

    let anchor = store::require_conversation(&tx, &request.access, &request.conversation_id)?;
    if anchor.role != DocumentRole::ConversationAnchor {
        return Err(CoreError::new(
            "DocumentRoleMismatch",
            "The project conversation anchor is not available.",
        ));
    }
    let (source_epoch, policy_epoch) = store::epochs(&tx)?;
    let workshop_version = current_workshop_version(&tx)?;
    let mut targets = Vec::with_capacity(request.drafts.len());
    let mut target_ids = HashSet::new();
    let mut draft_ids = HashSet::new();

    for (ordinal, draft_ref) in request.drafts.iter().enumerate() {
        let fields = draft_ref_fields(draft_ref)?;
        let draft_document_id = fields.document_id.clone();
        if !draft_ids.insert(draft_document_id.clone()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Chat adoption drafts must be unique.",
            ));
        }
        let mut draft = store::read_draft(
            &tx,
            &request.access,
            &request.conversation_id,
            &draft_document_id,
        )?;
        if draft.stale {
            return Err(CoreError::new(
                "ContextChanged",
                "The assistant draft was produced against an older story or policy epoch.",
            ));
        }
        if draft.disposition != "pending" {
            return Err(CoreError::new(
                "DraftNotAdoptable",
                "Only a pending assistant draft can be prepared for adoption.",
            ));
        }
        require_exact_draft_ref(&fields, &draft)?;
        if draft.document.role != DocumentRole::AssistantDraft {
            return Err(CoreError::new(
                "DocumentRoleMismatch",
                "Chat adoption requires an isolated assistant draft.",
            ));
        }
        if draft.document.kind == "chapter"
            || !["note", "world", "character", "theme", "hook", "scene"]
                .contains(&draft.document.kind.as_str())
        {
            return Err(CoreError::new(
                "InvalidDocument",
                "Chat adoption can target only nonchapter material documents.",
            ));
        }
        // Autosave advances the working draft without forcing a checkpoint
        // on every keystroke. Preparing review captures that exact saved head.
        let revision = checkpoint_at(&tx, &draft.document, "chatAdoptionPreviewDraft")?;
        let draft_revision_id = revision.id.clone();
        draft.document.last_checkpoint_id = Some(draft_revision_id.clone());
        // Ensure the checkpoint is valid before exposing a review preview.
        let revision = read_revision(&tx, &draft_revision_id)?;
        if revision.head.document_id != draft_document_id || revision.head != draft.document.head {
            return Err(CoreError::new(
                "DraftChanged",
                "The assistant draft revision no longer matches its current head.",
            ));
        }

        let (document_id, title, kind, before) = if let Some(expected) = draft.target.clone() {
            if !target_ids.insert(expected.document_id.clone()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat adoption targets must be unique.",
                ));
            }
            let current = read_document(&tx, &expected.document_id)?;
            require_head(&current.head, &expected)?;
            if current.role != DocumentRole::Ordinary {
                return Err(CoreError::new(
                    "DocumentRoleMismatch",
                    "An adoption target must be an ordinary story document.",
                ));
            }
            // A pre-chat document may have no checkpoint yet.  Creating this
            // immutable before reference does not alter its head or source
            // epoch and avoids duplicating its body in the preview ledger.
            checkpoint_at(&tx, &current, "chatAdoptionPreviewBefore")?;
            let current = read_document(&tx, &expected.document_id)?;
            (
                current.head.document_id.clone(),
                current.title.clone(),
                current.kind.clone(),
                Some(current),
            )
        } else {
            let reserved = new_id();
            if !target_ids.insert(reserved.clone()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat adoption targets must be unique.",
                ));
            }
            (
                reserved,
                draft.document.title.clone(),
                draft.document.kind.clone(),
                None,
            )
        };

        targets.push((
            draft_ref,
            fields,
            draft,
            draft_revision_id,
            document_id,
            title,
            kind,
            before,
            ordinal,
        ));
    }

    let material_targets = targets
        .iter()
        .map(
            |(_, _, draft, _, document_id, title, kind, _, _)| MaterialTarget {
                document_id: document_id.clone(),
                title: title.clone(),
                kind: kind.clone(),
                body: draft.document.body.clone(),
                expected: draft.target.clone(),
            },
        )
        .collect::<Vec<_>>();
    workshop_state::validate_chat_material_targets(&tx, &material_targets)?;

    let draft_document_ids = targets
        .iter()
        .map(|(_, fields, _, _, _, _, _, _, _)| fields.document_id.clone())
        .collect::<Vec<_>>();
    let origin = read_group_origin(
        &tx,
        &request.access,
        &request.conversation_id,
        &draft_document_ids,
    )?;
    let effects = build_adoption_effects(&tx, &targets, &origin, request.group_effects.as_ref())?;

    let preview_id = new_id();
    let mut expanded_targets = Vec::with_capacity(targets.len());
    let mut stored_targets = Vec::with_capacity(targets.len());
    for (draft_ref, fields, draft, draft_revision_id, document_id, title, kind, before, _) in
        targets
    {
        expanded_targets.push(ChatAdoptionTarget {
            draft: draft_ref.clone(),
            draft_revision_id: draft_revision_id.clone(),
            document_id: document_id.clone(),
            title: title.clone(),
            kind: kind.clone(),
            before: before.clone(),
            body: draft.document.body.clone(),
        });
        stored_targets.push(StoredPreviewTarget {
            draft: serde_json::to_value(draft_ref)?,
            draft_revision_id,
            draft_document_id: fields.document_id,
            disposition_version: fields.disposition_version,
            document_id,
            title,
            kind,
            before: before.map(stored_document_ref),
        });
    }
    let metadata_without_digest = ChatAdoptionPreview {
        id: preview_id,
        version: "1".into(),
        digest: String::new(),
        project_id: request.access.project_id.clone(),
        operation_namespace: request.access.operation_namespace.clone(),
        conversation_id: request.conversation_id.clone(),
        source_epoch: source_epoch.into(),
        policy_epoch,
        workshop_version,
        targets: expanded_targets,
        effects: Some(effects.clone()),
    };
    let digest = preview_digest(&metadata_without_digest)?;
    let mut preview = metadata_without_digest;
    preview.digest = digest.clone();
    let stored = StoredPreview {
        schema_version: "chat-adoption-preview.v1".into(),
        preview: StoredPreviewMetadata {
            id: preview.id.clone(),
            version: preview.version.clone(),
            digest,
            project_id: preview.project_id.clone(),
            operation_namespace: preview.operation_namespace.clone(),
            conversation_id: preview.conversation_id.clone(),
            source_epoch: preview.source_epoch.clone(),
            policy_epoch: preview.policy_epoch.clone(),
            workshop_version: preview.workshop_version.clone(),
            targets: stored_targets,
            effects: Some(effects),
        },
        request_hash: payload_hash,
    };
    let payload = serde_json::to_value(&stored)?;
    store::append_item(
        &tx,
        &request.access,
        &request.conversation_id,
        Some(&request.operation_id),
        PREVIEW_KIND,
        Some(&preview.id),
        &payload,
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(preview)
}

pub(crate) fn adopt(
    host: &mut impl ProjectChatHost,
    request: AdoptChatPreview,
) -> CoreResult<ChatAdoptionAck> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.conversation_id)?;
    check_id(&request.preview_id)?;
    parse_version(&request.preview_version)?;
    if !valid_hash(&request.preview_digest) {
        return Err(CoreError::new(
            "InvalidRequest",
            "The preview digest is invalid.",
        ));
    }
    let payload_hash = logical_hash(&request)?;
    let connection = host.db_mut()?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if let Some(existing) =
        existing_receipt(&tx, &request.access, &request.operation_id, &payload_hash)?
    {
        let stored: StoredAdoptionReceipt = serde_json::from_str(&existing)?;
        let documents = stored
            .documents
            .iter()
            .cloned()
            .map(|reference| read_historical_document_ref(&tx, reference))
            .collect::<CoreResult<Vec<_>>>()?;
        let ack = ChatAdoptionAck {
            preview_id: stored.preview_id,
            documents,
            decision_id: stored.decision_id,
        };
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(ack);
    }

    let item = store::read_item_by_reference(
        &tx,
        &request.conversation_id,
        PREVIEW_KIND,
        &request.preview_id,
    )?;
    let stored: StoredPreview = serde_json::from_value(item.payload)?;
    if stored.preview.id != request.preview_id
        || stored.preview.version != request.preview_version
        || stored.preview.digest != request.preview_digest
    {
        return Err(CoreError::new(
            "PreviewMismatch",
            "The selected chat adoption preview identity does not match.",
        ));
    }
    let preview = expand_preview(&tx, &request.access, stored.preview)?;
    validate_preview_current(&tx, &request, &preview)?;
    let targets = preview
        .targets
        .iter()
        .map(|target| MaterialTarget {
            document_id: target.document_id.clone(),
            title: target.title.clone(),
            kind: target.kind.clone(),
            body: target.body.clone(),
            expected: target.before.as_ref().map(|before| before.head.clone()),
        })
        .collect::<Vec<_>>();
    workshop_state::validate_chat_material_targets(&tx, &targets)?;

    let mut documents = Vec::with_capacity(targets.len());
    for target in &targets {
        documents.push(
            material_adoption::ApprovedMaterialWrite::new(&tx, target, MaterialAdoptionFlow::Chat)
                .apply()?,
        );
    }
    if !documents.is_empty() {
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
    }
    let committed_relationships = preview
        .effects
        .as_ref()
        .map(|effects| materialize_chat_relationships(effects, &documents))
        .transpose()?
        .unwrap_or_default();
    workshop_state::append_chat_relationships(
        &tx,
        &request.access.project_id,
        &request.access.operation_namespace,
        &request.operation_id,
        // New chat-origin workshop snapshots bind to the stable command
        // receipt hash.  The old tuple hash included ephemeral session and
        // lease fields nested inside the request and cannot be reproduced
        // after recovery; legacy snapshots are validated structurally.
        &payload_hash,
        parse_version(&preview.workshop_version)?,
        &committed_relationships,
    )?;
    let decision_id = new_id();
    for target in &preview.targets {
        let changed = tx.execute(
            "UPDATE assistant_drafts SET disposition='adopted',disposition_version=disposition_version+1 WHERE document_id=? AND conversation_id=? AND disposition='pending' AND disposition_version=?",
            params![
                target.draft_document_id(),
                request.conversation_id,
                draft_disposition_version(&target.draft)?,
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "DraftChanged",
                "An assistant draft changed while its preview was being adopted.",
            ));
        }
    }
    let decision_payload = json!({
        "schemaVersion": "chat-adoption-decision.v1",
        "previewId": preview.id,
        "previewVersion": preview.version,
        "previewDigest": preview.digest,
        "documentIds": documents.iter().map(|document| document.head.document_id.clone()).collect::<Vec<_>>(),
        "decisionId": decision_id,
        "effects": preview.effects,
        "snapshotPayloadHash": payload_hash,
    });
    store::append_item(
        &tx,
        &request.access,
        &request.conversation_id,
        Some(&request.operation_id),
        DECISION_KIND,
        Some(&decision_id),
        &decision_payload,
    )?;
    let receipt = StoredAdoptionReceipt {
        preview_id: preview.id.clone(),
        documents: documents.iter().cloned().map(stored_document_ref).collect(),
        decision_id: decision_id.clone(),
    };
    tx.execute(
        "INSERT INTO command_receipts(operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json) VALUES(?,?,?,?,?,?)",
        params![
            request.access.operation_namespace,
            request.operation_id,
            documents.first().map(|document| document.head.document_id.as_str()),
            payload_hash,
            RECEIPT_KIND,
            serde_json::to_string(&receipt)?,
        ],
    )?;
    let ack = ChatAdoptionAck {
        preview_id: preview.id,
        documents,
        decision_id,
    };
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(ack)
}
