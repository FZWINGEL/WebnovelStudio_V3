//! Explicit author instructions are immutable records, never manuscript revisions.
//!
//! # Two halves, one seam
//!
//! This module holds the **frozen half**: the vocabulary a packet carries, and
//! the operations that select, pin and validate guidance *at a story snapshot*.
//! The **authoring half** — heads, immutable versions, consumption receipts, and
//! the commands that append them — stays in `webnovel-core` and is destined for
//! `wns-conversation`.
//!
//! The seam is where the cycle was. `story_context` pins guidance into every
//! snapshot it freezes, so it calls [`select_guidance_at`], [`pin_guidance_at`]
//! and [`validate_guidance_at`]; the authoring half calls back for
//! `story_context::FrozenContext`. Both halves were in `webnovel-core`, so the
//! pair could never be separated into crates — `story_context` is destined for
//! `wns-story` (L4) and guidance authoring for `wns-conversation` (L5), and the
//! call in the L5→L4 direction is legal while the L4→L5 one is not.
//!
//! Moving the frozen half here puts it at L3, below both. The upward call is
//! gone, the downward one is unchanged, and neither module needs a host trait
//! it did not already have. Moving one half of a cycle down is cheaper than
//! moving both modules together, and it does not put guidance in a crate whose
//! stated concern is the conversation that authors it.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use wns_kernel::{CoreError, CoreResult, check_id, sha256_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum GuidanceScope {
    Request,
    Document,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuidanceVersion {
    pub guidance_id: String,
    pub version_id: String,
    pub version: String,
    pub scope: GuidanceScope,
    pub document_id: Option<String>,
    pub text: String,
    pub text_hash: String,
    pub active: bool,
    pub origin_message_id: Option<String>,
    pub created_at: String,
}

/// An exact author-room instruction selected when a request freezes. The
/// referenced version also survives in its own authoritative local store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenGuidance {
    pub handle: String,
    pub project_id: String,
    pub version: GuidanceVersion,
}

pub fn validate_frozen_guidance(
    records: &[FrozenGuidance],
    project_id: &str,
    document_id: &str,
    audience: super::Audience,
) -> Result<(), String> {
    if !records.is_empty() && audience != super::Audience::AuthorRoom {
        return Err("Author-room guidance cannot enter a restricted writing request.".into());
    }
    let mut versions = std::collections::HashSet::new();
    let mut heads = std::collections::HashSet::new();
    for record in records {
        let version = &record.version;
        let counter = version.version.parse::<i64>().ok();
        if record.project_id != project_id
            || record.handle != format!("guidance-{}", version.version_id)
            || !versions.insert(&version.version_id)
            || !heads.insert(&version.guidance_id)
            || !version.active
            || !counter.is_some_and(|value| value > 0 && value.to_string() == version.version)
            || version.guidance_id.is_empty()
            || version.version_id.is_empty()
            || version.text.trim().is_empty()
            || version.text.len() > 16_384
            || wns_kernel::sha256_hex(version.text.as_bytes()) != version.text_hash
        {
            return Err(
                "The frozen author guidance has an invalid identity, version, or text hash.".into(),
            );
        }
        match version.scope {
            GuidanceScope::Project if version.document_id.is_none() => {}
            GuidanceScope::Document | GuidanceScope::Request
                if version.document_id.as_deref() == Some(document_id) => {}
            _ => return Err("The author guidance does not apply to this document.".into()),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The frozen half: row conversion, reads, and snapshot selection.
//
// Moved down from `webnovel-core::projects::guidance`. The authoring half still
// reaches for the conversion helpers below — legal, because core (and later
// `wns-conversation` at L5) sits above this crate.
// ---------------------------------------------------------------------------

pub const MAX_GUIDANCE_BYTES: usize = 16 * 1024;

/// One `author_guidance_versions` row, before validation.
///
/// The row's `scope` arrives as text and its `version` as an integer; both are
/// converted by [`head_to_version`], which is also where every identity,
/// fingerprint and target rule is enforced. Nothing else may construct a
/// [`GuidanceVersion`] from a row.
#[derive(Debug, Clone, specta::Type)]
pub struct GuidanceHead {
    pub guidance_id: String,
    pub version_id: String,
    pub version: i64,
    pub scope: GuidanceScope,
    pub document_id: Option<String>,
    pub text: String,
    pub text_hash: String,
    pub active: bool,
    pub origin_message_id: Option<String>,
    pub created_at: String,
}

pub fn validate_text(text: &str) -> CoreResult<()> {
    if text.trim().is_empty() || text.len() > MAX_GUIDANCE_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "Guidance must be nonempty and at most 16 KiB of UTF-8 text.",
        ));
    }
    Ok(())
}

pub fn parse_scope(value: &str) -> CoreResult<GuidanceScope> {
    match value {
        "request" => Ok(GuidanceScope::Request),
        "document" => Ok(GuidanceScope::Document),
        "project" => Ok(GuidanceScope::Project),
        _ => Err(CoreError::new(
            "InvalidProject",
            "The project contains an invalid guidance scope.",
        )),
    }
}

pub fn head_to_version(head: GuidanceHead) -> CoreResult<GuidanceVersion> {
    check_id(&head.guidance_id)?;
    check_id(&head.version_id)?;
    if head.version < 1 || !valid_guidance_hash(&head.text_hash) {
        return Err(CoreError::new(
            "InvalidProject",
            "The project contains invalid guidance version metadata.",
        ));
    }
    validate_text(&head.text)?;
    if sha256_hex(head.text.as_bytes()) != head.text_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The guidance text does not match its fingerprint.",
        ));
    }
    match head.scope {
        GuidanceScope::Project if head.document_id.is_some() => {
            return Err(CoreError::new(
                "InvalidProject",
                "Project guidance contains a document target.",
            ));
        }
        GuidanceScope::Document | GuidanceScope::Request if head.document_id.is_none() => {
            return Err(CoreError::new(
                "InvalidProject",
                "Document guidance is missing its target.",
            ));
        }
        _ => {}
    }
    for id in head.document_id.iter().chain(head.origin_message_id.iter()) {
        check_id(id)?;
    }
    Ok(GuidanceVersion {
        guidance_id: head.guidance_id,
        version_id: head.version_id,
        version: head.version.to_string(),
        scope: head.scope,
        document_id: head.document_id,
        text: head.text,
        text_hash: head.text_hash,
        active: head.active,
        origin_message_id: head.origin_message_id,
        created_at: head.created_at,
    })
}

pub fn valid_guidance_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn read_version(db: &Connection, version_id: &str) -> CoreResult<GuidanceVersion> {
    check_id(version_id)?;
    let head = db.query_row(
        "SELECT guidance_id,version_id,version,scope,document_id,text,text_hash,active,origin_message_id,created_at
         FROM author_guidance_versions WHERE version_id=?",
        [version_id],
        |row| {
            Ok(GuidanceHead {
                guidance_id: row.get(0)?,
                version_id: row.get(1)?,
                version: row.get(2)?,
                scope: parse_scope(&row.get::<_, String>(3)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                document_id: row.get(4)?,
                text: row.get(5)?,
                text_hash: row.get(6)?,
                active: row.get::<_, i64>(7)? != 0,
                origin_message_id: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    )?;
    head_to_version(head)
}

pub fn read_guidance(
    db: &Connection,
    project_id: &str,
    document_id: &str,
) -> CoreResult<Vec<GuidanceVersion>> {
    check_id(project_id)?;
    check_id(document_id)?;
    let mut statement = db.prepare(
        "SELECT h.guidance_id,v.version_id,v.guidance_id,v.version,v.scope,
                v.document_id,v.text,v.text_hash,v.active,v.origin_message_id,v.created_at
         FROM author_guidance_heads h JOIN author_guidance_versions v ON v.version_id=h.current_version_id
         WHERE h.guidance_id=v.guidance_id AND v.active=1
           AND ((v.scope='project' AND v.document_id IS NULL)
             OR (v.scope IN ('document','request') AND v.document_id=?
                 AND NOT (v.scope='request' AND EXISTS(SELECT 1 FROM guidance_request_uses u WHERE u.version_id=v.version_id))))
         ORDER BY CASE v.scope WHEN 'project' THEN 0 WHEN 'document' THEN 1 ELSE 2 END,
                  h.guidance_id",
    )?;
    let rows = statement.query_map([document_id], |row| {
        Ok(GuidanceHead {
            guidance_id: row.get(0)?,
            version_id: row.get(1)?,
            version: row.get(3)?,
            scope: parse_scope(&row.get::<_, String>(4)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            document_id: row.get(5)?,
            text: row.get(6)?,
            text_hash: row.get(7)?,
            active: row.get::<_, i64>(8)? != 0,
            origin_message_id: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(head_to_version(row?)?);
    }
    Ok(result)
}

pub fn target_document_id(db: &Connection, snapshot_id: &str) -> CoreResult<String> {
    let manifest: String = db.query_row(
        "SELECT manifest_json FROM story_snapshots WHERE id=?",
        [snapshot_id],
        |row| row.get(0),
    )?;
    let value: Value = serde_json::from_str(&manifest)?;
    value
        .pointer("/snapshot/target/documentId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| CoreError::new("InvalidContext", "The snapshot has no target document."))
}

pub fn ensure_snapshot(
    db: &Connection,
    snapshot_id: &str,
    project_id: Option<&str>,
) -> CoreResult<String> {
    check_id(snapshot_id)?;
    let owner: Option<(String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace FROM story_snapshots WHERE id=?",
            [snapshot_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((owner, _)) = owner else {
        return Err(CoreError::new(
            "ContextNotFound",
            "The story snapshot is not available.",
        ));
    };
    if let Some(project_id) = project_id
        && owner != project_id
    {
        return Err(CoreError::new(
            "ContextProjectMismatch",
            "The guidance snapshot belongs to another project.",
        ));
    }
    Ok(owner)
}

/// Select the currently applicable guidance for a frozen target. Request
/// guidance is returned only until a successful request consumes it.
pub fn select_guidance_at(
    db: &Connection,
    project_id: &str,
    document_id: &str,
    include_request: bool,
) -> CoreResult<Vec<FrozenGuidance>> {
    let rows = read_guidance(db, project_id, document_id)?;
    let mut records = Vec::with_capacity(rows.len());
    for version in rows {
        if !include_request && version.scope == GuidanceScope::Request {
            continue;
        }
        records.push(FrozenGuidance {
            handle: format!("guidance-{}", version.version_id),
            project_id: project_id.to_owned(),
            version,
        });
    }
    Ok(records)
}

/// Persist the exact selected guidance beside a frozen story snapshot. This
/// does not consume request guidance; callers do that after packet compile.
pub fn pin_guidance_at(
    db: &Connection,
    snapshot_id: &str,
    records: &[FrozenGuidance],
) -> CoreResult<()> {
    let project_id = ensure_snapshot(db, snapshot_id, None)?;
    let target_document = target_document_id(db, snapshot_id)?;
    let mut handles = HashSet::new();
    let mut versions = HashSet::new();
    for record in records {
        if record.project_id != project_id
            || record.handle != format!("guidance-{}", record.version.version_id)
            || !handles.insert(record.handle.clone())
            || !versions.insert(record.version.version_id.clone())
        {
            return Err(CoreError::new(
                "InvalidContext",
                "The selected guidance contains a duplicate or foreign record.",
            ));
        }
        validate_version_for_project(db, &project_id, &target_document, &record.version)?;
        db.execute(
            "INSERT INTO snapshot_guidance(snapshot_id,version_id,handle,text_hash) VALUES(?,?,?,?)",
            params![snapshot_id, record.version.version_id, record.handle, record.version.text_hash],
        )?;
    }
    Ok(())
}

pub fn validate_guidance_at(
    db: &Connection,
    snapshot_id: &str,
    project_id: &str,
    records: &[FrozenGuidance],
) -> CoreResult<()> {
    ensure_snapshot(db, snapshot_id, Some(project_id))?;
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM snapshot_guidance WHERE snapshot_id=?",
        [snapshot_id],
        |row| row.get(0),
    )?;
    if count != records.len() as i64 {
        return Err(CoreError::new(
            "InvalidContext",
            "The frozen guidance pin set is incomplete.",
        ));
    }
    let mut seen = HashSet::new();
    let target = target_document_id(db, snapshot_id)?;
    for record in records {
        if record.project_id != project_id || !seen.insert(record.handle.clone()) {
            return Err(CoreError::new(
                "InvalidContext",
                "The guidance pin set contains a foreign record or duplicates.",
            ));
        }
        validate_version_for_project(db, project_id, &target, &record.version)?;
        let expected: Option<(String, String)> = db
            .query_row(
                "SELECT version_id,text_hash FROM snapshot_guidance WHERE snapshot_id=? AND handle=?",
                params![snapshot_id, record.handle],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if expected
            != Some((
                record.version.version_id.clone(),
                record.version.text_hash.clone(),
            ))
        {
            return Err(CoreError::new(
                "InvalidContext",
                "A frozen guidance record does not match its snapshot pin.",
            ));
        }
    }
    Ok(())
}

pub fn validate_version_for_project(
    db: &Connection,
    _project_id: &str,
    target_document: &str,
    expected: &GuidanceVersion,
) -> CoreResult<()> {
    let actual = db
        .query_row(
            "SELECT guidance_id,version_id,version,scope,document_id,text,text_hash,active,origin_message_id,created_at
             FROM author_guidance_versions WHERE version_id=?",
            [expected.version_id.as_str()],
            |row| Ok(GuidanceHead {
                guidance_id: row.get(0)?, version_id: row.get(1)?, version: row.get(2)?,
                scope: parse_scope(&row.get::<_, String>(3)?).map_err(|_| rusqlite::Error::InvalidQuery)?, document_id: row.get(4)?,
                text: row.get(5)?, text_hash: row.get(6)?, active: row.get::<_, i64>(7)? != 0, origin_message_id: row.get(8)?, created_at: row.get(9)?,
            }),
        )
        .optional()?
        .ok_or_else(|| CoreError::new("InvalidContext", "A pinned guidance version is unavailable."))?;
    let actual = head_to_version(actual)?;
    if actual != *expected {
        return Err(CoreError::new(
            "InvalidContext",
            "A pinned guidance version failed its exact identity check.",
        ));
    }
    if matches!(
        expected.scope,
        GuidanceScope::Document | GuidanceScope::Request
    ) && expected.document_id.as_deref() != Some(target_document)
    {
        return Err(CoreError::new(
            "GuidanceTargetMismatch",
            "The guidance target does not match the snapshot document.",
        ));
    }
    Ok(())
}
