use super::*;

pub(crate) struct StageDir {
    pub(crate) path: PathBuf,
}

impl Drop for StageDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn transfer_error(code: &str, detail: impl Into<String>) -> CoreError {
    CoreError::new(code, &detail.into())
}

pub(crate) fn zip_error(error: zip::result::ZipError) -> CoreError {
    transfer_error(
        "InvalidBackup",
        format!("backup archive is invalid: {error}"),
    )
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn sha256_file(path: &Path) -> CoreResult<String> {
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

pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn valid_import_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn valid_version(value: i64) -> CoreResult<String> {
    if value < 0 {
        return Err(transfer_error(
            "InvalidBackup",
            "SQLite contains a negative working version.",
        ));
    }
    Ok(value.to_string())
}

pub(crate) fn valid_version_string(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

pub(crate) fn validate_title(title: &str) -> CoreResult<()> {
    if title.trim().is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
        return Err(transfer_error(
            "InvalidRequest",
            "Enter a title of at most 512 bytes without control characters.",
        ));
    }
    Ok(())
}

pub(crate) fn canonical_body(body_json: &str, hash: &str, label: &str) -> CoreResult<()> {
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

pub(crate) fn marker(path: &Path) -> CoreResult<ProjectInfo> {
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

pub(crate) fn assert_marker_identity(root: &Path, expected: &ProjectInfo) -> CoreResult<()> {
    let actual = marker(root)?;
    if actual != *expected {
        return Err(transfer_error(
            "InvalidProject",
            "The project marker does not match its SQLite identity.",
        ));
    }
    Ok(())
}

pub(crate) fn source_root(path: &Path) -> CoreResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        transfer_error(
            "InvalidRequest",
            format!("Cannot resolve project path: {error}"),
        )
    })
}

pub(crate) fn output_path(path: &Path) -> CoreResult<PathBuf> {
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

pub(crate) fn output_outside(root: &Path, path: &Path) -> CoreResult<PathBuf> {
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

pub(crate) fn output_new(path: &Path) -> CoreResult<PathBuf> {
    let target = output_path(path)?;
    if target.exists() {
        return Err(transfer_error(
            "TargetExists",
            "The destination already exists; choose a new path.",
        ));
    }
    Ok(target)
}

pub(crate) fn stage(parent: &Path, prefix: &str) -> CoreResult<StageDir> {
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

pub(crate) fn install_file_no_replace(staged: &Path, target: &Path) -> CoreResult<()> {
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

pub(crate) fn install_new_text_file(target: &Path, bytes: &[u8]) -> CoreResult<()> {
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

pub(crate) fn online_backup(project: &impl TransferSource, staged_db: &Path) -> CoreResult<()> {
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
