//! Durable author guidance for the Story Context Engine.
//!
//! Guidance is an explicit author decision. Each mutation appends an
//! immutable version and advances a small mutable head; it never changes a
//! manuscript body or creates canon. Request-scoped guidance is consumed only
//! when a later request successfully binds it to a frozen snapshot.
use wns_kernel::{
    CoreError, CoreResult, ProjectAccess, Reply, check_id, logical_hash, new_id, parse_version,
    sha256_hex,
};
use wns_story::host::StoryHost;
use wns_story::story_context;
// The frozen half of guidance — row conversion, reads, and selection at a
// snapshot — moved down to wns-context (L3), which is what lets `story_context`
// reach it without an upward call. The authoring half below still uses the
// conversion helpers, which is a legal downward edge.
use wns_context::guidance::{
    FrozenGuidance, GuidanceHead, GuidanceScope, GuidanceVersion, ensure_snapshot, head_to_version,
    parse_scope, read_guidance, read_version, target_document_id, valid_guidance_hash,
    validate_text, validate_version_for_project,
};
// Re-exported rather than privately imported: `story_context` pins guidance into
// every snapshot it freezes and calls these three by path.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

pub enum GuidanceCommand {
    Save(SaveGuidance, Reply<GuidanceVersion>),
    List(ProjectAccess, String, Reply<Vec<GuidanceVersion>>),
}

// Actor-side logic, as free functions over `StoryHost`.

pub fn handle_guidance(host: &mut impl StoryHost, command: GuidanceCommand) {
    match command {
        GuidanceCommand::Save(request, reply) => {
            let result = save_guidance(host, request);
            host.fence_uncertain(&result);
            let _ = reply.send(result);
        }
        GuidanceCommand::List(access, document_id, reply) => {
            let _ =
                reply
                    .send(host.check_access(&access).and_then(|()| {
                        read_guidance(host.db()?, &access.project_id, &document_id)
                    }));
        }
    }
}

pub fn save_guidance(
    host: &mut impl StoryHost,
    request: SaveGuidance,
) -> CoreResult<GuidanceVersion> {
    host.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    let expected = parse_version(&request.expected_version)?;
    check_id(&request.guidance_id)?;
    validate_text(&request.text)?;
    validate_scope(host.db()?, &request)?;
    validate_origin(host.db()?, &request)?;

    let payload = logical_hash(&request)?;
    let tx = host
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
    let text_hash = sha256_hex(request.text.as_bytes());
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
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0 AND role='ordinary')",
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

/// Reuse only exact, still-active one-use instructions from the validated
/// preceding attempt. New request guidance stays available for the next new
/// request. The original consumption receipt remains the single use record.
pub fn retry_request_guidance_at(
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
pub fn consume_request_guidance_at(
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

/// Validate the guidance tables while creating or recovering a backup. This
/// checks internal row integrity without requiring historical snapshots to
/// belong to the current project identity: recovery retains authored guidance
/// rows alongside old snapshots and receipts as history.
pub fn validate_guidance_storage(db: &Connection) -> CoreResult<()> {
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
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND role='ordinary')",
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
