//! Narrow interface to the Workshop concern.
//!
//! `ProjectSession` exposes 28 public methods, so every caller of a project —
//! whether it opens a document, drives the Workshop or reads storage info —
//! depends on the union of all of them. This is the first of the per-concern
//! facades described in `docs/V3_ARCHITECTURE_MODULAR.md` §3.5: a caller that
//! only runs Workshop operations depends on six methods instead of 28.
//!
//! It holds the **same** `Arc<Handle>` as the session and issues its own command
//! variants, so the actor, the channel, the 64-slot backpressure and the
//! ordering guarantees are untouched. This is an interface change only, which is
//! why the existing suite is the whole contract.

use super::*;
use super::{Command, Handle, Reply};
use std::sync::{Arc, mpsc};

/// Workshop operations for one open project.
#[derive(Clone)]
pub struct WorkshopApi {
    handle: Arc<Handle>,
}

impl WorkshopApi {
    pub(crate) fn new(handle: Arc<Handle>) -> Self {
        Self { handle }
    }

    /// Send one command and wait for its reply — the same exchange
    /// [`ProjectSession::request`] performs, kept identical on purpose so the
    /// fallback semantics for a stopped actor do not diverge.
    fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle
            .queue
            .send(command(sender))
            .map_err(|_| CoreError::disconnected())?;
        receiver.recv().map_err(|_| CoreError::disconnected())?
    }

    pub fn start(
        &self,
        request: workshop_generation::StartWorkshop,
    ) -> CoreResult<discussions::DiscussionStart> {
        self.request(|reply| Command::WorkshopStart(Box::new(request), reply))
    }

    pub fn read(&self, access: ProjectAccess) -> CoreResult<workshop::WorkshopView> {
        self.request(|reply| Command::WorkshopRead(access, reply))
    }

    pub fn save(
        &self,
        request: workshop::SaveWorkshop,
    ) -> CoreResult<workshop::WorkshopSnapshot> {
        self.request(|reply| Command::WorkshopSave(request, reply))
    }

    pub fn history(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<Vec<workshop::WorkshopSnapshot>> {
        self.request(|reply| Command::WorkshopHistory(access, reply))
    }

    pub fn preview_adoption(
        &self,
        request: workshop::PreviewWorkshopAdoption,
    ) -> CoreResult<workshop::WorkshopAdoptionPreview> {
        self.request(|reply| Command::WorkshopPreview(request, reply))
    }

    pub fn adopt(
        &self,
        access: ProjectAccess,
        operation_id: String,
        preview_id: String,
    ) -> CoreResult<workshop::WorkshopAdoptionAck> {
        self.request(|reply| Command::WorkshopAdopt(access, operation_id, preview_id, reply))
    }
}
