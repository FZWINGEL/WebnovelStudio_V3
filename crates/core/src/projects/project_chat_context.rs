//! Project-chat context ownership and draft/source fences.
//!
//! A project conversation is a durable application projection rooted at a
//! blank control anchor.  The anchor is useful for grouping runs, but it is
//! never a story source.  This module adds the small amount of typed metadata
//! needed to keep an ordinary working snapshot, an explicitly selected
//! unadopted draft, and the conversation projection distinct.

use super::conversation_context;
use super::story_context::FrozenContext;
// Moved to wns-context (L2); see project_chat.rs for why. Re-exported at the
// historical path so `crate::projects::project_chat_context::{…}` resolves.
pub use wns_context::chat_vocabulary::{
    FrozenProjectChat, ProjectChatDraftRef, ProjectChatFreeze,
};
// The frozen validation half moved to `wns-context::frozen` (L3). It had to:
// `story_context` calls `validate_frozen_project_chat` from `validate_pins`, so
// at L5 it was an upward call blocking `story_context` from reaching
// `wns-story`. Re-exported here for the freeze side above it, and for
// `discussions` and `project_chat/store`.
pub use wns_context::frozen::{
    collect_project_chat_dispositions, project_chat_basis_is_current, require_blank_anchor,
};
use super::*;
use crate::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, SourceDescriptor, SourceKind,
    SourceRef, evaluate_sources,
};
use rusqlite::OptionalExtension;
use std::collections::HashSet;


/// Freeze a project-level discussion on top of the ordinary working context.
/// The blank conversation anchor is the structural target; ordinary source
/// eligibility remains owned by the shared compiler. This function only
/// authenticates the project projection and appends explicitly requested
/// pending drafts as author-room task material.
pub(crate) fn freeze_project_chat_at(
    tx: &Connection,
    request: &crate::projects::story_context::FreezeStory,
    payload_hash: &str,
    chat: &ProjectChatFreeze,
) -> CoreResult<FrozenContext> {
    if request.basis != BasisKind::Working
        || request.purpose != ContextPurpose::Discuss
        || request.policy.audience != Audience::AuthorRoom
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project chat requires a Working author-room discussion context.",
        ));
    }
    super::story_context::freeze_project_chat_at(tx, request, payload_hash, chat)
}

/// Decorate an already-built working context before its immutable snapshot is
/// persisted.  Kept separate so the normal freeze path remains byte-stable.
pub(crate) fn augment_frozen_chat(
    tx: &Connection,
    request: &crate::projects::story_context::FreezeStory,
    frozen: &mut FrozenContext,
    chat: &ProjectChatFreeze,
) -> CoreResult<()> {
    check_id(&chat.conversation_id)?;
    let (project, namespace, anchor): (String, String, String) = tx
        .query_row(
            "SELECT project_id,operation_namespace,anchor_document_id FROM project_conversations WHERE id=?",
            [&chat.conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "ProjectConversationNotFound",
                "The project conversation is not available in this project.",
            )
        })?;
    if project != request.access.project_id || namespace != request.access.operation_namespace {
        return Err(CoreError::new(
            "ProjectConversationMismatch",
            "The project conversation belongs to another project or operation namespace.",
        ));
    }
    let _anchor_document = require_blank_anchor(tx, &anchor)?;

    let mut seen_sources = HashSet::new();
    for head in &chat.source_refs {
        let document = read_document_with_role(tx, &head.document_id, DocumentRole::Ordinary)?;
        if document.head != *head {
            return Err(CoreError::new(
                "SourceChanged",
                "A project-chat source head is no longer current.",
            ));
        }
        if !seen_sources.insert(head.document_id.clone()) {
            return Err(CoreError::new(
                "DuplicateSource",
                "A project-chat source was attached more than once.",
            ));
        }
        let exists = frozen.snapshot.sources.iter().any(|source| {
            source.source.document_id == head.document_id
                && source.source.body_hash == head.body_hash
        });
        if !exists {
            return Err(CoreError::new(
                "SourceOutsideFrozenContext",
                "A project-chat source is not present in the frozen working context.",
            ));
        }
    }

    let mut seen_drafts = HashSet::new();
    for draft_ref in &chat.task_draft_refs {
        check_id(&draft_ref.head.document_id)?;
        let requested_version = parse_version(&draft_ref.disposition_version)?;
        if !seen_drafts.insert(draft_ref.head.document_id.clone()) {
            return Err(CoreError::new(
                "DuplicateDraft",
                "A project-chat draft was attached more than once.",
            ));
        }
        let row: Option<(String, String, String, i64)> = tx
            .query_row(
                "SELECT project_id,operation_namespace,disposition,disposition_version FROM assistant_drafts WHERE document_id=?",
                [&draft_ref.head.document_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((draft_project, draft_namespace, disposition, stored_version)) = row else {
            return Err(CoreError::new(
                "DraftNotFound",
                "The selected project-chat draft is not available.",
            ));
        };
        if draft_project != request.access.project_id
            || draft_namespace != request.access.operation_namespace
            || disposition != "pending"
            || stored_version != requested_version
        {
            return Err(CoreError::new(
                "DraftChanged",
                "The selected draft is no longer pending at the requested disposition version.",
            ));
        }
        let draft = read_document_with_role(
            tx,
            &draft_ref.head.document_id,
            DocumentRole::AssistantDraft,
        )?;
        if draft.head != draft_ref.head {
            return Err(CoreError::new(
                "DraftChanged",
                "The selected project-chat draft head is no longer current.",
            ));
        }
        let revision = checkpoint_at(tx, &draft, "projectChatContext")?;
        let source = SourceRef {
            project_id: request.access.project_id.clone(),
            document_id: revision.head.document_id.clone(),
            revision_id: revision.id.clone(),
            body_hash: revision.head.body_hash.clone(),
        };
        let descriptor = SourceDescriptor {
            handle: revision.id,
            source,
            display_name: draft.title,
            kind: SourceKind::AssistantDraft,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: None,
                visible_to_characters: Vec::new(),
                author_only: true,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        };
        if frozen
            .snapshot
            .sources
            .iter()
            .any(|source| source.source == descriptor.source)
        {
            return Err(CoreError::new(
                "DuplicateSource",
                "A project-chat draft duplicates an existing frozen source.",
            ));
        }
        frozen.snapshot.sources.push(descriptor);
    }

    frozen.project_chat = Some(FrozenProjectChat {
        conversation_id: chat.conversation_id.clone(),
        anchor_document_id: anchor.clone(),
        operation_namespace: namespace.clone(),
        source_refs: chat.source_refs.clone(),
        task_draft_refs: chat.task_draft_refs.clone(),
        prompt_recipe_version: chat.prompt_recipe_version.clone(),
        dispositions: Vec::new(),
    });
    if frozen.snapshot.target.document_id != anchor
        || !frozen.snapshot.sources.iter().any(|source| {
            source.source == frozen.snapshot.target
                && source.kind == SourceKind::ConversationControl
        })
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project chat must freeze its blank conversation anchor as the structural target.",
        ));
    }
    frozen.conversation = conversation_context::select_project_conversation_at(
        tx,
        &request.access,
        &chat.conversation_id,
        &anchor,
        &request.policy.version,
    )?;
    let dispositions = collect_project_chat_dispositions(tx, frozen)?;
    frozen
        .project_chat
        .as_mut()
        .expect("project-chat metadata was just installed")
        .dispositions = dispositions;

    let handles: Vec<String> = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| source.handle.clone())
        .collect();
    evaluate_sources(&frozen.snapshot, &frozen.policy, frozen.purpose, &handles)
        .map_err(|error| CoreError::new("ContextSourceDisallowed", &error.to_string()))?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    // Named at the crate that owns it — the module itself no longer calls it.
    use wns_context::frozen::validate_frozen_project_chat;
    use crate::context::{
        Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, EligibilityErrorCode,
        InformationPolicy, StorySnapshot,
    };
    use rusqlite::Connection;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn source(document_id: &str, revision_id: &str, body_hash: &str) -> SourceRef {
        SourceRef {
            project_id: "project".into(),
            document_id: document_id.into(),
            revision_id: revision_id.into(),
            body_hash: body_hash.into(),
        }
    }

    fn descriptor(source: SourceRef, kind: SourceKind, author_only: bool) -> SourceDescriptor {
        SourceDescriptor {
            handle: source.revision_id.clone(),
            source,
            display_name: "fixture".into(),
            kind,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: Some("1".into()),
                visible_to_characters: Vec::new(),
                author_only,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        }
    }

    fn policy(audience: Audience) -> InformationPolicy {
        InformationPolicy {
            version: "0".into(),
            audience,
            reader_frontier: (audience == Audience::RestrictedWriting).then(|| "1".into()),
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        }
    }

    fn snapshot(target: SourceRef, sources: Vec<SourceDescriptor>) -> StorySnapshot {
        StorySnapshot {
            snapshot_id: "snapshot".into(),
            project_id: "project".into(),
            basis: BasisKind::Working,
            target,
            context_source_epoch: "0".into(),
            ordering_epoch: "0".into(),
            disclosure_policy_version: "0".into(),
            sources,
            reviewed_basis: None,
        }
    }

    fn frozen(
        snapshot: StorySnapshot,
        purpose: ContextPurpose,
        policy: InformationPolicy,
    ) -> FrozenContext {
        FrozenContext {
            snapshot,
            policy,
            purpose,
            aliases: BTreeMap::new(),
            excluded_source_count: 0,
            guidance: Vec::new(),
            conversation: None,
            navigation_views: Vec::new(),
            reviewed_evidence: Vec::new(),
            reviewed_promises: Vec::new(),
            reviewed_knowledge: Vec::new(),
            reviewed_summaries: Vec::new(),
            project_chat: None,
        }
    }

    fn valid_body(text: Option<&str>) -> (String, String) {
        let body = match text {
            Some(text) => json!({
                "schemaVersion": 1,
                "body": {"type": "doc", "content": [{
                    "type": "paragraph", "attrs": {"id": "p"},
                    "content": [{"type": "text", "text": text}]
                }]}
            }),
            None => json!({
                "schemaVersion": 1,
                "body": {"type": "doc", "content": [{
                    "type": "paragraph", "attrs": {"id": "p"}
                }]}
            }),
        };
        let validated =
            crate::validate_snapshot_json(&body.to_string()).expect("valid fixture body");
        (validated.canonical_json, validated.hash)
    }

    fn role_fixture() -> Connection {
        let db = Connection::open_in_memory().expect("open fixture db");
        db.execute_batch(
            "CREATE TABLE documents(
                id TEXT PRIMARY KEY, title TEXT NOT NULL, kind TEXT NOT NULL,
                working_version INTEGER NOT NULL, metadata_version INTEGER NOT NULL,
                body_hash TEXT NOT NULL, body_json TEXT NOT NULL,
                last_checkpoint_id TEXT, role TEXT NOT NULL,
                trashed INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE project_conversations(
                id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
                operation_namespace TEXT NOT NULL, anchor_document_id TEXT NOT NULL
            );
            CREATE TABLE assistant_drafts(
                document_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL,
                project_id TEXT NOT NULL, operation_namespace TEXT NOT NULL,
                origin_run_id TEXT NOT NULL, output_ordinal INTEGER NOT NULL,
                packet_id TEXT NOT NULL, source_epoch INTEGER NOT NULL,
                policy_epoch INTEGER NOT NULL, target_json TEXT,
                initial_revision_id TEXT NOT NULL, predecessor_document_id TEXT,
                disposition TEXT NOT NULL, disposition_version INTEGER NOT NULL
            );",
        )
        .expect("create fixture tables");
        db
    }

    fn insert_document(
        db: &Connection,
        id: &str,
        kind: &str,
        role: DocumentRole,
        version: i64,
        body: &(String, String),
    ) {
        db.execute(
            "INSERT INTO documents(id,title,kind,working_version,metadata_version,body_hash,body_json,role)
             VALUES(?,?,?,?,?,?,?,?)",
            rusqlite::params![
                id,
                id,
                kind,
                version,
                "1",
                body.1,
                body.0,
                role.storage_name()
            ],
        )
        .expect("insert fixture document");
    }

    #[test]
    fn restricted_policy_rejects_explicit_assistant_draft_source() {
        let draft = source("draft", "draft-revision", &"a".repeat(64));
        let story = snapshot(
            draft.clone(),
            vec![descriptor(draft, SourceKind::AssistantDraft, true)],
        );
        let error = evaluate_sources(
            &story,
            &policy(Audience::RestrictedWriting),
            ContextPurpose::Revise,
            &["draft-revision".into()],
        )
        .expect_err("unadopted draft must not enter restricted writing");
        assert_eq!(error.code, EligibilityErrorCode::PrivateSource);
    }

    #[test]
    fn forged_control_source_without_project_chat_metadata_is_rejected() {
        let anchor = source("anchor", "anchor-revision", &"b".repeat(64));
        let context = frozen(
            snapshot(
                anchor.clone(),
                vec![descriptor(anchor, SourceKind::ConversationControl, true)],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        let json = serde_json::to_string(&context).expect("serialize context");
        let error = super::super::story_context::decode_snapshot(
            &json,
            &crate::sha256_hex(json.as_bytes()),
        )
        .expect_err("control metadata cannot be forged by an ordinary packet");
        assert_eq!(error.code, "InvalidProjectChatContext");
    }

    #[test]
    fn conversation_anchor_must_remain_a_blank_note() {
        let db = role_fixture();
        let nonblank = valid_body(Some("edited anchor"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &nonblank,
        );
        let error = require_blank_anchor(&db, "anchor")
            .expect_err("a nonblank control anchor must be refused");
        assert_eq!(error.code, "InvalidProjectChat");
    }

    #[test]
    fn edited_ordinary_source_head_is_rejected_before_project_chat_freeze() {
        let db = role_fixture();
        let anchor_body = valid_body(None);
        let current_body = valid_body(Some("new source"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &anchor_body,
        );
        insert_document(
            &db,
            "source",
            "world",
            DocumentRole::Ordinary,
            2,
            &current_body,
        );
        db.execute(
            "INSERT INTO project_conversations(id,project_id,operation_namespace,anchor_document_id)
             VALUES('conversation','project','namespace','anchor')",
            [],
        )
        .expect("insert conversation");
        let old_head = Head {
            document_id: "source".into(),
            version: "1".into(),
            body_hash: "a".repeat(64),
        };
        let anchor_ref = source("anchor", "anchor-revision", &anchor_body.1);
        let source_ref = source("source", "source-revision-v1", &old_head.body_hash);
        let mut context = frozen(
            snapshot(
                anchor_ref,
                vec![
                    descriptor(source_ref, SourceKind::CurrentDraft, false),
                    descriptor(
                        source("anchor", "anchor-revision", &anchor_body.1),
                        SourceKind::ConversationControl,
                        true,
                    ),
                ],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        let request = crate::projects::story_context::FreezeStory {
            access: ProjectAccess {
                project_id: "project".into(),
                session: "session".into(),
                writer_lease: "lease".into(),
                operation_namespace: "namespace".into(),
            },
            operation_id: "operation".into(),
            expected: Head {
                document_id: "anchor".into(),
                version: "0".into(),
                body_hash: anchor_body.1.clone(),
            },
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: policy(Audience::AuthorRoom),
        };
        let chat = ProjectChatFreeze {
            conversation_id: "conversation".into(),
            source_refs: vec![old_head],
            task_draft_refs: Vec::new(),
            prompt_recipe_version: None,
        };
        let error = augment_frozen_chat(&db, &request, &mut context, &chat)
            .expect_err("an edited source must invalidate a new chat request");
        assert_eq!(error.code, "SourceChanged");
    }

    #[test]
    fn frozen_assistant_draft_requires_the_isolated_document_role() {
        let db = role_fixture();
        let anchor_body = valid_body(None);
        let draft_body = valid_body(Some("candidate"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &anchor_body,
        );
        // Deliberately install the candidate as an ordinary document. A
        // forged assistant_drafts row must not upgrade that row's authority.
        insert_document(
            &db,
            "draft",
            "world",
            DocumentRole::Ordinary,
            1,
            &draft_body,
        );
        db.execute(
            "INSERT INTO project_conversations(id,project_id,operation_namespace,anchor_document_id)
             VALUES('conversation','project','namespace','anchor')",
            [],
        )
        .expect("insert conversation");
        db.execute(
            "INSERT INTO assistant_drafts(
                document_id,conversation_id,project_id,operation_namespace,
                origin_run_id,output_ordinal,packet_id,source_epoch,policy_epoch,
                target_json,initial_revision_id,predecessor_document_id,disposition,disposition_version
             ) VALUES('draft','conversation','project','namespace','run',0,'packet',0,0,NULL,'revision',NULL,'pending',0)",
            [],
        )
        .expect("insert draft provenance");
        let anchor_ref = source("anchor", "anchor-revision", &anchor_body.1);
        let draft_ref = source("draft", "draft-revision", &draft_body.1);
        let mut context = frozen(
            snapshot(
                anchor_ref.clone(),
                vec![
                    descriptor(anchor_ref, SourceKind::ConversationControl, true),
                    descriptor(draft_ref.clone(), SourceKind::AssistantDraft, true),
                ],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        context.project_chat = Some(FrozenProjectChat {
            conversation_id: "conversation".into(),
            anchor_document_id: "anchor".into(),
            operation_namespace: "namespace".into(),
            source_refs: Vec::new(),
            task_draft_refs: vec![ProjectChatDraftRef {
                head: Head {
                    document_id: "draft".into(),
                    version: "1".into(),
                    body_hash: draft_body.1,
                },
                disposition_version: "0".into(),
            }],
            prompt_recipe_version: None,
            dispositions: Vec::new(),
        });
        let error = validate_frozen_project_chat(&db, &context, "namespace")
            .expect_err("ordinary rows cannot masquerade as assistant drafts");
        assert_eq!(error.code, "InvalidProjectChatContext");
    }
}
