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
use wns_story::run_vocabulary::DiscussionStart;
use wns_kernel::{CoreResult, ProjectAccess};

pub trait WorkshopHost {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()>;
    fn db(&self) -> CoreResult<&Connection>;
    fn db_mut(&mut self) -> CoreResult<&mut Connection>;
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>);
    /// The discussion run a workshop request is expressed as.
    fn start_discussion(&mut self, request: StartDiscussion) -> CoreResult<DiscussionStart>;
    /// The immutable discussion row, for the recovery window after a commit.
    fn read_start(&self, run_id: &str) -> CoreResult<DiscussionStart>;
}
