//! Durable document history and restore — the session half.
//!
//! The module's vocabulary and its actor-side logic moved to `wns-documents`,
//! which is where document lifecycle belongs. What stays here is the command
//! plumbing, because `Command::History` is a variant of the actor's own enum and
//! an inherent `impl` must live in the crate that owns the type.
//!
//! This is the second module to move, and the first one that moved *because*
//! the shared primitive layer did. Its selection looked clean — it was the only
//! module under `projects/` with no cross-module `module::` call — but its
//! actor side reached eight crate-private free functions that lived in
//! `projects.rs`. Moving the primitives down to `wns-kernel` and `wns-storage`
//! is what made this file possible; the criterion that said "self-contained"
//! was reading imports when the dependency lives in the bodies.

use super::*;
use rusqlite::Connection;

pub use wns_documents::history::{
    HistoryPage, HistoryHost, RestoreAck, RestoreRevision, RevisionSummary,
};
pub(crate) use wns_documents::history::{HistoryCommand, validate_history_storage};
pub use wns_kernel::RestoredDecision;

impl ProjectSession {
    /// Return bounded revision metadata in descending working-version order.
    pub fn list_document_history(
        &self,
        access: ProjectAccess,
        document_id: String,
        before_version: Option<String>,
        limit: u32,
    ) -> CoreResult<HistoryPage> {
        self.request(|reply| {
            Command::History(Box::new(HistoryCommand::List(
                access,
                document_id,
                before_version,
                limit,
                reply,
            )))
        })
    }

    /// Read exactly one retained revision body after its document ownership is
    /// checked. This explicit read may inspect a trashed source; active
    /// history listing and all writing paths still require a live document.
    /// The body fingerprint is revalidated by `read_revision`.
    pub fn read_document_revision(
        &self,
        access: ProjectAccess,
        document_id: String,
        revision_id: String,
    ) -> CoreResult<Revision> {
        self.request(|reply| {
            Command::History(Box::new(HistoryCommand::Read(
                access,
                document_id,
                revision_id,
                reply,
            )))
        })
    }

    pub fn restore_revision(&self, request: RestoreRevision) -> CoreResult<RestoreAck> {
        self.request(|reply| Command::History(Box::new(HistoryCommand::Restore(request, reply))))
    }
}

/// What the moved module needs from the actor: four methods and the crash hook.
impl HistoryHost for OwnedProject {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()> {
        OwnedProject::check_access(self, access)
    }
    fn db(&self) -> CoreResult<&Connection> {
        OwnedProject::db(self)
    }
    fn db_mut(&mut self) -> CoreResult<&mut Connection> {
        OwnedProject::db_mut(self)
    }
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>) {
        OwnedProject::fence_uncertain(self, result)
    }
    /// The crash-injection hook stays here, in the crate whose test binary
    /// needs it. `wns-documents` declares the method unconditionally so its
    /// trait compiles either way; only this implementation reaches the hook,
    /// and only when core is built for test.
    fn hold_after_commit_before_ack(&self, operation_id: &str) {
        #[cfg(test)]
        super::tests::hold_after_commit_before_ack(operation_id);
        #[cfg(not(test))]
        let _ = operation_id;
    }
}

impl OwnedProject {
    pub(super) fn handle_history(&mut self, command: HistoryCommand) {
        wns_documents::history::handle_history(self, command);
    }
}
