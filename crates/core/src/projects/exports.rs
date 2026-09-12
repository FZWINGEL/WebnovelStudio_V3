//! Session façade for whole-document draft exports.
//!
//! The logic moved to `wns-transfer`; what is left is the half that cannot
//! travel — `ProjectSession`'s three command senders, plus the `command`
//! plumbing they ride on. Everything else is re-exported below.

use super::*;
use wns_transfer::transfer::{DraftExportPreview, ExportRecord};

impl ProjectSession {
    pub(crate) fn resolve_reviewed_export_source(
        &self,
        access: ProjectAccess,
        expected: Head,
    ) -> CoreResult<(String, Revision)> {
        self.request(|reply| {
            Command::Export(Box::new(ExportCommand::ResolveReviewedSource(
                access, expected, reply,
            )))
        })
    }

    pub(crate) fn install_export(
        &self,
        access: ProjectAccess,
        preview: DraftExportPreview,
        target: std::path::PathBuf,
        basename: String,
    ) -> CoreResult<ExportRecord> {
        self.request(|reply| {
            Command::Export(Box::new(ExportCommand::Install(
                access,
                Box::new(preview),
                target,
                basename,
                reply,
            )))
        })
    }

    /// Read an export recorded under the current operation namespace.  This is
    /// intentionally a read-only metadata operation; it cannot authorize a
    /// subsequent export and copied historical namespaces are never adopted.
    pub fn read_export_record(
        &self,
        access: ProjectAccess,
        export_id: String,
    ) -> CoreResult<ExportRecord> {
        self.request(|reply| {
            Command::Export(Box::new(ExportCommand::Read(access, export_id, reply)))
        })
    }
}

pub use wns_transfer::exports::*;
