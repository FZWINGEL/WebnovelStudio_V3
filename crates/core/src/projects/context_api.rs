//! Narrow interface to the context-freshness concern.
//!
//! The third per-concern facade (`docs/V3_ARCHITECTURE_MODULAR.md` §3.5).
//!
//! The source epoch is the fence every context operation checks: a frozen
//! snapshot is only usable while the project's epoch still matches. Callers that
//! only compare epochs had no business holding a 28-method façade.
//!
//! # Why this one needed care
//!
//! `OwnedProject` — the actor's own type — has a `context_source_epoch` method
//! too, and it is called as `self.context_source_epoch()` inside
//! `projects/context_packets.rs` and `projects/discussions.rs`. Those calls are
//! textually identical to the session's and must NOT be migrated: they run on
//! the actor thread against the live connection, which is a different operation
//! from asking the actor over the channel. A method name does not identify a
//! type; this file is the boundary that keeps the two apart.

use super::*;
use super::{Command, Handle, Reply};
use std::sync::{Arc, mpsc};

/// Context-freshness operations for one open project.
#[derive(Clone)]
pub struct ContextApi {
    handle: Arc<Handle>,
}

impl ContextApi {
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

    /// The project's current source epoch. A frozen context snapshot is only
    /// current while this value is unchanged.
    pub fn source_epoch(&self) -> CoreResult<SourceEpoch> {
        self.request(Command::ContextSourceEpoch)
    }
}
