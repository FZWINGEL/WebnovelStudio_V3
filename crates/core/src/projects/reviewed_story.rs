//! Author-only reviewed prose basis — the session half.
//!
//! The module's vocabulary and its actor-side logic moved to `wns-story`, its
//! namesake crate. What stays here is the command plumbing, because
//! `Command::Review` is a variant of the actor's own enum and an inherent
//! `impl` must live in the crate that owns the type.
//!
//! Everything public is re-exported, so `reviewed_story::X` resolves for the
//! four modules that call into it — `evidence_queries`, `exports`,
//! `story_context` and `transfer` — exactly as it did before the move.

use super::*;
use rusqlite::Connection;

pub use wns_story::reviewed_story::*;

impl ProjectSession {
    pub fn chapter_review_status(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<ReviewStatus> {
        self.request(|reply| {
            Command::Review(Box::new(ReviewCommand::Status(access, document_id, reply)))
        })
    }

    pub fn read_review_stage(
        &self,
        access: ProjectAccess,
        stage_id: String,
    ) -> CoreResult<ReviewStage> {
        self.request(|reply| {
            Command::Review(Box::new(ReviewCommand::ReadStage(access, stage_id, reply)))
        })
    }

    pub fn stage_author_review(&self, request: StageAuthorReview) -> CoreResult<ReviewStage> {
        self.request(|reply| Command::Review(Box::new(ReviewCommand::Stage(request, reply))))
    }

    pub fn mark_ready(&self, request: MarkReady) -> CoreResult<ReadyBundle> {
        self.request(|reply| Command::Review(Box::new(ReviewCommand::Mark(request, reply))))
    }

    pub fn read_reviewed_record_set(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<Option<ReviewedRecordSet>> {
        self.request(|reply| {
            Command::Review(Box::new(ReviewCommand::ReadRecords(
                access,
                document_id,
                reply,
            )))
        })
    }
}

/// What the moved module needs from the actor: four methods. No crash hook here
/// — that is what `history` needed and this does not.
impl ReviewedStoryHost for OwnedProject {
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
}

impl OwnedProject {
    pub(super) fn handle_review(&mut self, command: ReviewCommand) {
        wns_story::reviewed_story::handle_review(self, command);
    }
}
