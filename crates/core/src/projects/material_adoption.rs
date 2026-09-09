//! Transaction-local writes for author-approved material.
//!
//! This module deliberately does not own an operation receipt, source epoch,
//! relationship projection, or transaction boundary.  A caller validates its
//! domain-specific preview and then uses this helper while it owns the
//! `Immediate` transaction.  Keeping the document mutation here gives chat
//! adoption the same checkpoint and optimistic-head semantics as Workshop
//! adoption without making either feature an orchestrator for the other.

use super::*;
use rusqlite::{Connection, params};
use serde_json::Value;

/// A single ordinary, nonchapter document to write inside a caller-owned
/// transaction.
///
/// `expected` is `Some` for an update and `None` for an insert.  The helper
/// rechecks that identity at write time, so a preview cannot silently replace
/// a document that appeared after it was created.
#[derive(Debug, Clone)]
pub(super) struct MaterialTarget {
    pub(super) document_id: String,
    pub(super) title: String,
    pub(super) kind: String,
    pub(super) body: Value,
    pub(super) expected: Option<Head>,
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
pub(super) fn write_material_target_at(
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
}
