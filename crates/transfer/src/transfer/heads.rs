use super::*;

pub(crate) fn validate_database(
    path: &Path,
    expected: Option<&ProjectInfo>,
) -> CoreResult<DatabaseHeads> {
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

pub(crate) fn validate_project_connection_heads(
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
    )
    .map_err(|error| {
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
