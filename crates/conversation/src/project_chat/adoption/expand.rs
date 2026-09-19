use super::*;

// ---- ref-only persistence and validation ---------------------------------

#[derive(Debug, Clone)]

pub(crate) struct DraftRefFields {
    pub(crate) document_id: String,
    pub(crate) head: Head,
    pub(crate) disposition_version: String,
}

pub(crate) type PreparedChatTarget<'a> = (
    &'a ProjectChatDraftRef,
    DraftRefFields,
    AssistantDraft,
    String,
    String,
    String,
    String,
    Option<DocumentRecord>,
    usize,
);

pub(crate) fn draft_ref_fields(reference: &impl Serialize) -> CoreResult<DraftRefFields> {
    let value = serde_json::to_value(reference)?;
    let head: Head = value
        .get("head")
        .cloned()
        .ok_or_else(|| {
            CoreError::new(
                "InvalidRequest",
                "A chat draft reference has no exact head.",
            )
        })
        .and_then(|head| serde_json::from_value(head).map_err(CoreError::from))?;
    let document_id = head.document_id.clone();
    let disposition_version = value
        .get("dispositionVersion")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        })
        .ok_or_else(|| {
            CoreError::new(
                "InvalidRequest",
                "A chat draft reference has no disposition version.",
            )
        })?;
    check_id(&document_id)?;
    parse_version(&head.version)?;
    if !valid_hash(&head.body_hash) {
        return Err(CoreError::new(
            "InvalidRequest",
            "A chat draft reference has an invalid body hash.",
        ));
    }
    parse_version(&disposition_version)?;
    Ok(DraftRefFields {
        document_id: document_id.to_owned(),
        head,
        disposition_version,
    })
}

pub(crate) fn require_exact_draft_ref(
    fields: &DraftRefFields,
    draft: &AssistantDraft,
) -> CoreResult<()> {
    if draft.document.head != fields.head || draft.disposition_version != fields.disposition_version
    {
        return Err(CoreError::new(
            "DraftChanged",
            "The chat draft changed after the request reference was captured.",
        ));
    }
    Ok(())
}

/// Verify that a preview's assistant-draft document is owned by the exact
/// conversation identity that produced the preview.  The document role and
/// immutable revision checks are separate concerns; this membership check
/// prevents a copied draft from another project-chat namespace from being
/// accepted merely because its head and body are valid.
pub(crate) fn validate_assistant_draft_ownership(
    connection: &Connection,
    document_id: &str,
    conversation_id: &str,
    project_id: &str,
    operation_namespace: &str,
) -> CoreResult<()> {
    let owner: Option<(String, String, String)> = connection
        .query_row(
            "SELECT conversation_id,project_id,operation_namespace
             FROM assistant_drafts WHERE document_id=?",
            [document_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((owner_conversation, owner_project, owner_namespace)) = owner else {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "An adoption preview references an assistant draft without provenance.",
        ));
    };
    if owner_conversation != conversation_id
        || owner_project != project_id
        || owner_namespace != operation_namespace
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "An adoption preview assistant draft belongs to another conversation identity.",
        ));
    }
    Ok(())
}

pub(crate) fn read_group_origin(
    connection: &Transaction<'_>,
    access: &ProjectAccess,
    conversation_id: &str,
    draft_document_ids: &[String],
) -> CoreResult<GroupOrigin> {
    let mut run_id = None;
    for document_id in draft_document_ids {
        let row: (String, String, String, String) = connection.query_row(
            "SELECT origin_run_id,packet_id,project_id,operation_namespace FROM assistant_drafts
             WHERE document_id=? AND conversation_id=?",
            params![document_id, conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if row.2 != access.project_id || row.3 != access.operation_namespace {
            return Err(CoreError::new(
                "WrongProjectSession",
                "A chat adoption draft belongs to another project identity.",
            ));
        }
        if let Some(existing) = run_id.as_deref()
            && existing != row.0
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Grouped chat adoption drafts must come from one assistant response.",
            ));
        }
        run_id = Some(row.0);
    }
    let run_id = run_id.ok_or_else(|| {
        CoreError::new(
            "InvalidRequest",
            "Grouped chat adoption requires at least one originating run.",
        )
    })?;
    let (output_text, packet_id): (String, String) = connection.query_row(
        "SELECT output_text,packet_id FROM discussion_runs WHERE id=? AND project_id=? AND operation_namespace=?",
        params![&run_id, access.project_id, access.operation_namespace],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let output_hash = sha256_hex(output_text.as_bytes());
    let packet = context_packets::validated_packet_record(connection, &packet_id)?;
    let (frozen, _) =
        story_context::validated_snapshot_record(connection, &packet.receipt.snapshot_id)?;
    let handles = super::materialize::allowed_target_handles(connection, &frozen)?;
    let chapter_handles = super::materialize::allowed_chapter_target_handles(connection, &frozen)?;
    let predecessor_handles = super::materialize::allowed_predecessor_handles(connection, &frozen)?;
    let output = parse_project_assistant_output_with_predecessors_and_chapters(
        &output_text,
        &handles,
        &predecessor_handles,
        &chapter_handles,
    )?;
    let materialization_json: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM conversation_items
         WHERE conversation_id=? AND project_id=? AND operation_namespace=?
           AND kind='materializeChatResult' AND reference_id=?
         ORDER BY sequence DESC LIMIT 1",
            params![
                conversation_id,
                access.project_id,
                access.operation_namespace,
                &run_id
            ],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(materialization_json) = materialization_json {
        let materialization: Value = serde_json::from_str(&materialization_json)?;
        let persisted_effects = materialization
            .get("groupEffects")
            .filter(|value| !value.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()?;
        if persisted_effects != output.group_effects {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "The retained group effects do not match the assistant response.",
            ));
        }
    } else if output.group_effects.is_some() {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "Grouped effects have no retained materialization record.",
        ));
    }

    let mut handle_heads = HashMap::new();
    for source in &frozen.snapshot.sources {
        let revision = read_revision(connection, &source.source.revision_id)?;
        if revision.head.document_id != source.source.document_id
            || revision.head.body_hash != source.source.body_hash
        {
            return Err(CoreError::new(
                "ContextChanged",
                "A grouped chat source revision changed before adoption.",
            ));
        }
        let kind: String = connection.query_row(
            "SELECT kind FROM documents WHERE id=? AND trashed=0",
            [&source.source.document_id],
            |row| row.get(0),
        )?;
        handle_heads.insert(source.handle.clone(), (revision.head, kind));
    }
    let mut draft_keys = HashMap::new();
    for (ordinal, draft) in output.drafts.iter().enumerate() {
        if let Some(document_id) = connection
            .query_row(
                "SELECT document_id FROM assistant_drafts
                 WHERE origin_run_id=? AND output_ordinal=? AND conversation_id=?",
                params![&run_id, ordinal as i64, conversation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let document =
                read_document_with_role(connection, &document_id, DocumentRole::AssistantDraft)?;
            let kind = document.kind.clone();
            let head = document.head;
            draft_keys.insert(draft.key.clone(), (document_id, head, kind));
        }
    }
    Ok(GroupOrigin {
        output_hash,
        effects: output.group_effects,
        handles: handle_heads,
        draft_keys,
    })
}

pub(crate) fn resolve_effect_ref(
    origin: &GroupOrigin,
    target_ids: &HashSet<String>,
    draft_targets: &HashMap<String, (String, Head, String)>,
    reference: &str,
) -> CoreResult<(String, Head, String)> {
    if origin.draft_keys.contains_key(reference) {
        let Some((draft_document_id, draft_head, kind)) = draft_targets.get(reference) else {
            return Err(CoreError::new(
                "InvalidRequest",
                "A grouped effect references a draft the author did not include.",
            ));
        };
        if !target_ids.contains(draft_document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A grouped effect references a draft the author did not include.",
            ));
        }
        return Ok((draft_document_id.clone(), draft_head.clone(), kind.clone()));
    }
    let Some((head, kind)) = origin.handles.get(reference) else {
        return Err(CoreError::new(
            "InvalidRequest",
            "A grouped effect references a source outside the frozen request.",
        ));
    };
    Ok((head.document_id.clone(), head.clone(), kind.clone()))
}

pub(crate) fn build_adoption_effects(
    connection: &Connection,
    targets: &[PreparedChatTarget<'_>],
    origin: &GroupOrigin,
    requested: Option<&ChatGroupEffectsOutput>,
) -> CoreResult<ChatAdoptionEffects> {
    let retained = &origin.effects;
    if let Some(requested) = requested
        && retained.as_ref() != Some(requested)
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The submitted grouped effects do not match the retained assistant response.",
        ));
    }
    if retained.as_ref().is_some_and(|effects| {
        !effects.impacts.is_empty()
            || !effects.supersessions.is_empty()
            || !effects.placements.is_empty()
    }) {
        return Err(CoreError::new(
            "InvalidRequest",
            "Grouped impacts, supersessions, and placements are review-only until their atomic Workshop materialization is available.",
        ));
    }
    let target_ids = targets
        .iter()
        .map(|(_, _, _, _, document_id, _, _, _, _)| document_id.clone())
        .collect::<HashSet<_>>();
    let mut draft_targets = HashMap::new();
    for (_, fields, draft, _, document_id, _, kind, before, _) in targets {
        for (key, (assistant_document_id, _, _)) in &origin.draft_keys {
            if assistant_document_id == &fields.document_id {
                let head = before
                    .as_ref()
                    .map(|record| record.head.clone())
                    .unwrap_or_else(|| Head {
                        document_id: document_id.clone(),
                        version: "0".into(),
                        body_hash: draft.document.head.body_hash.clone(),
                    });
                draft_targets.insert(key.clone(), (document_id.clone(), head, kind.clone()));
            }
        }
    }
    let relationship_dependencies =
        workshop_state::chat_relationship_dependencies(connection, &target_ids)?
            .into_iter()
            .map(|relationship| ChatRelationshipDependency {
                relationship_id: relationship.id,
                from_document_id: relationship.from_document_id,
                to_document_id: relationship.to_document_id,
                relationship_type: relationship.relationship_type,
                from_head: relationship.source_heads[0].clone(),
                to_head: relationship.source_heads[1].clone(),
            })
            .collect::<Vec<_>>();
    let mut protected_content = Vec::new();
    for (_, _, _, _, document_id, _, _, before, _) in targets {
        let Some(before) = before else { continue };
        for text in workshop_state::chat_protected_text(connection, document_id)? {
            protected_content.push(ChatProtectedContent {
                target_document_id: document_id.clone(),
                source_head: before.head.clone(),
                text_hash: sha256_hex(text.as_bytes()),
                text,
            });
        }
    }
    let mut proposed_relationships = Vec::new();
    let mut relationship_ids = HashMap::new();
    if let Some(effects) = retained {
        for relationship in &effects.relationships {
            let (from_document_id, from_head, from_kind) =
                resolve_effect_ref(origin, &target_ids, &draft_targets, &relationship.from_ref)?;
            let (to_document_id, to_head, to_kind) =
                resolve_effect_ref(origin, &target_ids, &draft_targets, &relationship.to_ref)?;
            if !["character", "world"].contains(&from_kind.as_str())
                || !["character", "world"].contains(&to_kind.as_str())
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat relationships may only connect character or world material.",
                ));
            }
            let relationship_id = new_id();
            relationship_ids.insert(relationship.key.clone(), relationship_id.clone());
            proposed_relationships.push(ChatAdoptionRelationship {
                key: relationship.key.clone(),
                relationship_id,
                from_document_id,
                to_document_id,
                relationship_type: relationship.relationship_type.clone(),
                description: relationship.description.clone(),
                uncertainty: relationship.uncertainty.clone(),
                from_head,
                to_head,
            });
        }
        let resolve_target =
            |reference: &str| resolve_effect_ref(origin, &target_ids, &draft_targets, reference);
        let mut impacts = Vec::new();
        for impact in &effects.impacts {
            let (target_document_id, _, _) = resolve_target(&impact.target_ref)?;
            let relationship_id = impact
                .relationship_key
                .as_ref()
                .map(|key| {
                    relationship_ids.get(key).cloned().ok_or_else(|| {
                        CoreError::new(
                            "InvalidRequest",
                            "An impact references an unknown proposed relationship.",
                        )
                    })
                })
                .transpose()?;
            impacts.push(ChatAdoptionImpact {
                target_document_id,
                kind: impact.kind.clone(),
                reason: impact.reason.clone(),
                relationship_id,
                relationship_key: impact.relationship_key.clone(),
            });
        }
        let supersessions = effects
            .supersessions
            .iter()
            .map(|supersession| {
                let (target_document_id, _, _) = resolve_target(&supersession.target_ref)?;
                let (superseded_document_id, _, _) = resolve_target(&supersession.superseded_ref)?;
                Ok(ChatAdoptionSupersession {
                    target_document_id,
                    superseded_document_id,
                    reason: supersession.reason.clone(),
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        let placements = effects
            .placements
            .iter()
            .map(|placement| {
                let (target_document_id, _, _) = resolve_target(&placement.target_ref)?;
                let before_document_id = placement
                    .before_ref
                    .as_deref()
                    .map(|reference| resolve_target(reference).map(|(id, _, _)| id))
                    .transpose()?;
                let after_document_id = placement
                    .after_ref
                    .as_deref()
                    .map(|reference| resolve_target(reference).map(|(id, _, _)| id))
                    .transpose()?;
                Ok(ChatAdoptionPlacement {
                    target_document_id,
                    before_document_id,
                    after_document_id,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(ChatAdoptionEffects {
            version: CHAT_ADOPTION_EFFECTS_VERSION.into(),
            source_output_hash: origin.output_hash.clone(),
            relationship_dependencies,
            protected_content,
            proposed_relationships,
            impacts,
            supersessions,
            placements,
        })
    } else {
        Ok(ChatAdoptionEffects {
            version: CHAT_ADOPTION_EFFECTS_VERSION.into(),
            source_output_hash: origin.output_hash.clone(),
            relationship_dependencies,
            protected_content,
            proposed_relationships,
            impacts: Vec::new(),
            supersessions: Vec::new(),
            placements: Vec::new(),
        })
    }
}

pub(crate) fn materialize_chat_relationships(
    effects: &ChatAdoptionEffects,
    documents: &[DocumentRecord],
) -> CoreResult<Vec<wns_story::workshop_vocabulary::WorkshopRelationship>> {
    let heads = documents
        .iter()
        .map(|document| (document.head.document_id.clone(), document.head.clone()))
        .collect::<HashMap<_, _>>();
    effects
        .proposed_relationships
        .iter()
        .map(|relationship| {
            let from_head = heads
                .get(&relationship.from_document_id)
                .cloned()
                .unwrap_or_else(|| relationship.from_head.clone());
            let to_head = heads
                .get(&relationship.to_document_id)
                .cloned()
                .unwrap_or_else(|| relationship.to_head.clone());
            Ok(wns_story::workshop_vocabulary::WorkshopRelationship {
                id: relationship.relationship_id.clone(),
                from_document_id: relationship.from_document_id.clone(),
                to_document_id: relationship.to_document_id.clone(),
                relationship_type: relationship.relationship_type.clone(),
                description: relationship.description.clone(),
                uncertainty: relationship.uncertainty.clone(),
                status: wns_story::workshop_vocabulary::WorkshopRelationshipStatus::Chosen,
                source_heads: vec![from_head, to_head],
            })
        })
        .collect()
}

pub(crate) fn stored_document_ref(document: DocumentRecord) -> StoredDocumentRef {
    StoredDocumentRef {
        head: document.head,
        title: document.title,
        kind: document.kind,
        metadata_version: document.metadata_version,
        last_checkpoint_id: document.last_checkpoint_id,
        role: document.role,
    }
}

pub(crate) fn operation_item(
    connection: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
) -> CoreResult<Option<ConversationItem>> {
    let row: Option<(String, i64, String, Option<String>, String, String)> = connection
        .query_row(
            "SELECT id,sequence,kind,reference_id,payload_json,created_at FROM conversation_items WHERE conversation_id=? AND project_id=? AND operation_namespace=? AND operation_id=?",
            params![conversation_id, access.project_id, access.operation_namespace, operation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    row.map(|(id, sequence, kind, reference_id, payload, created_at)| {
        Ok(ConversationItem {
            id,
            sequence: sequence.to_string(),
            kind,
            reference_id,
            payload: serde_json::from_str(&payload)?,
            created_at,
        })
    })
    .transpose()
}

pub(crate) fn current_workshop_version(connection: &Connection) -> CoreResult<String> {
    let version: Option<i64> = connection
        .query_row(
            "SELECT version FROM workshop_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    parse_stored_version(version.unwrap_or(0))
}

pub(crate) fn expand_preview(
    connection: &Connection,
    access: &ProjectAccess,
    metadata: StoredPreviewMetadata,
) -> CoreResult<ChatAdoptionPreview> {
    if metadata.project_id != access.project_id
        || metadata.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "WrongProjectSession",
            "This chat adoption preview belongs to another project identity.",
        ));
    }
    let mut targets = Vec::with_capacity(metadata.targets.len());
    for target in metadata.targets {
        let draft: ProjectChatDraftRef = serde_json::from_value(target.draft.clone())?;
        read_document_with_role(
            connection,
            &target.draft_document_id,
            DocumentRole::AssistantDraft,
        )?;
        validate_assistant_draft_ownership(
            connection,
            &target.draft_document_id,
            &metadata.conversation_id,
            &metadata.project_id,
            &metadata.operation_namespace,
        )?;
        let revision = read_revision(connection, &target.draft_revision_id)?;
        if revision.head != draft.head || revision.head.document_id != target.draft_document_id {
            return Err(CoreError::new(
                "InvalidProject",
                "A preview references another assistant draft revision.",
            ));
        }
        let body = revision.body;
        let before = target
            .before
            .map(|stored| read_historical_document_ref(connection, stored))
            .transpose()?;
        targets.push(ChatAdoptionTarget {
            draft,
            draft_revision_id: target.draft_revision_id,
            document_id: target.document_id,
            title: target.title,
            kind: target.kind,
            before,
            body,
        });
    }
    let preview = ChatAdoptionPreview {
        id: metadata.id,
        version: metadata.version,
        digest: metadata.digest,
        project_id: metadata.project_id,
        operation_namespace: metadata.operation_namespace,
        conversation_id: metadata.conversation_id,
        source_epoch: metadata.source_epoch,
        policy_epoch: metadata.policy_epoch,
        workshop_version: metadata.workshop_version,
        targets,
        effects: metadata.effects,
    };
    let expected = preview_digest(&ChatAdoptionPreview {
        digest: String::new(),
        ..preview.clone()
    })?;
    if expected != preview.digest {
        return Err(CoreError::new(
            "InvalidProject",
            "The chat adoption preview failed its digest check.",
        ));
    }
    Ok(preview)
}

pub(crate) fn expand_document_ref(
    connection: &Connection,
    reference: StoredDocumentRef,
) -> CoreResult<DocumentRecord> {
    let current = read_document(connection, &reference.head.document_id)?;
    if current.head != reference.head
        || current.title != reference.title
        || current.kind != reference.kind
        || current.metadata_version != reference.metadata_version
        || current.last_checkpoint_id != reference.last_checkpoint_id
        || current.role != reference.role
    {
        return Err(CoreError::new(
            "VersionConflict",
            "A chat adoption preview target changed before adoption.",
        ));
    }
    Ok(current)
}

pub(crate) fn read_historical_document_ref(
    connection: &Connection,
    reference: StoredDocumentRef,
) -> CoreResult<DocumentRecord> {
    let checkpoint = reference.last_checkpoint_id.clone().ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "A chat adoption receipt has no committed document revision.",
        )
    })?;
    let revision = read_revision(connection, &checkpoint)?;
    if revision.head != reference.head || revision.head.document_id != reference.head.document_id {
        return Err(CoreError::new(
            "InvalidProject",
            "A chat adoption receipt has an invalid document revision reference.",
        ));
    }
    Ok(DocumentRecord {
        head: reference.head,
        title: reference.title,
        kind: reference.kind,
        metadata_version: reference.metadata_version,
        body: revision.body,
        last_checkpoint_id: Some(checkpoint),
        role: reference.role,
    })
}
