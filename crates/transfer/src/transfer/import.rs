use super::*;

pub(crate) const MAX_SOURCE_PIN_DOCUMENTS: usize = 64;

pub(crate) const MAX_IMPORT_LEGACY_RECORDS: usize = 20_000;
pub(crate) const MAX_IMPORT_LEGACY_BYTES: usize = 4 * 1024 * 1024;

/// Validate the durable source-pin state before a database can be backed up
/// or recovered.  Mutable sets belong to the database's current identity;
/// their receipts deliberately do not, because recovery retains receipts as
/// historical evidence under their original namespace.
pub(crate) fn validate_source_pin_storage(
    connection: &Connection,
    info: &ProjectInfo,
) -> CoreResult<()> {
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

pub(crate) struct ImportManifestRow {
    pub(crate) project_id: String,
    pub(crate) namespace: String,
    pub(crate) operation_id: String,
    pub(crate) source_project_id: String,
    pub(crate) source_schema_version: i64,
    pub(crate) source_sha256: String,
    pub(crate) source_bytes: i64,
    pub(crate) source_title: String,
    pub(crate) source_slug: String,
    pub(crate) request_sha256: String,
    pub(crate) counts_json: String,
    pub(crate) import_format_version: i64,
    pub(crate) created_at: String,
}

pub(crate) fn validate_import_storage(
    connection: &Connection,
    info: &ProjectInfo,
) -> CoreResult<()> {
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

pub(crate) fn validate_source_pin_result(
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

pub(crate) fn validate_source_pin_set_fields(
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

pub(crate) fn validate_ordinary_document_role(
    connection: &Connection,
    document_id: &str,
) -> CoreResult<()> {
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

pub(crate) fn manifest_for(database: &DatabaseHeads, hash: String) -> BackupManifest {
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

pub(crate) fn validate_manifest(manifest: &BackupManifest) -> CoreResult<()> {
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

pub(crate) fn validate_duplicate_basis(
    manifest: &BackupManifest,
    basis: &DuplicateBasis,
) -> CoreResult<()> {
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

pub(crate) fn write_archive(
    staged_db: &Path,
    manifest: &BackupManifest,
    archive: &Path,
) -> CoreResult<()> {
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

pub(crate) struct ArchiveContents {
    pub(crate) manifest: BackupManifest,
    pub(crate) database: Vec<u8>,
}

pub(crate) fn read_archive(path: &Path) -> CoreResult<ArchiveContents> {
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
