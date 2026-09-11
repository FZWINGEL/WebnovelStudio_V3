//! Memory app-server dispatch — the session half.

use super::*;

impl ProjectSession {
    /// Commit once after thread creation, before submitting its first turn.
    pub fn claim_memory_app_server_dispatch(
        &self,
        owner: MemoryOwner,
        dispatch: AppServerDispatch,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::ClaimAppServer(
                owner, dispatch, reply,
            )))
        })
    }

    pub fn acknowledge_memory_app_server_turn(
        &self,
        owner: MemoryOwner,
        dispatch: AppServerDispatch,
        turn_id: String,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::AckAppServer(
                owner, dispatch, turn_id, reply,
            )))
        })
    }
}

pub use wns_story::memory::app_server::*;
use wns_providers::codex_app_server::AppServerDispatch;
