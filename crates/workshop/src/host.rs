//! Workshop's access to the project actor and conversation-owned operations.
//!
//! Workshop owns its state, candidate interpretation, previews and adoption.
//! Conversation owns run selection, materialization and dispatch. Core forwards
//! those operations here without a dependency between the two domain crates.
//! Workshop still uses the actor's database for its own atomic workflows; the
//! completed-output reader must use that same connection during validation.

use rusqlite::Connection;
use wns_kernel::{CoreResult, ProjectAccess, ProjectInfo};
use wns_story::discussion_vocabulary::StartDiscussion;
use wns_story::run_vocabulary::{CompletedDiscussionOutput, DiscussionRun, DiscussionStart};

pub trait WorkshopHost {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()>;
    fn db(&self) -> CoreResult<&Connection>;
    fn db_mut(&mut self) -> CoreResult<&mut Connection>;
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>);
    /// The discussion run a workshop request is expressed as.
    fn start_discussion(&mut self, request: StartDiscussion) -> CoreResult<DiscussionStart>;
    /// The immutable discussion row, for the recovery window after a commit.
    fn read_start(&self, run_id: &str) -> CoreResult<DiscussionStart>;
    /// Every run identity in insertion order, before Workshop's intent filter.
    fn run_ids(&self) -> CoreResult<Vec<String>>;
    /// A run for this exact operation owner, regardless of discussion intent.
    fn run_id_for_operation(
        &self,
        project_id: &str,
        operation_namespace: &str,
        operation_id: &str,
    ) -> CoreResult<Option<String>>;
    /// Raw completed, delivered outputs on the caller's active connection.
    ///
    /// This stateless reader must not open another connection, dispatch an actor
    /// command or cache rows: save and adoption call it inside their transaction.
    /// Packet validation remains separate so unrelated malformed output can be
    /// skipped by candidate scanning without changing full-run validation.
    fn completed_outputs_at(connection: &Connection) -> CoreResult<Vec<CompletedDiscussionOutput>>;
    /// A fully materialized and validated run by identity.
    fn read_run(&self, run_id: &str) -> CoreResult<DiscussionRun>;
    /// The immutable authority chain for a chat-origin workshop snapshot.
    ///
    /// Its owner is conversation's `project_chat` module.
    fn validate_chat_workshop_snapshot(
        &self,
        origin: crate::workshop::WorkshopSnapshotOrigin<'_>,
        state: &crate::workshop::WorkshopState,
        previous_state: &crate::workshop::WorkshopState,
    ) -> CoreResult<()>;
    /// The actor reads two fields off this (`project_id`, `operation_namespace`).
    fn info(&self) -> &ProjectInfo;
}
