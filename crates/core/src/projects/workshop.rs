//! Durable Story Workshop state and reviewed adoption boundary -- the host
//! impl and a re-export.
//!
//! The module moved to `wns-workshop`. What stays is the `WorkshopHost` impl,
//! because `OwnedProject` is declared in this crate, plus a re-export so every
//! historical path resolves.

use super::*;
use wns_story::discussion_vocabulary::StartDiscussion;
use wns_story::run_vocabulary::{CompletedDiscussionOutput, DiscussionRun, DiscussionStart};
pub use wns_workshop::workshop::{WorkshopSnapshotOrigin, WorkshopState};

impl wns_workshop::host::WorkshopHost for OwnedProject {
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
    fn start_discussion(&mut self, request: StartDiscussion) -> CoreResult<DiscussionStart> {
        crate::projects::discussions::start_discussion(self, request)
    }
    fn read_start(&self, run_id: &str) -> CoreResult<DiscussionStart> {
        crate::projects::discussions::read_start(self.db()?, run_id)
    }
    fn run_ids(&self) -> CoreResult<Vec<String>> {
        wns_conversation::discussions::queries::run_ids_at(self.db()?)
    }
    fn run_id_for_operation(
        &self,
        project_id: &str,
        operation_namespace: &str,
        operation_id: &str,
    ) -> CoreResult<Option<String>> {
        wns_conversation::discussions::queries::run_id_for_operation_at(
            self.db()?,
            project_id,
            operation_namespace,
            operation_id,
        )
    }
    fn completed_outputs_at(connection: &Connection) -> CoreResult<Vec<CompletedDiscussionOutput>> {
        wns_conversation::discussions::queries::completed_outputs_at(connection)
    }
    fn read_run(&self, run_id: &str) -> CoreResult<DiscussionRun> {
        crate::projects::discussions::read_run(self.db()?, run_id)
    }
    fn validate_chat_workshop_snapshot(
        &self,
        origin: WorkshopSnapshotOrigin<'_>,
        state: &WorkshopState,
        previous_state: &WorkshopState,
    ) -> CoreResult<()> {
        crate::projects::project_chat::validate_chat_workshop_snapshot(
            self.db()?,
            origin,
            state,
            previous_state,
        )
    }
    fn info(&self) -> &ProjectInfo {
        &self.info
    }
}

pub use wns_workshop::workshop::*;
