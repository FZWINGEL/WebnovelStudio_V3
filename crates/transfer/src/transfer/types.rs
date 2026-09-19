use super::*;

pub(crate) const BACKUP_FORMAT_VERSION: u32 = 1;
pub(crate) const DATABASE_SCHEMA_VERSION: u32 = wns_storage::LATEST_SCHEMA_VERSION as u32;
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_DATABASE_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const DB_FILE: &str = "project.sqlite3";
pub(crate) const MARKER_FILE: &str = "project.wns.json";

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
pub(crate) struct DatabaseHeads {
    pub(crate) info: ProjectInfo,
    pub(crate) context_source_epoch: String,
    pub(crate) documents: Vec<DocumentHeadManifest>,
    pub(crate) revisions: Vec<RevisionHeadManifest>,
}
