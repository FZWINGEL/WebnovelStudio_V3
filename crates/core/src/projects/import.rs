//! F1 staged V2 import installation.
//!
//! The source path and reviewed fingerprint are the authority for an import;
//! callers never pass a preview DTO back as trusted installation input.

use super::{
    CoreError, CoreResult, CreationOrigin, OwnedProject, ProjectInfo, ProjectSession,
    write_creation_origin,
};
use crate::v2_import::{V2ChapterPreview, V2WorkingProse, preview_v2_import};
use crate::{sha256_hex, validate_snapshot_json};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum V2ChapterBodyChoice {
    Empty,
    Draft {
        #[serde(rename = "sourceDraftId")]
        source_draft_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct V2ChapterBodyDecision {
    pub source_chapter_id: String,
    pub choice: V2ChapterBodyChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct V2ImportRequest {
    pub operation_id: String,
    pub source_path: PathBuf,
    pub source_project_id: String,
    pub title: String,
    pub expected_source_sha256: String,
    pub choices: Vec<V2ChapterBodyDecision>,
}

/// Durable identity for an import operation.  This is stored in the existing
/// library `operations.source_fingerprint` column so an incomplete import can
/// be resumed after restart without asking the author to pick a different
/// source or silently changing the body choices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredV2ImportOperation {
    pub version: u8,
    pub source_sha256: String,
    pub source_project_id: String,
    pub request_sha256: String,
    pub choices: Vec<V2ChapterBodyDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V2ImportResult {
    pub operation_id: String,
    pub project: ProjectInfo,
    pub source_project_id: String,
    pub source_sha256: String,
    pub request_sha256: String,
    pub chapter_document_ids: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct SelectedChapter {
    source: V2ChapterPreview,
    choice_kind: &'static str,
    source_draft_id: Option<String>,
    source_text: String,
}

#[derive(Debug, Clone)]
struct ValidatedImport {
    source_sha256: String,
    request_sha256: String,
    source_project_id: String,
    chapters: Vec<SelectedChapter>,
    preview: crate::v2_import::V2ImportPreview,
}

pub(crate) fn request_fingerprint(
    request: &V2ImportRequest,
    source_sha256: &str,
) -> CoreResult<String> {
    validate_request_shape(request)?;
    let mut choices = request.choices.clone();
    choices.sort_by(|left, right| left.source_chapter_id.cmp(&right.source_chapter_id));
    let basis = serde_json::json!({
        "sourceSha256": source_sha256,
        "sourceProjectId": request.source_project_id,
        "title": request.title,
        "choices": choices,
    });
    Ok(sha256_hex(serde_json::to_string(&basis)?.as_bytes()))
}

pub(crate) fn encode_import_operation(
    request: &V2ImportRequest,
    request_sha256: &str,
) -> CoreResult<String> {
    let stored = StoredV2ImportOperation {
        version: 1,
        source_sha256: request.expected_source_sha256.clone(),
        source_project_id: request.source_project_id.clone(),
        request_sha256: request_sha256.to_owned(),
        choices: request.choices.clone(),
    };
    let encoded = serde_json::to_string(&stored)?;
    if encoded.len() > 1_048_576 {
        return Err(CoreError::new(
            "InvalidRequest",
            "The V2 import choices exceed the supported operation size.",
        ));
    }
    Ok(encoded)
}

pub(crate) fn decode_import_operation(
    fingerprint: &str,
    operation_id: &str,
    source_path: PathBuf,
    title: String,
) -> CoreResult<V2ImportRequest> {
    if fingerprint.len() > 1_048_576 {
        return Err(CoreError::new(
            "ImportRecoveryUnavailable",
            "The retained import request is too large to recover safely.",
        ));
    }
    let stored: StoredV2ImportOperation = serde_json::from_str(fingerprint).map_err(|_| {
        CoreError::new(
            "ImportRecoveryUnavailable",
            "This incomplete import predates durable body choices and must be reviewed again.",
        )
    })?;
    if stored.version != 1
        || stored.source_sha256.len() != 64
        || !stored
            .source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || stored.request_sha256.len() != 64
        || !stored
            .request_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CoreError::new(
            "ImportRecoveryUnavailable",
            "The retained import request is invalid and cannot be resumed.",
        ));
    }
    let request = V2ImportRequest {
        operation_id: operation_id.to_owned(),
        source_path,
        source_project_id: stored.source_project_id,
        title,
        expected_source_sha256: stored.source_sha256,
        choices: stored.choices,
    };
    let computed = request_fingerprint(&request, &request.expected_source_sha256)?;
    if !constant_time_equal(&computed, &stored.request_sha256) {
        return Err(CoreError::new(
            "ImportRecoveryUnavailable",
            "The retained import request does not match its fingerprint.",
        ));
    }
    Ok(request)
}

/// Re-read and validate the source, then stage a fresh V3 project.  A caller
/// supplied preview is intentionally not accepted by this API.
pub(crate) fn stage_v2_import(
    request: &V2ImportRequest,
    expected_request_sha256: &str,
    staging: &Path,
    destination: &Path,
    origin: &CreationOrigin,
) -> CoreResult<V2ImportResult> {
    validate_request_shape(request)?;
    if origin.operation_id != request.operation_id {
        return Err(CoreError::new(
            "InvalidRequest",
            "The import operation identity does not match its staging origin.",
        ));
    }
    let validated = validate_source(request, expected_request_sha256)?;
    if destination.exists() {
        if super::read_creation_origin(destination).ok().as_ref() == Some(origin) {
            return read_import_result(
                destination,
                &request.operation_id,
                &request.expected_source_sha256,
                expected_request_sha256,
            );
        }
        return Err(CoreError::new(
            "ProjectExists",
            "The import destination already exists.",
        ));
    }
    if staging.exists() {
        return resume_or_refuse_staging(&validated, staging, destination, origin, request);
    }
    let parent = staging
        .parent()
        .ok_or_else(|| CoreError::new("InvalidRequest", "Import staging has no parent."))?;
    if destination.parent() != Some(parent) || staging == destination {
        return Err(CoreError::new(
            "InvalidRequest",
            "Import staging and destination must be sibling folders.",
        ));
    }
    let mut project = OwnedProject::open_direct(staging.to_owned(), Some(request.title.clone()))?;
    write_creation_origin(staging, origin)?;
    let result = populate_project(&mut project, &validated, request, origin)?;
    drop(project);
    let _ = ProjectSession::create_staged(staging, destination, &request.title, origin)?;
    Ok(result)
}

pub(crate) fn read_import_result(
    project_path: &Path,
    operation_id: &str,
    expected_source_sha256: &str,
    expected_request_sha256: &str,
) -> CoreResult<V2ImportResult> {
    let origin = super::read_creation_origin(project_path)?;
    let mut marker_bytes = Vec::new();
    File::open(project_path.join("project.wns.json"))?
        .take(16_385)
        .read_to_end(&mut marker_bytes)?;
    if marker_bytes.len() > 16_384 {
        return Err(CoreError::new(
            "InvalidProject",
            "The project marker is too large.",
        ));
    }
    let marker: ProjectInfo = serde_json::from_slice(&marker_bytes)?;
    let connection = Connection::open_with_flags(
        project_path.join("project.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    connection.execute_batch("BEGIN DEFERRED")?;
    let result = read_import_result_transaction(
        &connection,
        &origin,
        &marker,
        operation_id,
        expected_source_sha256,
        expected_request_sha256,
    );
    let rollback = connection.execute_batch("ROLLBACK");
    match (result, rollback) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

fn read_import_result_transaction(
    connection: &Connection,
    origin: &CreationOrigin,
    marker: &ProjectInfo,
    operation_id: &str,
    expected_source_sha256: &str,
    expected_request_sha256: &str,
) -> CoreResult<V2ImportResult> {
    let (project_id, operation_namespace, title, format_version): (String, String, String, u32) =
        connection.query_row(
            "SELECT id,operation_namespace,title,format_version FROM project WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let current = ProjectInfo {
        project_id: project_id.clone(),
        operation_namespace: operation_namespace.clone(),
        title: title.clone(),
        format_version,
    };
    if marker.project_id != current.project_id
        || marker.operation_namespace != current.operation_namespace
        || marker.format_version != current.format_version
        || origin.operation_id != operation_id
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The imported project identity does not match its creation record.",
        ));
    }
    crate::transfer::validate_project_connection(connection, &current)?;
    let (manifest_project_id, manifest_namespace, recorded_operation_id, source_project_id, source_sha256, request_sha256): (String, String, String, String, String, String) = connection.query_row(
        "SELECT project_id,operation_namespace,operation_id,source_project_id,source_sha256,request_sha256 FROM import_manifest WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).map_err(|_| CoreError::new("InvalidProject", "The imported project has no valid import manifest."))?;
    if manifest_project_id != project_id
        || manifest_namespace != operation_namespace
        || recorded_operation_id != operation_id
        || !constant_time_equal(&source_sha256, expected_source_sha256)
        || !constant_time_equal(&request_sha256, expected_request_sha256)
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The imported project identity does not match its import manifest.",
        ));
    }
    let mut statement = connection.prepare(
        "SELECT source_id,v3_document_id FROM import_id_map WHERE source_table='chapters' ORDER BY rowid",
    )?;
    let chapter_document_ids = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(V2ImportResult {
        operation_id: recorded_operation_id,
        project: ProjectInfo {
            project_id,
            operation_namespace,
            title,
            format_version,
        },
        source_project_id,
        source_sha256,
        request_sha256,
        chapter_document_ids,
    })
}

/// Install a sealed import staging folder from its own immutable manifest.
/// No read of the original V2 database is performed on this path.
pub(crate) fn recover_import_staging(
    staging: &Path,
    destination: &Path,
    title: &str,
    origin: &CreationOrigin,
    operation_id: &str,
    expected_source_sha256: &str,
    expected_request_sha256: &str,
) -> CoreResult<V2ImportResult> {
    if destination.exists() {
        return Err(CoreError::new(
            "TargetExists",
            "The import destination already exists.",
        ));
    }
    if super::read_creation_origin(staging).ok().as_ref() != Some(origin) {
        return Err(CoreError::new(
            "IncompleteCreation",
            "Import staging belongs to another operation.",
        ));
    }
    let result = read_import_result(
        staging,
        operation_id,
        expected_source_sha256,
        expected_request_sha256,
    )?;
    if result.project.title != title {
        return Err(CoreError::new(
            "ImportRequestChanged",
            "Import staging does not match the retained title.",
        ));
    }
    drop(ProjectSession::create_staged(
        staging,
        destination,
        title,
        origin,
    )?);
    read_import_result(
        destination,
        operation_id,
        expected_source_sha256,
        expected_request_sha256,
    )
}

fn validate_source(
    request: &V2ImportRequest,
    expected_request_sha256: &str,
) -> CoreResult<ValidatedImport> {
    let preview = preview_v2_import(&request.source_path, &request.source_project_id)?;
    if !constant_time_equal(
        &preview.source.source_sha256,
        &request.expected_source_sha256,
    ) {
        return Err(CoreError::new(
            "SourceVersionChanged",
            "The V2 source fingerprint no longer matches the reviewed import.",
        ));
    }
    let request_sha256 = request_fingerprint(request, &preview.source.source_sha256)?;
    if !constant_time_equal(&request_sha256, expected_request_sha256) {
        return Err(CoreError::new(
            "ImportRequestChanged",
            "The import choices no longer match the reviewed operation.",
        ));
    }
    let choices = request
        .choices
        .iter()
        .map(|choice| (choice.source_chapter_id.as_str(), &choice.choice))
        .collect::<HashMap<_, _>>();
    if choices.len() != request.choices.len() {
        return Err(CoreError::new(
            "InvalidRequest",
            "Each imported chapter may have at most one body choice.",
        ));
    }
    let mut chapters = Vec::with_capacity(preview.chapters.len());
    for chapter in &preview.chapters {
        let (choice_kind, source_draft_id, source_text) = match &chapter.working_prose {
            V2WorkingProse::Present(text) => {
                if choices.contains_key(chapter.source_id.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A present working body must be imported exactly as stored.",
                    ));
                }
                ("working", None, text.clone())
            }
            V2WorkingProse::Missing => {
                let choice = choices.get(chapter.source_id.as_str()).ok_or_else(|| {
                    CoreError::new(
                        "BodyChoiceRequired",
                        "Choose Empty or a same-chapter draft for every missing working body.",
                    )
                })?;
                match choice {
                    V2ChapterBodyChoice::Empty => ("empty", None, String::new()),
                    V2ChapterBodyChoice::Draft { source_draft_id } => {
                        let draft = chapter
                            .drafts
                            .iter()
                            .find(|draft| draft.source_id == *source_draft_id)
                            .ok_or_else(|| {
                                CoreError::new(
                                    "InvalidBodyChoice",
                                    "The selected draft does not belong to the selected chapter.",
                                )
                            })?;
                        ("draft", Some(source_draft_id.clone()), draft.prose.clone())
                    }
                }
            }
        };
        chapters.push(SelectedChapter {
            source: chapter.clone(),
            choice_kind,
            source_draft_id,
            source_text,
        });
    }
    for chapter_id in choices.keys() {
        if !preview
            .chapters
            .iter()
            .any(|chapter| chapter.source_id == *chapter_id)
        {
            return Err(CoreError::new(
                "InvalidBodyChoice",
                "A body choice references another project or chapter.",
            ));
        }
    }
    Ok(ValidatedImport {
        source_sha256: preview.source.source_sha256.clone(),
        request_sha256,
        source_project_id: request.source_project_id.clone(),
        chapters,
        preview,
    })
}

fn populate_project(
    project: &mut OwnedProject,
    validated: &ValidatedImport,
    request: &V2ImportRequest,
    origin: &CreationOrigin,
) -> CoreResult<V2ImportResult> {
    let info = project.info.clone();
    let mut chapter_document_ids = BTreeMap::new();
    let tx = project
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut position = 0i64;
    for chapter in &validated.chapters {
        let id = Uuid::new_v4().to_string();
        let body = plain_body(&chapter.source_text, &format!("p-{position}"))?;
        insert_document(
            &tx,
            &id,
            "chapter",
            &format!(
                "Chapter {}: {}",
                chapter.source.chapter_number, chapter.source.title
            ),
            position,
            &body,
        )?;
        insert_checkpoint(&tx, &id, &body, "importedV2")?;
        chapter_document_ids.insert(chapter.source.source_id.clone(), id);
        position += 1;
    }
    let mut narrative_docs = Vec::new();
    let mut record_by_table = HashMap::<(&str, &str), &Value>::new();
    for record in &validated.preview.legacy.records {
        record_by_table.insert(
            (record.table.as_str(), record.source_id.as_str()),
            &record.payload,
        );
    }
    for (title, kind, text, source_table, source_id) in
        narrative_documents(&validated.preview, &record_by_table)?
    {
        let id = Uuid::new_v4().to_string();
        let body = plain_body(&text, &format!("p-{position}"))?;
        insert_document(&tx, &id, kind, &title, position, &body)?;
        insert_checkpoint(&tx, &id, &body, "importedV2")?;
        narrative_docs.push((source_table, source_id, id));
        position += 1;
    }
    for (source_id, document_id) in &chapter_document_ids {
        tx.execute(
            "INSERT INTO import_id_map(source_table,source_id,v3_document_id,source_project_id) VALUES('chapters',?,?,?)",
            params![source_id, document_id, validated.source_project_id],
        )?;
    }
    for (source_table, source_id, document_id) in narrative_docs {
        tx.execute(
            "INSERT INTO import_id_map(source_table,source_id,v3_document_id,source_project_id) VALUES(?,?,?,?)",
            params![source_table, source_id, document_id, validated.source_project_id],
        )?;
    }
    for (chapter_position, chapter) in validated.chapters.iter().enumerate() {
        let chapter_position = i64::try_from(chapter_position)
            .map_err(|_| CoreError::new("InvalidSource", "The V2 source has too many chapters."))?;
        let selected_body_hash =
            body_hash_for_text(&chapter.source_text, &format!("p-{chapter_position}"))?;
        tx.execute(
            "INSERT INTO import_body_decisions(source_chapter_id,choice_kind,source_draft_id,source_working_state,source_body_sha256,selected_body_hash) VALUES(?,?,?,?,?,?)",
            params![chapter.source.source_id, chapter.choice_kind, chapter.source_draft_id, match chapter.source.working_prose { V2WorkingProse::Missing => "missing", V2WorkingProse::Present(_) => "present" }, sha256_hex(chapter.source_text.as_bytes()), selected_body_hash],
        )?;
    }
    for record in &validated.preview.legacy.records {
        tx.execute(
            "INSERT INTO import_legacy_records(source_table,source_id,source_project_id,payload_json) VALUES(?,?,?,?)",
            params![record.table, record.source_id, validated.source_project_id, serde_json::to_string(&record.payload)?],
        )?;
    }
    let mut counts = validated.preview.legacy.record_counts.clone();
    counts.insert("chaptersImported".into(), validated.chapters.len());
    let counts_json = serde_json::to_string(&counts)?;
    let source_bytes = i64::try_from(validated.preview.source.source_bytes)
        .map_err(|_| CoreError::new("InvalidSource", "The V2 source is too large."))?;
    tx.execute("INSERT INTO import_manifest(singleton,project_id,operation_namespace,operation_id,import_format_version,source_project_id,source_schema_version,source_sha256,source_bytes,source_title,source_slug,request_sha256,counts_json) VALUES(1,?,?,?,?,?,?,?,?,?,?,?,?)", params![info.project_id, info.operation_namespace, origin.operation_id, 1, validated.source_project_id, validated.preview.source.schema_version, validated.source_sha256, source_bytes, validated.preview.project.title, validated.preview.project.slug, validated.request_sha256, counts_json])?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(V2ImportResult {
        operation_id: request.operation_id.clone(),
        project: info,
        source_project_id: validated.source_project_id.clone(),
        source_sha256: validated.source_sha256.clone(),
        request_sha256: validated.request_sha256.clone(),
        chapter_document_ids,
    })
}

type NarrativeDocument = (String, &'static str, String, &'static str, String);

fn narrative_documents(
    preview: &crate::v2_import::V2ImportPreview,
    records: &HashMap<(&str, &str), &Value>,
) -> CoreResult<Vec<NarrativeDocument>> {
    let mut output = Vec::new();
    for (table, title, kind) in [
        ("story_bibles", "Story Bible, themes and hooks", "note"),
        ("canon_entities", "Characters", "character"),
        ("termbase", "Terminology", "note"),
        ("plot_threads", "Plot Threads and Hooks", "hook"),
        ("story_arcs", "Story Arcs and World Plan", "world"),
    ] {
        let mut lines = Vec::new();
        for ((record_table, source_id), payload) in records {
            if *record_table != table {
                continue;
            }
            lines.push((
                source_id.to_owned(),
                narrative_record_label(table, payload),
                narrative_record_text(table, payload),
            ));
        }
        if !lines.is_empty() {
            lines.sort_by(|left, right| left.0.cmp(right.0));
            output.push((
                title.to_owned(),
                kind,
                lines
                    .into_iter()
                    .map(|(_, label, text)| format!("{label}: {text}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                table,
                format!("{}-summary", preview.project.source_project_id),
            ));
        }
    }
    Ok(output)
}

fn narrative_record_label(table: &str, payload: &Value) -> String {
    let text_field = |field: &str| {
        payload
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    match table {
        "story_bibles" => "Story Bible".to_owned(),
        "canon_entities" => text_field("name").unwrap_or_else(|| "Story entity".to_owned()),
        "termbase" => text_field("english_translation")
            .or_else(|| text_field("source_term"))
            .unwrap_or_else(|| "Terminology entry".to_owned()),
        "plot_threads" => text_field("title").unwrap_or_else(|| "Plot thread".to_owned()),
        "story_arcs" => {
            let title = text_field("title");
            let number = payload.get("arc_number").and_then(|value| {
                value
                    .as_i64()
                    .map(|number| number.to_string())
                    .or_else(|| value.as_str().map(str::to_owned))
            });
            match (number, title) {
                (Some(number), Some(title)) => format!("Arc {number}: {title}"),
                (None, Some(title)) => title,
                (Some(number), None) => format!("Arc {number}"),
                (None, None) => "Story arc".to_owned(),
            }
        }
        _ => "Imported note".to_owned(),
    }
}

fn narrative_record_text(table: &str, payload: &Value) -> String {
    let fields: &[&str] = match table {
        "story_bibles" => &[
            "premise",
            "core_conflict",
            "themes",
            "tone_guidelines",
            "serialization_hooks",
            "forbidden_tropes",
        ],
        "canon_entities" => &[
            "category",
            "name",
            "aliases",
            "role",
            "personality_traits",
            "voice_description",
            "goals",
            "status",
        ],
        "termbase" => &[
            "concept_id",
            "source_term",
            "english_translation",
            "pinyin",
            "forbidden_substitutions",
            "required_context_note",
        ],
        "plot_threads" => &["title", "description", "category", "status", "notes"],
        "story_arcs" => &[
            "arc_number",
            "title",
            "summary",
            "goals",
            "start_chapter",
            "end_chapter",
        ],
        _ => &[],
    };
    fields
        .iter()
        .filter_map(|field| {
            payload.get(*field).and_then(|value| {
                if value.is_null() {
                    None
                } else if let Some(text) = value.as_str() {
                    Some(format!("{field}: {text}"))
                } else {
                    Some(format!(
                        "{field}: {}",
                        serde_json::to_string(value).unwrap_or_default()
                    ))
                }
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain_body(text: &str, paragraph_id: &str) -> CoreResult<String> {
    let content = if text.is_empty() {
        Value::Array(Vec::new())
    } else {
        json!([{"type": "text", "text": text}])
    };
    let body = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{"type": "paragraph", "attrs": {"id": paragraph_id}, "content": content}]}
    });
    let validated = validate_snapshot_json(&serde_json::to_string(&body)?)
        .map_err(|error| CoreError::new("InvalidDocument", &error))?;
    Ok(validated.canonical_json)
}

fn body_hash_for_text(text: &str, paragraph_id: &str) -> CoreResult<String> {
    let body = plain_body(text, paragraph_id)?;
    Ok(sha256_hex(body.as_bytes()))
}

fn insert_document(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    kind: &str,
    title: &str,
    position: i64,
    body: &str,
) -> CoreResult<()> {
    let hash = sha256_hex(body.as_bytes());
    tx.execute("INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash,projection_dirty,trashed,role) VALUES(?,?,?, ?,0,1,?,?,0,0,'ordinary')", params![id,kind,title,position,body,hash])?;
    Ok(())
}

fn insert_checkpoint(
    tx: &rusqlite::Transaction<'_>,
    document_id: &str,
    body: &str,
    reason: &str,
) -> CoreResult<()> {
    let revision_id = Uuid::new_v4().to_string();
    let hash = sha256_hex(body.as_bytes());
    tx.execute("INSERT INTO revisions(id,document_id,source_working_version,schema_version,body_json,body_hash,parent_id,reason) VALUES(?,?,0,1,?,?,NULL,?)", params![revision_id,document_id,body,hash,reason])?;
    tx.execute(
        "UPDATE documents SET last_checkpoint_id=? WHERE id=?",
        params![revision_id, document_id],
    )?;
    Ok(())
}

fn resume_or_refuse_staging(
    validated: &ValidatedImport,
    staging: &Path,
    destination: &Path,
    origin: &CreationOrigin,
    request: &V2ImportRequest,
) -> CoreResult<V2ImportResult> {
    if destination.exists() {
        return Err(CoreError::new(
            "TargetExists",
            "The import destination already exists.",
        ));
    }
    if super::read_creation_origin(staging).ok().as_ref() != Some(origin) {
        return Err(CoreError::new(
            "IncompleteCreation",
            "Import staging belongs to another operation.",
        ));
    }
    recover_import_staging(
        staging,
        destination,
        &request.title,
        origin,
        &request.operation_id,
        &validated.source_sha256,
        &validated.request_sha256,
    )
}

fn validate_request_shape(request: &V2ImportRequest) -> CoreResult<()> {
    if request.operation_id.is_empty()
        || request.operation_id.len() > 64
        || !request
            .operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || request.source_project_id.is_empty()
        || request.source_project_id.len() > 128
        || request.title.trim().is_empty()
        || request.title.len() > 512
        || request.title.chars().any(char::is_control)
        || request.expected_source_sha256.len() != 64
        || !request
            .expected_source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || request.choices.len() > 10_000
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The V2 import request is invalid.",
        ));
    }
    for decision in &request.choices {
        if decision.source_chapter_id.is_empty()
            || decision.source_chapter_id.len() > 128
            || decision.source_chapter_id.chars().any(char::is_control)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A V2 chapter body choice has an invalid source chapter id.",
            ));
        }
        if let V2ChapterBodyChoice::Draft { source_draft_id } = &decision.choice
            && (source_draft_id.is_empty()
                || source_draft_id.len() > 128
                || source_draft_id.chars().any(char::is_control))
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A V2 chapter body choice has an invalid source draft id.",
            ));
        }
    }
    Ok(())
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}
