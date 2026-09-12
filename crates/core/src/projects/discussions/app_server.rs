//! Session half of the app-server claim/ack pair; see the parent module.
use super::*;
use crate::providers::codex_app_server::AppServerDispatch;

impl ProjectSession {
    /// One durable authorization, committed after thread creation and before
    /// turn/start. An existing claim is never permission to repeat submission.
    pub fn claim_app_server_dispatch(
        &self,
        owner: RunOwner,
        dispatch: AppServerDispatch,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ClaimAppServer(
                owner, dispatch, reply,
            )))
        })
    }

    pub fn acknowledge_app_server_turn(
        &self,
        owner: RunOwner,
        dispatch: AppServerDispatch,
        turn_id: String,
    ) -> CoreResult<()> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::AckAppServer(
                owner, dispatch, turn_id, reply,
            )))
        })
    }
}
