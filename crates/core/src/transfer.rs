//! Staged project transfer operations.
//!
//! Backups use SQLite's online backup API from a separate read-only connection,
//! then package the resulting database and a small manifest in a stored ZIP.
//! Restore always creates a new project identity; it never replaces the source
//! project or reuses its active operation namespace.

use crate::projects::{
    CheckpointReason, CheckpointRequest, CoreError, CoreResult, CreationOrigin, Head,
    ProjectAccess, ProjectInfo, ProjectSession, Revision, StoredResult, read_creation_origin,
    write_creation_origin,
};
use crate::{storage, validate_snapshot_json};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, backup::Backup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const BACKUP_FORMAT_VERSION: u32 = 1;
const DATABASE_SCHEMA_VERSION: u32 = crate::storage::LATEST_SCHEMA_VERSION as u32;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const DB_FILE: &str = "project.sqlite3";
const MARKER_FILE: &str = "project.wns.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupManifest {
    pub format_version: u32,
    pub source_project_id: String,
    pub source_operation_namespace: String,
    pub database_sha256: String,
    pub database_schema_version: u32,
    #[serde(default)]
    pub context_source_epoch: String,
    pub documents: Vec<DocumentHeadManifest>,
    pub revisions: Vec<RevisionHeadManifest>,
    pub assets: Vec<AssetManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentHeadManifest {
    pub document_id: String,
    pub version: String,
    pub body_hash: String,
    pub last_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionHeadManifest {
    pub revision_id: String,
    pub document_id: String,
    pub source_version: String,
    pub body_hash: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetManifest {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportManifest {
    pub format: String,
    pub format_loss: String,
    pub project_id: String,
    pub document_id: String,
    pub source_head: Head,
    pub checkpoint_id: String,
    pub utf8_bytes: u64,
    pub sha256: String,
}

/// Frozen source identity and document heads used to guard a duplicate
/// against a live-project change between the UI read and the backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateBasis {
    pub project_id: String,
    pub operation_namespace: String,
    pub context_source_epoch: String,
    pub document_heads: Vec<Head>,
}

pub fn capture_duplicate_basis(
    project: &ProjectSession,
    access: ProjectAccess,
) -> CoreResult<DuplicateBasis> {
    let metadata = project.project_metadata()?;
    let mut document_heads = project
        .documents(access)?
        .into_iter()
        .map(|document| document.head)
        .collect::<Vec<_>>();
    document_heads.sort_by(|left, right| left.document_id.cmp(&right.document_id));
    Ok(DuplicateBasis {
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        context_source_epoch: project.context_source_epoch()?,
        document_heads,
    })
}

#[derive(Debug, Clone)]
struct DatabaseHeads {
    info: ProjectInfo,
    context_source_epoch: String,
    documents: Vec<DocumentHeadManifest>,
    revisions: Vec<RevisionHeadManifest>,
}

struct StageDir {
    path: PathBuf,
}

impl Drop for StageDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn transfer_error(code: &str, detail: impl Into<String>) -> CoreError {
    CoreError::new(code, &detail.into())
}

fn zip_error(error: zip::result::ZipError) -> CoreError {
    transfer_error(
        "InvalidBackup",
        format!("backup archive is invalid: {error}"),
    )
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_file(path: &Path) -> CoreResult<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_version(value: i64) -> CoreResult<String> {
    if value < 0 {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite contains a negative working version.",
        ));
    }
    Ok(value.to_string())
}

fn valid_version_string(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

fn validate_title(title: &str) -> CoreResult<()> {
    if title.trim().is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
        return Err(transfer_error(
            "InvalidRequest",
            "Enter a title of at most 512 bytes without control characters.",
        ));
    }
    Ok(())
}

fn canonical_body(body_json: &str, hash: &str, label: &str) -> CoreResult<()> {
    let receipt = validate_snapshot_json(body_json)
        .map_err(|error| transfer_error("InvalidBackup", format!("{label} is invalid: {error}")))?;
    if receipt.hash != hash || receipt.canonical_json != body_json {
        return Err(transfer_error(
            "InvalidBackup",
            format!("{label} is not canonical or its hash does not match."),
        ));
    }
    Ok(())
}

fn marker(path: &Path) -> CoreResult<ProjectInfo> {
    let bytes = fs::read(path.join(MARKER_FILE))?;
    if bytes.len() > 16 * 1024 {
        return Err(transfer_error(
            "InvalidProject",
            "The project identity marker is too large.",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        transfer_error(
            "InvalidProject",
            format!("The project identity marker is invalid: {error}"),
        )
    })
}

fn assert_marker_identity(root: &Path, expected: &ProjectInfo) -> CoreResult<()> {
    let actual = marker(root)?;
    if actual != *expected {
        return Err(transfer_error(
            "InvalidProject",
            "The project marker does not match its SQLite identity.",
        ));
    }
    Ok(())
}

fn source_root(path: &Path) -> CoreResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        transfer_error(
            "InvalidRequest",
            format!("Cannot resolve project path: {error}"),
        )
    })
}

fn output_path(path: &Path) -> CoreResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().ok_or_else(|| {
        transfer_error("InvalidRequest", "The destination has no parent directory.")
    })?;
    let parent = fs::canonicalize(parent).map_err(|error| {
        transfer_error(
            "InvalidRequest",
            format!("Cannot resolve destination directory: {error}"),
        )
    })?;
    let name = absolute.file_name().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The destination must have a file or folder name.",
        )
    })?;
    Ok(parent.join(name))
}

fn output_outside(root: &Path, path: &Path) -> CoreResult<PathBuf> {
    let root = source_root(root)?;
    let target = output_path(path)?;
    if target.starts_with(&root) {
        return Err(transfer_error(
            "InvalidRequest",
            "The destination must be outside the open project.",
        ));
    }
    if target.exists() {
        return Err(transfer_error(
            "TargetExists",
            "The destination already exists; choose a new path.",
        ));
    }
    Ok(target)
}

fn output_new(path: &Path) -> CoreResult<PathBuf> {
    let target = output_path(path)?;
    if target.exists() {
        return Err(transfer_error(
            "TargetExists",
            "The destination already exists; choose a new path.",
        ));
    }
    Ok(target)
}

fn stage(parent: &Path, prefix: &str) -> CoreResult<StageDir> {
    for _ in 0..16 {
        let path = parent.join(format!(".{prefix}-{}", Uuid::new_v4()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(StageDir { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(transfer_error(
        "PersistenceUnavailable",
        "Could not allocate a unique staging directory.",
    ))
}

fn install_file_no_replace(staged: &Path, target: &Path) -> CoreResult<()> {
    if target.exists() {
        return Err(transfer_error(
            "TargetExists",
            "The destination appeared while the operation was running.",
        ));
    }
    fs::hard_link(staged, target).map_err(|error| {
        transfer_error(
            "PersistenceUnavailable",
            format!("Could not finalize destination without replacement: {error}"),
        )
    })?;
    // The staged file was synced before linking. Windows may reject a second
    // read-only sync on the hard link, so only remove the private staging name
    // after the atomic link succeeds, and keep the successful destination even
    // if cleanup is denied.
    let _ = fs::remove_file(staged);
    Ok(())
}

fn online_backup(project: &ProjectSession, staged_db: &Path) -> CoreResult<()> {
    let source_path = project.path.join(DB_FILE);
    let source = Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            transfer_error(
                "PersistenceUnavailable",
                format!("Could not open the project for backup: {error}"),
            )
        })?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut destination = Connection::open(staged_db).map_err(|error| {
        transfer_error(
            "PersistenceUnavailable",
            format!("Could not create the staged backup database: {error}"),
        )
    })?;
    {
        let backup = Backup::new(&source, &mut destination).map_err(|error| {
            transfer_error(
                "PersistenceUnavailable",
                format!("Could not start SQLite online backup: {error}"),
            )
        })?;
        backup
            .run_to_completion(64, Duration::from_millis(10), None)
            .map_err(|error| {
                transfer_error(
                    "PersistenceUnavailable",
                    format!("SQLite online backup failed: {error}"),
                )
            })?;
    }
    destination
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| {
            transfer_error(
                "PersistenceUnavailable",
                format!("Could not finish staged backup: {error}"),
            )
        })?;
    drop(destination);
    let size = fs::metadata(staged_db)?.len();
    if size > MAX_DATABASE_BYTES {
        return Err(transfer_error(
            "InvalidBackup",
            "The staged SQLite database exceeds the backup size limit.",
        ));
    }
    Ok(())
}

fn validate_database(path: &Path, expected: Option<&ProjectInfo>) -> CoreResult<DatabaseHeads> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(transfer_error(
            "InvalidBackup",
            format!("SQLite integrity_check failed: {integrity}"),
        ));
    }
    let mut foreign = connection.prepare("PRAGMA foreign_key_check")?;
    if foreign.query([])?.next()?.is_some() {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite foreign_key_check found an invalid reference.",
        ));
    }
    let schema: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema != i64::from(DATABASE_SCHEMA_VERSION) {
        return Err(transfer_error(
            "UnsupportedSchema",
            format!("The project database schema {schema} is unsupported."),
        ));
    }
    let info: ProjectInfo = connection.query_row(
        "SELECT id,operation_namespace,title,format_version FROM project WHERE singleton=1",
        [],
        |row| {
            Ok(ProjectInfo {
                project_id: row.get(0)?,
                operation_namespace: row.get(1)?,
                title: row.get(2)?,
                format_version: row.get(3)?,
            })
        },
    )?;
    if info.format_version != 1
        || !valid_id(&info.project_id)
        || !valid_id(&info.operation_namespace)
    {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite project identity is invalid.",
        ));
    }
    if let Some(expected) = expected
        && info != *expected
    {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite project identity does not match the project marker.",
        ));
    }
    let epoch: i64 = connection.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if epoch < 0 {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite contains a negative context source epoch.",
        ));
    }
    let context_source_epoch = valid_version(epoch)?;
    crate::projects::validate_stored_view_state(&connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The saved view state is invalid: {error}"),
        )
    })?;
    crate::projects::story_context::validate_context_storage(&connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The story context is invalid: {error}"),
        )
    })?;
    let mut documents = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,working_version,body_hash,last_checkpoint_id,body_json,schema_version \
         FROM documents ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    for row in rows {
        let (id, version, hash, checkpoint, body, schema_version) = row?;
        if !valid_id(&id) || !valid_hash(&hash) || schema_version != 1 {
            return Err(transfer_error(
                "InvalidBackup",
                "A document row has invalid identity, schema, or hash metadata.",
            ));
        }
        canonical_body(&body, &hash, "document body")?;
        documents.push(DocumentHeadManifest {
            document_id: id,
            version: valid_version(version)?,
            body_hash: hash,
            last_checkpoint_id: checkpoint,
        });
    }
    let mut revisions = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,document_id,source_working_version,body_hash,parent_id,body_json,schema_version \
         FROM revisions ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })?;
    for row in rows {
        let (id, document_id, source_version, hash, parent, body, schema_version) = row?;
        if !valid_id(&id) || !valid_id(&document_id) || !valid_hash(&hash) || schema_version != 1 {
            return Err(transfer_error(
                "InvalidBackup",
                "A revision row has invalid identity, schema, or hash metadata.",
            ));
        }
        canonical_body(&body, &hash, "revision body")?;
        revisions.push(RevisionHeadManifest {
            revision_id: id,
            document_id,
            source_version: valid_version(source_version)?,
            body_hash: hash,
            parent_id: parent,
        });
    }
    let mut receipts = connection.prepare(
        "SELECT operation_namespace,operation_id,document_id,payload_hash,result_json \
         FROM command_receipts",
    )?;
    let rows = receipts.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (namespace, operation, document, payload, result) = row?;
        if !valid_id(&namespace)
            || !valid_id(&operation)
            || !valid_id(&document)
            || !valid_hash(&payload)
        {
            return Err(transfer_error(
                "InvalidBackup",
                "A command receipt has invalid identity or hash metadata.",
            ));
        }
        let stored: StoredResult = serde_json::from_str(&result).map_err(|error| {
            transfer_error(
                "InvalidBackup",
                format!("A command receipt result is invalid: {error}"),
            )
        })?;
        if stored.head.document_id != document
            || !valid_hash(&stored.head.body_hash)
            || !valid_version_string(&stored.head.version)
            || !valid_version_string(&stored.saved_generation)
        {
            return Err(transfer_error(
                "InvalidBackup",
                "A command receipt result has invalid head metadata.",
            ));
        }
    }
    Ok(DatabaseHeads {
        info,
        context_source_epoch,
        documents,
        revisions,
    })
}

fn manifest_for(database: &DatabaseHeads, hash: String) -> BackupManifest {
    BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        source_project_id: database.info.project_id.clone(),
        source_operation_namespace: database.info.operation_namespace.clone(),
        database_sha256: hash,
        database_schema_version: DATABASE_SCHEMA_VERSION,
        context_source_epoch: database.context_source_epoch.clone(),
        documents: database.documents.clone(),
        revisions: database.revisions.clone(),
        assets: Vec::new(),
    }
}

fn validate_manifest(manifest: &BackupManifest) -> CoreResult<()> {
    if manifest.format_version != BACKUP_FORMAT_VERSION
        || !(1..=DATABASE_SCHEMA_VERSION).contains(&manifest.database_schema_version)
    {
        return Err(transfer_error(
            "UnsupportedSchema",
            "The backup manifest format or database schema is unsupported.",
        ));
    }
    if !valid_id(&manifest.source_project_id)
        || !valid_id(&manifest.source_operation_namespace)
        || !valid_hash(&manifest.database_sha256)
        || !valid_version_string(&manifest.context_source_epoch)
    {
        return Err(transfer_error(
            "InvalidBackup",
            "The backup manifest identity or database hash is invalid.",
        ));
    }
    if !manifest.assets.is_empty() {
        return Err(transfer_error(
            "InvalidBackup",
            "This backup format does not support attachment entries.",
        ));
    }
    let mut ids = HashSet::new();
    for document in &manifest.documents {
        if !valid_id(&document.document_id)
            || !valid_version_string(&document.version)
            || !valid_hash(&document.body_hash)
            || !ids.insert(document.document_id.as_str())
        {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup manifest contains invalid or duplicate document heads.",
            ));
        }
    }
    let mut revision_ids = HashSet::new();
    for revision in &manifest.revisions {
        if !valid_id(&revision.revision_id)
            || !valid_id(&revision.document_id)
            || !valid_version_string(&revision.source_version)
            || !valid_hash(&revision.body_hash)
            || !revision_ids.insert(revision.revision_id.as_str())
        {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup manifest contains invalid or duplicate revision heads.",
            ));
        }
    }
    Ok(())
}

fn validate_duplicate_basis(manifest: &BackupManifest, basis: &DuplicateBasis) -> CoreResult<()> {
    if !valid_id(&basis.project_id)
        || !valid_id(&basis.operation_namespace)
        || !valid_version_string(&basis.context_source_epoch)
    {
        return Err(transfer_error(
            "InvalidRequest",
            "The duplicate basis contains invalid project identity or epoch metadata.",
        ));
    }
    let mut expected = basis.document_heads.clone();
    expected.sort_by(|left, right| left.document_id.cmp(&right.document_id));
    if expected.iter().any(|head| {
        !valid_id(&head.document_id)
            || !valid_version_string(&head.version)
            || !valid_hash(&head.body_hash)
    }) {
        return Err(transfer_error(
            "InvalidRequest",
            "The duplicate basis contains an invalid document head.",
        ));
    }
    let actual = manifest
        .documents
        .iter()
        .map(|document| Head {
            document_id: document.document_id.clone(),
            version: document.version.clone(),
            body_hash: document.body_hash.clone(),
        })
        .collect::<Vec<_>>();
    if manifest.source_project_id != basis.project_id
        || manifest.source_operation_namespace != basis.operation_namespace
        || manifest.context_source_epoch != basis.context_source_epoch
        || actual != expected
    {
        return Err(transfer_error(
            "VersionConflict",
            "The project changed before its duplicate backup completed.",
        ));
    }
    Ok(())
}

fn write_archive(staged_db: &Path, manifest: &BackupManifest, archive: &Path) -> CoreResult<()> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let manifest_json = serde_json::to_vec(manifest)?;
    writer
        .start_file("manifest.json", options)
        .map_err(zip_error)?;
    writer.write_all(&manifest_json)?;
    writer.start_file(DB_FILE, options).map_err(zip_error)?;
    let mut db = File::open(staged_db)?;
    std::io::copy(&mut db, &mut writer)?;
    let file = writer.finish().map_err(zip_error)?;
    file.sync_all()?;
    Ok(())
}

struct ArchiveContents {
    manifest: BackupManifest,
    database: Vec<u8>,
}

fn read_archive(path: &Path) -> CoreResult<ArchiveContents> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(transfer_error(
            "InvalidBackup",
            "The backup archive exceeds the size limit.",
        ));
    }
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    if archive.len() != 2 {
        return Err(transfer_error(
            "InvalidBackup",
            "A backup must contain exactly manifest.json and project.sqlite3.",
        ));
    }
    let mut seen = HashSet::new();
    let mut manifest_bytes = None;
    let mut database = None;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().to_owned();
        if !seen.insert(name.clone()) {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup contains a duplicate entry.",
            ));
        }
        if Path::new(&name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            || name.contains('\\')
        {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup contains a path traversal entry.",
            ));
        }
        if entry.encrypted() || entry.is_dir() || entry.is_symlink() || !entry.is_file() {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup contains an encrypted, directory, symlink, or non-file entry.",
            ));
        }
        if entry.compression() != CompressionMethod::Stored {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup contains compressed data; only stored entries are accepted.",
            ));
        }
        let max_entry_size = if name == "manifest.json" {
            MAX_MANIFEST_BYTES
        } else {
            MAX_DATABASE_BYTES
        };
        if entry.size() > max_entry_size
            || entry.compressed_size() > MAX_DATABASE_BYTES
            || entry.size() != entry.compressed_size()
        {
            return Err(transfer_error(
                "InvalidBackup",
                "The backup contains an oversized or expanding entry.",
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(|error| {
            transfer_error(
                "InvalidBackup",
                format!("Could not read backup entry: {error}"),
            )
        })?;
        match name.as_str() {
            "manifest.json" if manifest_bytes.is_none() => manifest_bytes = Some(bytes),
            DB_FILE if database.is_none() => database = Some(bytes),
            _ => {
                return Err(transfer_error(
                    "InvalidBackup",
                    format!("Unexpected backup entry {name:?}."),
                ));
            }
        }
    }
    let manifest_bytes = manifest_bytes
        .ok_or_else(|| transfer_error("InvalidBackup", "The backup manifest is missing."))?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(transfer_error(
            "InvalidBackup",
            "The backup manifest is too large.",
        ));
    }
    let mut manifest: BackupManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            transfer_error(
                "InvalidBackup",
                format!("The backup manifest is invalid: {error}"),
            )
        })?;
    if manifest.database_schema_version == 1 && manifest.context_source_epoch.is_empty() {
        manifest.context_source_epoch = "0".into();
    }
    validate_manifest(&manifest)?;
    let database = database
        .ok_or_else(|| transfer_error("InvalidBackup", "The backup database is missing."))?;
    if sha256_bytes(&database) != manifest.database_sha256 {
        return Err(transfer_error(
            "InvalidBackup",
            "The backup database hash does not match its manifest.",
        ));
    }
    Ok(ArchiveContents { manifest, database })
}

/// Create a consistent stored ZIP backup without pausing the project actor.
pub fn create_backup(project: &ProjectSession, target: &Path) -> CoreResult<BackupManifest> {
    let metadata = project.project_metadata()?;
    assert_marker_identity(&project.path, &metadata.project)?;
    let target = output_outside(&project.path, target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The backup destination has no parent directory.",
        )
    })?;
    let staged = stage(parent, "wns-backup")?;
    let staged_db = staged.path.join(DB_FILE);
    online_backup(project, &staged_db)?;
    let database = validate_database(&staged_db, Some(&metadata.project))?;
    let manifest = manifest_for(&database, sha256_file(&staged_db)?);
    let staged_archive = staged.path.join("backup.wnsbackup");
    write_archive(&staged_db, &manifest, &staged_archive)?;
    install_file_no_replace(&staged_archive, &target)?;
    Ok(manifest)
}

fn write_marker(root: &Path, info: &ProjectInfo) -> CoreResult<()> {
    let marker = root.join(MARKER_FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)?;
    file.write_all(&serde_json::to_vec_pretty(info)?)?;
    file.sync_all()?;
    Ok(())
}

fn rotate_identity(path: &Path, old: &ProjectInfo, new: &ProjectInfo) -> CoreResult<()> {
    let mut connection = Connection::open(path)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = tx.execute(
        "UPDATE project SET id=?,operation_namespace=?,title=?,format_version=? \
         WHERE singleton=1 AND id=? AND operation_namespace=?",
        rusqlite::params![
            new.project_id,
            new.operation_namespace,
            new.title,
            new.format_version,
            old.project_id,
            old.operation_namespace
        ],
    )?;
    if changed != 1 {
        return Err(transfer_error(
            "InvalidBackup",
            "The recovered project identity could not be rotated.",
        ));
    }
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(())
}

/// Recover a backup into a new project directory with a fresh identity.
pub fn recover_backup(archive: &Path, target: &Path, title: &str) -> CoreResult<ProjectSession> {
    let target = output_new(target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The recovered project destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-recover-{}", Uuid::new_v4()));
    let result = recover_backup_staged(
        archive,
        &staging,
        &target,
        title,
        &CreationOrigin {
            operation_namespace: Uuid::new_v4().to_string(),
            operation_id: Uuid::new_v4().to_string(),
        },
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Recover into caller-selected staging and destination paths.
///
/// The staging folder is created exactly at the supplied path, and the
/// creation origin is written before installation. This is the entry point for
/// a library registry that has already recorded its pending operation.
pub fn recover_backup_staged(
    archive: &Path,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<ProjectSession> {
    validate_title(title)?;
    let target = output_path(target)?;
    let staging = output_path(staging)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The recovered project destination has no parent directory.",
        )
    })?;
    if staging.parent() != Some(parent) || staging == target {
        return Err(transfer_error(
            "InvalidRequest",
            "Staging must be a separate folder beside the destination.",
        ));
    }

    // A registry retry may arrive after the final rename succeeded but before
    // its completion record was written. The creation origin is the exact
    // operation identity, so reopening that final folder is safe and leaves
    // any diagnostic staging folder untouched.
    if target.exists() {
        if read_creation_origin(&target).ok().as_ref() == Some(origin) {
            if marker(&target)?.title != title {
                return Err(transfer_error(
                    "InvalidRequest",
                    "The recovery title does not match the completed destination.",
                ));
            }
            return ProjectSession::create_staged(&staging, &target, title, origin);
        }
        return Err(transfer_error(
            "TargetExists",
            "The recovery destination already exists.",
        ));
    }

    // A crash can leave a fully prepared folder beside the destination. Only
    // resume it when its creation record matches this operation; unknown or
    // malformed staging remains in place for inspection.
    if staging.exists() {
        if read_creation_origin(&staging).ok().as_ref() != Some(origin) {
            return Err(transfer_error(
                "IncompleteCreation",
                "The requested recovery staging folder belongs to another operation; inspect it before retrying.",
            ));
        }
        let staged_info = marker(&staging)?;
        assert_marker_identity(&staging, &staged_info)?;
        let staged_db = staging.join(DB_FILE);
        validate_database(&staged_db, Some(&staged_info))?;
        if staged_info.title != title {
            return Err(transfer_error(
                "InvalidRequest",
                "The recovery title does not match the prepared staging folder.",
            ));
        }
        return ProjectSession::create_staged(&staging, &target, title, origin);
    }

    let contents = read_archive(archive)?;
    fs::create_dir(&staging)?;
    let staged_db = staging.join(DB_FILE);
    let mut database = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_db)?;
    database.write_all(&contents.database)?;
    database.sync_all()?;
    drop(database);
    let mut migrated = Connection::open(&staged_db)?;
    storage::migrate(&mut migrated, &staging)?;
    storage::configure(&migrated)?;
    drop(migrated);
    let old = validate_database(&staged_db, None)?;
    if old.info.project_id != contents.manifest.source_project_id
        || old.info.operation_namespace != contents.manifest.source_operation_namespace
        || old.context_source_epoch != contents.manifest.context_source_epoch
        || old.documents != contents.manifest.documents
        || old.revisions != contents.manifest.revisions
    {
        return Err(transfer_error(
            "InvalidBackup",
            "The backup manifest does not match the database heads.",
        ));
    }
    let new = ProjectInfo {
        project_id: Uuid::new_v4().to_string(),
        operation_namespace: Uuid::new_v4().to_string(),
        title: title.to_owned(),
        format_version: 1,
    };
    rotate_identity(&staged_db, &old.info, &new)?;
    write_marker(&staging, &new)?;
    assert_marker_identity(&staging, &new)?;
    validate_database(&staged_db, Some(&new))?;
    write_creation_origin(&staging, origin)?;
    // ProjectSession owns the final directory rename and exact-origin
    // semantics used by new project setup.
    ProjectSession::create_staged(&staging, &target, title, origin)
}

/// Duplicate a live project by taking an online backup and recovering it.
pub fn duplicate_project(
    project: &ProjectSession,
    target: &Path,
    title: &str,
) -> CoreResult<ProjectSession> {
    let target = output_outside(&project.path, target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The duplicate destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-duplicate-stage-{}", Uuid::new_v4()));
    let result = duplicate_project_staged(
        project,
        &staging,
        &target,
        title,
        &CreationOrigin {
            operation_namespace: Uuid::new_v4().to_string(),
            operation_id: Uuid::new_v4().to_string(),
        },
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Duplicate using caller-selected staging and destination paths.
pub fn duplicate_project_staged(
    project: &ProjectSession,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<ProjectSession> {
    duplicate_project_staged_inner(project, staging, target, title, origin, None)
}

/// Duplicate using a caller-captured source identity, epoch, and document
/// heads. The expected basis is checked against the consistent backup before
/// the independent project is installed.
pub fn duplicate_project_with_basis(
    project: &ProjectSession,
    target: &Path,
    title: &str,
    basis: &DuplicateBasis,
) -> CoreResult<ProjectSession> {
    let target = output_outside(&project.path, target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The duplicate destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-duplicate-stage-{}", Uuid::new_v4()));
    let result = duplicate_project_staged_inner(
        project,
        &staging,
        &target,
        title,
        &CreationOrigin {
            operation_namespace: Uuid::new_v4().to_string(),
            operation_id: Uuid::new_v4().to_string(),
        },
        Some(basis),
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn duplicate_project_staged_with_basis(
    project: &ProjectSession,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
    basis: &DuplicateBasis,
) -> CoreResult<ProjectSession> {
    duplicate_project_staged_inner(project, staging, target, title, origin, Some(basis))
}

fn duplicate_project_staged_inner(
    project: &ProjectSession,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
    basis: Option<&DuplicateBasis>,
) -> CoreResult<ProjectSession> {
    validate_title(title)?;
    let target = output_outside(&project.path, target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The duplicate destination has no parent directory.",
        )
    })?;
    let staging = output_path(staging)?;
    if staging.parent() != Some(parent) || staging == target {
        return Err(transfer_error(
            "InvalidRequest",
            "Staging must be a separate folder beside the destination.",
        ));
    }
    // A registry retry may have a prepared folder (or a completed final
    // folder) from this exact operation. Reconcile that durable state before
    // consulting the live source again; the source may have changed since the
    // original backup was captured.
    let resumable = (target.exists()
        && read_creation_origin(&target).ok().as_ref() == Some(origin))
        || (!target.exists()
            && staging.exists()
            && read_creation_origin(&staging).ok().as_ref() == Some(origin));
    if resumable {
        let no_read_archive = parent.join(format!(
            ".wns-duplicate-resume-{}.wnsbackup",
            Uuid::new_v4()
        ));
        return recover_backup_staged(&no_read_archive, &staging, &target, title, origin);
    }
    let temporary = parent.join(format!(".wns-duplicate-{}.wnsbackup", Uuid::new_v4()));
    let result = create_backup(project, &temporary).and_then(|manifest| {
        if let Some(basis) = basis {
            validate_duplicate_basis(&manifest, basis)?;
        }
        recover_backup_staged(&temporary, &staging, &target, title, origin)
    });
    let _ = fs::remove_file(&temporary);
    result
}

fn plain_projection(body: &Value) -> CoreResult<String> {
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The checkpoint body has no document blocks.",
            )
        })?;
    let mut rendered = Vec::with_capacity(blocks.len());
    for block in blocks {
        let object = block.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The checkpoint contains an invalid block.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("sceneBreak") => rendered.push("[Scene break]".to_owned()),
            Some("paragraph") | Some("heading") => {
                let mut text = String::new();
                if let Some(content) = object.get("content").and_then(Value::as_array) {
                    for inline in content {
                        let inline = inline.as_object().ok_or_else(|| {
                            transfer_error(
                                "InvalidDocument",
                                "The checkpoint contains an invalid inline node.",
                            )
                        })?;
                        match inline.get("type").and_then(Value::as_str) {
                            Some("text") => text.push_str(
                                inline.get("text").and_then(Value::as_str).ok_or_else(|| {
                                    transfer_error("InvalidDocument", "A text node has no text.")
                                })?,
                            ),
                            Some("hardBreak") => text.push('\n'),
                            _ => {
                                return Err(transfer_error(
                                    "InvalidDocument",
                                    "The checkpoint contains an unsupported inline node.",
                                ));
                            }
                        }
                    }
                }
                rendered.push(text);
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The checkpoint contains an unsupported block.",
                ));
            }
        }
    }
    Ok(rendered.join("\n\n"))
}

/// Checkpoint an exact saved head, then export its body as a labelled UTF-8 draft.
pub fn export_draft_txt(
    project: &ProjectSession,
    access: ProjectAccess,
    head: Head,
    target: &Path,
) -> CoreResult<ExportManifest> {
    let target = output_outside(&project.path, target)?;
    let revision: Revision = project.checkpoint(CheckpointRequest {
        access,
        expected: head.clone(),
        reason: CheckpointReason::Export,
    })?;
    if revision.head != head {
        return Err(transfer_error(
            "VersionConflict",
            "The export checkpoint did not retain the requested head.",
        ));
    }
    let text = plain_projection(&revision.body)?;
    let bytes = text.as_bytes();
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The export destination has no parent directory.",
        )
    })?;
    let staged = stage(parent, "wns-export")?;
    let staged_file = staged.path.join("draft.txt");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_file)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let manifest = ExportManifest {
        format: "text/plain; charset=utf-8".into(),
        format_loss: "Rich marks and links are omitted; hard breaks remain newlines and scene breaks become [Scene break].".into(),
        project_id: project.project_metadata()?.project.project_id,
        document_id: head.document_id.clone(),
        source_head: head,
        checkpoint_id: revision.id,
        utf8_bytes: bytes.len() as u64,
        sha256: sha256_bytes(bytes),
    };
    install_file_no_replace(&staged_file, &target)?;
    Ok(manifest)
}
