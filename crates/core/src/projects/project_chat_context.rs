//! Project-chat context ownership and draft/source fences.
//!
//! A project conversation is a durable application projection rooted at a
//! blank control anchor.  The anchor is useful for grouping runs, but it is
//! never a story source.  This module adds the small amount of typed metadata
//! needed to keep an ordinary working snapshot, an explicitly selected
//! unadopted draft, and the conversation projection distinct.

use super::story_context::FrozenContext;
// Moved to wns-context (L2); see project_chat.rs for why. Re-exported at the
// historical path so `crate::projects::project_chat_context::{…}` resolves.
pub use wns_context::chat_vocabulary::{
    ProjectChatDraftRef, ProjectChatFreeze,
};
// The frozen half moved to `wns-context::frozen` (L3): the validators, the
// disposition projection, `augment_frozen_chat`, and `project_chat_basis_is_current`.
// It had to, because `story_context` calls into all of them and at L5 those were
// upward calls blocking `story_context` from reaching `wns-story`. What is left
// here is the dispatch that wraps `story_context::freeze_project_chat_at`.
pub use wns_context::frozen::project_chat_basis_is_current;
use super::*;
use crate::context::{Audience, BasisKind, ContextPurpose};


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

#[cfg(test)]
mod tests {
    use super::*;
    // Named at the crates that own them — the module itself no longer calls them.
    use wns_context::chat_vocabulary::FrozenProjectChat;
    use wns_context::frozen::{augment_frozen_chat, require_blank_anchor, validate_frozen_project_chat};
    use crate::context::evaluate_sources;
    use crate::context::{
        Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, EligibilityErrorCode,
        InformationPolicy, SourceDescriptor, SourceRef, StorySnapshot,
    };
    use crate::context::SourceKind;
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
