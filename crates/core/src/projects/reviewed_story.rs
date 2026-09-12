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

impl OwnedProject {
    pub(super) fn handle_review(&mut self, command: ReviewCommand) {
        wns_story::reviewed_story::handle_review(self, command);
    }
}
