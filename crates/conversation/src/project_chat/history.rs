//! Authenticated, read-only access to retained project-chat conversations.
//!
//! A recovered or duplicated project keeps the original conversation rows as
//! immutable evidence under their original project identity.  The active
//! `ProjectAccess` remains the authority for opening the database; callers
//! must additionally name the historical conversation identity they want to
//! inspect.  This module deliberately exposes no composer, draft, adoption,
//! or generation operation against that identity.

use super::*;
use crate::discussions::{DiscussionMessage, DiscussionMessageRole, DiscussionRun, FeedbackIntent};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use wns_context::{SourceDescriptor, SourceKind};
use wns_kernel::Revision;

const HISTORY_PAGE_SIZE: u32 = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalConversationRef {
    pub project_id: String,
    pub operation_namespace: String,
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadProjectChatHistory {
    pub access: ProjectAccess,
    pub conversation: HistoricalConversationRef,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default = "history_page_size")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalConversationSummary {
    pub conversation: HistoricalConversationRef,
    pub anchor_document_id: String,
    pub item_count: u32,
    pub current: bool,
}

fn history_page_size() -> u32 {
    HISTORY_PAGE_SIZE
}

pub(super) fn list(
    db: &Connection,
    access: &ProjectAccess,
) -> CoreResult<Vec<HistoricalConversationSummary>> {
    let mut query = db.prepare(
        "SELECT id,project_id,operation_namespace,anchor_document_id,
                (SELECT COUNT(*) FROM conversation_items i
                 WHERE i.conversation_id=c.id
                   AND i.project_id=c.project_id
                   AND i.operation_namespace=c.operation_namespace)
         FROM project_conversations c
         ORDER BY (project_id=? AND operation_namespace=?) DESC, created_at ASC, id ASC",
    )?;
    let rows = query.query_map(
        params![access.project_id, access.operation_namespace],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let mut summaries = Vec::new();
    for row in rows {
        let (conversation_id, project_id, operation_namespace, anchor_document_id, item_count) =
            row?;
        let reference = HistoricalConversationRef {
            project_id,
            operation_namespace,
            conversation_id,
        };
        valid_identity(&reference)?;
        if item_count < 0 || item_count > u32::MAX as i64 {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A retained conversation has an invalid item count.",
            ));
        }
        read_document_with_role(db, &anchor_document_id, DocumentRole::ConversationAnchor)?;
        summaries.push(HistoricalConversationSummary {
            current: reference.project_id == access.project_id
                && reference.operation_namespace == access.operation_namespace,
            conversation: reference,
            anchor_document_id,
            item_count: item_count as u32,
        });
    }
    Ok(summaries)
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalSourceRevision {
    pub handle: String,
    pub kind: SourceKind,
    pub descriptor: SourceDescriptor,
    pub revision: Revision,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalDraftRevision {
    pub document_id: String,
    pub initial: bool,
    pub revision: Revision,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalConversationItem {
    pub item: ConversationItem,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<DiscussionRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<DiscussionMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_revisions: Vec<HistoricalSourceRevision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft_revisions: Vec<HistoricalDraftRevision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalConversation {
    pub conversation: HistoricalConversationRef,
    pub anchor_document_id: String,
    pub items: Vec<HistoricalConversationItem>,
    pub older_before: Option<String>,
}

struct HydratedRun {
    run: DiscussionRun,
    messages: Vec<DiscussionMessage>,
    source_revisions: Vec<HistoricalSourceRevision>,
    draft_revisions: Vec<HistoricalDraftRevision>,
}

fn valid_identity(reference: &HistoricalConversationRef) -> CoreResult<()> {
    for (value, label) in [
        (&reference.project_id, "historical project"),
        (
            &reference.operation_namespace,
            "historical operation namespace",
        ),
        (&reference.conversation_id, "historical conversation"),
    ] {
        check_id(value).map_err(|_| {
            CoreError::new(
                "InvalidRequest",
                &format!("The {label} identity is invalid."),
            )
        })?;
    }
    Ok(())
}

pub(super) fn read(
    db: &Connection,
    request: ReadProjectChatHistory,
) -> CoreResult<HistoricalConversation> {
    valid_identity(&request.conversation)?;
    let limit = if request.limit == 0 {
        HISTORY_PAGE_SIZE
    } else {
        request.limit
    };
    if limit > HISTORY_PAGE_SIZE {
        return Err(CoreError::new(
            "InvalidRequest",
            "Historical conversation pages may contain at most 40 items.",
        ));
    }
    let before = request
        .before
        .as_deref()
        .map(parse_version)
        .transpose()?
        .unwrap_or(i64::MAX);
    let anchor: Option<String> = db
        .query_row(
            "SELECT anchor_document_id FROM project_conversations
             WHERE id=? AND project_id=? AND operation_namespace=?",
            params![
                request.conversation.conversation_id,
                request.conversation.project_id,
                request.conversation.operation_namespace
            ],
            |row| row.get(0),
        )
        .optional()?;
    let Some(anchor_document_id) = anchor else {
        return Err(CoreError::new(
            "HistoricalConversationNotFound",
            "The requested historical conversation identity is not retained in this project.",
        ));
    };
    let anchor_record =
        read_document_with_role(db, &anchor_document_id, DocumentRole::ConversationAnchor)?;
    let rows = {
        let mut query = db.prepare(
            "SELECT id,sequence,kind,reference_id,payload_json,payload_hash,created_at
             FROM conversation_items
             WHERE conversation_id=? AND project_id=? AND operation_namespace=? AND sequence<?
             ORDER BY sequence DESC,id DESC LIMIT ?",
        )?;
        query
            .query_map(
                params![
                    request.conversation.conversation_id,
                    request.conversation.project_id,
                    request.conversation.operation_namespace,
                    before,
                    i64::from(limit) + 1
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?
    };
    let has_older = rows.len() > limit as usize;
    let rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
    let older_before = has_older
        .then(|| rows.last().map(|row| row.1.to_string()))
        .flatten();
    let mut items = Vec::with_capacity(rows.len());
    for (item_id, sequence, kind, reference_id, payload_json, payload_hash, created_at) in
        rows.into_iter().rev()
    {
        if sequence <= 0
            || !valid_hash(&payload_hash)
            || wns_kernel::sha256_hex(payload_json.as_bytes()) != payload_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A historical conversation item failed its immutable identity check.",
            ));
        }
        let item = ConversationItem {
            id: item_id,
            sequence: sequence.to_string(),
            kind: kind.clone(),
            reference_id: reference_id.clone(),
            payload: serde_json::from_str(&payload_json)?,
            created_at,
        };
        let (run, messages, source_revisions, draft_revisions) = if is_run_item(&kind) {
            let run_id = reference_id.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidProjectChat",
                    "A historical request item has no run reference.",
                )
            })?;
            let hydrated = hydrate_run(
                db,
                &request.conversation,
                run_id,
                &kind,
                &anchor_record.head.document_id,
            )?;
            if kind == "chapterRequest" && !matches!(hydrated.run.intent, FeedbackIntent::Discuss) {
                // Restricted prose work is intentionally not part of the
                // project conversation history. It remains available through
                // its chapter run and immutable snapshot authorities.
                continue;
            }
            (
                Some(hydrated.run),
                hydrated.messages,
                hydrated.source_revisions,
                hydrated.draft_revisions,
            )
        } else if kind == "saveAssistantDraft" {
            let document_id = reference_id.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidProjectChat",
                    "A historical draft event has no draft reference.",
                )
            })?;
            (
                None,
                Vec::new(),
                Vec::new(),
                hydrate_draft_revisions(db, &request.conversation, None, Some(document_id))?,
            )
        } else {
            (None, Vec::new(), Vec::new(), Vec::new())
        };
        items.push(HistoricalConversationItem {
            item,
            run,
            messages,
            source_revisions,
            draft_revisions,
        });
    }
    Ok(HistoricalConversation {
        conversation: request.conversation,
        anchor_document_id: anchor_record.head.document_id,
        items,
        older_before,
    })
}

fn is_run_item(kind: &str) -> bool {
    matches!(kind, "request" | "chapterRequest" | "materializeChatResult")
}

fn hydrate_run(
    db: &Connection,
    reference: &HistoricalConversationRef,
    run_id: &str,
    item_kind: &str,
    anchor_document_id: &str,
) -> CoreResult<HydratedRun> {
    check_id(run_id)?;
    let run = discussions::read_run(db, run_id)?;
    if run.owner.project_id != reference.project_id
        || run.owner.operation_namespace != reference.operation_namespace
    {
        return Err(CoreError::new(
            "HistoricalConversationMismatch",
            "The retained run belongs to another project identity.",
        ));
    }
    let thread: Option<(String, String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace,document_id
             FROM discussion_threads WHERE id=?",
            [&run.thread_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((thread_project, thread_namespace, thread_document)) = thread else {
        return Err(CoreError::new(
            "HistoricalConversationMismatch",
            "The retained run has no discussion thread.",
        ));
    };
    let expected_thread_document = if item_kind == "chapterRequest" {
        &run.target.document_id
    } else {
        anchor_document_id
    };
    if thread_project != reference.project_id
        || thread_namespace != reference.operation_namespace
        || thread_document != expected_thread_document
    {
        return Err(CoreError::new(
            "HistoricalConversationMismatch",
            "The retained run is not rooted in the requested conversation.",
        ));
    }
    let messages = read_messages(db, run_id, &run.thread_id)?;
    let source_revisions = read_source_revisions(db, reference, &run.packet_id)?;
    let draft_revisions = hydrate_draft_revisions(db, reference, Some(run_id), None)?;
    Ok(HydratedRun {
        run,
        messages,
        source_revisions,
        draft_revisions,
    })
}

fn read_messages(
    db: &Connection,
    run_id: &str,
    expected_thread_id: &str,
) -> CoreResult<Vec<DiscussionMessage>> {
    let mut query = db.prepare(
        "SELECT id,thread_id,run_id,role,content,scope_json,packet_id,created_at
         FROM discussion_messages WHERE run_id=? ORDER BY rowid ASC",
    )?;
    let rows = query.query_map([run_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let mut messages = Vec::new();
    for row in rows {
        let (id, thread_id, stored_run_id, role, content, scope_json, packet_id, created_at) = row?;
        let role = match role.as_str() {
            "user" => DiscussionMessageRole::User,
            "assistant" => DiscussionMessageRole::Assistant,
            _ => {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "A retained conversation message has an unknown role.",
                ));
            }
        };
        if stored_run_id.as_deref() != Some(run_id) {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A retained conversation message has the wrong run reference.",
            ));
        }
        if thread_id != expected_thread_id {
            return Err(CoreError::new(
                "HistoricalConversationMismatch",
                "A retained conversation message belongs to another discussion thread.",
            ));
        }
        messages.push(DiscussionMessage {
            id,
            thread_id,
            run_id: stored_run_id,
            role,
            content,
            scope: scope_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            packet_id,
            created_at,
        });
    }
    Ok(messages)
}

fn read_source_revisions(
    db: &Connection,
    reference: &HistoricalConversationRef,
    packet_id: &str,
) -> CoreResult<Vec<HistoricalSourceRevision>> {
    let row: Option<(String, String, String, String)> = db
        .query_row(
            "SELECT p.project_id,p.operation_namespace,s.manifest_json,s.manifest_hash
             FROM context_packets p JOIN story_snapshots s ON s.id=p.snapshot_id
             WHERE p.id=? AND p.project_id=? AND p.operation_namespace=?
               AND s.project_id=? AND s.operation_namespace=?",
            params![
                packet_id,
                reference.project_id,
                reference.operation_namespace,
                reference.project_id,
                reference.operation_namespace
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((_project, _namespace, manifest, manifest_hash)) = row else {
        return Err(CoreError::new(
            "HistoricalConversationMismatch",
            "The retained run packet is outside the requested project identity.",
        ));
    };
    let frozen = wns_story::story_context::decode_snapshot(&manifest, &manifest_hash)?;
    let mut seen = HashSet::new();
    let mut revisions = Vec::with_capacity(frozen.snapshot.sources.len());
    for descriptor in frozen.snapshot.sources {
        if !seen.insert(descriptor.source.revision_id.clone()) {
            continue;
        }
        let revision = read_revision(db, &descriptor.source.revision_id)?;
        if revision.head.document_id != descriptor.source.document_id
            || revision.head.body_hash != descriptor.source.body_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A historical source revision no longer matches its frozen source.",
            ));
        }
        revisions.push(HistoricalSourceRevision {
            handle: descriptor.handle.clone(),
            kind: descriptor.kind,
            descriptor,
            revision,
        });
    }
    Ok(revisions)
}

fn hydrate_draft_revisions(
    db: &Connection,
    reference: &HistoricalConversationRef,
    origin_run_id: Option<&str>,
    document_id: Option<&str>,
) -> CoreResult<Vec<HistoricalDraftRevision>> {
    let mut query = db.prepare(
        "SELECT a.document_id,a.initial_revision_id,d.last_checkpoint_id
         FROM assistant_drafts a JOIN documents d ON d.id=a.document_id
         WHERE a.conversation_id=? AND a.project_id=? AND a.operation_namespace=?
           AND ((? IS NOT NULL AND a.origin_run_id=?) OR (? IS NOT NULL AND a.document_id=?))
         ORDER BY a.output_ordinal,a.document_id",
    )?;
    let rows = query.query_map(
        params![
            reference.conversation_id,
            reference.project_id,
            reference.operation_namespace,
            origin_run_id,
            origin_run_id,
            document_id,
            document_id
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )?;
    let mut revisions = Vec::new();
    for row in rows {
        let (document_id, initial_id, current_id) = row?;
        let initial = read_revision(db, &initial_id)?;
        if initial.head.document_id != document_id {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A historical draft revision has the wrong document identity.",
            ));
        }
        revisions.push(HistoricalDraftRevision {
            document_id: document_id.clone(),
            initial: true,
            revision: initial,
        });
        if let Some(current_id) = current_id.filter(|id| id != &initial_id) {
            let current = read_revision(db, &current_id)?;
            if current.head.document_id != document_id {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "A current historical draft revision has the wrong document identity.",
                ));
            }
            revisions.push(HistoricalDraftRevision {
                document_id,
                initial: false,
                revision: current,
            });
        }
    }
    Ok(revisions)
}
