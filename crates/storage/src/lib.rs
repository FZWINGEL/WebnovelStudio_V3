//! L1 — SQLite open, durability configuration and the migration chain.
//!
//! Extracted from `webnovel-core`, where importing `CoreError` back out of the
//! module that owns the document model formed the storage↔projects cycle. This
//! crate now depends only on `wns-kernel`, which is what lets the cycle stay
//! broken: nothing here can reach the document model, by construction.
//!
//! The migration chain is **move-only**. Schema 40 stays 40; the SQL files and
//! their `if version < N` ordering were relocated verbatim, because the
//! byte-compatibility of historical packet bytes and hashes depends on
//! migrations never being rewritten.

use rusqlite::{Connection, OpenFlags, OptionalExtension, backup::Backup, params};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, DocumentRole, Head, Revision, StoredResult, new_id,
    parse_stored_version, parse_version, validate_snapshot_json,
};

pub const LATEST_SCHEMA_VERSION: i64 = 40;

/// The schema-1 creation script.
///
/// Exposed so that a fixture can build a legacy v1 project without reaching
/// into this crate's source tree by filesystem path — which is what the
/// metadata suite did before this crate existed, and what made the extraction
/// break it. Migrations are append-only history: this constant is byte-identical
/// to what [`migrate`] runs at `if version < 1` and must never change.
#[doc(hidden)]
pub const SCHEMA_001_PROJECTS_SQL: &str = include_str!("001_projects.sql");

pub fn configure(connection: &Connection) -> CoreResult<()> {
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
    )?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let sync: i64 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    if mode != "wal" || sync != 2 || keys != 1 {
        return Err(CoreError::new(
            "PersistenceUnavailable",
            "Required SQLite durability settings are unavailable.",
        ));
    }
    Ok(())
}

pub fn migrate(connection: &mut Connection, root: &Path) -> CoreResult<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > LATEST_SCHEMA_VERSION {
        return Err(CoreError::new(
            "UnsupportedSchema",
            "This project needs a newer version of WebnovelStudio.",
        ));
    }
    if version < LATEST_SCHEMA_VERSION {
        if version > 0 {
            backup_before_upgrade(root, version)?;
        }
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if version < 1 {
            tx.execute_batch(include_str!("001_projects.sql"))?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("002_view_state.sql"))?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("003_story_context.sql"))?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("004_context_packets.sql"))?;
        }
        if version < 5 {
            tx.execute_batch(include_str!("005_discussions.sql"))?;
        }
        if version < 6 {
            tx.execute_batch(include_str!("006_guidance.sql"))?;
        }
        if version < 7 {
            tx.execute_batch(include_str!("007_discussion_retry.sql"))?;
        }
        if version < 8 {
            tx.execute_batch(include_str!("008_proposals.sql"))?;
        }
        if version < 9 {
            tx.execute_batch(include_str!("009_exports.sql"))?;
        }
        if version < 10 {
            tx.execute_batch(include_str!("010_context_source_pins.sql"))?;
        }
        if version < 11 {
            tx.execute_batch(include_str!("011_safe_brief.sql"))?;
        }
        if version < 12 {
            tx.execute_batch(include_str!("012_v2_import.sql"))?;
        }
        if version < 13 {
            tx.execute_batch(include_str!("013_provider_results.sql"))?;
        }
        if version < 14 {
            tx.execute_batch(include_str!("014_reviewed_story.sql"))?;
        }
        if version < 15 {
            tx.execute_batch(include_str!("015_reviewed_context.sql"))?;
        }
        if version < 16 {
            tx.execute_batch(include_str!("016_memory.sql"))?;
        }
        if version < 17 {
            tx.execute_batch(include_str!("017_navigation_context.sql"))?;
        }
        if version < 19 {
            tx.execute_batch(include_str!("019_continuation.sql"))?;
        }
        if version < 20 {
            tx.execute_batch(include_str!("020_reviewed_export.sql"))?;
        }
        if version < 21 {
            tx.execute_batch(include_str!("021_reviewed_story_evidence.sql"))?;
        }
        if version < 22 {
            tx.execute_batch(include_str!("022_structured_proposals.sql"))?;
        }
        if version < 23 {
            tx.execute_batch(include_str!("023_reviewed_promises.sql"))?;
        }
        if version < 24 {
            tx.execute_batch(include_str!("024_discussion_lookup.sql"))?;
        }
        if version < 26 {
            tx.execute_batch(include_str!("026_http_provider_delivery.sql"))?;
        }
        if version < 29 {
            tx.execute_batch(include_str!("029_memory_provider_delivery.sql"))?;
        }
        if version < 30 {
            tx.execute_batch(include_str!("030_claude_provider_result.sql"))?;
        }
        // Schema 31 changes no tables and exists as a reader floor for
        // author-room structured document-development packets. Older readers
        // must reject the project before reopening packets they cannot
        // validate.
        if version < 31 {
            tx.execute_batch("SELECT 1;")?;
        }
        if version < 32 {
            let columns = [
                ("review_stages", "summary_json"),
                ("review_stages", "summary_hash"),
                ("ready_bundles", "summary_json"),
                ("ready_bundles", "summary_hash"),
            ];
            let mut missing = Vec::new();
            for &(table, column) in &columns {
                let present: bool = tx.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |row| row.get(0),
                )?;
                if !present {
                    missing.push((table, column));
                }
            }
            if missing.len() == columns.len() {
                tx.execute_batch(include_str!("032_reviewed_summaries.sql"))?;
            } else {
                for (table, column) in missing {
                    tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
                }
            }
        }
        if version < 33 {
            let columns = [
                ("review_stages", "knowledge_json"),
                ("review_stages", "knowledge_hash"),
                ("ready_bundles", "knowledge_json"),
                ("ready_bundles", "knowledge_hash"),
            ];
            let mut missing = Vec::new();
            for &(table, column) in &columns {
                let present: bool = tx.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |row| row.get(0),
                )?;
                if !present {
                    missing.push((table, column));
                }
            }
            if missing.len() == columns.len() {
                tx.execute_batch(include_str!("033_reviewed_knowledge.sql"))?;
            } else {
                for (table, column) in missing {
                    tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
                }
            }
        }
        // Schema 34 changes no tables. It raises the reader floor for the
        // reviewed-memory lookup capability retained in immutable lookup
        // packet and read rows. Older readers cannot validate those operation
        // shapes; legacy packets without the optional capability remain
        // byte-compatible and are not rewritten by this step.
        if version < 34 {
            tx.execute_batch(include_str!("034_story_memory_lookups.sql"))?;
        }
        if version < 35 {
            tx.execute_batch(include_str!("035_workshop.sql"))?;
        }
        // Schema 36 raises the reader floor for the typed relationship
        // identity carried by Workshop sessions and frozen context packets.
        // Those optional fields live in existing immutable JSON rows, so the
        // migration deliberately performs no rewrite.
        if version < 36 {
            tx.execute_batch(include_str!("036_workshop_relationship_context.sql"))?;
        }
        // Schema 37 raises the reader floor for typed story possibilities
        // carried by Workshop session/context JSON. The optional field is
        // absent from old rows and empty sessions serialize byte-identically.
        if version < 37 {
            tx.execute_batch(include_str!("037_workshop_story_possibilities.sql"))?;
        }
        if version < 38 {
            tx.execute_batch(include_str!("038_codex_app_server.sql"))?;
        }
        if version < 39 {
            tx.execute_batch(include_str!("039_document_roles.sql"))?;
        }
        if version < 40 {
            tx.execute_batch(include_str!("040_project_chat.sql"))?;
        }
        // Schema 30 adds the nullable Claude terminal model claim. NULL keeps
        // historical Codex and HTTP receipts byte-compatible while the reader
        // floor prevents older applications from reopening Claude receipts.
        // Schema 28 changes no tables: new lookup packets include a versioned
        // source-title projection that older readers cannot validate. Absent
        // projections in historical packets keep their exact serialized bytes.
        // Schema 27 changes no tables: author-selected Codex bindings have a
        // distinct profile that older readers cannot validate. Exact earlier
        // packet bytes and the fixed maintenance profile remain unchanged.
        // Schema 25 also changes no tables: new provider bindings record the
        // app profile and observed CLI identity independently. Older readers
        // cannot validate those receipts. Historical packet bytes stay intact.
        // Schema 18 changes no tables or historical bytes. It raises the reader
        // floor: schema-17 readers reject chapter-only navigation views from
        // an earlier source epoch, now valid under their exact dependencies.
        // The project version prevents those readers opening newer receipts.
        tx.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)?;
        tx.commit().map_err(CoreError::uncertain)?;
    }
    Ok(())
}

/// Take a durable, consistent pre-upgrade snapshot before changing an existing DB.
/// The backup remains beside the project so a failed migration can be
/// diagnosed or recovered without relying on the altered database.
fn backup_before_upgrade(root: &Path, version: i64) -> CoreResult<()> {
    let migrations = root.join("migrations");
    fs::create_dir_all(&migrations)?;
    let path = migrations.join(format!(
        "schema{version}-before-schema{LATEST_SCHEMA_VERSION}-{}.sqlite3",
        Uuid::new_v4()
    ));
    let db_path = root.join("project.sqlite3");
    let source = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let placeholder = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    drop(placeholder);
    let mut destination = Connection::open(&path)?;
    Backup::new(&source, &mut destination)?.run_to_completion(
        64,
        Duration::from_millis(10),
        None,
    )?;
    drop(destination);
    let backup = OpenOptions::new().read(true).write(true).open(&path)?;
    backup.sync_all()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Typed row access.
//
// These six functions were the shared primitive layer inside
// `webnovel-core::projects`: 240 call sites across twelve files under
// `projects/`, and every one of them named the same readers. They lived above
// the schema they read, which is why the twenty remaining step-7 modules could
// not move — a module cannot travel to another crate while the helpers its
// bodies call are still in the crate it is leaving.
//
// They read the tables this crate's migrations create, so they belong here.
// `webnovel-core` re-exports every one of them at its historical path, so the
// 240 call sites are unchanged.
// ---------------------------------------------------------------------------

type DocumentRow = (
    String,
    String,
    i64,
    i64,
    String,
    String,
    Option<String>,
    String,
);

/// Read an ordinary story document.  This narrow helper is intentionally the
/// only generic read path: control anchors and assistant drafts cannot leak
/// into project attach, editor, source, memory, review, or export flows.
pub fn read_document(connection: &Connection, id: &str) -> CoreResult<DocumentRecord> {
    read_document_with_role(connection, id, DocumentRole::Ordinary)
}

/// Read one document through an explicitly authorized role path.  Callers
/// must name the role they expect; there is no broad "include hidden" flag.
pub fn read_document_with_role(
    connection: &Connection,
    id: &str,
    expected_role: DocumentRole,
) -> CoreResult<DocumentRecord> {
    let row: Option<DocumentRow> = connection.query_row("SELECT title,kind,working_version,metadata_version,body_hash,body_json,last_checkpoint_id,role FROM documents WHERE id=? AND trashed=0", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?;
    let (title, kind, version, metadata_version, hash, body, checkpoint, stored_role) = row
        .ok_or_else(|| {
            CoreError::new(
                "DocumentNotFound",
                "This document is not available in this project.",
            )
        })?;
    let role = DocumentRole::from_storage(&stored_role)?;
    if role != expected_role {
        return Err(CoreError::new(
            "DocumentRoleMismatch",
            "This document is not available through the requested authority path.",
        ));
    }
    let valid = validate_snapshot_json(&body).map_err(|e| CoreError::new("InvalidDocument", &e))?;
    if valid.hash != hash || valid.canonical_json != body {
        return Err(CoreError::new(
            "InvalidDocument",
            "The saved document failed its fingerprint check.",
        ));
    }
    Ok(DocumentRecord {
        head: Head {
            document_id: id.into(),
            version: version.to_string(),
            body_hash: hash,
        },
        title,
        kind,
        metadata_version: parse_stored_version(metadata_version)?,
        body: valid.snapshot,
        last_checkpoint_id: checkpoint,
        role,
    })
}

pub fn read_revision(connection: &Connection, id: &str) -> CoreResult<Revision> {
    let (doc,version,body,hash,reason,parent): (String,i64,String,String,String,Option<String>) = connection.query_row("SELECT document_id,source_working_version,body_json,body_hash,reason,parent_id FROM revisions WHERE id=?", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let valid = validate_snapshot_json(&body).map_err(|e| CoreError::new("InvalidDocument", &e))?;
    if valid.hash != hash || valid.canonical_json != body {
        return Err(CoreError::new(
            "InvalidDocument",
            "The revision failed its fingerprint check.",
        ));
    }
    Ok(Revision {
        id: id.into(),
        head: Head {
            document_id: doc,
            version: version.to_string(),
            body_hash: hash,
        },
        body: valid.snapshot,
        reason,
        parent_id: parent,
    })
}

/// Retain the document's current body as a revision, or return the revision
/// already recorded at this working version. The `documents.last_checkpoint_id`
/// update is what makes the next revision's parent the one just written.
pub fn checkpoint_at(
    connection: &Connection,
    document: &DocumentRecord,
    reason: &str,
) -> CoreResult<Revision> {
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM revisions WHERE document_id=? AND source_working_version=?",
            params![
                document.head.document_id,
                parse_version(&document.head.version)?
            ],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return read_revision(connection, &id);
    }
    let id = new_id();
    connection.execute("INSERT INTO revisions(id,document_id,source_working_version,schema_version,body_json,body_hash,parent_id,reason) VALUES(?,?,?,1,?,?,?,?)", params![id, document.head.document_id, parse_version(&document.head.version)?, serde_json::to_string(&document.body)?, document.head.body_hash, document.last_checkpoint_id, reason])?;
    connection.execute(
        "UPDATE documents SET last_checkpoint_id=? WHERE id=?",
        params![id, document.head.document_id],
    )?;
    read_revision(connection, &id)
}

/// The immutable decision already recorded for one operation, if any.
///
/// The two lookups before `command_receipts` are the cross-kind reuse fence:
/// an operation id spent on a workshop or proposal command may not be spent
/// again on a document command, so this one function is the single place the
/// three receipt tables are reconciled.
pub fn existing_receipt(
    connection: &Connection,
    namespace: &str,
    id: &str,
    kind: &str,
    payload: &str,
) -> CoreResult<Option<StoredResult>> {
    let workshop_receipt: Option<(String, String)> = connection
        .query_row(
            "SELECT operation_kind,payload_hash FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
            params![namespace, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if workshop_receipt.is_some() {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for a workshop command.",
        ));
    }
    let proposal_receipt: Option<(String, String)> = connection
        .query_row(
            "SELECT kind,payload_hash FROM proposal_receipts \
             WHERE operation_namespace=? AND operation_id=?",
            params![namespace, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if proposal_receipt.is_some() {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for a proposal command.",
        ));
    }
    let found: Option<(String, String, String)> = connection.query_row("SELECT operation_kind,payload_hash,result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?", params![namespace, id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    found
        .map(|(stored_kind, stored_payload, result)| {
            if stored_kind != kind || stored_payload != payload {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This operation ID was already used for a different request.",
                ));
            }
            Ok(serde_json::from_str(&result)?)
        })
        .transpose()
}

pub fn insert_receipt(
    connection: &Connection,
    namespace: &str,
    id: &str,
    kind: &str,
    payload: &str,
    result: &StoredResult,
) -> CoreResult<()> {
    connection.execute("INSERT INTO command_receipts(operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json) VALUES(?,?,?,?,?,?)", params![namespace, id, result.head.document_id, payload, kind, serde_json::to_string(result)?])?;
    Ok(())
}

/// The creation record written beside a project database.
pub mod creation;
pub use creation::{
    CreationOrigin, ProjectMetadata, read_creation_origin, write_creation_origin,
};
