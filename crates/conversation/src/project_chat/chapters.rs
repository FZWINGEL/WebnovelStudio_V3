//! Chapter requests submitted from the project conversation.
//!
//! A chapter request is a normal discussion run whose exact chapter target,
//! scope, and optional approved brief are captured in the project composer.
//! The conversation item is only a durable timeline reference; the chapter
//! discussion and its source packet remain authoritative for generation.

use super::store;
use super::*;
use crate::discussions::{self, FeedbackIntent, StartDiscussion};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde_json::json;

fn validate_chapter_composer(composer: &ProjectComposer) -> CoreResult<&ProjectChapterComposer> {
    let chapter = composer.chapter.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidChapterRequest",
            "A chapter task is required for this request.",
        )
    })?;
    if !composer.task_draft_refs.is_empty() {
        return Err(CoreError::new(
            "InvalidChapterRequest",
            "Unadopted project-chat drafts cannot enter a restricted chapter request.",
        ));
    }
    if matches!(chapter.intent, FeedbackIntent::WorkshopExplore) {
        return Err(CoreError::new(
            "InvalidChapterRequest",
            "Workshop exploration belongs in the project conversation, not a chapter request.",
        ));
    }
    if let Some(brief) = chapter.safe_brief.as_ref()
        && (!brief.confirmed || brief.text.trim().is_empty())
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A chapter brief must be nonempty and explicitly confirmed.",
        ));
    }
    Ok(chapter)
}

fn exact_pinned_documents(
    tx: &rusqlite::Transaction<'_>,
    composer: &ProjectComposer,
    target: &Head,
) -> CoreResult<Vec<String>> {
    let mut ids = Vec::with_capacity(composer.source_refs.len() + 1);
    for head in composer.source_refs.iter().chain(
        composer
            .focused_document_ref
            .iter()
            .filter(|head| head.document_id != target.document_id),
    ) {
        let document = read_document_with_role(tx, &head.document_id, DocumentRole::Ordinary)?;
        require_head(&document.head, head)?;
        if document.kind == "chapter" && head.document_id != target.document_id {
            // A chapter can be pinned as context, but the target remains the
            // only chapter whose writing scope is mutable.
        }
        if !ids.contains(&head.document_id) {
            ids.push(head.document_id.clone());
        }
    }
    Ok(ids)
}

// Actor-side logic, as free functions over `ProjectChatHost`.

pub fn start_project_chapter(
    host: &mut impl ProjectChatHost,
    request: StartProjectChapter,
) -> CoreResult<DiscussionStart> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    store::validate_composer(&request.composer)?;
    let chapter = validate_chapter_composer(&request.composer)?;
    if request.composer.text.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidChapterRequest",
            "Write an instruction for the chapter task first.",
        ));
    }

    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    store::require_conversation(&tx, &request.access, &request.conversation_id)?;

    // Check an existing generation before the composer CAS. This makes a
    // retry return the original start receipt even if the composer has
    // already been cleared by the first successful transaction.
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT id,payload_hash FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",
            params![
                request.access.project_id,
                request.access.operation_namespace,
                request.operation_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((run_id, previous_hash)) = existing {
        if previous_hash != payload {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This request ID already has another chapter task.",
            ));
        }
        store::read_item_by_reference(&tx, &request.conversation_id, "chapterRequest", &run_id)?;
        let result = discussions::read_start(&tx, &run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(result);
    }
    if existing_receipt(
        &tx,
        &request.access.operation_namespace,
        &request.operation_id,
        "chapterRequest",
        &payload,
    )?
    .is_some()
    {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This request ID already belongs to a local chapter operation.",
        ));
    }

    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM discussion_runs r JOIN conversation_items i
         ON i.reference_id=r.id AND i.kind IN ('request','chapterRequest')
         WHERE i.conversation_id=? AND r.status IN ('queued','running','stopping'))",
        [&request.conversation_id],
        |row| row.get(0),
    )?;
    if active {
        return Err(CoreError::new(
            "ProjectChatBusy",
            "Wait for this project's request or stop it before sending another.",
        ));
    }
    let current = store::read_composer(&tx, &request.conversation_id)?;
    if current.version != request.expected_composer_version || current.body != request.composer {
        return Err(CoreError::new(
            "VersionConflict",
            "Save the exact chapter task before sending.",
        ));
    }

    let target = read_document_with_role(&tx, &chapter.target.document_id, DocumentRole::Ordinary)?;
    require_head(&target.head, &chapter.target)?;
    if target.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidChapterRequest",
            "Chapter requests require an ordinary chapter document.",
        ));
    }
    let pinned = exact_pinned_documents(&tx, &request.composer, &chapter.target)?;
    let discussion = StartDiscussion {
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        expected: chapter.target.clone(),
        instruction: request.composer.text.clone(),
        intent: chapter.intent,
        basis: chapter.basis,
        scope: chapter.scope.clone(),
        pinned_document_ids: pinned,
        safe_brief: chapter.safe_brief.clone(),
        budget: request.budget,
        provider_binding: request.provider_binding,
        previous_run_id: None,
        lookup: None,
    };
    discussions::validate_start(&discussion)?;
    let chapter_range_response =
        chapter.intent == FeedbackIntent::Discuss && chapter.scope.is_none();
    let result =
        discussions::start_discussion_at(&tx, &discussion, &payload, None, chapter_range_response)?;
    store::append_item(
        &tx,
        &request.access,
        &request.conversation_id,
        Some(&request.operation_id),
        "chapterRequest",
        Some(&result.run.id),
        &json!({
            "composerVersion": request.expected_composer_version,
            "target": chapter.target.clone(),
            "intent": chapter.intent,
            "basis": chapter.basis,
            "scope": chapter.scope,
            "safeBrief": chapter.safe_brief.clone(),
            "userMessageId": result.user_message.id,
        }),
    )?;
    // The submitted chapter task is consumed with the run reference. A
    // newer renderer buffer must be saved against the next CAS version.
    tx.execute(
        "UPDATE project_conversations SET composer_version=composer_version+1,composer_json=? WHERE id=?",
        params![serde_json::to_string(&ProjectComposer::default())?, request.conversation_id],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(result)
}
