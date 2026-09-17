use super::*;

/// Validate a stored ref-only preview while transferring a database.
///
/// This deliberately rehydrates bodies from the immutable revision named by
/// the preview.  It does not compare the stored `before` reference with the
/// document's current working head: a later author edit makes a preview stale
/// for adoption, but it must not make an otherwise valid historical backup
/// unreadable.
pub(crate) fn validate_backup_preview(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    conversation_id: &str,
    reference_id: &str,
    payload: &Value,
) -> CoreResult<()> {
    check_id(project_id)?;
    check_id(operation_namespace)?;
    check_id(conversation_id)?;
    check_id(reference_id)?;
    let stored: StoredPreview = serde_json::from_value(payload.clone()).map_err(|error| {
        CoreError::new(
            "InvalidProjectChat",
            &format!("The stored adoption preview is invalid: {error}"),
        )
    })?;
    if stored.schema_version != "chat-adoption-preview.v1"
        || stored.preview.id != reference_id
        || stored.preview.project_id != project_id
        || stored.preview.operation_namespace != operation_namespace
        || stored.preview.conversation_id != conversation_id
        || stored.preview.targets.is_empty()
        || stored.preview.targets.len() > MAX_TARGETS
        || !valid_hash(&stored.request_hash)
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The stored adoption preview has invalid identity or bounds.",
        ));
    }
    parse_version(&stored.preview.version)?;
    if !valid_hash(&stored.preview.digest) {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The stored adoption preview has invalid version metadata.",
        ));
    }
    parse_version(&stored.preview.source_epoch)?;
    parse_version(&stored.preview.policy_epoch)?;
    parse_version(&stored.preview.workshop_version)?;
    let conversation_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM project_conversations
         WHERE id=? AND project_id=? AND operation_namespace=?)",
        rusqlite::params![conversation_id, project_id, operation_namespace],
        |row| row.get(0),
    )?;
    if !conversation_exists {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The adoption preview has no matching project conversation.",
        ));
    }

    let mut target_ids = HashSet::new();
    let mut targets = Vec::with_capacity(stored.preview.targets.len());
    for target in stored.preview.targets {
        let draft: ProjectChatDraftRef = serde_json::from_value(target.draft.clone())?;
        let draft_fields = draft_ref_fields(&draft)?;
        if draft_fields.document_id != target.draft_document_id
            || draft_fields.disposition_version != target.disposition_version
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "The adoption preview draft reference is inconsistent.",
            ));
        }
        let role: String = connection.query_row(
            "SELECT role FROM documents WHERE id=?",
            [&target.draft_document_id],
            |row| row.get(0),
        )?;
        if role != DocumentRole::AssistantDraft.storage_name() {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview does not point to an assistant draft.",
            ));
        }
        validate_assistant_draft_ownership(
            connection,
            &target.draft_document_id,
            conversation_id,
            project_id,
            operation_namespace,
        )?;
        let draft_revision = read_revision(connection, &target.draft_revision_id)?;
        if draft_revision.head.document_id != target.draft_document_id
            || draft_revision.head != draft_fields.head
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview draft revision does not match its frozen head.",
            ));
        }
        if !target_ids.insert(target.document_id.clone()) {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview contains duplicate targets.",
            ));
        }
        check_id(&target.document_id)?;
        let before = if let Some(before) = target.before {
            if before.role != DocumentRole::Ordinary
                || before.head.document_id != target.document_id
            {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "An adoption preview before reference is not ordinary story material.",
                ));
            }
            Some(read_historical_document_ref(connection, before)?)
        } else {
            let existing_role: Option<String> = connection
                .query_row(
                    "SELECT role FROM documents WHERE id=?",
                    [&target.document_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing_role.is_some_and(|role| role != DocumentRole::Ordinary.storage_name()) {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "An adoption preview reserves an isolated document identity.",
                ));
            }
            None
        };
        targets.push(ChatAdoptionTarget {
            draft,
            draft_revision_id: target.draft_revision_id,
            document_id: target.document_id,
            title: target.title,
            kind: target.kind,
            before,
            body: draft_revision.body,
        });
    }
    let preview = ChatAdoptionPreview {
        id: stored.preview.id,
        version: stored.preview.version,
        digest: stored.preview.digest,
        project_id: stored.preview.project_id,
        operation_namespace: stored.preview.operation_namespace,
        conversation_id: stored.preview.conversation_id,
        source_epoch: stored.preview.source_epoch,
        policy_epoch: stored.preview.policy_epoch,
        workshop_version: stored.preview.workshop_version,
        targets,
        effects: stored.preview.effects,
    };
    let expected = preview_digest(&ChatAdoptionPreview {
        digest: String::new(),
        ..preview.clone()
    })?;
    if expected != preview.digest {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The adoption preview digest does not match its immutable references.",
        ));
    }
    validate_effects_shape(connection, &preview)?;
    Ok(())
}

pub(crate) fn preview_digest(preview: &ChatAdoptionPreview) -> CoreResult<String> {
    let mut value = serde_json::to_value(preview)?;
    value["digest"] = Value::String(String::new());
    logical_hash(&value)
}

pub(crate) fn validate_historical_relationship_endpoint(
    connection: &Connection,
    document_id: &str,
    head: &Head,
) -> CoreResult<()> {
    let document = read_document(connection, document_id)?;
    if document.role != DocumentRole::Ordinary
        || !["character", "world"].contains(&document.kind.as_str())
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A proposed relationship endpoint is outside the frozen ordinary material.",
        ));
    }
    let version = parse_version(&head.version)?;
    let revision_id: Option<String> = connection
        .query_row(
            "SELECT id FROM revisions
             WHERE document_id=? AND source_working_version=? AND body_hash=?
             ORDER BY rowid DESC LIMIT 1",
            params![document_id, version, head.body_hash],
            |row| row.get(0),
        )
        .optional()?;
    let Some(revision_id) = revision_id else {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A proposed relationship endpoint has no retained source revision.",
        ));
    };
    let revision = read_revision(connection, &revision_id)?;
    if revision.head != *head {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A proposed relationship endpoint revision does not match its frozen head.",
        ));
    }
    Ok(())
}

/// Validate the immutable shape of a grouped manifest while reading a
/// backup. This intentionally does not apply current-head drift rules: a
/// historical preview remains readable after later author edits, but malformed
/// or silently unbound effect references must never enter the recovered view.
pub(crate) fn validate_effects_shape(
    connection: &Connection,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    let Some(effects) = preview.effects.as_ref() else {
        return Ok(());
    };
    if effects.version != CHAT_ADOPTION_EFFECTS_VERSION
        || !valid_hash(&effects.source_output_hash)
        || !effects.impacts.is_empty()
        || !effects.supersessions.is_empty()
        || !effects.placements.is_empty()
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption effect manifest has an unsupported or incomplete effect set.",
        ));
    }
    let target_ids = preview
        .targets
        .iter()
        .map(|target| target.document_id.as_str())
        .collect::<HashSet<_>>();
    let mut relationship_ids = HashSet::new();
    for dependency in &effects.relationship_dependencies {
        check_id(&dependency.relationship_id)?;
        check_id(&dependency.from_document_id)?;
        check_id(&dependency.to_document_id)?;
        if dependency.from_document_id == dependency.to_document_id
            || dependency.from_head.document_id != dependency.from_document_id
            || dependency.to_head.document_id != dependency.to_document_id
            || !valid_hash(&dependency.from_head.body_hash)
            || !valid_hash(&dependency.to_head.body_hash)
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A relationship dependency has invalid endpoint provenance.",
            ));
        }
        parse_version(&dependency.from_head.version)?;
        parse_version(&dependency.to_head.version)?;
    }
    for protected in &effects.protected_content {
        if !target_ids.contains(protected.target_document_id.as_str())
            || protected.source_head.document_id != protected.target_document_id
            || !valid_hash(&protected.source_head.body_hash)
            || sha256_hex(protected.text.as_bytes()) != protected.text_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "Protected chat content is not bound to an adoption target.",
            ));
        }
        parse_version(&protected.source_head.version)?;
    }
    for relationship in &effects.proposed_relationships {
        check_id(&relationship.relationship_id)?;
        if !relationship_ids.insert(relationship.relationship_id.as_str())
            || relationship.from_document_id == relationship.to_document_id
            || relationship.from_head.document_id != relationship.from_document_id
            || relationship.to_head.document_id != relationship.to_document_id
            || !valid_hash(&relationship.from_head.body_hash)
            || !valid_hash(&relationship.to_head.body_hash)
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A proposed relationship has invalid identity or provenance.",
            ));
        }
        parse_version(&relationship.from_head.version)?;
        parse_version(&relationship.to_head.version)?;
        for (document_id, head) in [
            (&relationship.from_document_id, &relationship.from_head),
            (&relationship.to_document_id, &relationship.to_head),
        ] {
            if target_ids.contains(document_id.as_str()) {
                continue;
            }
            validate_historical_relationship_endpoint(connection, document_id, head)?;
        }
    }
    Ok(())
}

/// Validate a workshop snapshot created by grouped project-chat adoption.
///
/// Chat adoption deliberately uses a separate authority from
/// `workshop_receipts`: its proof is the immutable `adoptChatPreview` command receipt plus the
/// conversation's preview and decision records.  New snapshots bind their
/// payload hash to that command receipt.  Older schema snapshots used a
/// nested tuple hash that included ephemeral access fields, so they take the
/// explicit structural-proof path below instead of pretending that hash can
/// be reproduced after recovery.
pub(crate) fn validate_chat_workshop_snapshot(
    connection: &Connection,
    origin: workshop::WorkshopSnapshotOrigin<'_>,
    state: &workshop::WorkshopState,
    previous_state: &workshop::WorkshopState,
) -> CoreResult<()> {
    let workshop::WorkshopSnapshotOrigin {
        project_id,
        namespace: operation_namespace,
        operation: operation_id,
        version: snapshot_version,
        payload_hash: snapshot_payload_hash,
    } = origin;
    let command: Option<(String, String, String, String)> = connection
        .query_row(
            "SELECT document_id,payload_hash,operation_kind,result_json
             FROM command_receipts
             WHERE operation_namespace=? AND operation_id=?",
            params![operation_namespace, operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((receipt_document_id, command_payload_hash, operation_kind, receipt_json)) = command
    else {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot has no matching command receipt.",
        ));
    };
    if operation_kind != RECEIPT_KIND || !valid_hash(&command_payload_hash) {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot has the wrong command receipt kind.",
        ));
    }
    let receipt: StoredAdoptionReceipt = serde_json::from_str(&receipt_json).map_err(|error| {
        CoreError::new(
            "InvalidProjectChat",
            &format!("A chat adoption receipt is invalid: {error}"),
        )
    })?;
    check_id(&receipt.preview_id)?;
    check_id(&receipt.decision_id)?;
    if receipt.documents.is_empty() || receipt.documents.len() > MAX_TARGETS {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption receipt has an invalid document count.",
        ));
    }
    if receipt_document_id != receipt.documents[0].head.document_id {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption receipt head does not match its command receipt.",
        ));
    }

    let decision_rows = {
        let mut statement = connection.prepare(
            "SELECT conversation_id,project_id,operation_namespace,reference_id,
                    payload_json,payload_hash
             FROM conversation_items
             WHERE operation_namespace=? AND operation_id=? AND kind='adoptionDecision'",
        )?;
        statement
            .query_map(params![operation_namespace, operation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    if decision_rows.len() != 1 {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption snapshot must have exactly one immutable decision event.",
        ));
    }
    let (
        conversation_id,
        decision_project_id,
        decision_namespace,
        decision_reference_id,
        decision_json,
        decision_payload_hash,
    ) = decision_rows.into_iter().next().expect("one decision row");
    if decision_project_id != project_id
        || decision_namespace != operation_namespace
        || decision_reference_id.as_deref() != Some(receipt.decision_id.as_str())
        || sha256_hex(decision_json.as_bytes()) != decision_payload_hash
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption decision has invalid project or immutable identity.",
        ));
    }
    let decision: StoredChatAdoptionDecision =
        serde_json::from_str(&decision_json).map_err(|error| {
            CoreError::new(
                "InvalidProjectChat",
                &format!("A chat adoption decision is invalid: {error}"),
            )
        })?;
    if decision.schema_version != "chat-adoption-decision.v1"
        || decision.decision_id != receipt.decision_id
        || decision.preview_id != receipt.preview_id
        || decision.effects.is_none()
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption decision has invalid preview provenance.",
        ));
    }

    let preview_row: Option<(String, String, String)> = connection
        .query_row(
            "SELECT payload_json,payload_hash,project_id
             FROM conversation_items
             WHERE conversation_id=? AND project_id=? AND operation_namespace=?
               AND kind='adoptionPreview' AND reference_id=?",
            params![
                conversation_id,
                project_id,
                operation_namespace,
                receipt.preview_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((preview_json, preview_payload_hash, preview_project_id)) = preview_row else {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption decision has no matching immutable preview.",
        ));
    };
    if preview_project_id != project_id
        || sha256_hex(preview_json.as_bytes()) != preview_payload_hash
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption preview has invalid immutable identity.",
        ));
    }
    let preview_value: Value = serde_json::from_str(&preview_json)?;
    validate_backup_preview(
        connection,
        project_id,
        operation_namespace,
        &conversation_id,
        &receipt.preview_id,
        &preview_value,
    )?;
    let stored_preview: StoredPreview = serde_json::from_value(preview_value)?;
    let access = ProjectAccess {
        project_id: project_id.to_owned(),
        session: String::new(),
        writer_lease: String::new(),
        operation_namespace: operation_namespace.to_owned(),
    };
    let preview = expand_preview(connection, &access, stored_preview.preview)?;
    let Some(effects) = preview.effects.clone() else {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot has no grouped effects.",
        ));
    };
    if effects.proposed_relationships.is_empty() {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot has no committed relationship effects.",
        ));
    }
    if preview.id != receipt.preview_id
        || preview.conversation_id != conversation_id
        || preview.project_id != project_id
        || preview.operation_namespace != operation_namespace
        || preview.version != decision.preview_version
        || preview.digest != decision.preview_digest
        || decision.effects.as_ref() != Some(&effects)
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption decision is not bound to its exact preview and effects.",
        ));
    }
    let preview_document_ids = preview
        .targets
        .iter()
        .map(|target| target.document_id.clone())
        .collect::<Vec<_>>();
    let receipt_document_ids = receipt
        .documents
        .iter()
        .map(|document| document.head.document_id.clone())
        .collect::<Vec<_>>();
    if preview_document_ids != receipt_document_ids || decision.document_ids != receipt_document_ids
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption receipt has mismatched committed document identities.",
        ));
    }

    let mut committed_documents = Vec::with_capacity(receipt.documents.len());
    for (target, reference) in preview.targets.iter().zip(&receipt.documents) {
        if reference.role != DocumentRole::Ordinary
            || reference.head.document_id != target.document_id
            || reference.title != target.title
            || reference.kind != target.kind
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A chat adoption receipt document does not match its preview target.",
            ));
        }
        let committed = read_historical_document_ref(connection, reference.clone())?;
        let body = wns_kernel::validate_snapshot_json(&serde_json::to_string(&target.body)?)
            .map_err(|error| CoreError::new("InvalidProjectChat", &error))?;
        let expected_version = target
            .before
            .as_ref()
            .map(|before| {
                parse_version(&before.head.version).and_then(|version| {
                    version.checked_add(1).ok_or_else(|| {
                        CoreError::new(
                            "InvalidProjectChat",
                            "A chat adoption target version overflowed.",
                        )
                    })
                })
            })
            .transpose()?
            .unwrap_or(0);
        let expected_head = Head {
            document_id: target.document_id.clone(),
            version: expected_version.to_string(),
            body_hash: body.hash,
        };
        if committed.role != DocumentRole::Ordinary
            || committed.head != expected_head
            || committed.body != body.snapshot
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A chat adoption receipt does not rehydrate the exact committed draft body.",
            ));
        }
        committed_documents.push(committed);
    }

    let expected_version = parse_version(&preview.workshop_version)?
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidProjectChat", "A workshop version overflowed."))?;
    if snapshot_version != expected_version {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot has the wrong next version.",
        ));
    }
    let committed_relationships = materialize_chat_relationships(&effects, &committed_documents)?;
    let mut expected_state = previous_state.clone();
    expected_state.relationships.extend(committed_relationships);
    if state != &expected_state {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat-origin workshop snapshot is not the exact adoption transition.",
        ));
    }

    let command_request = AdoptChatPreview {
        access,
        operation_id: operation_id.to_owned(),
        conversation_id,
        preview_id: preview.id,
        preview_version: preview.version,
        preview_digest: preview.digest,
    };
    let expected_command_hash = logical_hash(&command_request)?;
    if command_payload_hash != expected_command_hash {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption command receipt has an invalid request hash.",
        ));
    }
    if let Some(bound_snapshot_hash) = decision.snapshot_payload_hash.as_deref() {
        if bound_snapshot_hash != expected_command_hash
            || snapshot_payload_hash != expected_command_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A chat adoption snapshot is not bound to its command receipt.",
            ));
        }
    } else if snapshot_payload_hash == expected_command_hash {
        // A schema-40 snapshot with the new command hash is valid even when
        // its decision predates the optional binding field.  The complete
        // structural proof above remains mandatory for both forms.
    } else {
        // Legacy schema snapshots used an unrecoverable nested tuple hash.
        // All identity, preview, receipt, revision, and state-transition
        // proofs above are mandatory before accepting that historical form.
    }
    Ok(())
}

pub(crate) fn body_text(value: &Value) -> String {
    value["body"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| {
            block["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|node| {
                    if node["type"] == "hardBreak" {
                        "\n"
                    } else {
                        node["text"].as_str().unwrap_or_default()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn validate_preview_current(
    connection: &Connection,
    request: &AdoptChatPreview,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    if preview.project_id != request.access.project_id
        || preview.operation_namespace != request.access.operation_namespace
        || preview.conversation_id != request.conversation_id
    {
        return Err(CoreError::new(
            "WrongProjectSession",
            "The chat adoption preview belongs to another project identity.",
        ));
    }
    let (source_epoch, policy_epoch) = store::epochs(connection)?;
    if source_epoch != preview.source_epoch.as_str() || policy_epoch != preview.policy_epoch {
        return Err(CoreError::new(
            "ContextChanged",
            "The story or permissions changed after this chat preview.",
        ));
    }
    if current_workshop_version(connection)? != preview.workshop_version {
        return Err(CoreError::new(
            "WorkshopChanged",
            "Workshop protection or relationships changed after this chat preview.",
        ));
    }
    let mut target_ids = HashSet::new();
    for target in &preview.targets {
        if let Some(before) = &target.before {
            expand_document_ref(connection, stored_document_ref(before.clone()))?;
        }
        if !target_ids.insert(&target.document_id) {
            return Err(CoreError::new(
                "InvalidProject",
                "A chat adoption preview contains duplicate targets.",
            ));
        }
        let draft = store::read_draft(
            connection,
            &request.access,
            &request.conversation_id,
            &target.draft_document_id(),
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
                "A chat draft is no longer pending adoption.",
            ));
        }
        if draft.document.last_checkpoint_id.as_deref() != Some(target.draft_revision_id.as_str()) {
            return Err(CoreError::new(
                "DraftChanged",
                "A chat draft revision changed after preview preparation.",
            ));
        }
        let revision = read_revision(connection, &target.draft_revision_id)?;
        if revision.body != target.body {
            return Err(CoreError::new(
                "DraftChanged",
                "A chat draft body changed after preview preparation.",
            ));
        }
        let fields = draft_ref_fields(&target.draft)?;
        require_exact_draft_ref(&fields, &draft)?;
    }
    validate_effects_current(connection, preview)?;
    Ok(())
}

pub(crate) fn validate_effects_current(
    connection: &Connection,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    let Some(effects) = preview.effects.as_ref() else {
        // Historical previews predate the grouped manifest. Their original
        // digest and body-only adoption semantics remain unchanged.
        return Ok(());
    };
    if effects.version != CHAT_ADOPTION_EFFECTS_VERSION || !valid_hash(&effects.source_output_hash)
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The chat adoption effect manifest has invalid version or provenance.",
        ));
    }
    let target_ids = preview
        .targets
        .iter()
        .map(|target| target.document_id.clone())
        .collect::<HashSet<_>>();
    let dependencies = workshop_state::chat_relationship_dependencies(connection, &target_ids)?;
    for expected in &effects.relationship_dependencies {
        let Some(actual) = dependencies
            .iter()
            .find(|relationship| relationship.id == expected.relationship_id)
        else {
            return Err(CoreError::new(
                "WorkshopChanged",
                "A relationship in this chat preview no longer exists.",
            ));
        };
        if actual.from_document_id != expected.from_document_id
            || actual.to_document_id != expected.to_document_id
            || actual.relationship_type != expected.relationship_type
            || actual.source_heads.first() != Some(&expected.from_head)
            || actual.source_heads.get(1) != Some(&expected.to_head)
        {
            return Err(CoreError::new(
                "WorkshopChanged",
                "A relationship dependency changed after this chat preview.",
            ));
        }
        for head in [&expected.from_head, &expected.to_head] {
            let current = read_document(connection, &head.document_id)?;
            let target_is_changed = target_ids.contains(&head.document_id);
            if !target_is_changed && current.head != *head {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A relationship endpoint changed after this chat preview.",
                ));
            }
        }
    }
    for protected in &effects.protected_content {
        let current = read_document(connection, &protected.target_document_id)?;
        if current.head != protected.source_head
            || sha256_hex(protected.text.as_bytes()) != protected.text_hash
            || !workshop_state::chat_protected_text(connection, &protected.target_document_id)?
                .contains(&protected.text)
        {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "Protected chat content changed after this preview.",
            ));
        }
        if !body_text(&current.body).contains(&protected.text) {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "The adoption result removed protected chat content.",
            ));
        }
    }
    for relationship in &effects.proposed_relationships {
        for head in [&relationship.from_head, &relationship.to_head] {
            if target_ids.contains(&head.document_id) {
                let target = preview
                    .targets
                    .iter()
                    .find(|target| target.document_id == head.document_id)
                    .and_then(|target| target.before.as_ref())
                    .map(|before| &before.head);
                if target.is_some_and(|current| current != head) {
                    return Err(CoreError::new(
                        "DraftChanged",
                        "A proposed relationship target changed after preview preparation.",
                    ));
                }
            } else if read_document(connection, &head.document_id)?.head != *head {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A proposed relationship endpoint changed after preview preparation.",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn draft_disposition_version(reference: &ProjectChatDraftRef) -> CoreResult<String> {
    draft_ref_fields(reference).map(|fields| fields.disposition_version)
}

pub(crate) fn existing_receipt(
    connection: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
    payload_hash: &str,
) -> CoreResult<Option<String>> {
    let mut conflicting_receipt = false;
    for (table, kind_column) in [
        ("workshop_receipts", "operation_kind"),
        ("proposal_receipts", "kind"),
    ] {
        let found: Option<String> = connection
            .query_row(
                &format!("SELECT {kind_column} FROM {table} WHERE operation_namespace=? AND operation_id=?"),
                params![access.operation_namespace, operation_id],
                |row| row.get(0),
            )
            .optional()?;
        if found.is_some() {
            conflicting_receipt = true;
        }
    }
    let found: Option<(String, String)> = connection
        .query_row(
            "SELECT operation_kind,payload_hash,result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((kind, hash)) = found else {
        if conflicting_receipt {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another command.",
            ));
        }
        return Ok(None);
    };
    if kind != RECEIPT_KIND || hash != payload_hash {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for another command.",
        ));
    }
    connection
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(CoreError::from)
}

// These methods intentionally keep field extraction at the boundary.  The
// parent module can evolve ProjectChatDraftRef without making persisted
// previews depend on a second copy of its body.
pub(crate) trait ChatTargetExt {
    fn draft_document_id(&self) -> String;
}
impl ChatTargetExt for ChatAdoptionTarget {
    fn draft_document_id(&self) -> String {
        draft_ref_fields(&self.draft)
            .map(|fields| fields.document_id)
            .unwrap_or_default()
    }
}
