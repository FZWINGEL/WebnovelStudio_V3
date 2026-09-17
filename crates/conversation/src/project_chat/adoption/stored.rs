use super::*;

pub(crate) fn read_preview(
    host: &impl ProjectChatHost,
    access: &ProjectAccess,
    conversation_id: &str,
    preview_id: &str,
) -> CoreResult<ChatAdoptionPreview> {
    host.check_access(access)?;
    check_id(preview_id)?;
    let db = host.db()?;
    store::require_conversation(db, access, conversation_id)?;
    let item = store::read_item_by_reference(db, conversation_id, PREVIEW_KIND, preview_id)?;
    let stored: StoredPreview = serde_json::from_value(item.payload)?;
    if stored.preview.id != preview_id || stored.preview.conversation_id != conversation_id {
        return Err(CoreError::new(
            "PreviewMismatch",
            "The saved preview belongs to another review.",
        ));
    }
    // Reading an old review never refreshes its source proof or authorizes Apply.
    expand_preview(db, access, stored.preview)
}

/// The durable payload for a preview is intentionally ref-only.  `before`
/// contains metadata and its checkpoint identity, while `body_revision_id`
/// points at the immutable draft revision that supplies the proposed body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredPreview {
    pub(crate) schema_version: String,
    pub(crate) preview: StoredPreviewMetadata,
    pub(crate) request_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredPreviewMetadata {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) digest: String,
    pub(crate) project_id: String,
    pub(crate) operation_namespace: String,
    pub(crate) conversation_id: String,
    pub(crate) source_epoch: SourceEpoch,
    pub(crate) policy_epoch: String,
    pub(crate) workshop_version: String,
    pub(crate) targets: Vec<StoredPreviewTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) effects: Option<ChatAdoptionEffects>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredPreviewTarget {
    pub(crate) draft: Value,
    pub(crate) draft_revision_id: String,
    pub(crate) draft_document_id: String,
    pub(crate) disposition_version: String,
    pub(crate) document_id: String,
    pub(crate) title: String,
    pub(crate) kind: String,
    pub(crate) before: Option<StoredDocumentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredDocumentRef {
    pub(crate) head: Head,
    pub(crate) title: String,
    pub(crate) kind: String,
    pub(crate) metadata_version: String,
    pub(crate) last_checkpoint_id: Option<String>,
    pub(crate) role: DocumentRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredAdoptionReceipt {
    pub(crate) preview_id: String,
    pub(crate) documents: Vec<StoredDocumentRef>,
    pub(crate) decision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredChatAdoptionDecision {
    pub(crate) schema_version: String,
    pub(crate) preview_id: String,
    pub(crate) preview_version: String,
    pub(crate) preview_digest: String,
    pub(crate) document_ids: Vec<String>,
    pub(crate) decision_id: String,
    pub(crate) effects: Option<ChatAdoptionEffects>,
    /// Present on snapshots written after the stable command-hash binding
    /// was introduced.  Older decision records intentionally omit it and
    /// use the structural-proof compatibility path during recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) snapshot_payload_hash: Option<String>,
}
