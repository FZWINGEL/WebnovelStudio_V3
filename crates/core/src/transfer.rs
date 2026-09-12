//! Session façade for the portability concern.
//!
//! The logic moved to `wns-transfer` (L6); what is left is the half that cannot
//! travel, because `wns-transfer` may not name a type it does not own:
//!
//! * [`TransferSource`] for `ProjectSession` — the seven reads a backup, a
//!   duplicate or an export makes of a live project.
//! * [`CoreProjectFactory`] — the one `create_staged` call that *returns* a
//!   project handle.
//! * Six thin wrappers that fix the factory, so no caller ever names it.
//!
//! Every other item is re-exported below, so `webnovel_core::transfer::{…}`
//! resolves exactly as it did when the module lived here.

use wns_kernel::SourceEpoch;

use crate::projects::{
    CoreResult, CreationOrigin, DocumentRecord, Head, OwnedProject, ProjectAccess, ProjectInfo,
    ProjectMetadata, ProjectSession, Revision,
};
use std::path::{Path, PathBuf};
use wns_documents::records::CheckpointRequest;
use wns_transfer::host::{TransferFactory, TransferSource};
pub use wns_transfer::transfer::{DraftExportPreview, ExportRecord};

pub use wns_transfer::transfer::*;

impl TransferSource for ProjectSession {
    fn project_path(&self) -> &Path {
        &self.path
    }
    fn metadata(&self) -> CoreResult<ProjectMetadata> {
        self.project().metadata()
    }
    fn document_records(&self, access: &ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.documents().list(access.clone())
    }
    fn source_epoch(&self) -> CoreResult<SourceEpoch> {
        self.context().source_epoch()
    }
    fn checkpoint(&self, request: CheckpointRequest) -> CoreResult<Revision> {
        self.documents().checkpoint(request)
    }
    fn resolve_reviewed_export_source(
        &self,
        access: ProjectAccess,
        expected: Head,
    ) -> CoreResult<(String, Revision)> {
        ProjectSession::resolve_reviewed_export_source(self, access, expected)
    }
    fn install_export(
        &self,
        access: ProjectAccess,
        preview: DraftExportPreview,
        target: PathBuf,
        basename: String,
    ) -> CoreResult<ExportRecord> {
        ProjectSession::install_export(self, access, preview, target, basename)
    }
}

/// The staging handle an import populates. Only the crates that fix the
/// factory's parameters ever name it; `wns-transfer` sees it as `F::Staging`.
pub struct CoreStaging(OwnedProject);

/// The project type `wns-transfer` cannot name.
#[derive(Debug, Clone, Copy)]
pub struct CoreProjectFactory;

impl TransferFactory for CoreProjectFactory {
    type Session = ProjectSession;
    type Staging = CoreStaging;

    fn create_staged(
        staging: &Path,
        destination: &Path,
        title: &str,
        origin: &CreationOrigin,
    ) -> CoreResult<ProjectSession> {
        ProjectSession::create_staged(staging, destination, title, origin)
    }
    fn open_staging(path: &Path, title: &str) -> CoreResult<CoreStaging> {
        Ok(CoreStaging(OwnedProject::open_direct(
            path.to_owned(),
            Some(title.to_owned()),
        )?))
    }
    fn staging_info(staging: &CoreStaging) -> &ProjectInfo {
        &staging.0.info
    }
    fn staging_db_mut(staging: &mut CoreStaging) -> CoreResult<&mut rusqlite::Connection> {
        staging.0.db_mut()
    }
}

/// Install a reviewed V2 import into a fresh project. The source path and its
/// fingerprint are the authority; a caller-supplied preview is never accepted.
pub fn stage_v2_import(
    request: &wns_transfer::import::V2ImportRequest,
    expected_request_sha256: &str,
    staging: &Path,
    destination: &Path,
    origin: &CreationOrigin,
) -> CoreResult<wns_transfer::import::V2ImportResult> {
    wns_transfer::import::stage_v2_import::<CoreProjectFactory>(
        request,
        expected_request_sha256,
        staging,
        destination,
        origin,
    )
}

/// Resume an import whose staging folder survived an interrupted install.
pub fn recover_import_staging(
    staging: &Path,
    destination: &Path,
    title: &str,
    origin: &CreationOrigin,
    operation_id: &str,
    expected_source_sha256: &str,
    expected_request_sha256: &str,
) -> CoreResult<wns_transfer::import::V2ImportResult> {
    wns_transfer::import::recover_import_staging::<CoreProjectFactory>(
        staging,
        destination,
        title,
        origin,
        operation_id,
        expected_source_sha256,
        expected_request_sha256,
    )
}

/// Recover a backup into a **new** project. The source is never replaced and
/// its operation namespace is never reused.
pub fn recover_backup(archive: &Path, target: &Path, title: &str) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::recover_backup::<CoreProjectFactory>(archive, target, title)
}

/// Resume a recovery whose staging folder survived an interrupted install.
pub fn recover_backup_staged(
    archive: &Path,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::recover_backup_staged::<CoreProjectFactory>(
        archive, staging, target, title, origin,
    )
}

pub fn duplicate_project(
    project: &ProjectSession,
    target: &Path,
    title: &str,
) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::duplicate_project::<CoreProjectFactory>(project, target, title)
}

pub fn duplicate_project_staged(
    project: &ProjectSession,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::duplicate_project_staged::<CoreProjectFactory>(
        project, staging, target, title, origin,
    )
}

pub fn duplicate_project_with_basis(
    project: &ProjectSession,
    target: &Path,
    title: &str,
    basis: &DuplicateBasis,
) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::duplicate_project_with_basis::<CoreProjectFactory>(
        project, target, title, basis,
    )
}

pub fn duplicate_project_staged_with_basis(
    project: &ProjectSession,
    staging: &Path,
    target: &Path,
    title: &str,
    origin: &CreationOrigin,
    basis: &DuplicateBasis,
) -> CoreResult<ProjectSession> {
    wns_transfer::transfer::duplicate_project_staged_with_basis::<CoreProjectFactory>(
        project, staging, target, title, origin, basis,
    )
}
