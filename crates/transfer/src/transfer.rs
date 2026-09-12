//! Staged project transfer operations.
//!
//! Backups use SQLite's online backup API from a separate read-only connection,
//! then package the resulting database and a small manifest in a stored ZIP.
//! Restore always creates a new project identity; it never replaces the source
//! project or reuses its active operation namespace.

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, backup::Backup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use wns_documents::records::{CheckpointReason, CheckpointRequest};
use wns_kernel::{
    CoreError, CoreResult, DocumentRole, Head, ProjectAccess, ProjectInfo, SourceEpoch,
    StoredResult,};
use wns_kernel::validate_snapshot_json;
use wns_story::source_pins::AUTHOR_ROOM_AUDIENCE;
use wns_storage::{configure, migrate};
use wns_storage::{CreationOrigin, read_creation_origin, write_creation_origin};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::host::{TransferFactory, TransferSource};

const BACKUP_FORMAT_VERSION: u32 = 1;
const DATABASE_SCHEMA_VERSION: u32 = wns_storage::LATEST_SCHEMA_VERSION as u32;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const DB_FILE: &str = "project.sqlite3";
const MARKER_FILE: &str = "project.wns.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupManifest {
    pub format_version: u32,
    pub source_project_id: String,
    pub source_operation_namespace: String,
    pub database_sha256: String,
    pub database_schema_version: u32,
    #[serde(default)]
    pub context_source_epoch: SourceEpoch,
    pub documents: Vec<DocumentHeadManifest>,
    pub revisions: Vec<RevisionHeadManifest>,
    pub assets: Vec<AssetManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentHeadManifest {
    pub document_id: String,
    pub version: String,
    pub body_hash: String,
    pub last_checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionHeadManifest {
    pub revision_id: String,
    pub document_id: String,
    pub source_version: String,
    pub body_hash: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetManifest {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DraftFormat {
    PlainText,
    Markdown,
}

impl DraftFormat {
    pub(crate) fn storage_name(self) -> &'static str {
        match self {
            Self::PlainText => "plainText",
            Self::Markdown => "markdown",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self, ()> {
        match value {
            "plainText" => Ok(Self::PlainText),
            "markdown" => Ok(Self::Markdown),
            _ => Err(()),
        }
    }

    pub(crate) fn is_supported(self) -> bool {
        matches!(self, Self::PlainText | Self::Markdown)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftExportPreview {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub source_head: Head,
    pub revision_id: String,
    pub format: DraftFormat,
    pub format_version: u32,
    pub utf8_bytes: u64,
    pub sha256: String,
    pub format_loss: String,
    pub preview_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportRecord {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub document_id: String,
    pub source_head: Head,
    pub revision_id: String,
    pub working_draft: bool,
    pub format: DraftFormat,
    pub format_version: u32,
    pub utf8_bytes: u64,
    pub sha256: String,
    pub basename: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_bundle_id: Option<String>,
}

pub(crate) struct ProjectedDraft {
    pub text: String,
    pub utf8_bytes: u64,
    pub sha256: String,
}

/// Frozen source identity and document heads used to guard a duplicate
/// against a live-project change between the UI read and the backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateBasis {
    pub project_id: String,
    pub operation_namespace: String,
    pub context_source_epoch: SourceEpoch,
    pub document_heads: Vec<Head>,
}

pub fn capture_duplicate_basis(
    project: &impl TransferSource,
    access: ProjectAccess,
) -> CoreResult<DuplicateBasis> {
    let metadata = project.metadata()?;
    let mut document_heads = project
        .document_records(&access)?
        .into_iter()
        .map(|document| document.head)
        .collect::<Vec<_>>();
    document_heads.sort_by(|left, right| left.document_id.cmp(&right.document_id));
    Ok(DuplicateBasis {
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        context_source_epoch: project.source_epoch()?,
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

fn valid_import_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
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

pub(crate) fn install_export_file(root: &Path, target: &Path, bytes: &[u8]) -> CoreResult<()> {
    let target = output_outside(root, target)?;
    install_new_text_file(&target, bytes)
}

fn install_new_text_file(target: &Path, bytes: &[u8]) -> CoreResult<()> {
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The export destination has no parent directory.",
        )
    })?;
    let staged = stage(parent, "wns-export")?;
    let staged_file = staged.path.join("output");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_file)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    install_file_no_replace(&staged_file, target)
}

/// A separate Markdown copy of an in-memory document, not a project save or
/// backup. No project actor, database, writer lease, or current head is read:
/// this must remain available when project storage itself has failed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryCopyReceipt {
    pub snapshot_hash: String,
    pub sha256: String,
    pub utf8_bytes: u64,
}

pub fn save_recovery_copy(body: &Value, target: &Path) -> CoreResult<RecoveryCopyReceipt> {
    let validated = validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| transfer_error("InvalidDocument", error))?;
    let projected = project_draft(&validated.snapshot, DraftFormat::Markdown)?;
    let target = output_new(target)?;
    let basename = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            transfer_error(
                "InvalidRequest",
                "The recovery copy must have a UTF-8 filename.",
            )
        })?;
    crate::exports::validate_basename(basename)?;
    install_new_text_file(&target, projected.text.as_bytes())?;
    Ok(RecoveryCopyReceipt {
        snapshot_hash: validated.hash,
        sha256: projected.sha256,
        utf8_bytes: projected.utf8_bytes,
    })
}

fn online_backup(project: &impl TransferSource, staged_db: &Path) -> CoreResult<()> {
    let source_path = project.project_path().join(DB_FILE);
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
    // Keep the entire validation pass on one consistent read snapshot.  The
    // same-connection entry point below is also used by import replay, where
    // the caller owns an already-open read-only connection and transaction.
    connection.execute_batch("BEGIN DEFERRED")?;
    let result = validate_project_connection_heads(&connection, expected);
    match result {
        Ok(heads) => {
            connection.execute_batch("COMMIT")?;
            Ok(heads)
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// Validate a project database through an existing read-only connection.
///
/// The caller should hold a `BEGIN DEFERRED` transaction when the validation
/// must share a snapshot with another read performed as part of the same
/// reconciliation.  This function never migrates, writes, or opens another
/// connection; it is therefore safe for import replay after the original
/// source database has disappeared.
pub fn validate_project_connection(
    connection: &Connection,
    expected: &ProjectInfo,
) -> CoreResult<()> {
    validate_project_connection_heads(connection, Some(expected)).map(|_| ())
}

fn validate_project_connection_heads(
    connection: &Connection,
    expected: Option<&ProjectInfo>,
) -> CoreResult<DatabaseHeads> {
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
    wns_story::reviewed_story::validate_review_storage(connection)?;
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
    wns_documents::validate_stored_view_state(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The saved view state is invalid: {error}"),
        )
    })?;
    wns_story::story_context::validate_context_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The story context is invalid: {error}"),
        )
    })?;
    wns_story::context_packets::validate_context_packets(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The prepared requests are invalid: {error}"),
        )
    })?;
    wns_story::memory::validate_memory_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The story memory is invalid: {error}"),
        )
    })?;
    wns_conversation::discussions::validate_provider_results(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The provider results are invalid: {error}"),
        )
    })?;
    wns_workshop::workshop::validate_storage(
        connection,
        &wns_conversation::project_chat::validate_chat_workshop_snapshot,
    ).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The Story Workshop records are invalid: {error}"),
        )
    })?;
    wns_conversation::discussion_lookup::validate_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The discussion lookup records are invalid: {error}"),
        )
    })?;
    wns_conversation::guidance::validate_guidance_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The author guidance records are invalid: {error}"),
        )
    })?;
    wns_conversation::proposals::validate_proposal_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The suggestion history is invalid: {error}"),
        )
    })?;
    wns_documents::history::validate_history_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The document history is invalid: {error}"),
        )
    })?;
    crate::exports::validate_export_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The export records are invalid: {error}"),
        )
    })?;
    validate_source_pin_storage(connection, &info)?;
    validate_import_storage(connection, &info)?;
    let mut documents = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,working_version,body_hash,last_checkpoint_id,body_json,schema_version,role \
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
            row.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (id, version, hash, checkpoint, body, schema_version, role) = row?;
        if !valid_id(&id) || !valid_hash(&hash) || schema_version != 1 {
            return Err(transfer_error(
                "InvalidBackup",
                "A document row has invalid identity, schema, or hash metadata.",
            ));
        }
        DocumentRole::from_storage(&role).map_err(|error| {
            transfer_error(
                "InvalidBackup",
                format!("A document row has an invalid authority role: {error}"),
            )
        })?;
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
        "SELECT operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json \
         FROM command_receipts",
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
        let (namespace, operation, document, payload, operation_kind, result) = row?;
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
        // Project-chat adoption uses a ref-only receipt rather than the
        // ordinary StoredResult envelope.  Its immutable preview, decision,
        // revision, and provenance are validated by the project-chat transfer
        // pass below; do not coerce it into the older document receipt shape.
        if operation_kind != "adoptChatPreview" {
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
    }
    wns_conversation::project_chat::validate_storage(connection).map_err(|error| {
        transfer_error(
            "InvalidBackup",
            format!("The project-chat records are invalid: {error}"),
        )
    })?;
    Ok(DatabaseHeads {
        info,
        context_source_epoch,
        documents,
        revisions,
    })
}

const MAX_SOURCE_PIN_DOCUMENTS: usize = 64;

const MAX_IMPORT_LEGACY_RECORDS: usize = 20_000;
const MAX_IMPORT_LEGACY_BYTES: usize = 4 * 1024 * 1024;

/// Validate the durable source-pin state before a database can be backed up
/// or recovered.  Mutable sets belong to the database's current identity;
/// their receipts deliberately do not, because recovery retains receipts as
/// historical evidence under their original namespace.
fn validate_source_pin_storage(connection: &Connection, info: &ProjectInfo) -> CoreResult<()> {
    let mut sets = connection.prepare(
        "SELECT project_id,operation_namespace,scope,target_document_id,version,
                source_document_ids_json,audience
         FROM source_pin_sets
         ORDER BY project_id,operation_namespace,scope,target_document_id",
    )?;
    let rows = sets.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (project_id, namespace, scope, target, version, ids_json, audience) = row?;
        if project_id != info.project_id || namespace != info.operation_namespace {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin set does not belong to the current project identity.",
            ));
        }
        validate_source_pin_set_fields(&scope, &target, version, &ids_json, &audience)?;
        if scope == "document" {
            validate_ordinary_document_role(connection, &target)?;
        }
        let ids: Vec<String> = serde_json::from_str(&ids_json).map_err(|_| {
            transfer_error("InvalidBackup", "A source-pin set contains invalid JSON.")
        })?;
        for id in ids {
            validate_ordinary_document_role(connection, &id)?;
        }
    }

    // Receipt rows are immutable historical records.  A recovery rotates the
    // live set identity but keeps these original project and namespace values
    // so an audit can still establish where the decision came from.
    let mut receipts = connection.prepare(
        "SELECT project_id,operation_namespace,operation_id,scope,target_document_id,
                expected_version,payload_hash,operation_kind,result_json
         FROM source_pin_receipts
         ORDER BY operation_namespace,operation_id",
    )?;
    let rows = receipts.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (
            project_id,
            namespace,
            operation_id,
            scope,
            target,
            expected_version,
            payload_hash,
            operation_kind,
            result_json,
        ) = row?;
        if !valid_id(&project_id)
            || !valid_id(&namespace)
            || !valid_id(&operation_id)
            || !valid_hash(&payload_hash)
            || operation_kind != "saveSourcePins"
        {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin receipt has invalid identity or operation metadata.",
            ));
        }
        if expected_version < 0 {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin receipt has a negative expected version.",
            ));
        }
        let result: wns_story::source_pins::SourcePinSet = serde_json::from_str(&result_json)
            .map_err(|_| {
                transfer_error(
                    "InvalidBackup",
                    "A source-pin receipt result is not a valid source-pin set.",
                )
            })?;
        validate_source_pin_result(&scope, &target, &result)?;
        if scope == "document" {
            validate_ordinary_document_role(connection, &target)?;
        }
        for id in &result.source_document_ids {
            validate_ordinary_document_role(connection, id)?;
        }
        let result_version = result.version.parse::<i64>().map_err(|_| {
            transfer_error(
                "InvalidBackup",
                "A source-pin receipt has an invalid result version.",
            )
        })?;
        if result_version != expected_version
            && Some(result_version) != expected_version.checked_add(1)
        {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin receipt version does not follow its request.",
            ));
        }
    }
    Ok(())
}

struct ImportManifestRow {
    project_id: String,
    namespace: String,
    operation_id: String,
    source_project_id: String,
    source_schema_version: i64,
    source_sha256: String,
    source_bytes: i64,
    source_title: String,
    source_slug: String,
    request_sha256: String,
    counts_json: String,
    import_format_version: i64,
    created_at: String,
}

fn validate_import_storage(connection: &Connection, info: &ProjectInfo) -> CoreResult<()> {
    let manifest: Option<ImportManifestRow> = connection
        .query_row(
            "SELECT project_id,operation_namespace,operation_id,source_project_id,
                    source_schema_version,source_sha256,source_bytes,source_title,
                    source_slug,request_sha256,counts_json,import_format_version,created_at
             FROM import_manifest WHERE singleton=1",
            [],
            |row| {
                Ok(ImportManifestRow {
                    project_id: row.get(0)?,
                    namespace: row.get(1)?,
                    operation_id: row.get(2)?,
                    source_project_id: row.get(3)?,
                    source_schema_version: row.get(4)?,
                    source_sha256: row.get(5)?,
                    source_bytes: row.get(6)?,
                    source_title: row.get(7)?,
                    source_slug: row.get(8)?,
                    request_sha256: row.get(9)?,
                    counts_json: row.get(10)?,
                    import_format_version: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
        .optional()?;
    let Some(manifest) = manifest else {
        let orphaned: i64 = connection.query_row(
            "SELECT (SELECT COUNT(*) FROM import_id_map)
                    +(SELECT COUNT(*) FROM import_body_decisions)
                    +(SELECT COUNT(*) FROM import_legacy_records)",
            [],
            |row| row.get(0),
        )?;
        if orphaned != 0 {
            return Err(transfer_error(
                "InvalidBackup",
                "Import evidence exists without its manifest.",
            ));
        }
        return Ok(());
    };
    if manifest.project_id != info.project_id
        || manifest.namespace != info.operation_namespace
        || !valid_id(&manifest.project_id)
        || !valid_id(&manifest.namespace)
        || !valid_id(&manifest.operation_id)
        || !valid_import_id(&manifest.source_project_id)
        || manifest.import_format_version != 1
        || manifest.source_schema_version != 8
        || manifest.source_bytes < 0
        || !valid_hash(&manifest.source_sha256)
        || !valid_hash(&manifest.request_sha256)
        || manifest.source_title.is_empty()
        || manifest.source_title.len() > 512
        || manifest.source_title.chars().any(char::is_control)
        || manifest.source_slug.len() > 512
        || manifest.source_slug.chars().any(char::is_control)
        || manifest.counts_json.len() > 64 * 1024
        || !matches!(
            serde_json::from_str::<Value>(&manifest.counts_json),
            Ok(Value::Object(_))
        )
        || manifest.created_at.is_empty()
    {
        return Err(transfer_error(
            "InvalidBackup",
            "The import manifest has invalid identity or source metadata.",
        ));
    }

    let map_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM import_id_map", [], |row| row.get(0))?;
    if usize::try_from(map_count)
        .ok()
        .is_none_or(|count| count > MAX_IMPORT_LEGACY_RECORDS)
    {
        return Err(transfer_error(
            "InvalidBackup",
            "The import identity map is too large.",
        ));
    }
    let mut maps = connection.prepare(
        "SELECT source_table,source_id,v3_document_id,source_project_id FROM import_id_map ORDER BY source_table,source_id",
    )?;
    let rows = maps.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (table, source_id, document_id, mapped_project) = row?;
        if !valid_id(&table)
            || !valid_import_id(&source_id)
            || !valid_id(&document_id)
            || mapped_project != manifest.source_project_id
            || connection
                .query_row(
                    "SELECT 1 FROM documents WHERE id=? AND role='ordinary'",
                    [&document_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                != Some(1)
        {
            return Err(transfer_error(
                "InvalidBackup",
                "The import identity map is invalid.",
            ));
        }
    }

    let decision_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM import_body_decisions", [], |row| {
            row.get(0)
        })?;
    if usize::try_from(decision_count)
        .ok()
        .is_none_or(|count| count > MAX_IMPORT_LEGACY_RECORDS)
    {
        return Err(transfer_error(
            "InvalidBackup",
            "The import body-decision set is too large.",
        ));
    }
    let mut decisions = connection.prepare(
        "SELECT source_chapter_id,choice_kind,source_draft_id,source_working_state,
                source_body_sha256,selected_body_hash
         FROM import_body_decisions ORDER BY source_chapter_id",
    )?;
    let rows = decisions.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (chapter_id, choice, draft_id, working_state, source_hash, selected_hash) = row?;
        let mapped_document: Option<(String, String)> = connection
            .query_row(
                "SELECT v3_document_id,source_project_id FROM import_id_map WHERE source_table='chapters' AND source_id=?",
                [&chapter_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((document_id, mapped_project)) = mapped_document else {
            return Err(transfer_error(
                "InvalidBackup",
                "An import body decision has no chapter map.",
            ));
        };
        let selected_checkpoint: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM revisions
                 WHERE document_id=? AND body_hash=? AND reason='importedV2' LIMIT 1",
                rusqlite::params![&document_id, &selected_hash],
                |row| row.get(0),
            )
            .optional()?;
        let draft_valid = draft_id.as_deref().is_none_or(valid_import_id);
        let shape_valid = valid_import_id(&chapter_id)
            && mapped_project == manifest.source_project_id
            && valid_hash(&source_hash)
            && valid_hash(&selected_hash)
            && selected_checkpoint == Some(1)
            && matches!(working_state.as_str(), "present" | "missing")
            && match choice.as_str() {
                "working" => working_state == "present" && draft_id.is_none(),
                "empty" => working_state == "missing" && draft_id.is_none(),
                "draft" => working_state == "missing" && draft_id.is_some() && draft_valid,
                _ => false,
            };
        if !shape_valid {
            return Err(transfer_error(
                "InvalidBackup",
                "An import body decision is invalid.",
            ));
        }
    }

    let mut legacy_bytes = 0usize;
    let mut legacy_rows = 0usize;
    let mut legacy = connection.prepare(
        "SELECT source_table,source_id,source_project_id,payload_json
         FROM import_legacy_records ORDER BY source_table,source_id",
    )?;
    let rows = legacy.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (table, source_id, mapped_project, payload) = row?;
        legacy_rows = legacy_rows.checked_add(1).ok_or_else(|| {
            transfer_error("InvalidBackup", "Import evidence row count overflow.")
        })?;
        legacy_bytes = legacy_bytes
            .checked_add(payload.len())
            .ok_or_else(|| transfer_error("InvalidBackup", "Import evidence size overflow."))?;
        if legacy_rows > MAX_IMPORT_LEGACY_RECORDS
            || legacy_bytes > MAX_IMPORT_LEGACY_BYTES
            || !valid_id(&table)
            || !valid_import_id(&source_id)
            || mapped_project != manifest.source_project_id
            || serde_json::from_str::<Value>(&payload).is_err()
        {
            return Err(transfer_error(
                "InvalidBackup",
                "An imported legacy record is invalid.",
            ));
        }
    }
    Ok(())
}

fn validate_source_pin_result(
    scope: &str,
    target: &str,
    result: &wns_story::source_pins::SourcePinSet,
) -> CoreResult<()> {
    let expected_scope = match scope {
        "project" => wns_story::source_pins::SourcePinScope::Project,
        "document" => wns_story::source_pins::SourcePinScope::Document,
        _ => {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin receipt has an invalid scope.",
            ));
        }
    };
    let target_matches = match scope {
        "project" => target.is_empty() && result.target_document_id.is_none(),
        "document" => valid_id(target) && result.target_document_id.as_deref() == Some(target),
        _ => false,
    };
    if result.scope != expected_scope
        || !target_matches
        || result.audience != AUTHOR_ROOM_AUDIENCE
        || !valid_version_string(&result.version)
        || result.source_document_ids.iter().any(|id| !valid_id(id))
        || result.source_document_ids.len() > MAX_SOURCE_PIN_DOCUMENTS
        || result
            .source_document_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(transfer_error(
            "InvalidBackup",
            "A source-pin receipt result does not match its recorded request.",
        ));
    }
    Ok(())
}

fn validate_source_pin_set_fields(
    scope: &str,
    target: &str,
    version: i64,
    ids_json: &str,
    audience: &str,
) -> CoreResult<()> {
    if audience != AUTHOR_ROOM_AUDIENCE || version < 0 {
        return Err(transfer_error(
            "InvalidBackup",
            "A source-pin set has invalid audience or version metadata.",
        ));
    }
    match scope {
        "project" if target.is_empty() => {}
        "document" if valid_id(target) => {}
        _ => {
            return Err(transfer_error(
                "InvalidBackup",
                "A source-pin set has an invalid scope or target.",
            ));
        }
    }
    let ids: Vec<String> = serde_json::from_str(ids_json)
        .map_err(|_| transfer_error("InvalidBackup", "A source-pin set contains invalid JSON."))?;
    if ids.len() > MAX_SOURCE_PIN_DOCUMENTS
        || ids.iter().any(|id| !valid_id(id))
        || ids.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(transfer_error(
            "InvalidBackup",
            "A source-pin set must contain at most 64 sorted unique document IDs.",
        ));
    }
    // A source may have been trashed after the pin was saved.  Such an ID is
    // retained so the author can remove it; discussion preparation refuses it
    // when it is no longer resolvable.  Structural ID validation above keeps
    // the transfer boundary bounded without destroying repairable state.
    Ok(())
}

fn validate_ordinary_document_role(connection: &Connection, document_id: &str) -> CoreResult<()> {
    let role: Option<String> = connection
        .query_row(
            "SELECT role FROM documents WHERE id=?",
            [document_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(role) = role
        && role != DocumentRole::Ordinary.storage_name()
    {
        return Err(transfer_error(
            "InvalidBackup",
            "A source-pin reference points at a non-ordinary document.",
        ));
    }
    Ok(())
}

fn manifest_for(database: &DatabaseHeads, hash: String) -> BackupManifest {
    BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        source_project_id: database.info.project_id.clone(),
        source_operation_namespace: database.info.operation_namespace.clone(),
        database_sha256: hash,
        database_schema_version: DATABASE_SCHEMA_VERSION,
        context_source_epoch: database.context_source_epoch.clone().into(),
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
pub fn create_backup(project: &impl TransferSource, target: &Path) -> CoreResult<BackupManifest> {
    let metadata = project.metadata()?;
    assert_marker_identity(project.project_path(), &metadata.project)?;
    let target = output_outside(project.project_path(), target)?;
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
    // Reviews in a recovered project remain historical author decisions.
    // A new independent project needs its own explicit current selections.
    tx.execute("DELETE FROM ready_heads", [])?;
    // Mutable source-pin sets follow the recovered live identity.  Receipts
    // are intentionally left untouched: their original namespace is the
    // historical authority and cannot authorize writes in the new copy.
    tx.execute(
        "UPDATE source_pin_sets SET project_id=?,operation_namespace=?
         WHERE project_id=? AND operation_namespace=?",
        rusqlite::params![
            new.project_id,
            new.operation_namespace,
            old.project_id,
            old.operation_namespace
        ],
    )?;
    tx.execute(
        "UPDATE import_manifest SET project_id=?,operation_namespace=?
         WHERE project_id=? AND operation_namespace=?",
        rusqlite::params![
            new.project_id,
            new.operation_namespace,
            old.project_id,
            old.operation_namespace
        ],
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(())
}

/// Recover a backup into a new project directory with a fresh identity.
pub fn recover_backup<F: TransferFactory>(archive: &Path, target: &Path, title: &str) -> CoreResult<F::Session> {
    let target = output_new(target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The recovered project destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-recover-{}", Uuid::new_v4()));
    let result = recover_backup_staged::<F>(
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
pub fn recover_backup_staged<F: TransferFactory>(
    archive: &Path,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<F::Session> {
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
            return F::create_staged(&staging, &target, title, origin);
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
        return F::create_staged(&staging, &target, title, origin);
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
    migrate(&mut migrated, &staging)?;
    configure(&migrated)?;
    drop(migrated);
    let old = validate_database(&staged_db, None)?;
    if old.info.project_id != contents.manifest.source_project_id
        || old.info.operation_namespace != contents.manifest.source_operation_namespace
        || old.context_source_epoch != contents.manifest.context_source_epoch.as_str()
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
    F::create_staged(&staging, &target, title, origin)
}

/// Duplicate a live project by taking an online backup and recovering it.
pub fn duplicate_project<F: TransferFactory>(
    project: &impl TransferSource,
    target: &Path,
    title: &str,
) -> CoreResult<F::Session> {
    let target = output_outside(project.project_path(), target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The duplicate destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-duplicate-stage-{}", Uuid::new_v4()));
    let result = duplicate_project_staged::<F>(
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
pub fn duplicate_project_staged<F: TransferFactory>(
    project: &impl TransferSource,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<F::Session> {
    duplicate_project_staged_inner::<F>(project, staging, target, title, origin, None)
}

/// Duplicate using a caller-captured source identity, epoch, and document
/// heads. The expected basis is checked against the consistent backup before
/// the independent project is installed.
pub fn duplicate_project_with_basis<F: TransferFactory>(
    project: &impl TransferSource,
    target: &Path,
    title: &str,
    basis: &DuplicateBasis,
) -> CoreResult<F::Session> {
    let target = output_outside(project.project_path(), target)?;
    let parent = target.parent().ok_or_else(|| {
        transfer_error(
            "InvalidRequest",
            "The duplicate destination has no parent directory.",
        )
    })?;
    let staging = parent.join(format!(".wns-duplicate-stage-{}", Uuid::new_v4()));
    let result = duplicate_project_staged_inner::<F>(
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

pub fn duplicate_project_staged_with_basis<F: TransferFactory>(
    project: &impl TransferSource,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
    basis: &DuplicateBasis,
) -> CoreResult<F::Session> {
    duplicate_project_staged_inner::<F>(project, staging, target, title, origin, Some(basis))
}

fn duplicate_project_staged_inner<F: TransferFactory>(
    project: &impl TransferSource,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
    basis: Option<&DuplicateBasis>,
) -> CoreResult<F::Session> {
    validate_title(title)?;
    let target = output_outside(project.project_path(), target)?;
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
        return recover_backup_staged::<F>(&no_read_archive, &staging, &target, title, origin);
    }
    let temporary = parent.join(format!(".wns-duplicate-{}.wnsbackup", Uuid::new_v4()));
    let result = create_backup(project, &temporary).and_then(|manifest| {
        if let Some(basis) = basis {
            validate_duplicate_basis(&manifest, basis)?;
        }
        recover_backup_staged::<F>(&temporary, &staging, &target, title, origin)
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

pub(crate) fn draft_format_loss(format: DraftFormat) -> &'static str {
    match format {
        DraftFormat::PlainText => {
            "Rich marks and links are omitted; hard breaks remain newlines and scene breaks become [Scene break]."
        }
        DraftFormat::Markdown => {
            "Markdown represents headings, bold, italic, safe links, and scene breaks. Spacing and end-of-paragraph breaks depend on the reader. Comments, history, and other project material are omitted."
        }
    }
}

/// Validate a complete immutable snapshot before projecting it into an export
/// format.  The returned bytes are the exact UTF-8 content installed on disk.
pub(crate) fn project_draft(body: &Value, format: DraftFormat) -> CoreResult<ProjectedDraft> {
    let encoded = serde_json::to_string(body)?;
    let validated = validate_snapshot_json(&encoded).map_err(|error| {
        transfer_error(
            "InvalidDocument",
            format!("The export source is invalid: {error}"),
        )
    })?;
    let text = match format {
        DraftFormat::PlainText => plain_projection(&validated.snapshot)?,
        DraftFormat::Markdown => markdown_projection(&validated.snapshot)?,
    };
    let utf8_bytes = text.len() as u64;
    let sha256 = sha256_bytes(text.as_bytes());
    Ok(ProjectedDraft {
        text,
        utf8_bytes,
        sha256,
    })
}

fn markdown_projection(body: &Value) -> CoreResult<String> {
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source has no document blocks.",
            )
        })?;
    let mut rendered = Vec::with_capacity(blocks.len());
    for block in blocks {
        let object = block.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source contains an invalid block.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("sceneBreak") => rendered.push("---".to_owned()),
            Some("paragraph") => rendered.push(markdown_block_text(object)?),
            Some("heading") => {
                let level = object
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("level"))
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        transfer_error("InvalidDocument", "The heading has no valid level.")
                    })?;
                let text = markdown_block_text(object)?;
                let prefix = "#".repeat(usize::try_from(level).map_err(|_| {
                    transfer_error("InvalidDocument", "The heading level is too large.")
                })?);
                rendered.push(if text.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {text}")
                });
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported block.",
                ));
            }
        }
    }
    // Markdown export has one deterministic line ending and never appends a
    // newline after the final block.  Source-backed trailing empty blocks and
    // hard breaks remain part of the projected content.
    let joined = rendered.join("\n\n");
    Ok(normalize_lf(&joined))
}

fn markdown_block_text(block: &serde_json::Map<String, Value>) -> CoreResult<String> {
    let Some(content) = block.get("content").and_then(Value::as_array) else {
        return Ok(String::new());
    };
    let mut result = String::new();
    for inline in content {
        let object = inline.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source contains an invalid inline node.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("hardBreak") => result.push_str("  \n"),
            Some("text") => {
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| transfer_error("InvalidDocument", "A text node has no text."))?;
                result.push_str(&markdown_text(text, object.get("marks"))?);
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported inline node.",
                ));
            }
        }
    }
    Ok(result)
}

fn markdown_text(text: &str, marks: Option<&Value>) -> CoreResult<String> {
    let text = normalize_lf(text);
    let marks = marks.and_then(Value::as_array).cloned().unwrap_or_default();
    let (leading, core, trailing) = boundary_whitespace(&text);
    if core.is_empty() {
        return Ok(markdown_leading_whitespace(&text));
    }
    let leading = markdown_leading_whitespace(leading);
    let mut rendered = markdown_escape(core);
    let mut link: Option<String> = None;
    for mark in marks {
        let object = mark
            .as_object()
            .ok_or_else(|| transfer_error("InvalidDocument", "A text mark is not an object."))?;
        match object.get("type").and_then(Value::as_str) {
            Some("bold") => rendered = format!("**{rendered}**"),
            Some("italic") => rendered = format!("*{rendered}*"),
            Some("link") => {
                let href = object
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("href"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| transfer_error("InvalidDocument", "A link mark has no href."))?;
                link = Some(href.to_owned());
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported mark.",
                ));
            }
        }
    }
    if let Some(href) = link {
        // Angle-bracket destinations preserve punctuation such as parentheses
        // and query entities.  Character references keep raw angle brackets
        // from terminating the CommonMark destination while preserving the
        // URL seen by a Markdown parser.
        let href = markdown_href(&href);
        rendered = format!("[{rendered}](<{href}>)");
    }
    Ok(format!("{leading}{rendered}{trailing}"))
}

fn markdown_leading_whitespace(value: &str) -> String {
    let indent = value
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .fold(0, |column, ch| {
            if ch == '\t' {
                column + 4 - column % 4
            } else {
                column + 1
            }
        });
    if indent < 4 {
        return value.to_owned();
    }
    let first = value.as_bytes()[0];
    let entity = match first {
        b' ' => "&#32;",
        b'\t' => "&#9;",
        _ => unreachable!("the leading character must be an ASCII indent"),
    };
    format!("{entity}{}", &value[1..])
}

fn boundary_whitespace(value: &str) -> (&str, &str, &str) {
    let leading_end = value
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    let trailing_start = value[leading_end..]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, ch)| leading_end + index + ch.len_utf8())
        .unwrap_or(leading_end);
    (
        &value[..leading_end],
        &value[leading_end..trailing_start],
        &value[trailing_start..],
    )
}

fn markdown_escape(value: &str) -> String {
    const ESCAPED: &str = r#"\\`*_{}[]()#+-!<>|~&"#;
    let mut result = String::with_capacity(value.len());
    let mut line_start = 0;
    for (index, ch) in value.char_indices() {
        if ch == '\n' {
            result.push_str("  \n");
            line_start = index + 1;
        } else {
            // Ordinary sentence/decimal periods are readable as-is. Escape a
            // possible ordered-list marker at a line's start (CommonMark 5.2).
            let ordered_marker = if ch == '.' {
                let prefix = value[line_start..index].trim_start_matches([' ', '\t']);
                (1..=9).contains(&prefix.len())
                    && prefix.bytes().all(|byte| byte.is_ascii_digit())
                    && value[index + 1..]
                        .chars()
                        .next()
                        .is_none_or(|next| matches!(next, ' ' | '\t' | '\n'))
            } else {
                false
            };
            if ESCAPED.contains(ch) || ordered_marker {
                result.push('\\');
            }
            result.push(ch);
        }
    }
    result
}

fn markdown_href(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(ch),
        }
    }
    result
}

fn normalize_lf(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            result.push('\n');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Freeze the requested current head and return a complete, tamper-detecting
/// preview.  The preview refers to this immutable revision even if the
/// working document changes before the file is installed.
pub fn prepare_draft_export(
    project: &impl TransferSource,
    access: &ProjectAccess,
    expected: Head,
    format: DraftFormat,
) -> CoreResult<DraftExportPreview> {
    if !format.is_supported() {
        return Err(transfer_error(
            "InvalidRequest",
            "The requested export format is unsupported.",
        ));
    }
    let revision = project.checkpoint(CheckpointRequest {
        access: access.clone(),
        expected,
        reason: CheckpointReason::Export,
    })?;
    let projected = project_draft(&revision.body, format)?;
    let metadata = project.metadata()?;
    Ok(DraftExportPreview {
        id: Uuid::new_v4().to_string(),
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        source_head: revision.head,
        revision_id: revision.id,
        format,
        format_version: 1,
        utf8_bytes: projected.utf8_bytes,
        sha256: projected.sha256,
        format_loss: draft_format_loss(format).into(),
        preview_text: projected.text,
        review_bundle_id: None,
    })
}

/// Freeze the current author-reviewed chapter source without creating a
/// checkpoint.  The project actor resolves the active ReadyBundle and returns
/// its immutable target revision; the caller-supplied head is only evidence for
/// that resolution.
pub fn prepare_reviewed_draft_export(
    project: &impl TransferSource,
    access: &ProjectAccess,
    expected: Head,
    format: DraftFormat,
) -> CoreResult<DraftExportPreview> {
    if !format.is_supported() {
        return Err(transfer_error(
            "InvalidRequest",
            "The requested export format is unsupported.",
        ));
    }
    let (review_bundle_id, revision) =
        project.resolve_reviewed_export_source(access.clone(), expected)?;
    let projected = project_draft(&revision.body, format)?;
    let metadata = project.metadata()?;
    Ok(DraftExportPreview {
        id: Uuid::new_v4().to_string(),
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        source_head: revision.head,
        revision_id: revision.id,
        format,
        format_version: 1,
        utf8_bytes: projected.utf8_bytes,
        sha256: projected.sha256,
        format_loss: draft_format_loss(format).into(),
        preview_text: projected.text,
        review_bundle_id: Some(review_bundle_id),
    })
}

/// Install one prepared whole-document export and then record its immutable
/// metadata.  Filesystem installation and SQLite recording intentionally have
/// separate failure boundaries; a record failure leaves the installed output
/// in place and asks the caller to prepare a new explicit export.
pub fn export_prepared_draft(
    project: &impl TransferSource,
    access: ProjectAccess,
    preview: DraftExportPreview,
    target: &Path,
) -> CoreResult<ExportRecord> {
    let candidate = output_path(target)?;
    let basename = candidate
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            transfer_error(
                "InvalidRequest",
                "The destination must have a UTF-8 basename.",
            )
        })?
        .to_owned();
    crate::exports::validate_basename(&basename)?;
    project.install_export(access, preview, candidate, basename)
}
