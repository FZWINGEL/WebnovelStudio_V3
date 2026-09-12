//! Narrow interface to the background-work concern.
//!
//! The second per-concern facade (`docs/V3_ARCHITECTURE_MODULAR.md` §3.5). The
//! background-work census is read on two very different paths — the normal
//! project-chat write path and the app-close sequence — and both only ever need
//! to read the census, persist a stop intent, or interrupt. Making them depend
//! on the full 28-method session façade for three operations is the coupling D2
//! describes.
//!
//! Shares the session's `Arc<Handle>` and issues its own command variants, so
//! the actor, the channel and the ordering guarantees are untouched.

use super::*;
use super::{Command, Handle, Reply};
use std::sync::{Arc, mpsc};

/// Background-work operations for one open project.
#[derive(Clone)]
pub struct WorkApi {
    handle: Arc<Handle>,
}

impl WorkApi {
    pub(crate) fn new(handle: Arc<Handle>) -> Self {
        Self { handle }
    }

    /// Identical exchange to [`ProjectSession::request`], kept in step on
    /// purpose so the stopped-actor fallback cannot diverge between the two.
    fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle
            .queue
            .send(command(sender))
            .map_err(|_| CoreError::disconnected())?;
        receiver.recv().map_err(|_| CoreError::disconnected())?
    }

    /// Read the exact active-work census. Callers pass this value back to
    /// [`Self::stop`] or [`Self::interrupt`] so a stop applies to the census
    /// they actually observed, never to whatever is running later.
    pub fn census(&self) -> CoreResult<background_work::BackgroundWork> {
        self.request(Command::BackgroundWork)
    }

    pub fn stop(
        &self,
        expected: background_work::BackgroundWork,
    ) -> CoreResult<background_work::BackgroundWork> {
        self.request(|reply| Command::StopBackgroundWork(expected, reply))
    }

    pub fn interrupt(
        &self,
        expected: background_work::BackgroundWork,
    ) -> CoreResult<background_work::BackgroundWork> {
        self.request(|reply| Command::InterruptBackgroundWork(expected, reply))
    }
}
