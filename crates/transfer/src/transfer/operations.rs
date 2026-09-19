use super::*;

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

pub(crate) fn write_marker(root: &Path, info: &ProjectInfo) -> CoreResult<()> {
    let marker = root.join(MARKER_FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)?;
    file.write_all(&serde_json::to_vec_pretty(info)?)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn rotate_identity(path: &Path, old: &ProjectInfo, new: &ProjectInfo) -> CoreResult<()> {
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
pub fn recover_backup<F: TransferFactory>(
    archive: &Path,
    target: &Path,
    title: &str,
) -> CoreResult<F::Session> {
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

pub(crate) fn duplicate_project_staged_inner<F: TransferFactory>(
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
