//! Durable author guidance for the Story Context Engine.
//!
//! Guidance is an explicit author decision. Each mutation appends an
//! immutable version and advances a small mutable head; it never changes a
//! manuscript body or creates canon. Request-scoped guidance is consumed only
//! when a later request successfully binds it to a frozen snapshot.
use super::*;
use crate::context::guidance::{FrozenGuidance, GuidanceScope, GuidanceVersion};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const MAX_GUIDANCE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveGuidance {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub guidance_id: String,
    pub expected_version: String,
    pub text: String,
    pub scope: GuidanceScope,
    pub document_id: Option<String>,
    pub active: bool,
    pub origin_message_id: Option<String>,
}

pub(super) enum GuidanceCommand {
    Save(SaveGuidance, Reply<GuidanceVersion>),
    List(ProjectAccess, String, Reply<Vec<GuidanceVersion>>),
}

impl ProjectSession {
    pub fn save_guidance(&self, request: SaveGuidance) -> CoreResult<GuidanceVersion> {
        self.request(|reply| Command::Guidance(Box::new(GuidanceCommand::Save(request, reply))))
    }

    /// Return active guidance applicable to the selected document. Request
    /// guidance already consumed by a successful request is excluded: it is
    /// historical context, not a promise to use it again.
    pub fn guidance(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<Vec<GuidanceVersion>> {
        self.request(|reply| {
            Command::Guidance(Box::new(GuidanceCommand::List(access, document_id, reply)))
        })
    }
}

impl OwnedProject {
    pub(super) fn handle_guidance(&mut self, command: GuidanceCommand) {
        match command {
            GuidanceCommand::Save(request, reply) => {
                let result = self.save_guidance(request);
                self.fence_uncertain(&result);
                let _ = reply.send(result);
            }
            GuidanceCommand::List(access, document_id, reply) => {
                let _ = reply
                    .send(self.check_access(&access).and_then(|()| {
                        read_guidance(self.db()?, &access.project_id, &document_id)
                    }));
            }
        }
    }

    fn save_guidance(&mut self, request: SaveGuidance) -> CoreResult<GuidanceVersion> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        let expected = parse_version(&request.expected_version)?;
        check_id(&request.guidance_id)?;
        validate_text(&request.text)?;
        validate_scope(self.db()?, &request)?;
        validate_origin(self.db()?, &request)?;

        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if let Some(stored) = existing_receipt(&tx, &request, &payload)? {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(stored);
        }

        let guidance_id = request.guidance_id.clone();
        let current = read_head(&tx, &guidance_id)?;
        if expected == 0 {
            if current.is_some() {
                return Err(CoreError::new(
                    "GuidanceConflict",
                    "This guidance ID already has a current version.",
                ));
            }
        } else {
            let current = current.as_ref().ok_or_else(|| {
                CoreError::new(
                    "GuidanceVersionConflict",
                    "The requested guidance version is not available.",
                )
            })?;
            if current.version != expected {
                return Err(CoreError::new(
                    "GuidanceVersionConflict",
                    format!(
                        "The guidance changed; expected version {}, current version {}.",
                        request.expected_version, current.version
                    )
                    .as_str(),
                ));
            }
        }

        if let Some(current) = current.as_ref()
            && current.scope == request.scope
            && current.document_id == request.document_id
            && current.text == request.text
            && current.active == request.active
            && current.origin_message_id == request.origin_message_id
        {
            let stored = head_to_version(current.clone())?;
            insert_receipt(&tx, &request, &payload, &stored)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(stored);
        }

        let version = match current.as_ref().map(|head| head.version) {
            None => 1,
            Some(version) => version.checked_add(1).ok_or_else(|| {
                CoreError::new("InvalidRequest", "The guidance version is exhausted.")
            })?,
        };
        let version_id = new_id();
        let text_hash = super::sha256_hex(request.text.as_bytes());
        let stored = insert_version(
            &tx,
            &request,
            &guidance_id,
            &version_id,
            version,
            &text_hash,
        )?;
        if current.is_some() {
            let changed = tx.execute(
                "UPDATE author_guidance_heads SET current_version_id=? WHERE guidance_id=?",
                params![version_id, guidance_id],
            )?;
            if changed != 1 {
                return Err(CoreError::new(
                    "GuidanceVersionConflict",
                    "The guidance changed before the new version was committed.",
                ));
            }
        } else {
            tx.execute(
                "INSERT INTO author_guidance_heads(guidance_id,current_version_id) VALUES(?,?)",
                params![guidance_id, version_id],
            )?;
        }

        // Every changed adoption/edit/retire advances freshness. The exact
        // no-op path returned above deliberately leaves the epoch unchanged.
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        insert_receipt(&tx, &request, &payload, &stored)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(stored)
    }
}

#[derive(Debug, Clone)]
struct GuidanceHead {
    guidance_id: String,
    version_id: String,
    version: i64,
    scope: GuidanceScope,
    document_id: Option<String>,
    text: String,
    text_hash: String,
    active: bool,
    origin_message_id: Option<String>,
    created_at: String,
}

type GuidanceRow = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    String,
    String,
    i64,
    Option<String>,
    String,
);

fn validate_text(text: &str) -> CoreResult<()> {
    if text.trim().is_empty() || text.len() > MAX_GUIDANCE_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "Guidance must be nonempty and at most 16 KiB of UTF-8 text.",
        ));
    }
    Ok(())
}

fn validate_scope(db: &Connection, request: &SaveGuidance) -> CoreResult<()> {
    match request.scope {
        GuidanceScope::Project => {
            if request.document_id.is_some() {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Project guidance cannot target a document.",
                ));
            }
        }
        GuidanceScope::Document | GuidanceScope::Request => {
            let document_id = request.document_id.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidRequest",
                    "Document and request guidance require a document target.",
                )
            })?;
            check_id(document_id)?;
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0)",
                [document_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::new(
                    "DocumentNotFound",
                    "The guidance target document is not available in this project.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_origin(db: &Connection, request: &SaveGuidance) -> CoreResult<()> {
    let Some(message_id) = request.origin_message_id.as_deref() else {
        return Ok(());
    };
    check_id(message_id)?;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT m.id,t.document_id
             FROM discussion_messages m JOIN discussion_threads t ON t.id=m.thread_id
             WHERE m.id=?",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((_, thread_document)) = row else {
        return Err(CoreError::new(
            "GuidanceOriginNotFound",
            "The guidance provenance message is not available in this project.",
        ));
    };
    if matches!(
        request.scope,
        GuidanceScope::Document | GuidanceScope::Request
    ) && request.document_id.as_deref() != Some(thread_document.as_str())
    {
        return Err(CoreError::new(
            "GuidanceOriginMismatch",
            "Document-scoped guidance provenance must belong to its target thread.",
        ));
    }
    Ok(())
}

fn scope_as_str(scope: GuidanceScope) -> &'static str {
    match scope {
        GuidanceScope::Request => "request",
        GuidanceScope::Document => "document",
        GuidanceScope::Project => "project",
    }
}

fn parse_scope(value: &str) -> CoreResult<GuidanceScope> {
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

fn head_to_version(head: GuidanceHead) -> CoreResult<GuidanceVersion> {
    check_id(&head.guidance_id)?;
    check_id(&head.version_id)?;
    if head.version < 1 || !valid_guidance_hash(&head.text_hash) {
        return Err(CoreError::new(
            "InvalidProject",
            "The project contains invalid guidance version metadata.",
        ));
    }
    validate_text(&head.text)?;
    if super::sha256_hex(head.text.as_bytes()) != head.text_hash {
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

fn read_head(db: &Connection, guidance_id: &str) -> CoreResult<Option<GuidanceHead>> {
    check_id(guidance_id)?;
    let row: Option<GuidanceRow> = db
        .query_row(
            "SELECT h.guidance_id,v.version_id,v.guidance_id,v.version,v.scope,
                    v.document_id,v.text,v.text_hash,v.active,v.origin_message_id,v.created_at
             FROM author_guidance_heads h JOIN author_guidance_versions v ON v.version_id=h.current_version_id
             WHERE h.guidance_id=?",
            [guidance_id],
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?
            )),
        )
        .optional()?;
    row.map(|row| {
        if row.0 != row.2 {
            return Err(CoreError::new(
                "InvalidProject",
                "The guidance head and version identify different records.",
            ));
        }
        Ok(GuidanceHead {
            guidance_id: row.0,
            version_id: row.1,
            version: row.3,
            scope: parse_scope(&row.4)?,
            document_id: row.5,
            text: row.6,
            text_hash: row.7,
            active: row.8 != 0,
            origin_message_id: row.9,
            created_at: row.10,
        })
    })
    .transpose()
}

fn insert_version(
    tx: &Connection,
    request: &SaveGuidance,
    guidance_id: &str,
    version_id: &str,
    version: i64,
    text_hash: &str,
) -> CoreResult<GuidanceVersion> {
    tx.execute(
        "INSERT INTO author_guidance_versions(version_id,guidance_id,version,scope,document_id,text,text_hash,active,origin_message_id)
         VALUES(?,?,?,?,?,?,?,?,?)",
        params![
            version_id,
            guidance_id,
            version,
            scope_as_str(request.scope),
            request.document_id,
            request.text,
            text_hash,
            i64::from(request.active),
            request.origin_message_id,
        ],
    )?;
    let head = tx.query_row(
        "SELECT guidance_id,version_id,version,scope,document_id,text,text_hash,active,origin_message_id,created_at
         FROM author_guidance_versions WHERE version_id=?",
        [version_id],
        |row| Ok(GuidanceHead {
            guidance_id: row.get(0)?, version_id: row.get(1)?, version: row.get(2)?,
            scope: parse_scope(&row.get::<_, String>(3)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
            document_id: row.get(4)?, text: row.get(5)?, text_hash: row.get(6)?, active: row.get::<_, i64>(7)? != 0,
            origin_message_id: row.get(8)?, created_at: row.get(9)?,
        }),
    )?;
    head_to_version(head)
}

fn existing_receipt(
    tx: &Connection,
    request: &SaveGuidance,
    payload: &str,
) -> CoreResult<Option<GuidanceVersion>> {
    let found: Option<(String, String, String, String)> = tx
        .query_row(
            "SELECT project_id,payload_hash,operation_kind,result_json FROM author_guidance_receipts
             WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    found
        .map(|(project_id, stored, kind, result)| {
            if project_id != request.access.project_id
                || stored != payload
                || kind != "saveGuidance"
            {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This operation ID was already used for another guidance request.",
                ));
            }
            let result: GuidanceVersion = serde_json::from_str(&result).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    format!("The guidance receipt is invalid: {error}").as_str(),
                )
            })?;
            let persisted = read_version(tx, &result.version_id)?;
            if persisted != result {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The guidance receipt does not match its immutable version.",
                ));
            }
            Ok(result)
        })
        .transpose()
}

fn insert_receipt(
    tx: &Connection,
    request: &SaveGuidance,
    payload: &str,
    result: &GuidanceVersion,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO author_guidance_receipts(operation_namespace,operation_id,project_id,payload_hash,operation_kind,result_json)
         VALUES(?,?,?,?,?,?)",
        params![
            request.access.operation_namespace,
            request.operation_id,
            request.access.project_id,
            payload,
            "saveGuidance",
            serde_json::to_string(result)?,
        ],
    )?;
    Ok(())
}

fn valid_guidance_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_version(db: &Connection, version_id: &str) -> CoreResult<GuidanceVersion> {
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

fn read_guidance(
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

fn target_document_id(db: &Connection, snapshot_id: &str) -> CoreResult<String> {
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

fn ensure_snapshot(
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
pub(super) fn select_guidance_at(
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

/// Reuse only exact, still-active one-use instructions from the validated
/// preceding attempt. New request guidance stays available for the next new
/// request. The original consumption receipt remains the single use record.
pub(super) fn retry_request_guidance_at(
    db: &Connection,
    access: &ProjectAccess,
    frozen: &story_context::FrozenContext,
) -> CoreResult<Vec<FrozenGuidance>> {
    let mut reused = Vec::new();
    for record in &frozen.guidance {
        if record.version.scope != GuidanceScope::Request {
            continue;
        }
        let current = read_head(db, &record.version.guidance_id)?;
        if !current.is_some_and(|head| head.active && head.version_id == record.version.version_id)
        {
            return Err(CoreError::new(
                "RetryGuidanceChanged",
                "Guidance for this attempt was edited or retired. Start a new request with the current guidance.",
            ));
        }
        let valid_use: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM guidance_request_uses u
             JOIN story_snapshots s ON s.id=u.snapshot_id
             JOIN snapshot_guidance g ON g.snapshot_id=s.id AND g.version_id=u.version_id
             WHERE u.version_id=? AND s.project_id=? AND s.operation_namespace=?)",
            params![
                record.version.version_id,
                access.project_id,
                access.operation_namespace
            ],
            |row| row.get(0),
        )?;
        if !valid_use {
            return Err(CoreError::new(
                "InvalidContext",
                "The retry instruction has no matching original request use in this project.",
            ));
        }
        reused.push(record.clone());
    }
    Ok(reused)
}

/// Persist the exact selected guidance beside a frozen story snapshot. This
/// does not consume request guidance; callers do that after packet compile.
pub(super) fn pin_guidance_at(
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

pub(super) fn consume_request_guidance_at(
    db: &Connection,
    snapshot_id: &str,
    records: &[FrozenGuidance],
) -> CoreResult<()> {
    let project_id = ensure_snapshot(db, snapshot_id, None)?;
    let target_document = target_document_id(db, snapshot_id)?;
    for record in records {
        if record.version.scope == GuidanceScope::Request {
            if record.project_id != project_id {
                return Err(CoreError::new(
                    "InvalidContext",
                    "Request guidance belongs to another project.",
                ));
            }
            validate_version_for_project(db, &project_id, &target_document, &record.version)?;
            let pinned: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshot_guidance WHERE snapshot_id=? AND handle=? AND version_id=? AND text_hash=?)",
                params![snapshot_id, record.handle, record.version.version_id, record.version.text_hash],
                |row| row.get(0),
            )?;
            if !pinned {
                return Err(CoreError::new(
                    "InvalidContext",
                    "Request guidance must be pinned before it is consumed.",
                ));
            }
            consume_one(db, snapshot_id, &target_document, &record.version)?;
        }
    }
    Ok(())
}

fn consume_one(
    db: &Connection,
    snapshot_id: &str,
    target_document: &str,
    version: &GuidanceVersion,
) -> CoreResult<()> {
    if version.document_id.as_deref() != Some(target_document) {
        return Err(CoreError::new(
            "GuidanceTargetMismatch",
            "Request guidance must target the snapshot document.",
        ));
    }
    let changed = db.execute(
        "INSERT INTO guidance_request_uses(version_id,snapshot_id)
         SELECT ?,? WHERE NOT EXISTS(SELECT 1 FROM guidance_request_uses WHERE version_id=?)",
        params![version.version_id, snapshot_id, version.version_id],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "GuidanceAlreadyUsed",
            "This request guidance was already consumed by another request.",
        ));
    }
    Ok(())
}

/// Verify that the supplied historical guidance records exactly match the
/// snapshot pins and the immutable source rows. This is used before reading a
/// packet or presenting a context inspector.
pub(super) fn validate_guidance_at(
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

/// Validate the guidance tables while creating or recovering a backup. This
/// checks internal row integrity without requiring historical snapshots to
/// belong to the current project identity: recovery retains authored guidance
/// rows alongside old snapshots and receipts as history.
pub(crate) fn validate_guidance_storage(db: &Connection) -> CoreResult<()> {
    let mut versions = db.prepare(
        "SELECT guidance_id,version_id,version,scope,document_id,text,text_hash,active,origin_message_id,created_at
         FROM author_guidance_versions ORDER BY version_id",
    )?;
    let rows = versions.query_map([], |row| {
        Ok(GuidanceHead {
            guidance_id: row.get(0)?,
            version_id: row.get(1)?,
            version: row.get(2)?,
            scope: parse_scope(&row.get::<_, String>(3)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            document_id: row.get(4)?,
            text: row.get(5)?,
            text_hash: row.get(6)?,
            active: row.get::<_, i64>(7)? != 0,
            origin_message_id: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;
    for row in rows {
        let version = row?;
        let document_id = version.document_id.clone();
        let origin_message_id = version.origin_message_id.clone();
        let _version = head_to_version(version)?;
        if let Some(document_id) = document_id {
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?)",
                [document_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A guidance version targets an unknown document.",
                ));
            }
        }
        if let Some(message_id) = origin_message_id {
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM discussion_messages WHERE id=?)",
                [message_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A guidance version references an unknown provenance message.",
                ));
            }
        }
    }
    let mut heads = db.prepare(
        "SELECT guidance_id,current_version_id FROM author_guidance_heads ORDER BY guidance_id",
    )?;
    let rows = heads.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (guidance_id, version_id) = row?;
        check_id(&guidance_id)?;
        check_id(&version_id)?;
        let actual: Option<String> = db
            .query_row(
                "SELECT guidance_id FROM author_guidance_versions WHERE version_id=?",
                [version_id],
                |row| row.get(0),
            )
            .optional()?;
        if actual != Some(guidance_id) {
            return Err(CoreError::new(
                "InvalidProject",
                "A guidance head does not point to its project's version.",
            ));
        }
    }
    let mut receipts = db.prepare(
        "SELECT operation_namespace,operation_id,project_id,payload_hash,operation_kind,result_json
         FROM author_guidance_receipts",
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
        let (namespace, operation, project_id, payload, kind, result) = row?;
        check_id(&namespace)?;
        check_id(&operation)?;
        check_id(&project_id)?;
        if kind != "saveGuidance" || !valid_guidance_hash(&payload) {
            return Err(CoreError::new(
                "InvalidProject",
                "A guidance operation receipt has invalid metadata.",
            ));
        }
        let result: GuidanceVersion = serde_json::from_str(&result)?;
        head_to_version(GuidanceHead {
            guidance_id: result.guidance_id,
            version_id: result.version_id,
            version: parse_version(&result.version)?,
            scope: result.scope,
            document_id: result.document_id,
            text: result.text,
            text_hash: result.text_hash,
            active: result.active,
            origin_message_id: result.origin_message_id,
            created_at: result.created_at,
        })?;
    }
    let mut pins =
        db.prepare("SELECT snapshot_id,version_id,handle,text_hash FROM snapshot_guidance")?;
    let rows = pins.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (snapshot_id, version_id, handle, text_hash) = row?;
        check_id(&snapshot_id)?;
        check_id(&version_id)?;
        if handle != format!("guidance-{version_id}") || !valid_guidance_hash(&text_hash) {
            return Err(CoreError::new(
                "InvalidProject",
                "A snapshot guidance pin has invalid metadata.",
            ));
        }
        let actual: Option<String> = db
            .query_row(
                "SELECT text_hash FROM author_guidance_versions WHERE version_id=?",
                [version_id],
                |row| row.get(0),
            )
            .optional()?;
        if actual != Some(text_hash) {
            return Err(CoreError::new(
                "InvalidProject",
                "A snapshot guidance pin does not match its version.",
            ));
        }
    }
    let mut uses = db.prepare("SELECT version_id,snapshot_id FROM guidance_request_uses")?;
    let rows = uses.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (version_id, snapshot_id) = row?;
        let scope: Option<String> = db
            .query_row(
                "SELECT scope FROM author_guidance_versions WHERE version_id=?",
                [version_id],
                |row| row.get(0),
            )
            .optional()?;
        if scope.as_deref() != Some("request") {
            return Err(CoreError::new(
                "InvalidProject",
                "A request guidance use points to a non-request version.",
            ));
        }
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_snapshots WHERE id=?)",
            [snapshot_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::new(
                "InvalidProject",
                "A request guidance use points to an unknown snapshot.",
            ));
        }
    }
    Ok(())
}

fn validate_version_for_project(
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
