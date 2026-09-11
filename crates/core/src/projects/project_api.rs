//! Narrow interface to project-level lifecycle and inspection.
//!
//! The fourth per-concern facade (`docs/V3_ARCHITECTURE_MODULAR.md` §3.5).
//! Metadata reads, the two renames and the storage report are what a library or
//! transfer path needs; none of them touches the document or conversation
//! surfaces that make up the rest of the session façade.
//!
//! # Why this one needed care too
//!
//! `OwnedProject` implements `project_metadata`, `rename_project`,
//! `rename_document` and `storage_info` as well, and calls them on itself from
//! `projects.rs` and from the actor's dispatch block. Those are the same
//! operations running one layer down — against the live connection rather than
//! over the channel — and they must not be migrated. Only
//! `ProjectSession`'s callers moved.

use super::*;
use super::{Command, Handle, Reply};
use std::sync::{Arc, mpsc};

/// Project-level lifecycle and inspection for one open project.
#[derive(Clone)]
pub struct ProjectApi {
    handle: Arc<Handle>,
}

impl ProjectApi {
    pub(crate) fn new(handle: Arc<Handle>) -> Self {
        Self { handle }
    }

    /// Identical exchange to [`ProjectSession::request`], kept in step on
    /// purpose so the stopped-actor fallback cannot diverge.
    fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle
            .queue
            .send(command(sender))
            .map_err(|_| CoreError::disconnected())?;
        receiver.recv().map_err(|_| CoreError::disconnected())?
    }

    pub fn metadata(&self) -> CoreResult<ProjectMetadata> {
        self.request(Command::ProjectMetadata)
    }

    /// Rename against the caller's expected metadata version. A concurrent
    /// rename makes this refuse rather than overwrite it.
    pub fn rename(
        &self,
        access: ProjectAccess,
        expected_metadata_version: String,
        title: String,
    ) -> CoreResult<ProjectMetadata> {
        self.request(|r| Command::RenameProject(access, expected_metadata_version, title, r))
    }

    /// Rename one document against the caller's expected metadata version.
    pub fn rename_document(
        &self,
        access: ProjectAccess,
        document_id: String,
        expected_metadata_version: String,
        title: String,
    ) -> CoreResult<DocumentRecord> {
        self.request(|r| {
            Command::RenameDocument(access, document_id, expected_metadata_version, title, r)
        })
    }

    /// The project database's storage report — diagnostics about the file this
    /// project is kept in, not about its story.
    pub fn storage(&self) -> CoreResult<StorageInfo> {
        self.request(Command::StorageInfo)
    }
}
