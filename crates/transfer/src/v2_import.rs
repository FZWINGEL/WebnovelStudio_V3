//! Read-only preview of a verified WebnovelStudio V2 schema-8 database.
//!
//! This module deliberately stops at a preview.  It never installs rows into a V3
//! project.  On the supported Windows path it reads the original through a
//! share-read OS handle and opens only an owned copy with SQLite. The separate
//! projects::import installer consumes a validated preview within its staging boundary.

use rusqlite::{Connection, OpenFlags, Row, types::ValueRef};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
#[cfg(windows)]
use uuid::Uuid;
use wns_kernel::{CoreError, CoreResult};

const SUPPORTED_SCHEMA: i64 = 8;
#[cfg(windows)]
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_LEGACY_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_LEGACY_ROWS: usize = 20_000;

const MIGRATIONS: [&str; 8] = [
    "001-persistence-floor",
    "002-authority-boundary",
    "003-draft-run",
    "004-audit-approval",
    "005-settlement",
    "006-option-model",
    "007-draft-segments",
    "008-local-first-retire",
];

const REQUIRED_TABLES: [&str; 19] = [
    "schema_migrations",
    "projects",
    "story_bibles",
    "canon_entities",
    "termbase",
    "plot_threads",
    "story_arcs",
    "chapters",
    "chapter_drafts",
    "generation_options",
    "authoritative_revisions",
    "generation_runs",
    "context_receipts",
    "audit_runs",
    "audit_findings",
    "finding_waivers",
    "settlement_runs",
    "settlement_proposals",
    "draft_segments",
];

const TABLES: [&str; 18] = [
    "projects",
    "story_bibles",
    "canon_entities",
    "termbase",
    "plot_threads",
    "story_arcs",
    "chapters",
    "chapter_drafts",
    "generation_options",
    "authoritative_revisions",
    "generation_runs",
    "context_receipts",
    "audit_runs",
    "audit_findings",
    "finding_waivers",
    "settlement_runs",
    "settlement_proposals",
    "draft_segments",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2ImportPreview {
    pub import_format_version: u32,
    pub source: V2SourceManifest,
    pub project: V2ProjectPreview,
    pub chapters: Vec<V2ChapterPreview>,
    pub legacy: V2LegacyPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2SourceManifest {
    pub schema_version: i64,
    pub source_bytes: u64,
    pub source_sha256: String,
    pub migration_versions: Vec<String>,
    pub project_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2ProjectPreview {
    pub source_project_id: String,
    pub title: String,
    pub slug: String,
    pub chapter_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2ProjectSummary {
    pub source_project_id: String,
    pub title: String,
    pub slug: String,
    pub chapter_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2ChapterPreview {
    pub source_id: String,
    pub chapter_number: i64,
    pub title: String,
    pub retired_at: Option<String>,
    pub working_prose: V2WorkingProse,
    pub body_selection: V2BodySelection,
    pub working_prose_based_on_draft_id: Option<String>,
    pub approved_draft_id: Option<String>,
    pub draft_count: usize,
    pub drafts: Vec<V2DraftPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2DraftPreview {
    pub source_id: String,
    pub version: i64,
    pub prose: String,
    pub is_approved: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", tag = "state", content = "text")]
pub enum V2WorkingProse {
    Missing,
    Present(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum V2BodySelection {
    WorkingProse,
    RequiresAuthorChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2LegacyPreview {
    pub record_counts: BTreeMap<String, usize>,
    pub records: Vec<V2LegacyRecord>,
    pub total_json_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct V2LegacyRecord {
    pub table: String,
    pub source_id: String,
    pub payload: serde_json::Value,
}

/// Open and validate a V2 source, then return a bounded selected-project preview.
pub fn preview_v2_import(path: &Path, source_project_id: &str) -> CoreResult<V2ImportPreview> {
    if source_project_id.is_empty() || source_project_id.len() > 128 {
        return Err(v2_error("InvalidV2Project", "source project id is invalid"));
    }
    let (snapshot, source) = source_snapshot(path)?;
    let connection = Connection::open_with_flags(&snapshot.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.execute_batch("BEGIN")?;
    let result = (|| {
        validate_schema(&connection, source.schema_version)?;
        let mut source = source;
        source.schema_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let migrations = query_migrations(&connection)?;
        source.migration_versions = (1..=SUPPORTED_SCHEMA)
            .filter_map(|version| migrations.get(&version).cloned())
            .collect();
        let project = project_row(&connection, source_project_id)?;
        let catalog = load_catalog(&connection)?;
        validate_project_references(&connection, source_project_id, &project, &catalog)?;
        let chapters = chapter_previews(&connection, source_project_id, &catalog)?;
        let legacy = legacy_preview(&connection, source_project_id, &catalog)?;
        Ok(V2ImportPreview {
            import_format_version: 1,
            source: V2SourceManifest {
                project_count: catalog.project_count,
                ..source
            },
            project: V2ProjectPreview {
                source_project_id: source_project_id.to_owned(),
                title: project.title,
                slug: project.slug,
                chapter_count: chapters.len(),
            },
            chapters,
            legacy,
        })
    })();
    let _ = connection.execute_batch("ROLLBACK");
    result
}

/// List bounded source-project metadata so callers can present a choice before
/// requesting a selected-project preview. The source is still copied through
/// the same Windows-only, read-only snapshot boundary as preview/import.
pub fn list_v2_projects(path: &Path) -> CoreResult<Vec<V2ProjectSummary>> {
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(v2_error(
            "UnsupportedPlatform",
            "V2 import preview currently requires the Windows source snapshot boundary",
        ))
    }
    #[cfg(windows)]
    {
        let (snapshot, source) = source_snapshot(path)?;
        let connection =
            Connection::open_with_flags(&snapshot.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.execute_batch("BEGIN")?;
        let result = (|| {
            validate_schema(&connection, source.schema_version)?;
            let mut statement = connection.prepare(
                "SELECT p.id,p.title,p.slug,COUNT(c.id)
                 FROM projects p LEFT JOIN chapters c ON c.project_id=p.id
                 GROUP BY p.id,p.title,p.slug ORDER BY p.title COLLATE NOCASE,p.id",
            )?;
            let rows = statement.query_map([], |row| {
                let chapter_count: i64 = row.get(3)?;
                Ok(V2ProjectSummary {
                    source_project_id: row.get(0)?,
                    title: row.get(1)?,
                    slug: row.get(2)?,
                    chapter_count: usize::try_from(chapter_count).unwrap_or(usize::MAX),
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })();
        let _ = connection.execute_batch("ROLLBACK");
        result
    }
}

struct OwnedSnapshot {
    #[cfg(windows)]
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

#[cfg(windows)]
impl Drop for OwnedSnapshot {
    fn drop(&mut self) {
        cleanup_snapshot(&self.dir, &self.path);
    }
}

#[cfg(windows)]
fn cleanup_snapshot(dir: &Path, path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-journal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(Path::new(&sidecar));
    }
    let _ = std::fs::remove_dir(dir);
}

#[cfg(windows)]
fn has_live_sidecar(path: &Path) -> bool {
    ["-wal", "-journal", "-shm"].iter().any(|suffix| {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        Path::new(&value).exists()
    })
}

#[cfg(windows)]
fn has_wal_header(path: &Path) -> CoreResult<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 20];
    file.read_exact(&mut header)?;
    Ok(header[18] == 2 || header[19] == 2)
}

#[cfg(windows)]
fn source_snapshot(path: &Path) -> CoreResult<(OwnedSnapshot, V2SourceManifest)> {
    if has_live_sidecar(path) || has_wal_header(path)? {
        return Err(v2_error(
            "InvalidV2Snapshot",
            "V2 source has a live SQLite journal; provide a stable database copy",
        ));
    }
    let metadata_before = std::fs::metadata(path)?;
    if metadata_before.len() > MAX_SOURCE_BYTES {
        return Err(v2_error(
            "InvalidV2Snapshot",
            "V2 source exceeds the bounded import size",
        ));
    }
    let snapshot_dir = std::env::temp_dir().join(format!("v2-import-{}", Uuid::new_v4()));
    std::fs::create_dir(&snapshot_dir)?;
    let snapshot_path = snapshot_dir.join("source.db");
    let snapshot = OwnedSnapshot {
        dir: snapshot_dir,
        path: snapshot_path.clone(),
    };
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&snapshot_path)?;
    let mut source = OpenOptions::new()
        .read(true)
        .share_mode(0x00000001)
        .open(path)
        .map_err(|_| {
            v2_error(
                "InvalidV2Snapshot",
                "V2 source is busy or cannot be opened for a stable read",
            )
        })?;
    let mut hasher = Sha256::new();
    let mut bytes_read = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read as u64)
            .ok_or_else(|| v2_error("InvalidV2Snapshot", "V2 source size overflow"))?;
        if bytes_read > MAX_SOURCE_BYTES {
            return Err(v2_error(
                "InvalidV2Snapshot",
                "V2 source exceeds the bounded import size",
            ));
        }
        target.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
    }
    target.sync_all()?;
    let metadata_after = std::fs::metadata(path)?;
    if metadata_before.len() != metadata_after.len()
        || metadata_before.modified().ok() != metadata_after.modified().ok()
        || has_live_sidecar(path)
        || has_wal_header(path)?
    {
        return Err(v2_error(
            "InvalidV2Snapshot",
            "V2 source changed while it was being copied",
        ));
    }
    let hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    drop(target);
    Ok((
        snapshot,
        V2SourceManifest {
            schema_version: 0,
            source_bytes: bytes_read,
            source_sha256: hash,
            migration_versions: Vec::new(),
            project_count: 0,
        },
    ))
}

#[cfg(not(windows))]
fn source_snapshot(_path: &Path) -> CoreResult<(OwnedSnapshot, V2SourceManifest)> {
    Err(v2_error(
        "UnsupportedPlatform",
        "V2 import preview currently requires the Windows source snapshot boundary",
    ))
}

fn validate_schema(connection: &Connection, schema_hint: i64) -> CoreResult<()> {
    let schema_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema_version != SUPPORTED_SCHEMA || schema_hint != 0 && schema_hint != schema_version {
        return Err(v2_error(
            "UnsupportedV2Schema",
            &format!("only V2 schema {SUPPORTED_SCHEMA} is supported"),
        ));
    }
    let tables = query_strings(
        connection,
        "SELECT name FROM sqlite_master WHERE type='table'",
    )?;
    let table_set: HashSet<&str> = tables.iter().map(String::as_str).collect();
    for table in REQUIRED_TABLES {
        if !table_set.contains(table) {
            return Err(v2_error(
                "InvalidV2Snapshot",
                &format!("required table {table} is missing"),
            ));
        }
    }
    let migrations = query_migrations(connection)?;
    for (version, name) in MIGRATIONS.iter().enumerate() {
        let expected = (version + 1) as i64;
        if migrations.get(&expected).map(String::as_str) != Some(*name) {
            return Err(v2_error(
                "InvalidV2Snapshot",
                &format!("migration ledger is missing or inconsistent at version {expected}"),
            ));
        }
    }
    for table in REQUIRED_TABLES {
        let columns = query_table_columns(connection, table)?;
        let required = required_columns(table);
        for column in required {
            if !columns.iter().any(|value| value == column) {
                return Err(v2_error(
                    "InvalidV2Snapshot",
                    &format!("required column {table}.{column} is missing"),
                ));
            }
        }
    }
    let mut foreign_keys = connection.prepare("PRAGMA foreign_key_check")?;
    let mut rows = foreign_keys.query([])?;
    if rows.next()?.is_some() {
        return Err(v2_error(
            "InvalidV2Snapshot",
            "V2 foreign-key integrity check failed",
        ));
    }
    let results = query_strings(connection, "PRAGMA integrity_check")?;
    if results.is_empty() || results.iter().any(|result| result != "ok") {
        return Err(v2_error(
            "InvalidV2Snapshot",
            "V2 SQLite integrity check failed",
        ));
    }
    Ok(())
}

fn required_columns(table: &str) -> &'static [&'static str] {
    match table {
        "schema_migrations" => &["version", "name"],
        "projects" => &["id", "title", "slug"],
        "story_bibles" => &["id", "project_id"],
        "canon_entities" => &["id", "project_id", "name", "category"],
        "termbase" => &["id", "project_id", "concept_id", "english_translation"],
        "plot_threads" => &["id", "project_id", "title"],
        "story_arcs" => &["id", "project_id", "title"],
        "chapters" => &[
            "id",
            "project_id",
            "chapter_number",
            "title",
            "working_prose",
            "working_prose_based_on_draft_id",
            "approved_draft_id",
            "retired_at",
        ],
        "chapter_drafts" => &["id", "chapter_id", "prose", "created_at"],
        "generation_options" => &["id", "project_id", "reference_id", "payload"],
        "authoritative_revisions" => &["id", "project_id", "target_type", "target_id", "payload"],
        "generation_runs" => &["id", "project_id", "status", "frozen_request"],
        "context_receipts" => &[
            "id",
            "project_id",
            "chapter_id",
            "system_message",
            "user_message",
        ],
        "audit_runs" => &["id", "draft_id", "status"],
        "audit_findings" => &["id", "audit_run_id", "message"],
        "finding_waivers" => &["id", "finding_id", "rationale"],
        "settlement_runs" => &["id", "chapter_id", "status"],
        "settlement_proposals" => &["id", "project_id", "chapter_id", "payload"],
        "draft_segments" => &["id", "draft_id", "sequence", "prose"],
        _ => &[],
    }
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn query_strings(connection: &Connection, sql: &str) -> CoreResult<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([])?;
    let mut values = Vec::new();
    while let Some(row) = rows.next()? {
        values.push(row.get::<_, String>(0).unwrap_or_default());
    }
    Ok(values)
}

fn query_table_columns(connection: &Connection, table: &str) -> CoreResult<Vec<String>> {
    let mut statement =
        connection.prepare(&format!("PRAGMA table_info({})", quote_ident(table)))?;
    let mut rows = statement.query([])?;
    let mut values = Vec::new();
    while let Some(row) = rows.next()? {
        values.push(row.get::<_, String>(1)?);
    }
    Ok(values)
}

fn query_migrations(connection: &Connection) -> CoreResult<HashMap<i64, String>> {
    let mut statement = connection.prepare("SELECT version, name FROM schema_migrations")?;
    let mut rows = statement.query([])?;
    let mut migrations = HashMap::new();
    while let Some(row) = rows.next()? {
        migrations.insert(row.get(0)?, row.get(1)?);
    }
    Ok(migrations)
}

#[derive(Debug, Clone)]
struct ProjectRow {
    title: String,
    slug: String,
}

fn project_row(connection: &Connection, project_id: &str) -> CoreResult<ProjectRow> {
    connection
        .query_row(
            "SELECT title, slug FROM projects WHERE id=?1",
            [project_id],
            |row| {
                Ok(ProjectRow {
                    title: row.get(0)?,
                    slug: row.get(1)?,
                })
            },
        )
        .map_err(|_| v2_error("InvalidV2Project", "selected V2 project does not exist"))
}

#[derive(Debug, Clone)]
struct CatalogRow {
    table: &'static str,
    id: String,
    project_id: String,
    payload: serde_json::Value,
}

#[derive(Debug, Default)]
struct Catalog {
    rows: Vec<CatalogRow>,
    project_count: usize,
}

fn load_catalog(connection: &Connection) -> CoreResult<Catalog> {
    let project_count: usize = connection
        .query_row("SELECT COUNT(*) FROM projects", [], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .map_err(|_| v2_error("InvalidV2Snapshot", "project count is out of bounds"))?;
    let existing: HashSet<String> = query_strings(
        connection,
        "SELECT name FROM sqlite_master WHERE type='table'",
    )?
    .into_iter()
    .collect();
    let mut catalog = Catalog {
        project_count,
        ..Catalog::default()
    };
    for table in TABLES {
        if !existing.contains(table) {
            continue;
        }
        let sql = ownership_query(table);
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let project_id: String = row.get(1)?;
            let payload = row_payload(row)?;
            catalog.rows.push(CatalogRow {
                table,
                id: id.clone(),
                project_id: project_id.clone(),
                payload,
            });
        }
    }
    Ok(catalog)
}

fn ownership_query(table: &str) -> &'static str {
    match table {
        "projects" => "SELECT id, id AS project_id, * FROM projects",
        "story_bibles" => "SELECT id, project_id, * FROM story_bibles",
        "canon_entities" => "SELECT id, project_id, * FROM canon_entities",
        "termbase" => "SELECT id, project_id, * FROM termbase",
        "plot_threads" => "SELECT id, project_id, * FROM plot_threads",
        "story_arcs" => "SELECT id, project_id, * FROM story_arcs",
        "chapters" => "SELECT id, project_id, * FROM chapters",
        "chapter_drafts" => {
            "SELECT d.id, c.project_id, d.* FROM chapter_drafts d JOIN chapters c ON c.id=d.chapter_id"
        }
        "generation_options" => "SELECT o.id, o.project_id, o.* FROM generation_options o",
        "authoritative_revisions" => "SELECT id, project_id, * FROM authoritative_revisions",
        "context_receipts" => "SELECT r.id, r.project_id, r.* FROM context_receipts r",
        "generation_runs" => "SELECT r.id, r.project_id, r.* FROM generation_runs r",
        "audit_runs" => {
            "SELECT a.id, c.project_id, a.* FROM audit_runs a JOIN chapter_drafts d ON d.id=a.draft_id JOIN chapters c ON c.id=d.chapter_id"
        }
        "audit_findings" => {
            "SELECT f.id, c.project_id, f.* FROM audit_findings f JOIN audit_runs a ON a.id=f.audit_run_id JOIN chapter_drafts d ON d.id=a.draft_id JOIN chapters c ON c.id=d.chapter_id"
        }
        "finding_waivers" => {
            "SELECT w.id, c.project_id, w.* FROM finding_waivers w JOIN audit_findings f ON f.id=w.finding_id JOIN audit_runs a ON a.id=f.audit_run_id JOIN chapter_drafts d ON d.id=a.draft_id JOIN chapters c ON c.id=d.chapter_id"
        }
        "settlement_runs" => {
            "SELECT s.id, c.project_id, s.* FROM settlement_runs s JOIN chapters c ON c.id=s.chapter_id"
        }
        "settlement_proposals" => "SELECT s.id, s.project_id, s.* FROM settlement_proposals s",
        "draft_segments" => {
            "SELECT s.id, c.project_id, s.* FROM draft_segments s JOIN chapter_drafts d ON d.id=s.draft_id JOIN chapters c ON c.id=d.chapter_id"
        }
        _ => "SELECT id, '' AS project_id, * FROM sqlite_master WHERE 0",
    }
}

fn row_payload(row: &Row<'_>) -> CoreResult<serde_json::Value> {
    let mut object = serde_json::Map::new();
    for index in 2..row.as_ref().column_count() {
        let name = row
            .as_ref()
            .column_name(index)
            .unwrap_or("column")
            .to_owned();
        object.insert(name, sqlite_value(row.get_ref(index)?));
    }
    Ok(serde_json::Value::Object(object))
}

fn sqlite_value(value: ValueRef<'_>) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(value) => value.into(),
        ValueRef::Real(value) => serde_json::json!(value),
        ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned().into(),
        ValueRef::Blob(value) => {
            serde_json::json!({ "blobHex": value.iter().map(|byte| format!("{byte:02x}")).collect::<String>() })
        }
    }
}

fn validate_project_references(
    connection: &Connection,
    selected: &str,
    project: &ProjectRow,
    catalog: &Catalog,
) -> CoreResult<()> {
    let _ = (connection, project);
    let rows: Vec<&CatalogRow> = catalog
        .rows
        .iter()
        .filter(|row| row.project_id == selected)
        .collect();
    for row in rows {
        if row.table == "model_catalog" || row.project_id.is_empty() {
            continue;
        }
        validate_json_references(row, selected, catalog)?;
        validate_row_references(row, selected, catalog)?;
    }
    for chapter in catalog
        .rows
        .iter()
        .filter(|row| row.table == "chapters" && row.project_id == selected)
    {
        let based = string_or_null(&chapter.payload, "working_prose_based_on_draft_id");
        let approved = string_or_null(&chapter.payload, "approved_draft_id");
        for draft in [based, approved].into_iter().flatten() {
            require_draft_for_chapter(catalog, draft, chapter.id.as_str(), selected)?;
        }
    }
    Ok(())
}

fn validate_row_references(row: &CatalogRow, selected: &str, catalog: &Catalog) -> CoreResult<()> {
    let reference = |field: &str, table: &str| -> CoreResult<()> {
        if let Some(id) = string_or_null(&row.payload, field) {
            require_same_project(catalog, table, id, selected)?;
        }
        Ok(())
    };
    match row.table {
        "canon_entities" | "termbase" | "plot_threads" | "story_arcs" => {
            reference("active_revision_id", "authoritative_revisions")?;
        }
        "chapters" => {
            reference("arc_id", "story_arcs")?;
            reference("pov_character_id", "canon_entities")?;
            reference("active_plan_revision_id", "authoritative_revisions")?;
            reference("accepted_summary_revision_id", "authoritative_revisions")?;
        }
        "context_receipts" => {
            reference("chapter_id", "chapters")?;
            reference("plan_revision_id", "authoritative_revisions")?;
            if let Some(value) = row.payload.get("sources_json") {
                validate_context_sources(value, selected, catalog)?;
            }
        }
        "authoritative_revisions" => {
            reference("parent_revision_id", "authoritative_revisions")?;
            let target_table = match string_or_null(&row.payload, "target_type") {
                Some("canon_entity") => Some("canon_entities"),
                Some("term") => Some("termbase"),
                Some("plot_thread") => Some("plot_threads"),
                Some("story_arc") => Some("story_arcs"),
                Some("chapter_plan") | Some("chapter_summary") => Some("chapters"),
                _ => None,
            };
            if let Some(table) = target_table
                && let Some(id) = string_or_null(&row.payload, "target_id")
            {
                require_same_project(catalog, table, id, selected)?;
            }
        }
        "settlement_proposals" => {
            reference("chapter_id", "chapters")?;
            reference("source_draft_id", "chapter_drafts")?;
            reference("prior_revision_id", "authoritative_revisions")?;
            reference("resulting_revision_id", "authoritative_revisions")?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_context_sources(
    value: &serde_json::Value,
    selected: &str,
    catalog: &Catalog,
) -> CoreResult<()> {
    let Some(values) = json_value(value).and_then(|value| value.as_array().cloned()) else {
        return Err(v2_error(
            "InvalidV2Reference",
            "context receipt sources are not a JSON array",
        ));
    };
    for source in values {
        if let Some(id) = source.as_str() {
            require_same_project(catalog, "canon_entities", id, selected)?;
            continue;
        }
        let Some(source) = source.as_object() else {
            return Err(v2_error(
                "InvalidV2Reference",
                "context receipt source is not an object",
            ));
        };
        let id = source
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| v2_error("InvalidV2Reference", "context receipt source has no id"))?;
        let kind = source
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let table = match kind {
            "story_bible" => Some("story_bibles"),
            "canon" => Some("canon_entities"),
            "term" => Some("termbase"),
            "plot_thread" => Some("plot_threads"),
            "chapter_plan" => Some("chapters"),
            "prior_prose" => None,
            _ => {
                return Err(v2_error(
                    "InvalidV2Reference",
                    "context receipt contains an unsupported source kind",
                ));
            }
        };
        if let Some(table) = table {
            require_same_project(catalog, table, id, selected)?;
        }
        for revision_key in ["revisionId", "revision_id"] {
            if let Some(revision_id) = source.get(revision_key).and_then(serde_json::Value::as_str)
            {
                require_same_project(catalog, "authoritative_revisions", revision_id, selected)?;
            }
        }
    }
    Ok(())
}

fn require_draft_for_chapter(
    catalog: &Catalog,
    draft_id: &str,
    chapter_id: &str,
    selected: &str,
) -> CoreResult<()> {
    let draft = catalog
        .rows
        .iter()
        .find(|row| row.table == "chapter_drafts" && row.id == draft_id)
        .ok_or_else(|| v2_error("InvalidV2Reference", "chapter draft reference is missing"))?;
    if draft.project_id != selected
        || string_or_null(&draft.payload, "chapter_id") != Some(chapter_id)
    {
        return Err(v2_error(
            "InvalidV2Reference",
            "chapter draft reference crosses chapter or project boundary",
        ));
    }
    Ok(())
}

fn validate_json_references(row: &CatalogRow, selected: &str, catalog: &Catalog) -> CoreResult<()> {
    let refs: &[(&str, &str)] = match row.table {
        "chapters" => &[
            ("active_canon_ids", "canon_entities"),
            ("active_plot_thread_ids", "plot_threads"),
        ],
        _ => &[],
    };
    for (field, table) in refs {
        if let Some(value) = row.payload.get(field) {
            if *table == "authoritative_revisions" {
                if let Some(id) = value.as_str() {
                    require_same_project(catalog, table, id, selected)?;
                }
            } else if let Some(values) =
                json_value(value).and_then(|value| value.as_array().cloned())
            {
                for id in values.iter().filter_map(serde_json::Value::as_str) {
                    require_same_project(catalog, table, id, selected)?;
                }
            }
        }
    }
    Ok(())
}

fn json_value(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Some(value.clone()),
        serde_json::Value::String(text) => serde_json::from_str(text).ok(),
        _ => None,
    }
}

fn require_same_project(
    catalog: &Catalog,
    table: &str,
    id: &str,
    selected: &str,
) -> CoreResult<()> {
    if catalog
        .rows
        .iter()
        .any(|row| row.table == table && row.id == id && row.project_id == selected)
    {
        Ok(())
    } else {
        Err(v2_error(
            "InvalidV2Reference",
            &format!("{table} reference {id} is missing or belongs to another project"),
        ))
    }
}

fn string_or_null<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    payload.get(field).and_then(serde_json::Value::as_str)
}

fn payload_string<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    payload.get(field).and_then(serde_json::Value::as_str)
}

fn payload_i64(payload: &serde_json::Value, field: &str) -> Option<i64> {
    payload.get(field).and_then(serde_json::Value::as_i64)
}

fn chapter_previews(
    connection: &Connection,
    selected: &str,
    catalog: &Catalog,
) -> CoreResult<Vec<V2ChapterPreview>> {
    let mut statement = connection.prepare("SELECT id, chapter_number, title, retired_at, working_prose, working_prose_based_on_draft_id, approved_draft_id FROM chapters WHERE project_id=?1 ORDER BY chapter_number, id")?;
    let mut rows = statement.query([selected])?;
    let mut chapters = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let working: Option<String> = row.get(4)?;
        let based: Option<String> = row.get(5)?;
        let approved: Option<String> = row.get(6)?;
        let drafts = catalog
            .rows
            .iter()
            .filter(|item| {
                item.table == "chapter_drafts"
                    && item.project_id == selected
                    && item
                        .payload
                        .get("chapter_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(id.as_str())
            })
            .filter_map(|item| {
                Some(V2DraftPreview {
                    source_id: item.id.clone(),
                    version: payload_i64(&item.payload, "version")?,
                    prose: payload_string(&item.payload, "prose")?.to_owned(),
                    is_approved: payload_i64(&item.payload, "is_approved")? != 0,
                    created_at: payload_string(&item.payload, "created_at")?.to_owned(),
                })
            })
            .collect::<Vec<_>>();
        let draft_count = drafts.len();
        chapters.push(V2ChapterPreview {
            source_id: id,
            chapter_number: row.get(1)?,
            title: row.get(2)?,
            retired_at: row.get(3)?,
            working_prose: working.map_or(V2WorkingProse::Missing, V2WorkingProse::Present),
            body_selection: if matches!(row.get_ref(4)?, ValueRef::Null) {
                V2BodySelection::RequiresAuthorChoice
            } else {
                V2BodySelection::WorkingProse
            },
            working_prose_based_on_draft_id: based,
            approved_draft_id: approved,
            draft_count,
            drafts,
        });
    }
    Ok(chapters)
}

fn legacy_preview(
    _connection: &Connection,
    selected: &str,
    catalog: &Catalog,
) -> CoreResult<V2LegacyPreview> {
    let mut counts = BTreeMap::new();
    let mut records = Vec::new();
    let mut total = 0usize;
    for table in TABLES {
        let selected_rows = catalog
            .rows
            .iter()
            .filter(|row| {
                row.table == table && (row.project_id == selected || row.project_id.is_empty())
            })
            .collect::<Vec<_>>();
        counts.insert(table.to_owned(), selected_rows.len());
        for row in selected_rows {
            let record = V2LegacyRecord {
                table: table.to_owned(),
                source_id: row.id.clone(),
                payload: row.payload.clone(),
            };
            let bytes = serde_json::to_vec(&record)?.len();
            total = total
                .checked_add(bytes)
                .ok_or_else(|| v2_error("InvalidV2Snapshot", "legacy evidence size overflow"))?;
            if total > MAX_LEGACY_JSON_BYTES || records.len() >= MAX_LEGACY_ROWS {
                return Err(v2_error(
                    "InvalidV2Snapshot",
                    "legacy evidence exceeds the bounded preview",
                ));
            }
            records.push(record);
        }
    }
    Ok(V2LegacyPreview {
        record_counts: counts,
        records,
        total_json_bytes: total,
    })
}

fn v2_error(code: &str, detail: &str) -> CoreError {
    CoreError::new(code, detail)
}
