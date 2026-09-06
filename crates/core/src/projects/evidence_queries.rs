//! Read-only author identity choices and source-bound evidence history.
use super::*;
use crate::context::SourceRef;
use crate::context::evidence_history::{EvidenceHistory, query_evidence_history};
use crate::context::knowledge_history::{KnowledgeHistory, query_knowledge_history};
use crate::context::promise_history::{PromiseHistory, query_promise_history};
use crate::projects::reviewed_story::ReviewedRecordSet;
use crate::projects::story_records::StoryEntityRef;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEntityChoice {
    pub entity: StoryEntityRef,
    pub label_variants: Vec<String>,
    pub first_document_id: String,
    pub first_document_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEntityCatalog {
    pub project_id: String,
    pub operation_namespace: String,
    pub source_epoch: String,
    pub entities: Vec<ReviewedEntityChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedHistoryResult {
    pub snapshot_id: String,
    pub current: bool,
    pub history: EvidenceHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedPromiseHistoryResult {
    pub snapshot_id: String,
    pub current: bool,
    pub history: PromiseHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedKnowledgeHistoryResult {
    pub snapshot_id: String,
    pub current: bool,
    pub history: KnowledgeHistory,
}

pub(super) enum EvidenceQueryCommand {
    Entities(ProjectAccess, Reply<ReviewedEntityCatalog>),
    History(ProjectAccess, String, String, Reply<ReviewedHistoryResult>),
    PromiseEntities(ProjectAccess, Reply<ReviewedEntityCatalog>),
    PromiseHistory(
        ProjectAccess,
        String,
        String,
        Reply<ReviewedPromiseHistoryResult>,
    ),
    KnowledgeCharacters(ProjectAccess, Reply<ReviewedEntityCatalog>),
    KnowledgeTopics(ProjectAccess, Reply<ReviewedEntityCatalog>),
    KnowledgeHistory(
        ProjectAccess,
        String,
        String,
        Option<String>,
        Reply<ReviewedKnowledgeHistoryResult>,
    ),
}

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

impl OwnedProject {
    pub(super) fn handle_evidence_query(&self, command: EvidenceQueryCommand) {
        match command {
            EvidenceQueryCommand::Entities(access, reply) => {
                let _ = reply.send(self.reviewed_entity_catalog(&access));
            }
            EvidenceQueryCommand::History(access, snapshot_id, object_id, reply) => {
                let _ =
                    reply.send(self.reviewed_evidence_history(&access, &snapshot_id, &object_id));
            }
            EvidenceQueryCommand::PromiseEntities(access, reply) => {
                let _ = reply.send(self.reviewed_promise_catalog(&access));
            }
            EvidenceQueryCommand::PromiseHistory(access, snapshot_id, promise_id, reply) => {
                let _ =
                    reply.send(self.reviewed_promise_history(&access, &snapshot_id, &promise_id));
            }
            EvidenceQueryCommand::KnowledgeCharacters(access, reply) => {
                let _ = reply.send(self.reviewed_knowledge_catalog(&access, true));
            }
            EvidenceQueryCommand::KnowledgeTopics(access, reply) => {
                let _ = reply.send(self.reviewed_knowledge_catalog(&access, false));
            }
            EvidenceQueryCommand::KnowledgeHistory(
                access,
                snapshot_id,
                character_id,
                topic_id,
                reply,
            ) => {
                let _ = reply.send(self.reviewed_knowledge_history(
                    &access,
                    &snapshot_id,
                    &character_id,
                    topic_id.as_deref(),
                ));
            }
        }
    }

    fn reviewed_entity_catalog(&self, access: &ProjectAccess) -> CoreResult<ReviewedEntityCatalog> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        // The chooser is an author surface. It reuses identities, never grants
        // story authority or disclosure permission to the next record.
        let rows = {
            let mut statement = tx.prepare("SELECT d.id,d.title,b.target_revision_id,b.target_body_hash FROM documents d JOIN ready_heads h ON h.document_id=d.id JOIN ready_bundles b ON b.id=h.bundle_id WHERE d.kind='chapter' AND d.trashed=0 AND h.project_id=? AND h.operation_namespace=? ORDER BY d.position,d.id")?;
            statement
                .query_map(
                    params![access.project_id, access.operation_namespace],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        let sources: Vec<_> = rows
            .iter()
            .map(|(document_id, _, revision_id, body_hash)| SourceRef {
                project_id: access.project_id.clone(),
                document_id: document_id.clone(),
                revision_id: revision_id.clone(),
                body_hash: body_hash.clone(),
            })
            .collect();
        let sets = reviewed_story::current_records_for_sources(&tx, access, &sources)?;
        let titles: HashMap<_, _> = rows
            .iter()
            .map(|(id, title, _, _)| (id.as_str(), title.as_str()))
            .collect();
        let mut entities: Vec<ReviewedEntityChoice> = Vec::new();
        let mut indices = HashMap::<String, usize>::new();
        for set in sets {
            for record in set.records {
                for entity in std::iter::once(record.object).chain(record.holder) {
                    if let Some(index) = indices.get(&entity.id) {
                        if !entities[*index].label_variants.contains(&entity.label) {
                            entities[*index].label_variants.push(entity.label);
                        }
                    } else {
                        indices.insert(entity.id.clone(), entities.len());
                        entities.push(ReviewedEntityChoice {
                            label_variants: vec![entity.label.clone()],
                            entity,
                            first_document_id: set.target.document_id.clone(),
                            first_document_title: titles
                                .get(set.target.document_id.as_str())
                                .unwrap_or(&"Untitled chapter")
                                .to_string(),
                        });
                    }
                }
            }
        }
        let source_epoch = source_epoch(&tx)?;
        tx.commit()?;
        Ok(ReviewedEntityCatalog {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            source_epoch,
            entities,
        })
    }

    fn reviewed_evidence_history(
        &self,
        access: &ProjectAccess,
        snapshot_id: &str,
        object_id: &str,
    ) -> CoreResult<ReviewedHistoryResult> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        let frozen = story_context::load_snapshot(&tx, access, snapshot_id)?;
        let current = frozen.snapshot.context_source_epoch == source_epoch(&tx)?;
        let history = query_evidence_history(&frozen, object_id)?;
        tx.commit()?;
        Ok(ReviewedHistoryResult {
            snapshot_id: snapshot_id.to_owned(),
            current,
            history,
        })
    }

    fn reviewed_promise_catalog(
        &self,
        access: &ProjectAccess,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        let rows = {
            let mut statement = tx.prepare("SELECT d.id,d.title,b.target_revision_id,b.target_body_hash FROM documents d JOIN ready_heads h ON h.document_id=d.id JOIN ready_bundles b ON b.id=h.bundle_id WHERE d.kind='chapter' AND d.trashed=0 AND h.project_id=? AND h.operation_namespace=? ORDER BY d.position,d.id")?;
            statement
                .query_map(
                    params![access.project_id, access.operation_namespace],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        let sources: Vec<_> = rows
            .iter()
            .map(|(document_id, _, revision_id, body_hash)| SourceRef {
                project_id: access.project_id.clone(),
                document_id: document_id.clone(),
                revision_id: revision_id.clone(),
                body_hash: body_hash.clone(),
            })
            .collect();
        let sets = reviewed_story::current_records_for_sources(&tx, access, &sources)?;
        let titles: HashMap<_, _> = rows
            .iter()
            .map(|(id, title, _, _)| (id.as_str(), title.as_str()))
            .collect();
        let mut entities: Vec<ReviewedEntityChoice> = Vec::new();
        let mut indices = HashMap::<String, usize>::new();
        for set in sets {
            for promise in set.promises.unwrap_or_default() {
                let entity = promise.promise;
                if let Some(index) = indices.get(&entity.id) {
                    if !entities[*index].label_variants.contains(&entity.label) {
                        entities[*index].label_variants.push(entity.label);
                    }
                } else {
                    indices.insert(entity.id.clone(), entities.len());
                    entities.push(ReviewedEntityChoice {
                        label_variants: vec![entity.label.clone()],
                        entity,
                        first_document_id: set.target.document_id.clone(),
                        first_document_title: titles
                            .get(set.target.document_id.as_str())
                            .unwrap_or(&"Untitled chapter")
                            .to_string(),
                    });
                }
            }
        }
        let source_epoch = source_epoch(&tx)?;
        tx.commit()?;
        Ok(ReviewedEntityCatalog {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            source_epoch,
            entities,
        })
    }

    fn reviewed_promise_history(
        &self,
        access: &ProjectAccess,
        snapshot_id: &str,
        promise_id: &str,
    ) -> CoreResult<ReviewedPromiseHistoryResult> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        let frozen = story_context::load_snapshot(&tx, access, snapshot_id)?;
        let current = frozen.snapshot.context_source_epoch == source_epoch(&tx)?;
        let history = query_promise_history(&frozen, promise_id)?;
        tx.commit()?;
        Ok(ReviewedPromiseHistoryResult {
            snapshot_id: snapshot_id.to_owned(),
            current,
            history,
        })
    }

    fn reviewed_knowledge_catalog(
        &self,
        access: &ProjectAccess,
        characters: bool,
    ) -> CoreResult<ReviewedEntityCatalog> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        let rows = current_review_rows(&tx, access)?;
        let sources: Vec<_> = rows
            .iter()
            .map(|(document_id, _, revision_id, body_hash)| SourceRef {
                project_id: access.project_id.clone(),
                document_id: document_id.clone(),
                revision_id: revision_id.clone(),
                body_hash: body_hash.clone(),
            })
            .collect();
        let sets = reviewed_story::current_records_for_sources(&tx, access, &sources)?;
        let titles: HashMap<_, _> = rows
            .iter()
            .map(|(id, title, _, _)| (id.as_str(), title.as_str()))
            .collect();
        let mut entities: Vec<ReviewedEntityChoice> = Vec::new();
        let mut indices = HashMap::<String, usize>::new();
        for set in sets {
            if characters {
                for record in &set.records {
                    if let Some(entity) = &record.holder {
                        add_catalog_entity(
                            &mut entities,
                            &mut indices,
                            entity.clone(),
                            &set,
                            &titles,
                        );
                    }
                }
            }
            for record in set.knowledge.as_ref().into_iter().flatten() {
                let entity = if characters {
                    record.character.clone()
                } else {
                    record.topic.clone()
                };
                add_catalog_entity(&mut entities, &mut indices, entity, &set, &titles);
            }
        }
        let source_epoch = source_epoch(&tx)?;
        tx.commit()?;
        Ok(ReviewedEntityCatalog {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            source_epoch,
            entities,
        })
    }

    fn reviewed_knowledge_history(
        &self,
        access: &ProjectAccess,
        snapshot_id: &str,
        character_id: &str,
        topic_id: Option<&str>,
    ) -> CoreResult<ReviewedKnowledgeHistoryResult> {
        self.check_access(access)?;
        let tx = self.db()?.unchecked_transaction()?;
        let frozen = story_context::load_snapshot(&tx, access, snapshot_id)?;
        let current = frozen.snapshot.context_source_epoch == source_epoch(&tx)?;
        let history = query_knowledge_history(&frozen, character_id, topic_id)?;
        tx.commit()?;
        Ok(ReviewedKnowledgeHistoryResult {
            snapshot_id: snapshot_id.to_owned(),
            current,
            history,
        })
    }
}

fn add_catalog_entity(
    entities: &mut Vec<ReviewedEntityChoice>,
    indices: &mut HashMap<String, usize>,
    entity: StoryEntityRef,
    set: &ReviewedRecordSet,
    titles: &HashMap<&str, &str>,
) {
    if let Some(index) = indices.get(&entity.id) {
        if !entities[*index].label_variants.contains(&entity.label) {
            entities[*index].label_variants.push(entity.label);
        }
    } else {
        indices.insert(entity.id.clone(), entities.len());
        entities.push(ReviewedEntityChoice {
            label_variants: vec![entity.label.clone()],
            entity,
            first_document_id: set.target.document_id.clone(),
            first_document_title: titles
                .get(set.target.document_id.as_str())
                .unwrap_or(&"Untitled chapter")
                .to_string(),
        });
    }
}

fn current_review_rows(
    db: &Connection,
    access: &ProjectAccess,
) -> CoreResult<Vec<(String, String, String, String)>> {
    let mut statement = db.prepare(
        "SELECT d.id,d.title,b.target_revision_id,b.target_body_hash FROM documents d JOIN ready_heads h ON h.document_id=d.id JOIN ready_bundles b ON b.id=h.bundle_id WHERE d.kind='chapter' AND d.trashed=0 AND h.project_id=? AND h.operation_namespace=? ORDER BY d.position,d.id",
    )?;
    Ok(statement
        .query_map(
            params![access.project_id, access.operation_namespace],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?)
}

fn source_epoch(db: &Connection) -> CoreResult<String> {
    let epoch = db.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    parse_stored_version(epoch)
}
