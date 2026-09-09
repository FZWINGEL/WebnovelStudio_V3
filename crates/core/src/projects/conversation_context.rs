//! Bounded recent-turn selection over retained discussion records. No provider
//! call, summary, or extra text authority is involved.
use super::*;
use crate::context::conversation::{
    ConversationMessage, ConversationTurn, FrozenConversation, MAX_CONTEXT_BYTES, MAX_CONTEXT_TURNS,
};
use crate::context::{Audience, ContextPurpose};
use crate::documents::{ScopeValidationRequest, validate_scope};

pub(super) fn select_conversation_at(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
    policy: &str,
) -> CoreResult<Option<FrozenConversation>> {
    let thread: Option<String> = db.query_row(
        "SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?",
        params![access.project_id, access.operation_namespace, document_id], |row| row.get(0),
    ).optional()?;
    let Some(thread_id) = thread else {
        return Ok(None);
    };
    let total: i64 = db.query_row(
        "SELECT COUNT(*) FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id
         JOIN story_snapshots s ON s.id=p.snapshot_id
         WHERE r.thread_id=? AND r.status='completed' AND r.dispatch_state='delivered'
           AND s.disclosure_policy_epoch=?
           AND json_extract(s.manifest_json,'$.policy.audience')='authorRoom'
           AND json_extract(s.manifest_json,'$.purpose')='discuss'",
        params![thread_id, parse_version(policy)?],
        |row| row.get(0),
    )?;
    if total == 0 {
        return Ok(None);
    }
    let mut query = db.prepare(
        "SELECT r.id,COALESCE((SELECT SUM(length(CAST(m.content AS BLOB))+COALESCE(length(CAST(m.scope_json AS BLOB)),0)) FROM discussion_messages m WHERE m.run_id=r.id),0)
         FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id
         JOIN story_snapshots s ON s.id=p.snapshot_id
         WHERE r.thread_id=? AND r.status='completed' AND r.dispatch_state='delivered'
           AND s.disclosure_policy_epoch=?
           AND json_extract(s.manifest_json,'$.policy.audience')='authorRoom'
           AND json_extract(s.manifest_json,'$.purpose')='discuss'
         ORDER BY r.created_at DESC,r.rowid DESC LIMIT ?",
    )?;
    let rows = query
        .query_map(
            params![thread_id, parse_version(policy)?, MAX_CONTEXT_TURNS as i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut turns = Vec::new();
    let mut used = 0usize;
    for (run_id, raw_bytes) in rows {
        if raw_bytes < 0 || raw_bytes as usize > MAX_CONTEXT_BYTES.saturating_sub(used) {
            break;
        }
        let turn = read_turn(
            db,
            &run_id,
            &access.project_id,
            &access.operation_namespace,
            document_id,
            &thread_id,
            None,
        )?;
        // A whitespace-only completed reply carries no conversational answer.
        // Retain it in history without blocking future discussion requests.
        if turn.assistant.content.trim().is_empty() {
            continue;
        }
        let size = serde_json::to_vec(&turn)?.len();
        if size > MAX_CONTEXT_BYTES.saturating_sub(used) {
            break;
        }
        used += size;
        turns.push(turn);
    }
    let omitted_turns = u32::try_from(total.saturating_sub(turns.len() as i64))
        .map_err(|_| invalid("The discussion count exceeds the supported range."))?;
    Ok(Some(FrozenConversation {
        project_id: access.project_id.clone(),
        operation_namespace: access.operation_namespace.clone(),
        document_id: document_id.to_owned(),
        project_conversation_id: None,
        thread_id,
        turns,
        omitted_turns,
    }))
}

/// Select the bounded history projection for a project-level conversation.
/// Membership comes from immutable conversation items rather than from a
/// document thread, so runs concerning different ordinary documents can share
/// one author-room history. A producing run with a rejected or superseded
/// assistant draft is omitted as a whole; raw candidate JSON is never
/// reintroduced by the history selector.
pub(super) fn select_project_conversation_at(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    anchor_document_id: &str,
    policy: &str,
) -> CoreResult<Option<FrozenConversation>> {
    let row: Option<(String, String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace,anchor_document_id FROM project_conversations WHERE id=?",
            [conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((project, namespace, anchor)) = row else {
        return Err(invalid("The project conversation is not available."));
    };
    if project != access.project_id
        || namespace != access.operation_namespace
        || anchor != anchor_document_id
    {
        return Err(invalid(
            "The project conversation belongs to another project or anchor.",
        ));
    }
    let eligible = "EXISTS(
            SELECT 1 FROM conversation_items ci
            WHERE ci.conversation_id=? AND ci.project_id=?
              AND ci.operation_namespace=? AND ci.reference_id=r.id
        ) AND NOT EXISTS(
            SELECT 1 FROM assistant_drafts ad
            WHERE ad.conversation_id=? AND ad.origin_run_id=r.id
              AND ad.disposition IN ('rejected','superseded')
        ) AND NOT EXISTS(
            SELECT 1 FROM conversation_items cd
            WHERE cd.conversation_id=? AND cd.project_id=?
              AND cd.operation_namespace=? AND cd.kind='chatDisposition'
              AND substr(cd.reference_id,1,length(r.id)+1)=r.id || ':'
              AND json_extract(cd.payload_json,'$.disposition')='assumptionReject'
              AND NOT EXISTS(
                  SELECT 1 FROM conversation_items newer
                  WHERE newer.conversation_id=cd.conversation_id
                    AND newer.project_id=cd.project_id
                    AND newer.operation_namespace=cd.operation_namespace
                    AND newer.kind='chatDisposition'
                    AND newer.reference_id=cd.reference_id
                    AND newer.sequence>cd.sequence
              )
        )";
    let total: i64 = db.query_row(
        &format!(
            "SELECT COUNT(*) FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id
             JOIN story_snapshots s ON s.id=p.snapshot_id
             WHERE {eligible} AND r.project_id=? AND r.operation_namespace=?
               AND r.status='completed' AND r.dispatch_state='delivered'
               AND s.disclosure_policy_epoch=?
               AND json_extract(s.manifest_json,'$.policy.audience')='authorRoom'
               AND json_extract(s.manifest_json,'$.purpose')='discuss'"
        ),
        params![
            conversation_id,
            access.project_id,
            access.operation_namespace,
            conversation_id,
            conversation_id,
            access.project_id,
            access.operation_namespace,
            access.project_id,
            access.operation_namespace,
            parse_version(policy)?
        ],
        |r| r.get(0),
    )?;
    if total == 0 {
        return Ok(None);
    }
    let query = format!(
        "SELECT r.id,COALESCE((SELECT SUM(length(CAST(m.content AS BLOB))+COALESCE(length(CAST(m.scope_json AS BLOB)),0)) FROM discussion_messages m WHERE m.run_id=r.id),0)
         FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id
         JOIN story_snapshots s ON s.id=p.snapshot_id
         WHERE {eligible} AND r.project_id=? AND r.operation_namespace=?
           AND r.status='completed' AND r.dispatch_state='delivered'
           AND s.disclosure_policy_epoch=?
           AND json_extract(s.manifest_json,'$.policy.audience')='authorRoom'
           AND json_extract(s.manifest_json,'$.purpose')='discuss'
         ORDER BY r.created_at DESC,r.rowid DESC LIMIT ?"
    );
    let mut statement = db.prepare(&query)?;
    let rows = statement
        .query_map(
            params![
                conversation_id,
                access.project_id,
                access.operation_namespace,
                conversation_id,
                conversation_id,
                access.project_id,
                access.operation_namespace,
                access.project_id,
                access.operation_namespace,
                parse_version(policy)?,
                MAX_CONTEXT_TURNS as i64
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut turns = Vec::new();
    let mut used = 0usize;
    let thread_id = db
        .query_row(
            "SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?",
            params![access.project_id, access.operation_namespace, anchor_document_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| conversation_id.to_owned());
    for (run_id, raw_bytes) in rows {
        if raw_bytes < 0 || raw_bytes as usize > MAX_CONTEXT_BYTES.saturating_sub(used) {
            break;
        }
        let turn = read_turn(
            db,
            &run_id,
            &access.project_id,
            &access.operation_namespace,
            anchor_document_id,
            &thread_id,
            Some(conversation_id),
        )?;
        if turn.assistant.content.trim().is_empty() {
            continue;
        }
        let size = serde_json::to_vec(&turn)?.len();
        if size > MAX_CONTEXT_BYTES.saturating_sub(used) {
            break;
        }
        used += size;
        turns.push(turn);
    }
    let omitted_turns = u32::try_from(total.saturating_sub(turns.len() as i64))
        .map_err(|_| invalid("The project conversation exceeds the supported history range."))?;
    Ok(Some(FrozenConversation {
        project_id: access.project_id.clone(),
        operation_namespace: access.operation_namespace.clone(),
        document_id: anchor_document_id.to_owned(),
        project_conversation_id: Some(conversation_id.to_owned()),
        thread_id,
        turns,
        omitted_turns,
    }))
}

pub(super) fn validate_conversation_at(
    db: &Connection,
    frozen: &story_context::FrozenContext,
) -> CoreResult<()> {
    let Some(conversation) = &frozen.conversation else {
        return Ok(());
    };
    let namespace: String = db.query_row(
        "SELECT operation_namespace FROM story_snapshots WHERE id=?",
        [&frozen.snapshot.snapshot_id],
        |row| row.get(0),
    )?;
    if namespace != conversation.operation_namespace {
        return Err(invalid(
            "The frozen discussion belongs to another operation namespace.",
        ));
    }
    for turn in &conversation.turns {
        let actual = read_turn(
            db,
            &turn.run_id,
            &conversation.project_id,
            &conversation.operation_namespace,
            &conversation.document_id,
            &conversation.thread_id,
            conversation.project_conversation_id.as_deref(),
        )?;
        if &actual != turn {
            return Err(invalid(
                "A frozen discussion turn differs from its retained messages.",
            ));
        }
    }
    Ok(())
}

fn read_turn(
    db: &Connection,
    run_id: &str,
    project_id: &str,
    namespace: &str,
    document_id: &str,
    thread_id: &str,
    project_conversation_id: Option<&str>,
) -> CoreResult<ConversationTurn> {
    let row: (String,String,String,String,String,String,String,String,String) = db.query_row(
        "SELECT r.project_id,r.operation_namespace,r.target_document_id,r.thread_id,r.packet_id,p.snapshot_id,s.manifest_json,s.manifest_hash,r.target_body_hash
         FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id JOIN story_snapshots s ON s.id=p.snapshot_id
         WHERE r.id=? AND r.status='completed' AND r.dispatch_state='delivered'
           AND p.project_id=r.project_id AND p.operation_namespace=r.operation_namespace
           AND s.project_id=r.project_id AND s.operation_namespace=r.operation_namespace",
        [run_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)),
    ).optional()?.ok_or_else(|| invalid("A prior discussion is incomplete or has invalid source ownership."))?;
    if row.0 != project_id
        || row.1 != namespace
        || (project_conversation_id.is_none() && row.2 != document_id)
        || (project_conversation_id.is_none() && row.3 != thread_id)
    {
        return Err(invalid(
            "The prior discussion belongs to another project or document.",
        ));
    }
    // Do not recursively validate older conversation packets: each turn pins
    // its own immutable messages, and whole-backup validation visits all rows.
    let source = story_context::decode_snapshot(&row.6, &row.7)?;
    let project_membership = if let Some(conversation_id) = project_conversation_id {
        db.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM conversation_items
                 WHERE conversation_id=? AND project_id=? AND operation_namespace=?
                   AND reference_id=? AND kind IN ('request','chapterRequest')
             )",
            params![conversation_id, project_id, namespace, run_id],
            |r| r.get(0),
        )?
    } else {
        false
    };
    let project_chapter_membership = if let Some(conversation_id) = project_conversation_id {
        db.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM conversation_items
                 WHERE conversation_id=? AND project_id=? AND operation_namespace=?
                   AND reference_id=? AND kind='chapterRequest'
             )",
            params![conversation_id, project_id, namespace, run_id],
            |r| r.get(0),
        )?
    } else {
        false
    };
    let project_target_ok = project_conversation_id.is_some_and(|conversation_id| {
        source.project_chat.as_ref().is_some_and(|chat| {
            chat.conversation_id == conversation_id && chat.anchor_document_id == document_id
        }) || (project_chapter_membership
            && source.snapshot.target.document_id == row.2
            && source.project_chat.is_none())
    });
    // Project-chat runs use the blank anchor's thread. A chapter Discuss run
    // remains owned by its chapter thread; keep that thread identity when
    // reading its messages into the project timeline projection.
    let message_thread_id = if project_membership && source.project_chat.is_none() {
        row.3.as_str()
    } else {
        thread_id
    };
    if source.snapshot.snapshot_id != row.5
        || source.snapshot.project_id != project_id
        || (!project_target_ok && source.snapshot.target.document_id != document_id)
        || source.snapshot.target.body_hash != row.8
        || source.policy.audience != Audience::AuthorRoom
        || source.purpose != ContextPurpose::Discuss
    {
        return Err(invalid(
            "The prior turn is not an author-room discussion of this document.",
        ));
    }
    if let Some(conversation_id) = project_conversation_id {
        if let Some(chat) = source.project_chat.as_ref() {
            if chat.conversation_id != conversation_id
                || chat.anchor_document_id != document_id
                || source.policy.audience != Audience::AuthorRoom
                || source.purpose != ContextPurpose::Discuss
            {
                return Err(invalid(
                    "The prior turn is not owned by this project conversation.",
                ));
            }
        } else if !project_chapter_membership {
            return Err(invalid(
                "The project conversation turn has no project-chat or chapter provenance.",
            ));
        }
    }
    let mut query = db.prepare("SELECT id,role,content,scope_json,thread_id,packet_id FROM discussion_messages WHERE run_id=?")?;
    let rows = query
        .query_map([run_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() != 2 {
        return Err(invalid(
            "A complete prior turn needs exactly one author message and one reply.",
        ));
    }
    let mut user = None;
    let mut assistant = None;
    for (id, role, content, scope_json, owner, packet) in rows {
        // W4's completed assistant messages retain the packet through run_id;
        // older rows have no direct packet_id. An explicit reference, when
        // present, must agree, and the author message always has one.
        let packet_matches =
            packet.as_deref() == Some(row.4.as_str()) || (role == "assistant" && packet.is_none());
        if owner != message_thread_id || !packet_matches {
            return Err(invalid(
                "A prior message does not match its thread and packet.",
            ));
        }
        let message = ConversationMessage {
            id,
            content,
            scope: scope_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
        };
        match role.as_str() {
            "user" if user.is_none() => user = Some(message),
            "assistant" if assistant.is_none() => assistant = Some(message),
            _ => {
                return Err(invalid(
                    "A prior turn has repeated or unknown message roles.",
                ));
            }
        }
    }
    let user = user.ok_or_else(|| invalid("The prior author message is missing."))?;
    if let Some(scope) = &user.scope {
        let revision = read_revision(db, &source.snapshot.target.revision_id)?;
        validate_scope(&ScopeValidationRequest {
            source_snapshot: revision.body.clone(),
            result_snapshot: revision.body,
            scope: scope.clone(),
        })
        .map_err(|_| invalid("The prior selection no longer matches its immutable source."))?;
    }
    Ok(ConversationTurn {
        run_id: run_id.to_owned(),
        packet_id: row.4,
        source_snapshot_id: row.5,
        policy_version: source.policy.version,
        user,
        assistant: assistant.ok_or_else(|| invalid("The prior assistant reply is missing."))?,
    })
}

fn invalid(detail: &str) -> CoreError {
    CoreError::new("InvalidConversationContext", detail)
}
