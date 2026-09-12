//! What a transfer operation needs from the project it is reading, and from
//! the one it is creating.
//!
//! Two traits, because the two directions are genuinely different shapes. A
//! backup, a duplicate and an export *read* a live project: they ask for its
//! path, its identity, its document heads, its source epoch, and a checkpoint.
//! Recovery *creates* one, which no method on a value can express — the call
//! returns a new handle of a type this crate must not name.
//!
//! So [`TransferSource`] is implemented by the session handle and every reader
//! takes `&impl TransferSource`. [`TransferFactory`] is a marker with an
//! associated `Session`, implemented once by the crate that owns the type, and
//! every creator is generic over it. `webnovel-core` fixes the parameter with
//! thin wrappers so no caller outside it ever names the factory.

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use wns_documents::records::CheckpointRequest;
use wns_kernel::{CoreResult, DocumentRecord, Head, ProjectAccess, Revision, SourceEpoch};
use wns_storage::ProjectMetadata;

use crate::transfer::{DraftExportPreview, ExportRecord};

pub trait TransferSource {
    /// The project folder. A backup reads `project.sqlite3` from it and refuses
    /// a destination inside it.
    fn project_path(&self) -> &Path;
    fn metadata(&self) -> CoreResult<ProjectMetadata>;
    fn document_records(&self, access: &ProjectAccess) -> CoreResult<Vec<DocumentRecord>>;
    /// The epoch a duplicate's basis records, so the copy is not mistaken for
    /// a stale continuation of the original.
    fn source_epoch(&self) -> CoreResult<SourceEpoch>;
    /// Freeze a head as an immutable revision before an export is written.
    fn checkpoint(&self, request: CheckpointRequest) -> CoreResult<Revision>;
    /// Resolve the active author-reviewed bundle for an export that must not
    /// create a checkpoint.
    fn resolve_reviewed_export_source(
        &self,
        access: ProjectAccess,
        expected: Head,
    ) -> CoreResult<(String, Revision)>;
    /// Record an installed export file. The filesystem write happens outside
    /// this call so the two failure boundaries stay separate.
    fn install_export(
        &self,
        access: ProjectAccess,
        preview: DraftExportPreview,
        target: PathBuf,
        basename: String,
    ) -> CoreResult<ExportRecord>;
}

/// The project the transfer crate cannot construct, because it does not own
/// the type.
pub trait TransferFactory {
    /// What a completed recovery or import hands back to the caller. It is also
    /// readable as a source: the library registers and inspects the project it
    /// just created.
    type Session: TransferSource;
    /// The handle a V2 import populates before the atomic rename. It never
    /// escapes: it is opened, filled and dropped inside one function.
    type Staging;

    /// Create the destination through the actor's own staging and rename, which
    /// is what gives a recovered project its fresh identity and exact-origin
    /// semantics.
    fn create_staged(
        staging: &Path,
        destination: &Path,
        title: &str,
        origin: &wns_storage::CreationOrigin,
    ) -> CoreResult<Self::Session>;
    /// Open a staging folder as a writeable project, writing an empty document
    /// set into it. Only the V2 import installer uses this; recovery writes its
    /// database file directly.
    fn open_staging(path: &Path, title: &str) -> CoreResult<Self::Staging>;
    fn staging_info(staging: &Self::Staging) -> &wns_kernel::ProjectInfo;
    fn staging_db_mut(staging: &mut Self::Staging) -> CoreResult<&mut Connection>;
}
