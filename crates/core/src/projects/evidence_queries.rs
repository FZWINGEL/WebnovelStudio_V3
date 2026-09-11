//! Read-only author identity choices and source-bound evidence history — the
//! session half.
//!
//! The actor-side logic moved to `wns-story`, whose `reviewed_story` and
//! `story_context` it was already reaching for; both are there now, so this
//! module simply follows them. Its actor side needs two things off the actor —
//! `check_access` and `db` — which `StoryHost` already declares.

use super::*;

impl ProjectSession {
    pub fn reviewed_entity_catalog(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::Entities(access, reply)))
        })
    }

    pub fn reviewed_evidence_history(
        &self,
        access: ProjectAccess,
        snapshot_id: String,
        object_id: String,
    ) -> CoreResult<ReviewedHistoryResult> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::History(
                access,
                snapshot_id,
                object_id,
                reply,
            )))
        })
    }

    pub fn reviewed_promise_catalog(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::PromiseEntities(
                access, reply,
            )))
        })
    }

    pub fn reviewed_promise_history(
        &self,
        access: ProjectAccess,
        snapshot_id: String,
        promise_id: String,
    ) -> CoreResult<ReviewedPromiseHistoryResult> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::PromiseHistory(
                access,
                snapshot_id,
                promise_id,
                reply,
            )))
        })
    }

    pub fn reviewed_knowledge_character_catalog(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::KnowledgeCharacters(
                access, reply,
            )))
        })
    }

    pub fn reviewed_knowledge_topic_catalog(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::KnowledgeTopics(
                access, reply,
            )))
        })
    }

    pub fn reviewed_knowledge_history(
        &self,
        access: ProjectAccess,
        snapshot_id: String,
        character_id: String,
        topic_id: Option<String>,
    ) -> CoreResult<ReviewedKnowledgeHistoryResult> {
        self.request(|reply| {
            Command::EvidenceQuery(Box::new(EvidenceQueryCommand::KnowledgeHistory(
                access,
                snapshot_id,
                character_id,
                topic_id,
                reply,
            )))
        })
    }
}

pub use wns_story::evidence_queries::*;
