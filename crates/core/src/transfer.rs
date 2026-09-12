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

use crate::projects::{
    CoreResult, CreationOrigin, DocumentRecord, Head, ProjectAccess, ProjectMetadata,
    ProjectSession, Revision,
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
    fn source_epoch(&self) -> CoreResult<String> {
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

/// The project type `wns-transfer` cannot name.
#[derive(Debug, Clone, Copy)]
pub struct CoreProjectFactory;

impl TransferFactory for CoreProjectFactory {
    type Session = ProjectSession;
    fn create_staged(
        staging: &Path,
        destination: &Path,
        title: &str,
        origin: &CreationOrigin,
    ) -> CoreResult<ProjectSession> {
        ProjectSession::create_staged(staging, destination, title, origin)
    }
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
