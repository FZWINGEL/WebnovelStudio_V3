//! Durable metadata for explicitly installed whole-document draft exports.
//!
//! The file itself is installed by `transfer` before this actor records the
//! result.  This module owns the SQLite row and revalidates the immutable
//! source and projection on every read/write; a copied record keeps its old
//! namespace and is therefore historical evidence only.

use wns_kernel::{
    CoreError, CoreResult, Head, ProjectAccess, ProjectInfo, Reply, Revision, check_id,
    parse_version, valid_hash,
};
use wns_storage::read_revision;
use wns_story::host::StoryHost;
use wns_story::reviewed_story;

use crate::transfer::{DraftExportPreview, DraftFormat, ExportRecord, project_draft};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub enum ExportCommand {
    ResolveReviewedSource(ProjectAccess, Head, Reply<(String, Revision)>),
    Install(
        ProjectAccess,
        Box<DraftExportPreview>,
        std::path::PathBuf,
        String,
        Reply<ExportRecord>,
    ),
    Read(ProjectAccess, String, Reply<ExportRecord>),
}

pub fn handle_export(host: &mut impl StoryHost, command: ExportCommand) {
    macro_rules! respond {
        ($reply:expr, $result:expr) => {{
            let result = $result;
            host.fence_uncertain(&result);
            let _ = $reply.send(result);
        }};
    }
    match command {
        ExportCommand::ResolveReviewedSource(access, expected, reply) => {
            respond!(
                reply,
                host.check_access(&access).and_then(|()| {
                    reviewed_story::resolve_reviewed_export_source(host, &access, &expected)
                })
            );
        }
        ExportCommand::Install(access, preview, target, basename, reply) => {
            respond!(
                reply,
                export_prepared_draft(host, access, &preview, &target, &basename)
            );
        }
        ExportCommand::Read(access, id, reply) => {
            respond!(
                reply,
                host.check_access(&access).and_then(|()| {
                    read_export_record(host.db()?, &access.operation_namespace, &id)
                })
            );
        }
    }
}

pub fn export_prepared_draft(
    host: &mut impl StoryHost,
    access: ProjectAccess,
    preview: &DraftExportPreview,
    target: &std::path::Path,
    basename: &str,
) -> CoreResult<ExportRecord> {
    host.check_access(&access)?;
    let existing = validate_export_preview(host.db()?, host.info(), &access, preview, basename)?;
    if let Some(record) = existing {
        return Err(export_already_recorded(&record));
    }

    // Finalization runs entirely on the owned project actor.  No second
    // caller can pass the same preview through preflight while this file
    // is being installed, so a duplicate cannot create a second output.
    let source = if let Some(review_bundle_id) = preview.review_bundle_id.as_deref() {
        let (resolved_bundle_id, revision) =
            reviewed_story::resolve_reviewed_export_source(host, &access, &preview.source_head)?;
        if resolved_bundle_id != review_bundle_id
            || revision.id != preview.revision_id
            || revision.head != preview.source_head
        {
            return Err(CoreError::new(
                "ReviewedExportStale",
                "The selected reviewed bundle or chapter changed after preview.",
            ));
        }
        revision
    } else {
        read_revision_checked(
            host.db()?,
            &preview.source_head.document_id,
            &preview.revision_id,
        )?
    };
    let projected = project_draft(&source.body, preview.format)?;
    if projected.text != preview.preview_text
        || projected.utf8_bytes != preview.utf8_bytes
        || projected.sha256 != preview.sha256
    {
        return Err(CoreError::new(
            "ExportPreviewMismatch",
            "The export preview does not match its retained revision.",
        ));
    }
    crate::transfer::install_export_file(host.path(), target, projected.text.as_bytes())?;

    let record_result = (|| -> CoreResult<ExportRecord> {
        let tx = host
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // The actor is serialized, but check again inside the write
        // transaction so a future caller cannot insert a second row
        // between validation and the durable record.
        if let Some(record) =
            read_export_record_optional(&tx, &access.operation_namespace, &preview.id)?
        {
            validate_export_record(&tx, &record)?;
            if same_payload(&record, preview, basename) {
                return Err(export_already_recorded(&record));
            }
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This export ID was already used for a different export.",
            ));
        }
        tx.execute(
            "INSERT INTO export_records(
                id,project_id,operation_namespace,document_id,revision_id,
                source_version,source_body_hash,working_draft,format,format_version,
                utf8_bytes,sha256,basename,review_bundle_id
             ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                preview.id,
                preview.project_id,
                preview.operation_namespace,
                preview.source_head.document_id,
                preview.revision_id,
                parse_version(&preview.source_head.version)?,
                preview.source_head.body_hash,
                i64::from(preview.review_bundle_id.is_none()),
                preview.format.storage_name(),
                preview.format_version,
                i64::try_from(preview.utf8_bytes).map_err(|_| {
                    CoreError::new("InvalidRequest", "The export is too large to record.")
                })?,
                preview.sha256,
                basename,
                preview.review_bundle_id,
            ],
        )?;
        let record = read_export_record(&tx, &access.operation_namespace, &preview.id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(record)
    })();
    match record_result {
        Ok(record) => Ok(record),
        Err(error) => {
            // The file is already installed.  Preserve the output and
            // surface the explicit record boundary to the caller; an
            // uncertain SQLite commit still fences this actor connection.
            if error.code == "UncertainOutcome" {
                let uncertain: CoreResult<()> = Err(error.clone());
                host.fence_uncertain(&uncertain);
            }
            Err(CoreError::new(
                "ExportRecordUnavailable",
                &format!(
                    "The export file was installed, but its durable record is unavailable: {error}"
                ),
            ))
        }
    }
}

fn export_already_recorded(record: &ExportRecord) -> CoreError {
    CoreError::new(
        "ExportAlreadyRecorded",
        &format!(
            "This export is already recorded for destination {:?}.",
            record.basename
        ),
    )
}

fn same_payload(record: &ExportRecord, preview: &DraftExportPreview, basename: &str) -> bool {
    record.id == preview.id
        && record.project_id == preview.project_id
        && record.operation_namespace == preview.operation_namespace
        && record.document_id == preview.source_head.document_id
        && record.revision_id == preview.revision_id
        && record.source_head == preview.source_head
        && record.working_draft == preview.review_bundle_id.is_none()
        && record.format == preview.format
        && record.format_version == preview.format_version
        && record.utf8_bytes == preview.utf8_bytes
        && record.sha256 == preview.sha256
        && record.basename == basename
        && record.review_bundle_id == preview.review_bundle_id
}

fn validate_export_preview(
    connection: &Connection,
    info: &ProjectInfo,
    access: &ProjectAccess,
    preview: &DraftExportPreview,
    basename: &str,
) -> CoreResult<Option<ExportRecord>> {
    check_id(&preview.id)?;
    if preview.project_id != info.project_id
        || preview.project_id != access.project_id
        || preview.operation_namespace != info.operation_namespace
        || preview.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "WrongProjectSession",
            "The export preview belongs to another project.",
        ));
    }
    check_id(&preview.source_head.document_id)?;
    check_id(&preview.revision_id)?;
    parse_version(&preview.source_head.version)?;
    if !valid_hash(&preview.source_head.body_hash)
        || !valid_hash(&preview.sha256)
        || preview.format_version != 1
        || !preview.format.is_supported()
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The export preview contains invalid source or format metadata.",
        ));
    }
    validate_basename(basename)?;
    if let Some(review_bundle_id) = preview.review_bundle_id.as_deref() {
        check_id(review_bundle_id)?;
        wns_story::reviewed_story::validate_reviewed_export_source(
            connection,
            &preview.project_id,
            &preview.operation_namespace,
            review_bundle_id,
            &preview.source_head,
            &preview.revision_id,
        )?;
    }
    let source = read_revision_checked(
        connection,
        &preview.source_head.document_id,
        &preview.revision_id,
    )?;
    if source.head != preview.source_head {
        return Err(CoreError::new(
            "RevisionMismatch",
            "The retained revision no longer matches the export preview.",
        ));
    }
    let projected = project_draft(&source.body, preview.format)?;
    if projected.text != preview.preview_text
        || projected.utf8_bytes != preview.utf8_bytes
        || projected.sha256 != preview.sha256
        || preview.format_loss != crate::transfer::draft_format_loss(preview.format)
    {
        return Err(CoreError::new(
            "ExportPreviewMismatch",
            "The export preview does not match its retained revision.",
        ));
    }
    let existing =
        read_export_record_optional(connection, &access.operation_namespace, &preview.id)?;
    if let Some(record) = existing {
        validate_export_record(connection, &record)?;
        if same_payload(&record, preview, basename) {
            return Ok(Some(record));
        }
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This export ID was already used for a different export.",
        ));
    }
    Ok(None)
}

fn read_revision_checked(
    connection: &Connection,
    document_id: &str,
    revision_id: &str,
) -> CoreResult<Revision> {
    let owner: Option<String> = connection
        .query_row(
            "SELECT document_id FROM revisions WHERE id=?",
            [revision_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(owner) = owner else {
        return Err(CoreError::new(
            "RevisionNotFound",
            "This document revision is not available in the project.",
        ));
    };
    if owner != document_id {
        return Err(CoreError::new(
            "RevisionDocumentMismatch",
            "The selected revision belongs to another document.",
        ));
    }
    read_revision(connection, revision_id)
}

fn read_export_record_optional(
    connection: &Connection,
    operation_namespace: &str,
    export_id: &str,
) -> CoreResult<Option<ExportRecord>> {
    connection
        .query_row(
            "SELECT id,project_id,operation_namespace,document_id,revision_id,
                    source_version,source_body_hash,working_draft,format,format_version,
                    utf8_bytes,sha256,basename,created_at,review_bundle_id
             FROM export_records WHERE operation_namespace=? AND id=?",
            params![operation_namespace, export_id],
            |row| {
                let source_version: i64 = row.get(5)?;
                let utf8_bytes: i64 = row.get(10)?;
                Ok(ExportRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_namespace: row.get(2)?,
                    document_id: row.get(3)?,
                    revision_id: row.get(4)?,
                    source_head: Head {
                        document_id: row.get(3)?,
                        version: source_version.to_string(),
                        body_hash: row.get(6)?,
                    },
                    working_draft: row.get::<_, i64>(7)? == 1,
                    format: DraftFormat::from_storage(&row.get::<_, String>(8)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    format_version: row
                        .get::<_, i64>(9)?
                        .try_into()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    utf8_bytes: utf8_bytes
                        .try_into()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    sha256: row.get(11)?,
                    basename: row.get(12)?,
                    created_at: row.get(13)?,
                    review_bundle_id: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(CoreError::from)
}

fn read_export_record(
    connection: &Connection,
    operation_namespace: &str,
    export_id: &str,
) -> CoreResult<ExportRecord> {
    check_id(operation_namespace)?;
    check_id(export_id)?;
    let record = read_export_record_optional(connection, operation_namespace, export_id)?
        .ok_or_else(|| CoreError::new("ExportNotFound", "The export record is not available."))?;
    validate_export_record(connection, &record)?;
    Ok(record)
}

pub fn validate_export_storage(connection: &Connection) -> CoreResult<()> {
    let mut statement = connection.prepare(
        "SELECT operation_namespace,id FROM export_records ORDER BY operation_namespace,id",
    )?;
    let ids = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (namespace, id) in ids {
        let record = read_export_record(connection, &namespace, &id)?;
        if record.project_id.is_empty() {
            return Err(CoreError::new(
                "InvalidProject",
                "An export record has no source project identity.",
            ));
        }
    }
    Ok(())
}

fn validate_export_record(connection: &Connection, record: &ExportRecord) -> CoreResult<()> {
    for value in [
        &record.id,
        &record.project_id,
        &record.operation_namespace,
        &record.document_id,
        &record.revision_id,
    ] {
        check_id(value).map_err(|_| {
            CoreError::new(
                "InvalidProject",
                "An export record contains an invalid identity.",
            )
        })?;
    }
    parse_version(&record.source_head.version).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "An export record contains an invalid source version.",
        )
    })?;
    if record.source_head.document_id != record.document_id
        || !valid_hash(&record.source_head.body_hash)
        || !valid_hash(&record.sha256)
        || (record.working_draft != record.review_bundle_id.is_none())
        || record.format_version != 1
        || !record.format.is_supported()
        || record.utf8_bytes > i64::MAX as u64
    {
        return Err(CoreError::new(
            "InvalidProject",
            "An export record contains invalid source or output metadata.",
        ));
    }
    let (current_project_id, current_namespace): (String, String) = connection.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if record.operation_namespace == current_namespace && record.project_id != current_project_id {
        return Err(CoreError::new(
            "InvalidProject",
            "An export record in the current operation namespace has a different project identity.",
        ));
    }
    validate_basename(&record.basename).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "An export record contains an invalid destination basename.",
        )
    })?;
    let source = read_revision_checked(connection, &record.document_id, &record.revision_id)?;
    if source.head != record.source_head {
        return Err(CoreError::new(
            "InvalidProject",
            "An export record points at a different revision head.",
        ));
    }
    if let Some(review_bundle_id) = record.review_bundle_id.as_deref() {
        check_id(review_bundle_id).map_err(|_| {
            CoreError::new(
                "InvalidProject",
                "A reviewed export record contains an invalid bundle identity.",
            )
        })?;
        wns_story::reviewed_story::validate_reviewed_export_source(
            connection,
            &record.project_id,
            &record.operation_namespace,
            review_bundle_id,
            &record.source_head,
            &record.revision_id,
        )?;
    }
    let projected = project_draft(&source.body, record.format)?;
    if projected.utf8_bytes != record.utf8_bytes || projected.sha256 != record.sha256 {
        return Err(CoreError::new(
            "InvalidProject",
            "An export record does not match its immutable projection.",
        ));
    }
    Ok(())
}

pub fn validate_basename(value: &str) -> CoreResult<()> {
    // Keep portable export records valid on the initial Windows target too.
    // An extension does not make a device name safe (for example NUL.txt).
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let device = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if value.is_empty()
        || value.len() > 255
        || value == "."
        || value == ".."
        || value.chars().any(char::is_control)
        || value.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        || value.ends_with([' ', '.'])
        || device
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The export destination must have a valid file basename.",
        ));
    }
    Ok(())
}
