//! Durable AuthorRoom source preferences.
//!
//! A pin set contains document identities only.  A discussion resolves those
//! identities to the current checkpoint inside its own freeze transaction;
//! packets therefore retain exact immutable revision handles while future
//! discussions see later source edits.  Receipts are kept in a separate
//! namespace so a copied historical database cannot authorize a new write.

use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const AUTHOR_ROOM_AUDIENCE: &str = "authorRoom";
const MAX_SOURCE_DOCUMENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SourcePinScope {
    Project,
    Document,
}

impl SourcePinScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Document => "document",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePinSet {
    pub scope: SourcePinScope,
    pub target_document_id: Option<String>,
    pub version: String,
    pub source_document_ids: Vec<String>,
    pub audience: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePinsView {
    pub project: SourcePinSet,
    pub document: SourcePinSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSourcePins {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub scope: SourcePinScope,
    pub target_document_id: Option<String>,
    pub expected_version: String,
    pub source_document_ids: Vec<String>,
}

pub(super) enum SourcePinCommand {
    Read(ProjectAccess, String, Reply<SourcePinsView>),
    Save(SaveSourcePins, Reply<SourcePinSet>),
}

impl ProjectSession {
    pub fn read_source_pins(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<SourcePinsView> {
        self.request(|reply| {
            Command::SourcePins(Box::new(SourcePinCommand::Read(access, document_id, reply)))
        })
    }

    pub fn save_source_pins(&self, request: SaveSourcePins) -> CoreResult<SourcePinSet> {
        self.request(|reply| Command::SourcePins(Box::new(SourcePinCommand::Save(request, reply))))
    }
}

impl OwnedProject {
    pub(super) fn handle_source_pins(&mut self, command: SourcePinCommand) {
        match command {
            SourcePinCommand::Read(access, document_id, reply) => {
                let result = self
                    .check_access(&access)
                    .and_then(|()| read_source_pins(self.db()?, &access, &document_id));
                let _ = reply.send(result);
            }
            SourcePinCommand::Save(request, reply) => {
                let result = self.save_source_pins(request);
                self.fence_uncertain(&result);
                let _ = reply.send(result);
            }
        }
    }

    fn save_source_pins(&mut self, request: SaveSourcePins) -> CoreResult<SourcePinSet> {
        self.check_access(&request.access)?;
        validate_save_request(&request)?;
        let expected = parse_optional_version(&request.expected_version)?;
        let source_document_ids = canonical_ids(&request.source_document_ids)?;
        let payload_hash = logical_hash(&request)?;

        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = existing_receipt(&tx, &request, &payload_hash)? {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(result);
        }

        // A fresh write must resolve its target and sources while they are
        // active.  Receipt identity is checked first so a replay remains
        // idempotent after a referenced source is trashed and can be removed
        // by a later explicit edit.
        validate_source_state(&tx, &request, &source_document_ids)?;

        let target_key = target_key(request.scope, request.target_document_id.as_deref());
        let current = read_set(
            &tx,
            &request.access.project_id,
            &request.access.operation_namespace,
            request.scope,
            target_key,
        )?;
        let current_version = parse_version(&current.version)?;
        if current_version != expected {
            return Err(CoreError::new(
                "SourcePinVersionConflict",
                format!(
                    "The source list changed; expected version {}, current version {}.",
                    request.expected_version, current.version
                )
                .as_str(),
            ));
        }

        if current.source_document_ids == source_document_ids {
            insert_receipt(&tx, &request, &payload_hash, &current)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }

        let next_version = current_version.checked_add(1).ok_or_else(|| {
            CoreError::new("InvalidRequest", "The source list version is exhausted.")
        })?;
        let result = SourcePinSet {
            scope: request.scope,
            target_document_id: request.target_document_id.clone(),
            version: next_version.to_string(),
            source_document_ids,
            audience: AUTHOR_ROOM_AUDIENCE.to_owned(),
        };
        tx.execute(
            "INSERT INTO source_pin_sets(project_id,operation_namespace,scope,target_document_id,version,source_document_ids_json,audience)
             VALUES(?,?,?,?,?,?,?)
             ON CONFLICT(project_id,operation_namespace,scope,target_document_id) DO UPDATE SET
             version=excluded.version,source_document_ids_json=excluded.source_document_ids_json,audience=excluded.audience",
            params![
                request.access.project_id,
                request.access.operation_namespace,
                request.scope.as_str(),
                target_key,
                parse_version(&result.version)?,
                serde_json::to_string(&result.source_document_ids)?,
                AUTHOR_ROOM_AUDIENCE,
            ],
        )?;
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        insert_receipt(&tx, &request, &payload_hash, &result)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }
}

fn validate_save_request(request: &SaveSourcePins) -> CoreResult<()> {
    check_id(&request.operation_id)?;
    match request.scope {
        SourcePinScope::Project => {
            if request.target_document_id.is_some() {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Project source pins cannot target a document.",
                ));
            }
        }
        SourcePinScope::Document => {
            let target = request.target_document_id.as_deref().ok_or_else(|| {
                CoreError::new(
                    "InvalidRequest",
                    "Document source pins require a document target.",
                )
            })?;
            check_id(target)?;
        }
    }
    if request.source_document_ids.len() > MAX_SOURCE_DOCUMENTS {
        return Err(CoreError::new(
            "InvalidRequest",
            "A source list may contain at most 64 documents.",
        ));
    }
    parse_optional_version(&request.expected_version)?;
    Ok(())
}

fn validate_source_state(
    db: &Connection,
    request: &SaveSourcePins,
    source_document_ids: &[String],
) -> CoreResult<()> {
    if let SourcePinScope::Document = request.scope {
        let target = request.target_document_id.as_deref().ok_or_else(|| {
            CoreError::new(
                "InvalidRequest",
                "Document source pins require a document target.",
            )
        })?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0 AND role='ordinary')",
            [target],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::new(
                "DocumentNotFound",
                "The source pin target document is not available in this project.",
            ));
        }
    }
    validate_sources(db, source_document_ids)
}

fn canonical_ids(ids: &[String]) -> CoreResult<Vec<String>> {
    let mut sorted = BTreeSet::new();
    for id in ids {
        check_id(id)?;
        sorted.insert(id.clone());
    }
    Ok(sorted.into_iter().collect())
}

fn validate_sources(db: &Connection, ids: &[String]) -> CoreResult<()> {
    for id in ids {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0 AND role='ordinary')",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::new(
                "DocumentNotFound",
                "A source document is not available in this project.",
            ));
        }
    }
    Ok(())
}

fn parse_optional_version(value: &str) -> CoreResult<i64> {
    if value.is_empty() {
        Ok(0)
    } else {
        parse_version(value)
    }
}

fn target_key(scope: SourcePinScope, target: Option<&str>) -> &str {
    match scope {
        SourcePinScope::Project => "",
        SourcePinScope::Document => target.unwrap_or_default(),
    }
}

fn default_set(scope: SourcePinScope, target_document_id: Option<&str>) -> SourcePinSet {
    SourcePinSet {
        scope,
        target_document_id: target_document_id.map(str::to_owned),
        version: "0".to_owned(),
        source_document_ids: Vec::new(),
        audience: AUTHOR_ROOM_AUDIENCE.to_owned(),
    }
}

fn read_source_pins(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<SourcePinsView> {
    check_id(document_id)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0 AND role='ordinary')",
        [document_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::new(
            "DocumentNotFound",
            "The source pin document is not available in this project.",
        ));
    }
    Ok(SourcePinsView {
        project: read_set(
            db,
            &access.project_id,
            &access.operation_namespace,
            SourcePinScope::Project,
            "",
        )?,
        document: read_set(
            db,
            &access.project_id,
            &access.operation_namespace,
            SourcePinScope::Document,
            document_id,
        )?,
    })
}

/// Resolve the persistent sets used by an AuthorRoom discussion.  Restricted
/// writing requests deliberately receive an empty list; their transient
/// source selection remains the only caller-controlled pin set.
pub(super) fn persistent_for_discussion(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
    include: bool,
) -> CoreResult<Vec<String>> {
    if !include {
        return Ok(Vec::new());
    }
    let project = read_set(
        db,
        &access.project_id,
        &access.operation_namespace,
        SourcePinScope::Project,
        "",
    )?;
    let document = read_set(
        db,
        &access.project_id,
        &access.operation_namespace,
        SourcePinScope::Document,
        document_id,
    )?;
    let mut merged = BTreeSet::new();
    merged.extend(project.source_document_ids);
    merged.extend(document.source_document_ids);
    Ok(merged.into_iter().collect())
}

fn read_set(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    scope: SourcePinScope,
    target_key: &str,
) -> CoreResult<SourcePinSet> {
    let row: Option<(i64, String, String)> = db
        .query_row(
            "SELECT version,source_document_ids_json,audience FROM source_pin_sets
             WHERE project_id=? AND operation_namespace=? AND scope=? AND target_document_id=?",
            params![project_id, operation_namespace, scope.as_str(), target_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((version, json, audience)) = row else {
        return Ok(default_set(
            scope,
            (!target_key.is_empty()).then_some(target_key),
        ));
    };
    let version = parse_stored_version(version)?;
    if audience != AUTHOR_ROOM_AUDIENCE {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved source pin audience is invalid.",
        ));
    }
    let ids: Vec<String> = serde_json::from_str(&json)
        .map_err(|_| CoreError::new("InvalidProject", "The saved source pin list is invalid."))?;
    if ids.len() > MAX_SOURCE_DOCUMENTS || canonical_ids(&ids).ok().as_ref() != Some(&ids) {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved source pin list is not canonical.",
        ));
    }
    Ok(SourcePinSet {
        scope,
        target_document_id: (!target_key.is_empty()).then_some(target_key.to_owned()),
        version,
        source_document_ids: ids,
        audience,
    })
}

fn existing_receipt(
    db: &Connection,
    request: &SaveSourcePins,
    payload_hash: &str,
) -> CoreResult<Option<SourcePinSet>> {
    let found: Option<(String, String, String, String, String, String)> = db
        .query_row(
            "SELECT project_id,scope,target_document_id,payload_hash,operation_kind,result_json
             FROM source_pin_receipts WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    if let Some((project, scope, target, stored, kind, result)) = found {
        if project != request.access.project_id
            || scope != request.scope.as_str()
            || target != target_key(request.scope, request.target_document_id.as_deref())
            || stored != payload_hash
            || kind != "saveSourcePins"
        {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another source-pin request.",
            ));
        }
        let result: SourcePinSet = serde_json::from_str(&result)
            .map_err(|_| CoreError::new("InvalidProject", "The source-pin receipt is invalid."))?;
        if result.scope != request.scope
            || result.target_document_id != request.target_document_id
            || result.audience != AUTHOR_ROOM_AUDIENCE
            || canonical_ids(&request.source_document_ids)? != result.source_document_ids
            || !matches!(
                parse_version(&result.version),
                Ok(version) if version == parse_optional_version(&request.expected_version)?
                    || Some(version) == parse_optional_version(&request.expected_version)?.checked_add(1)
            )
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The source-pin receipt does not match its request.",
            ));
        }
        return Ok(Some(result));
    }

    let command_exists: Option<i64> = db
        .query_row(
            "SELECT 1 FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| row.get(0),
        )
        .optional()?;
    let proposal_exists: Option<i64> = db
        .query_row(
            "SELECT 1 FROM proposal_receipts WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| row.get(0),
        )
        .optional()?;
    let guidance_exists: Option<i64> = db
        .query_row(
            "SELECT 1 FROM author_guidance_receipts WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| row.get(0),
        )
        .optional()?;
    if command_exists.is_some() || proposal_exists.is_some() || guidance_exists.is_some() {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for another project command.",
        ));
    }
    Ok(None)
}

fn insert_receipt(
    db: &Connection,
    request: &SaveSourcePins,
    payload_hash: &str,
    result: &SourcePinSet,
) -> CoreResult<()> {
    db.execute(
        "INSERT INTO source_pin_receipts(project_id,operation_namespace,operation_id,scope,target_document_id,expected_version,payload_hash,operation_kind,result_json)
         VALUES(?,?,?,?,?,?,?,?,?)",
        params![
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            request.scope.as_str(),
            target_key(request.scope, request.target_document_id.as_deref()),
            parse_optional_version(&request.expected_version)?,
            payload_hash,
            "saveSourcePins",
            serde_json::to_string(result)?,
        ],
    )?;
    Ok(())
}
