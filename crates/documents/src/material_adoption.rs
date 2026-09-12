//! Transaction-local writes for author-approved material.
//!
//! This module deliberately does not own an operation receipt, source epoch,
//! relationship projection, or transaction boundary.  A caller validates its
//! domain-specific preview and then uses this helper while it owns the
//! `Immediate` transaction.  Keeping the document mutation here gives chat
//! adoption the same checkpoint and optimistic-head semantics as Workshop
//! adoption without making either feature an orchestrator for the other.

use rusqlite::{Connection, Transaction, params};
use serde_json::Value;
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, Head, check_id, parse_version, valid_hash,
    validate_snapshot_json, validate_title,
};
use wns_storage::{checkpoint_at, read_document};

/// A single ordinary, nonchapter document to write inside a caller-owned
/// transaction.
///
/// `expected` is `Some` for an update and `None` for an insert.  The helper
/// rechecks that identity at write time, so a preview cannot silently replace
/// a document that appeared after it was created.
#[derive(Debug, Clone)]
pub struct MaterialTarget {
    pub document_id: String,
    pub title: String,
    pub kind: String,
    pub body: Value,
    pub expected: Option<Head>,
}

/// The two domain adoption flows that may create an ordinary material write.
///
/// Keeping the origins closed here prevents callers from manufacturing a
/// checkpoint reason that looks like an adoption from some unrelated path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialAdoptionFlow {
    Chat,
    Workshop,
}

impl MaterialAdoptionFlow {
    fn checkpoint_reasons(self) -> (&'static str, &'static str) {
        match self {
            Self::Chat => ("beforeChatAdoption", "chatAdoption"),
            Self::Workshop => ("beforeWorkshopAdoption", "workshopAdoption"),
        }
    }
}

/// A single approved material write borrowed from a caller-owned transaction.
///
/// The capability owns no connection and exposes no SQL or transaction
/// controls.  It is consumed by [`Self::apply`], so one capability cannot be
/// applied twice. The transaction remains borrowed while the capability is in
/// use, so it cannot be committed before [`Self::apply`] consumes the value.
/// The caller must validate the domain preview and access before constructing
/// it; possession of this helper is not proof of author intent.
///
/// ```compile_fail
/// use rusqlite::Connection;
/// use wns_documents::material_adoption::{ApprovedMaterialWrite, MaterialAdoptionFlow, MaterialTarget};
/// fn from_connection(connection: &Connection, target: &MaterialTarget) {
///     // An autocommit connection is not an adoption capability source.
///     let _ = ApprovedMaterialWrite::new(connection, target, MaterialAdoptionFlow::Chat);
/// }
/// ```
///
/// ```compile_fail
/// use rusqlite::Transaction;
/// use wns_documents::material_adoption::{ApprovedMaterialWrite, MaterialAdoptionFlow, MaterialTarget};
/// fn raw_sql(transaction: &Transaction<'_>, target: &MaterialTarget) {
///     let write = ApprovedMaterialWrite::new(transaction, target, MaterialAdoptionFlow::Chat);
///     // The capability is not a raw SQL handle and cannot be used to execute SQL.
///     let _ = write.execute("DELETE FROM documents", []);
/// }
/// ```
///
/// ```compile_fail
/// use wns_documents::material_adoption::{ApprovedMaterialWrite, MaterialAdoptionFlow, MaterialTarget};
/// fn apply_twice(transaction: &rusqlite::Transaction<'_>, target: &MaterialTarget) {
///     let write = ApprovedMaterialWrite::new(transaction, target, MaterialAdoptionFlow::Chat);
///     let _ = write.apply();
///     // Applying consumes the capability.
///     let _ = write.apply();
/// }
/// ```
///
/// ```compile_fail
/// use rusqlite::Connection;
/// use wns_documents::material_adoption::{ApprovedMaterialWrite, MaterialAdoptionFlow, MaterialTarget};
/// fn commit_before_apply(connection: &mut Connection, target: &MaterialTarget) {
///     let transaction = connection.transaction().unwrap();
///     let write = ApprovedMaterialWrite::new(&transaction, target, MaterialAdoptionFlow::Chat);
///     // The transaction remains borrowed until the capability is consumed.
///     transaction.commit().unwrap();
///     let _ = write.apply();
/// }
/// ```
pub struct ApprovedMaterialWrite<'borrow, 'connection, 'target> {
    transaction: &'borrow Transaction<'connection>,
    target: &'target MaterialTarget,
    flow: MaterialAdoptionFlow,
}

impl<'borrow, 'connection, 'target> ApprovedMaterialWrite<'borrow, 'connection, 'target> {
    /// Borrow one fixed target from the caller's existing transaction.
    pub fn new(
        transaction: &'borrow Transaction<'connection>,
        target: &'target MaterialTarget,
        flow: MaterialAdoptionFlow,
    ) -> Self {
        Self {
            transaction,
            target,
            flow,
        }
    }

    /// Apply this one target and consume the capability.
    pub fn apply(self) -> CoreResult<DocumentRecord> {
        let (before_reason, after_reason) = self.flow.checkpoint_reasons();
        write_material_target_at(self.transaction, self.target, before_reason, after_reason)
    }
}

/// Write one approved ordinary document and create its immutable revision
/// checkpoints.
///
/// The caller owns the surrounding transaction and any domain projections.
/// Existing document metadata is intentionally left untouched: title and kind
/// are checked against the frozen target, but this helper never updates them.
/// For an existing document the before checkpoint is recorded before the body
/// revision and the after checkpoint follows it.  A new document has no prior
/// body, so only the after checkpoint is meaningful.
fn write_material_target_at(
    connection: &Connection,
    target: &MaterialTarget,
    before_reason: &str,
    after_reason: &str,
) -> CoreResult<DocumentRecord> {
    validate_material_target(target)?;
    let (canonical, hash) = canonical_material_body(&target.body)?;
    let current = match read_document(connection, &target.document_id) {
        Ok(document) => Some(document),
        Err(error) if error.code == "DocumentNotFound" => None,
        Err(error) => return Err(error),
    };

    let Some(current) = current else {
        if target.expected.is_some() {
            return Err(CoreError::new(
                "DocumentNotFound",
                "The material target no longer exists.",
            ));
        }

        // Omitting role preserves compatibility with the pre-chat schema;
        // migration 039 supplies the ordinary default for this insert.
        let affected = connection.execute(
            "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,?,?,(SELECT COALESCE(MAX(position),-1)+1 FROM documents WHERE role='ordinary'),0,1,?,?)",
            params![
                target.document_id,
                target.kind,
                target.title,
                serde_json::to_string(&canonical)?,
                hash,
            ],
        )?;
        if affected != 1 {
            return Err(CoreError::new(
                "PersistenceUnavailable",
                "The new material target was not inserted.",
            ));
        }
        let inserted = read_document(connection, &target.document_id)?;
        checkpoint_at(connection, &inserted, after_reason)?;
        return read_document(connection, &target.document_id);
    };

    let expected = target.expected.as_ref().ok_or_else(|| {
        CoreError::new("VersionConflict", "A new material target already exists.")
    })?;
    if &current.head != expected {
        let mut error = CoreError::new(
            "VersionConflict",
            "The material target changed since its preview.",
        );
        error.current_head = Some(current.head.clone());
        return Err(error);
    }
    if current.kind != target.kind || current.head.document_id != target.document_id {
        return Err(CoreError::new(
            "InvalidRequest",
            "A material target kind or identity changed.",
        ));
    }
    if current.title != target.title {
        return Err(CoreError::new(
            "InvalidAdoption",
            "An existing document title cannot change during material adoption.",
        ));
    }

    checkpoint_at(connection, &current, before_reason)?;
    let current_version = parse_version(&current.head.version)?;
    let next_version = current_version
        .checked_add(1)
        .ok_or_else(|| CoreError::new("VersionLimit", "The document version limit was reached."))?;
    let affected = connection.execute(
        "UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1 WHERE id=? AND working_version=? AND body_hash=?",
        params![
            next_version,
            serde_json::to_string(&canonical)?,
            hash,
            target.document_id,
            current_version,
            current.head.body_hash,
        ],
    )?;
    if affected != 1 {
        let latest = read_document(connection, &target.document_id).ok();
        let mut error = CoreError::new(
            "VersionConflict",
            "The material target changed while it was being adopted.",
        );
        error.current_head = latest.map(|document| document.head);
        return Err(error);
    }
    let updated = read_document(connection, &target.document_id)?;
    checkpoint_at(connection, &updated, after_reason)?;
    read_document(connection, &target.document_id)
}

fn validate_material_target(target: &MaterialTarget) -> CoreResult<()> {
    check_id(&target.document_id)?;
    validate_title(&target.title)?;
    if !["note", "character", "world", "theme", "hook", "scene"].contains(&target.kind.as_str()) {
        return Err(CoreError::new(
            "InvalidDocument",
            "Material adoption can target only ordinary nonchapter documents.",
        ));
    }
    if let Some(expected) = &target.expected {
        if expected.document_id != target.document_id || !valid_hash(&expected.body_hash) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A material target has an invalid expected head.",
            ));
        }
        parse_version(&expected.version)?;
    }
    Ok(())
}

fn canonical_material_body(body: &Value) -> CoreResult<(Value, String)> {
    let receipt = validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidDocument", &error))?;
    Ok((receipt.snapshot, receipt.hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blank_document;
    use rusqlite::{Connection, TransactionBehavior};

    fn test_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open test database");
        connection
            .execute_batch(
                "CREATE TABLE documents(
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    title TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    working_version INTEGER NOT NULL,
                    schema_version INTEGER NOT NULL,
                    body_json TEXT NOT NULL,
                    body_hash TEXT NOT NULL,
                    metadata_version INTEGER NOT NULL,
                    last_checkpoint_id TEXT,
                    role TEXT NOT NULL,
                    trashed INTEGER NOT NULL,
                    projection_dirty INTEGER NOT NULL
                );
                CREATE TABLE revisions(
                    id TEXT PRIMARY KEY,
                    document_id TEXT NOT NULL,
                    source_working_version INTEGER NOT NULL,
                    schema_version INTEGER NOT NULL,
                    body_json TEXT NOT NULL,
                    body_hash TEXT NOT NULL,
                    parent_id TEXT,
                    reason TEXT NOT NULL
                );",
            )
            .expect("create test schema");
        connection
    }

    fn test_body(text: &str) -> Value {
        serde_json::json!({
            "schemaVersion": 1,
            "body": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "attrs": {"id": "paragraph-test"},
                    "content": [{"type": "text", "text": text}]
                }]
            }
        })
    }

    fn seed_document(connection: &Connection, body: &Value) -> DocumentRecord {
        let (canonical, hash) = canonical_material_body(body).expect("canonical test body");
        connection
            .execute(
                "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash,metadata_version,last_checkpoint_id,role,trashed,projection_dirty) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    "material",
                    "note",
                    "Material",
                    0i64,
                    0i64,
                    1i64,
                    serde_json::to_string(&canonical).expect("serialize test body"),
                    hash,
                    0i64,
                    Option::<String>::None,
                    "ordinary",
                    0i64,
                    0i64,
                ],
            )
            .expect("seed test document");
        read_document(connection, "material").expect("read seeded document")
    }

    fn assert_flow_reasons(flow: MaterialAdoptionFlow, before_reason: &str, after_reason: &str) {
        let mut connection = test_connection();
        let before = seed_document(&connection, &test_body("before"));
        let target = MaterialTarget {
            document_id: "material".into(),
            title: before.title.clone(),
            kind: before.kind.clone(),
            body: test_body("after"),
            expected: Some(before.head.clone()),
        };
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin immediate transaction");
        let applied = ApprovedMaterialWrite::new(&tx, &target, flow)
            .apply()
            .expect("apply material write");
        assert_eq!(
            read_document(&tx, "material")
                .expect("read in transaction")
                .head,
            applied.head
        );
        let reasons = {
            let mut statement = tx
                .prepare("SELECT reason FROM revisions ORDER BY source_working_version")
                .expect("prepare checkpoint query");
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query checkpoints")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect checkpoint reasons")
        };
        assert_eq!(reasons, vec![before_reason, after_reason]);
        tx.commit().expect("commit adoption transaction");
        assert_eq!(
            read_document(&connection, "material")
                .expect("read committed document")
                .head,
            applied.head
        );
    }

    #[test]
    fn target_rejects_chapters_before_touching_database() {
        let target = MaterialTarget {
            document_id: "chapter".into(),
            title: "Chapter".into(),
            kind: "chapter".into(),
            body: blank_document(),
            expected: None,
        };
        let error = validate_material_target(&target).expect_err("chapter must be rejected");
        assert_eq!(error.code, "InvalidDocument");
    }

    #[test]
    fn chat_capability_preserves_visibility_and_checkpoint_origins() {
        assert_flow_reasons(
            MaterialAdoptionFlow::Chat,
            "beforeChatAdoption",
            "chatAdoption",
        );
    }

    #[test]
    fn workshop_capability_preserves_checkpoint_origins() {
        assert_flow_reasons(
            MaterialAdoptionFlow::Workshop,
            "beforeWorkshopAdoption",
            "workshopAdoption",
        );
    }

    #[test]
    fn capability_write_rolls_back_with_the_callers_transaction() {
        let mut connection = test_connection();
        let before = seed_document(&connection, &test_body("before"));
        let target = MaterialTarget {
            document_id: "material".into(),
            title: before.title.clone(),
            kind: before.kind.clone(),
            body: test_body("uncommitted"),
            expected: Some(before.head.clone()),
        };
        {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin immediate transaction");
            let applied = ApprovedMaterialWrite::new(&tx, &target, MaterialAdoptionFlow::Chat)
                .apply()
                .expect("apply material write");
            assert_ne!(applied.head, before.head);
            assert_eq!(
                read_document(&tx, "material")
                    .expect("read uncommitted document")
                    .head,
                applied.head
            );
        }
        assert_eq!(
            read_document(&connection, "material")
                .expect("read rolled-back document")
                .head,
            before.head
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM revisions", [], |row| row
                    .get::<_, i64>(0))
                .expect("count rolled-back checkpoints"),
            0
        );
    }

    #[test]
    fn capability_rejects_an_exact_head_conflict_without_checkpointing() {
        let mut connection = test_connection();
        let before = seed_document(&connection, &test_body("before"));
        let target = MaterialTarget {
            document_id: "material".into(),
            title: before.title.clone(),
            kind: before.kind.clone(),
            body: test_body("conflicting"),
            expected: Some(Head {
                document_id: before.head.document_id.clone(),
                version: before.head.version.clone(),
                body_hash: "0".repeat(64),
            }),
        };
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin immediate transaction");
        let error = ApprovedMaterialWrite::new(&tx, &target, MaterialAdoptionFlow::Chat)
            .apply()
            .expect_err("stale head must be rejected");
        assert_eq!(error.code, "VersionConflict");
        assert_eq!(error.current_head, Some(before.head.clone()));
        assert_eq!(
            read_document(&tx, "material")
                .expect("read unchanged document")
                .head,
            before.head
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM revisions", [], |row| row
                .get::<_, i64>(0))
                .expect("count checkpoints"),
            0
        );
    }

    #[test]
    fn capability_rejects_invalid_target_before_database_write() {
        let mut connection = test_connection();
        let target = MaterialTarget {
            document_id: "chapter".into(),
            title: "Chapter".into(),
            kind: "chapter".into(),
            body: blank_document(),
            expected: None,
        };
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin immediate transaction");
        let error = ApprovedMaterialWrite::new(&tx, &target, MaterialAdoptionFlow::Workshop)
            .apply()
            .expect_err("chapter must be rejected");
        assert_eq!(error.code, "InvalidDocument");
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM documents", [], |row| row
                .get::<_, i64>(0))
                .expect("count documents"),
            0
        );
    }
}
