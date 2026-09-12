//! Frozen, project-owned evidence — the session half.
//!
//! The actor-side logic moved to `wns-story` with `memory` and
//! `context_packets`; the three are mutually dependent and travel together.

use super::*;

impl ProjectSession {
    pub fn context_epochs(&self, access: ProjectAccess) -> CoreResult<ContextEpochs> {
        self.request(|reply| Command::Context(Box::new(ContextCommand::Epochs(access, reply))))
    }
    pub fn read_document_aliases(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<DocumentAliases> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::ReadAliases(
                access,
                document_id,
                reply,
            )))
        })
    }
    pub fn freeze_story(&self, request: FreezeStory) -> CoreResult<FrozenContext> {
        self.request(|reply| Command::Context(Box::new(ContextCommand::Freeze(request, reply))))
    }
    pub fn freeze_reviewed_continuation(
        &self,
        request: FreezeReviewedContinuation,
    ) -> CoreResult<FrozenContext> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::FreezeReviewed(request, reply)))
        })
    }
    pub fn story_snapshot(&self, access: ProjectAccess, id: String) -> CoreResult<FrozenContext> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::Snapshot(access, id, reply)))
        })
    }
    pub fn read_story_source(
        &self,
        access: ProjectAccess,
        snapshot: String,
        handle: String,
    ) -> CoreResult<SourceRead> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::Read(
                access, snapshot, handle, reply,
            )))
        })
    }
    pub fn search_story(&self, request: SearchStory) -> CoreResult<SearchResult> {
        self.request(|reply| Command::Context(Box::new(ContextCommand::Search(request, reply))))
    }
    /// A frozen source stays readable after editing, but an old prose proposal
    /// is stale even when the newly edited source was never retrieved.
    pub fn story_snapshot_is_current(&self, access: ProjectAccess, id: String) -> CoreResult<bool> {
        self.request(|reply| Command::Context(Box::new(ContextCommand::Fresh(access, id, reply))))
    }
    /// Call when an author's source permissions change. This revokes further
    /// reads/submissions from every older snapshot; it cannot recall sent text.
    pub fn revoke_story_context(
        &self,
        access: ProjectAccess,
        expected_policy: String,
    ) -> CoreResult<ContextEpochs> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::Revoke(
                access,
                expected_policy,
                reply,
            )))
        })
    }
    pub fn set_document_aliases(
        &self,
        access: ProjectAccess,
        document_id: String,
        expected_source_epoch: SourceEpoch,
        aliases: Vec<String>,
    ) -> CoreResult<ContextEpochs> {
        self.request(|reply| {
            Command::Context(Box::new(ContextCommand::Aliases(
                access,
                document_id,
                expected_source_epoch,
                aliases,
                reply,
            )))
        })
    }
    pub fn rebuild_story_index(&self, access: ProjectAccess) -> CoreResult<u32> {
        let documents = self.documents().list(access.clone())?;
        self.clear_story_index(access.clone())?;
        let mut count = 0;
        // Each document is a separate queue turn. Saves may run between turns
        // and a later edit marks only that document dirty again.
        for document in documents {
            count += self.request(|reply| {
                Command::Context(Box::new(ContextCommand::Index(
                    access.clone(),
                    Some(document.head.document_id),
                    reply,
                )))
            })?;
        }
        Ok(count)
    }
    pub fn clear_story_index(&self, access: ProjectAccess) -> CoreResult<u32> {
        self.request(|reply| Command::Context(Box::new(ContextCommand::Index(access, None, reply))))
    }
}

pub use wns_story::story_context::*;
