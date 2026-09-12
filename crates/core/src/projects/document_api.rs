//! Narrow interface to the document concern — the largest of the facades.
//!
//! The fifth per-concern facade (`docs/V3_ARCHITECTURE_MODULAR.md` §3.5).
//! Eleven of `ProjectSession`'s 28 methods are document operations, and they are
//! the ones that actually widen the session's surface: attach, create, read,
//! list, save, checkpoint, history, reconcile, the two view-state calls and the
//! attachment snapshot.
//!
//! # Why this one is the dangerous one
//!
//! `attach` and `save` are generic names. `OwnedProject` implements every method
//! here, and test fixtures do too — `f.save(...)` and `fixture.reconcile(...)`
//! are different types with the same names, sitting in the same files as the
//! session's callers. A rewrite keyed on the method name would corrupt them.
//!
//! The migration therefore did not rely on the rewrite being complete: the
//! session's methods were **removed first**, so the compiler enumerates every
//! remaining call site. A missed receiver is a build error, not a silent
//! mis-dispatch. That is the only completeness check that has held in this
//! migration, and it is why this facade was done last.
//!
//! # Naming
//!
//! `documents` is gone from the session, so the accessor takes that name and
//! the two list/read methods are `list` and `read`. Everything else keeps its
//! name, so the call-site rewrite is an insertion rather than a rename.

use super::*;
use super::{Command, Handle, Reply};
use std::sync::{Arc, mpsc};

/// Document operations for one open project.
#[derive(Clone)]
pub struct DocumentApi {
    handle: Arc<Handle>,
}

impl DocumentApi {
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

    /// Host-owned attachment: every renderer creation begins with a fresh lease.
    pub fn attach(&self, session: String) -> CoreResult<ProjectAccess> {
        self.request(|r| Command::Attach(session, r))
    }

    /// Read destination state before replacing the current lease. Failed opens
    /// must not retire an editor whose manuscript remains on screen.
    pub fn attach_snapshot(&self, session: String) -> CoreResult<AttachedProject> {
        self.request(|r| Command::AttachSnapshot(session, r))
    }

    pub fn create(&self, request: CreateDocument) -> CoreResult<DocumentRecord> {
        self.request(|r| Command::Create(request, r))
    }

    pub fn list(&self, access: ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.request(|r| Command::List(access, r))
    }

    pub fn read(&self, access: ProjectAccess, id: String) -> CoreResult<DocumentRecord> {
        self.request(|r| Command::Read(access, id, r))
    }

    pub fn save(&self, request: SaveSnapshot) -> CoreResult<SaveAck> {
        self.request(|r| Command::Save(request, r))
    }

    pub fn checkpoint(&self, request: CheckpointRequest) -> CoreResult<Revision> {
        self.request(|r| Command::Checkpoint(request, r))
    }

    pub fn history(&self, access: ProjectAccess, id: String) -> CoreResult<Vec<Revision>> {
        self.request(|r| Command::LegacyHistory(access, id, r))
    }

    pub fn reconcile(&self, request: ReconcileRequest) -> CoreResult<ReconciledDocument> {
        self.request(|r| Command::Reconcile(request, r))
    }

    pub fn view_state(&self, access: ProjectAccess) -> CoreResult<Option<ViewState>> {
        self.request(|r| Command::ViewState(access, r))
    }

    pub fn save_view_state(
        &self,
        access: ProjectAccess,
        head: Head,
        anchor: Endpoint,
        focus: Endpoint,
    ) -> CoreResult<ViewState> {
        self.request(|r| Command::SaveViewState(access, head, anchor, focus, r))
    }
}
