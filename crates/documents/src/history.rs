//! Bounded immutable document history and explicit whole-document restore.
//!
//! History reads expose metadata first and load one selected body on demand.
//! Restore is an author command: it advances the working version, retains the
//! current body in a before checkpoint, and records an immutable decision and
//! shared command receipt in one immediate transaction.
//!
//! # Why this file is here and not in `webnovel-core`
//!
//! Step 7 of the modular migration moves each `projects/` module's vocabulary
//! and actor-side logic out of `webnovel-core`, leaving only the command
//! plumbing behind, because an inherent `impl` must live in the crate that owns
//! the type. Document revision history is document lifecycle, so it belongs
//! with the document model.
//!
//! That move was blocked for two turns, and the blocker is worth recording. It
//! looked unblocked — this was the only module under `projects/` with no
//! cross-module `module::` call — but it was not: its actor side called eight
//! crate-private free functions (`read_document`, `read_revision`,
//! `checkpoint_at`, `existing_receipt`, `insert_receipt`, `valid_hash`,
//! `require_head`) that lived in `projects.rs` and had 240 call sites between
//! them. A selection criterion that reads imports cannot see that, because the
//! dependency is in the bodies.
//!
//! Those primitives have since moved down — vocabulary to `wns-kernel`, row
//! access to `wns-storage` — which is what makes this file possible. It is the
//! first module to move *because* they did.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, Head, ProjectAccess, Reply, RestoredDecision, Revision,
    StoredResult, check_id, logical_hash, parse_version, require_head, valid_hash,
};
use wns_storage::{checkpoint_at, existing_receipt, insert_receipt, read_document, read_revision};

const DEFAULT_HISTORY_LIMIT: u32 = 50;
const MAX_HISTORY_LIMIT: u32 = 100;

/// What one module needs from the actor, and nothing else.
///
/// A host trait is narrow only if the helpers travel with the module. Here they
/// did: the seven free functions this file used moved to `wns-storage` first,
/// so what remains is four actor methods plus one crash-injection hook.
pub trait HistoryHost {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()>;
    fn db(&self) -> CoreResult<&Connection>;
    fn db_mut(&mut self) -> CoreResult<&mut Connection>;
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>);
    /// Crash-injection point for `kill_after_commit_before_ack_recovers_once`.
    ///
    /// Declared unconditionally so this trait compiles in a non-test build of
    /// this crate. The implementation in `webnovel-core` compiles the real hook
    /// only into its own test binary, so production behaviour is unchanged and
    /// the hook does not become reachable from a released library.
    fn hold_after_commit_before_ack(&self, operation_id: &str);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionSummary {
    pub id: String,
    pub head: Head,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryPage {
    pub items: Vec<RevisionSummary>,
    pub next_before_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreRevision {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub revision_id: String,
    pub revision_hash: String,
    pub local_generation: String,
}

/// Restore has the same acknowledgement shape as Apply. On replay, `result`
/// is the historical first result while `document` is the latest committed
/// document, so the editor never replays an old mutation over newer text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreAck {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub already_applied: bool,
    pub result: StoredResult,
    pub document: DocumentRecord,
}

/// The actor's command vocabulary for this concern.
///
/// This is the type that sets the size of the move. `Command::History` in
/// `webnovel-core` carries it, which is a legal downward edge, so the session
/// half can stay where the `Command` enum lives.
pub enum HistoryCommand {
    List(
        ProjectAccess,
        String,
        Option<String>,
        u32,
        Reply<HistoryPage>,
    ),
    Read(ProjectAccess, String, String, Reply<Revision>),
    Restore(RestoreRevision, Reply<RestoreAck>),
}

pub fn handle_history(host: &mut impl HistoryHost, command: HistoryCommand) {
    macro_rules! respond {
        ($reply:expr, $result:expr) => {{
            let result = $result;
            host.fence_uncertain(&result);
            let _ = $reply.send(result);
        }};
    }
    match command {
        HistoryCommand::List(access, document_id, before, limit, reply) => respond!(
            reply,
            host.check_access(&access).and_then(|()| {
                list_document_history(host.db()?, &document_id, before.as_deref(), limit)
            })
        ),
        HistoryCommand::Read(access, document_id, revision_id, reply) => respond!(
            reply,
            host.check_access(&access).and_then(|()| {
                read_document_revision(host.db()?, &document_id, &revision_id)
            })
        ),
        HistoryCommand::Restore(request, reply) => {
            respond!(reply, restore_revision(host, request));
        }
    }
}

pub fn restore_revision(
    host: &mut impl HistoryHost,
    request: RestoreRevision,
) -> CoreResult<RestoreAck> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    check_id(&request.revision_id)?;
    parse_version(&request.expected.version)?;
    parse_version(&request.local_generation)?;
    if !valid_hash(&request.expected.body_hash) || !valid_hash(&request.revision_hash) {
        return Err(CoreError::new(
            "InvalidRequest",
            "Restore heads must contain valid body hashes.",
        ));
    }
    let payload = logical_hash(&request)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let prior = existing_receipt(
        &tx,
        &request.access.operation_namespace,
        &request.operation_id,
        "restore",
        &payload,
    )?;
    let already_applied = prior.is_some();
    let result = if let Some(result) = prior {
        validate_restore_result(&tx, &request.expected.document_id, &result)?;
        result
    } else {
        let before = read_document(&tx, &request.expected.document_id)?;
        require_head(&before.head, &request.expected)?;
        let source =
            read_revision_checked(&tx, &request.expected.document_id, &request.revision_id)?;
        if source.head.body_hash != request.revision_hash {
            return Err(CoreError::new(
                "RevisionHashMismatch",
                "The selected revision does not match its supplied fingerprint.",
            ));
        }
        if source.head.body_hash == before.head.body_hash {
            return Err(CoreError::new(
                "NoChanges",
                "The selected revision is already the current document body.",
            ));
        }
        let next = parse_version(&before.head.version)?
            .checked_add(1)
            .ok_or_else(|| {
                CoreError::new("VersionLimit", "The document version limit was reached.")
            })?;
        let before_revision = checkpoint_at(&tx, &before, "beforeRestore")?;
        let source_json = serde_json::to_string(&source.body)?;
        let changed = tx.execute(
            "UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1 \
             WHERE id=? AND working_version=? AND body_hash=?",
            params![
                next,
                source_json,
                source.head.body_hash,
                before.head.document_id,
                parse_version(&before.head.version)?,
                before.head.body_hash
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "VersionConflict",
                "The document changed before the revision was restored.",
            ));
        }
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        let after = read_document(&tx, &before.head.document_id)?;
        let after_revision = checkpoint_at(&tx, &after, "afterRestore")?;
        let restored = RestoredDecision {
            revision_id: source.id,
            before_revision_id: before_revision.id,
            after_revision_id: after_revision.id,
        };
        let result = StoredResult {
            head: after.head,
            saved_generation: request.local_generation.clone(),
            applied: None,
            restored: Some(restored),
        };
        insert_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "restore",
            &payload,
            &result,
        )?;
        result
    };
    let document = read_document(&tx, &request.expected.document_id)?;
    tx.commit().map_err(CoreError::uncertain)?;
    host.hold_after_commit_before_ack(&request.operation_id);
    Ok(RestoreAck {
        access: request.access,
        operation_id: request.operation_id,
        already_applied,
        result,
        document,
    })
}

fn ensure_document_exists(connection: &Connection, document_id: &str) -> CoreResult<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0 AND role='ordinary')",
        [document_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            "DocumentNotFound",
            "This document is not available in this project.",
        ))
    }
}

fn read_revision_checked(
    connection: &Connection,
    document_id: &str,
    revision_id: &str,
) -> CoreResult<Revision> {
    let owner: Option<String> = connection
        .query_row(
            "SELECT document_id FROM revisions WHERE id=?",
            [revision_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(owner) = owner else {
        return Err(CoreError::new(
            "RevisionNotFound",
            "This document revision is not available in the project.",
        ));
    };
    if owner != document_id {
        return Err(CoreError::new(
            "RevisionDocumentMismatch",
            "The selected revision belongs to another document.",
        ));
    }
    read_revision(connection, revision_id)
}

fn list_document_history(
    connection: &Connection,
    document_id: &str,
    before_version: Option<&str>,
    limit: u32,
) -> CoreResult<HistoryPage> {
    check_id(document_id)?;
    ensure_document_exists(connection, document_id)?;
    let limit = if limit == 0 {
        DEFAULT_HISTORY_LIMIT
    } else {
        limit
    };
    if limit > MAX_HISTORY_LIMIT {
        return Err(CoreError::new(
            "InvalidRequest",
            "History requests may contain at most 100 entries.",
        ));
    }
    let before = before_version.map(parse_version).transpose()?;
    let mut statement = connection.prepare(
        "SELECT id,source_working_version,body_hash,reason,created_at
         FROM revisions
         WHERE document_id=? AND (? IS NULL OR source_working_version < ?)
         ORDER BY source_working_version DESC, id DESC
         LIMIT ?",
    )?;
    let rows = statement.query_map(
        params![document_id, before, before, i64::from(limit) + 1],
        |row| {
            let id: String = row.get(0)?;
            let version: i64 = row.get(1)?;
            let hash: String = row.get(2)?;
            let reason: String = row.get(3)?;
            let created_at: String = row.get(4)?;
            Ok((id, version, hash, reason, created_at))
        },
    )?;
    let mut items = Vec::new();
    for row in rows {
        let (id, version, hash, reason, created_at) = row?;
        check_id(&id)?;
        if !valid_hash(&hash) {
            return Err(CoreError::new(
                "InvalidProject",
                "A history revision contains an invalid body hash.",
            ));
        }
        if version < 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "A history revision contains a negative working version.",
            ));
        }
        items.push(RevisionSummary {
            id,
            head: Head {
                document_id: document_id.to_owned(),
                version: version.to_string(),
                body_hash: hash,
            },
            reason,
            created_at,
        });
    }
    let next_before_version = if items.len() > limit as usize {
        let next = items[limit as usize - 1].head.version.clone();
        items.truncate(limit as usize);
        Some(next)
    } else {
        None
    };
    Ok(HistoryPage {
        items,
        next_before_version,
    })
}

fn read_document_revision(
    connection: &Connection,
    document_id: &str,
    revision_id: &str,
) -> CoreResult<Revision> {
    check_id(document_id)?;
    check_id(revision_id)?;
    ensure_document_retained(connection, document_id)?;
    read_revision_checked(connection, document_id, revision_id)
}

fn ensure_document_retained(connection: &Connection, document_id: &str) -> CoreResult<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND role='ordinary')",
        [document_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            "DocumentNotFound",
            "This document is not available in this project.",
        ))
    }
}

/// Validate restore decisions represented by shared command receipts when a
/// database is backed up or recovered. The receipt is the immutable decision
/// identity, so there is no second writable decision table to reconcile.
pub fn validate_history_storage(connection: &Connection) -> CoreResult<()> {
    let mut receipts = connection.prepare(
        "SELECT operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json
         FROM command_receipts",
    )?;
    let rows = receipts.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (namespace, operation, document_id, payload_hash, kind, result_json) = row?;
        for value in [&namespace, &operation, &document_id] {
            check_id(value).map_err(|_| {
                CoreError::new(
                    "InvalidProject",
                    "A restore receipt contains an invalid identifier.",
                )
            })?;
        }
        if !valid_hash(&payload_hash) {
            return Err(CoreError::new(
                "InvalidProject",
                "A restore receipt contains an invalid request hash.",
            ));
        }
        if kind == "restore"
            && !connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND role='ordinary')",
                [&document_id],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A restore receipt points at a non-ordinary document.",
            ));
        }
        // Grouped chat adoption has a ref-only, multi-document receipt. Its
        // revision/preview/decision proof is checked by project_chat's storage
        // validator; it cannot contain a single-document restore result.
        if kind == "adoptChatPreview" {
            continue;
        }
        let result: StoredResult = serde_json::from_str(&result_json)?;
        let Some(_restored) = result.restored.as_ref() else {
            if kind == "restore" {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A restore receipt is missing its decision metadata.",
                ));
            }
            continue;
        };
        if kind != "restore" {
            return Err(CoreError::new(
                "InvalidProject",
                "A restore decision is stored under a non-restore command kind.",
            ));
        }
        validate_restore_result(connection, &document_id, &result)?;
    }
    Ok(())
}

fn validate_restore_result(
    connection: &Connection,
    document_id: &str,
    result: &StoredResult,
) -> CoreResult<()> {
    let restored = result.restored.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidReceipt",
            "The restore receipt is missing its decision metadata.",
        )
    })?;
    let source = read_revision_checked(connection, document_id, &restored.revision_id)?;
    let before = read_revision_checked(connection, document_id, &restored.before_revision_id)?;
    let after = read_revision_checked(connection, document_id, &restored.after_revision_id)?;
    let before_version = parse_version(&before.head.version)?;
    let after_version = parse_version(&after.head.version)?;
    let expected_after_version = before_version.checked_add(1).ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "A restore before revision has an out-of-range version.",
        )
    })?;
    let source_version = parse_version(&source.head.version)?;
    if result.head != after.head
        || parse_version(&result.saved_generation).is_err()
        || source.head.body_hash != after.head.body_hash
        || source.body != after.body
        || source_version > before_version
        || before.body == after.body
        || after_version != expected_after_version
        || after.parent_id.as_deref() != Some(before.id.as_str())
        || result.applied.is_some()
    {
        return Err(CoreError::new(
            "InvalidReceipt",
            "The restore receipt does not match its source and before/after revisions.",
        ));
    }
    Ok(())
}
