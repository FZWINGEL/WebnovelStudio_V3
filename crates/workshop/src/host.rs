//! What the Workshop needs from the actor.
//!
//! Four of these are the same four `SourcePinHost`, `HistoryHost`,
//! `ReviewedStoryHost` and `StoryHost` declare. The other three are specific to
//! what a workshop is: **a workshop request is a discussion run**.
//!
//! `read_start` and `start_discussion` live on the host rather than travelling
//! with the vocabulary because the readers cannot travel. `read_run` and
//! `read_start` call `intent_for_packet`, `read_provider_result` and
//! `read_message`, so their call graph is roughly as large again as the types
//! they return — the difference between a bounded extraction and an unbounded
//! one, learned by reverting that extraction three times.
//!
//! `read_run` is not here: its one workshop caller is `read_workshop_results`, a
//! free function taking an explicit `&Connection`, and it wants the
//! already-materialised-input signature change rather than a trait method.

use rusqlite::Connection;
use wns_story::discussion_vocabulary::StartDiscussion;
use wns_story::run_vocabulary::{DiscussionRun, DiscussionStart};
use wns_kernel::{CoreResult, ProjectAccess, ProjectInfo};


pub trait WorkshopHost {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()>;
    fn db(&self) -> CoreResult<&Connection>;
    fn db_mut(&mut self) -> CoreResult<&mut Connection>;
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>);
    /// The discussion run a workshop request is expressed as.
    fn start_discussion(&mut self, request: StartDiscussion) -> CoreResult<DiscussionStart>;
    /// The immutable discussion row, for the recovery window after a commit.
    fn read_start(&self, run_id: &str) -> CoreResult<DiscussionStart>;
    /// A run by id.
    ///
    /// `workshop` needs a run, not a run's *reader*: `read_run` calls
    /// `intent_for_packet`, `read_provider_result` and `read_message`, so it
    /// stays in `discussions` and is reached from here instead of travelling.
    fn read_run(&self, run_id: &str) -> CoreResult<DiscussionRun>;
    /// The immutable authority chain for a chat-origin workshop snapshot.
    ///
    /// Its owner is `project_chat`; this is the seam rather than a move, which
    /// is what `project_chat` called it when the forwarding function was added.
    fn validate_chat_workshop_snapshot(
        &self,
        origin: crate::workshop::WorkshopSnapshotOrigin<'_>,
        state: &crate::workshop::WorkshopState,
        previous_state: &crate::workshop::WorkshopState,
    ) -> CoreResult<()>;
    /// The actor reads two fields off this (`project_id`, `operation_namespace`).
    fn info(&self) -> &ProjectInfo;
}
