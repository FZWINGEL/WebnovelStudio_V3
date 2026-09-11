//! Frozen, project-owned evidence. Revisions remain the only text authority;
//! passage projections can be deleted without losing story material.
// The frozen-context vocabulary moved to wns-context (L2) — the compiler
// consumes it, so it cannot sit above the compiler. Re-exported here so the
// many `crate::projects::story_context::{…}` imports keep resolving.
pub use wns_context::frozen::{
    FrozenContext, SearchHit, SearchMode, SearchResult, SearchStory, SourcePassage, SourceRead,
    search_saved_passages,
};
// The decoder and the eligibility check moved down to `wns-context::frozen` with
// the type they construct and the receipt they return. `decode_snapshot` is
// re-exported at its historical path for the eight call sites in `discussions`,
// `memory`, `project_chat/*` and `project_chat_context`.
pub(crate) use wns_context::frozen::decode_snapshot;
use wns_context::frozen::{eligibility, eligibility_error};
// `validate_frozen_project_chat` moved to `wns-context::frozen` and
// `ProjectChatFreeze` was already in `wns-context::chat_vocabulary`, so both are
// named at the crate that owns them. That leaves one edge from this module into
// `project_chat_context`: `augment_frozen_chat`, which is half of a direct
// function-level cycle with `freeze_project_chat_at` below and has to travel
// with whichever half moves.
use wns_context::chat_vocabulary::ProjectChatFreeze;
use wns_context::frozen::validate_frozen_project_chat;
use super::*;
use crate::context::navigation::{
    FrozenNavigationView, MAX_FROZEN_NAVIGATION_VIEWS, NavigationViewRef, navigation_content_hash,
    validate_frozen_navigation_views, validate_navigation_view_payload,
};
use crate::context::reviewed_evidence::{
    from_storage_parts, validate_evidence_payload,
    validate_frozen_evidence_set,
};
use crate::context::reviewed_knowledge::{
    from_storage_parts as knowledge_from_storage_parts,
    validate_frozen_knowledge_set, validate_knowledge_payload,
};
use crate::context::reviewed_promises::{
    from_storage_parts as promise_from_storage_parts,
    validate_frozen_promise_set, validate_promise_payload,
};
use crate::context::reviewed_summaries::{
    ReviewedSummarySet, validate_frozen_set as validate_frozen_summary,
};
use crate::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind, SourceRef,
    StorySnapshot,
};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextEpochs {
    pub source: String,
    pub policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentAliases {
    pub document_id: String,
    pub aliases: Vec<String>,
    pub source_epoch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FreezeStory {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub basis: BasisKind,
    pub purpose: ContextPurpose,
    pub policy: InformationPolicy,
}

/// Explicit reviewed-story continuation preparation. The target is the
/// current working chapter where output would be placed; only its earlier
/// selected author-reviewed prefix supplies reviewed authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FreezeReviewedContinuation {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub policy: InformationPolicy,
}


pub(crate) enum ContextCommand {
    Epochs(ProjectAccess, Reply<ContextEpochs>),
    ReadAliases(ProjectAccess, String, Reply<DocumentAliases>),
    Freeze(FreezeStory, Reply<FrozenContext>),
    FreezeReviewed(FreezeReviewedContinuation, Reply<FrozenContext>),
    Snapshot(ProjectAccess, String, Reply<FrozenContext>),
    Read(ProjectAccess, String, String, Reply<SourceRead>),
    Search(SearchStory, Reply<SearchResult>),
    Fresh(ProjectAccess, String, Reply<bool>),
    Revoke(ProjectAccess, String, Reply<ContextEpochs>),
    Aliases(
        ProjectAccess,
        String,
        String,
        Vec<String>,
        Reply<ContextEpochs>,
    ),
    Index(ProjectAccess, Option<String>, Reply<u32>),
}

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
        expected_source_epoch: String,
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

impl OwnedProject {
    pub(super) fn handle_context(&mut self, command: ContextCommand) {
        // Context mutations share the same uncertainty fence as saving. There
        // is no independent DB writer or detached indexing connection.
        macro_rules! respond {
            ($reply:expr, $result:expr) => {{
                let result = $result;
                self.fence_uncertain(&result);
                let _ = $reply.send(result);
            }};
        }
        match command {
            ContextCommand::Epochs(access, reply) => respond!(
                reply,
                self.check_access(&access).and_then(|()| epochs(self.db()?))
            ),
            ContextCommand::ReadAliases(access, id, reply) => {
                respond!(reply, self.context_document_aliases(access, &id))
            }
            ContextCommand::Freeze(request, reply) => respond!(reply, self.freeze_story(request)),
            ContextCommand::FreezeReviewed(request, reply) => {
                respond!(reply, self.freeze_reviewed_continuation(request))
            }
            ContextCommand::Snapshot(access, id, reply) => {
                respond!(reply, self.context_snapshot(&access, &id))
            }
            ContextCommand::Read(access, id, handle, reply) => {
                respond!(reply, self.context_read(&access, &id, &handle))
            }
            ContextCommand::Search(request, reply) => respond!(reply, self.context_search(request)),
            ContextCommand::Fresh(access, id, reply) => respond!(
                reply,
                self.context_snapshot(&access, &id)
                    .and_then(|frozen| Ok(
                        frozen.snapshot.context_source_epoch == epochs(self.db()?)?.source
                    ))
            ),
            ContextCommand::Revoke(access, expected, reply) => {
                respond!(reply, self.context_revoke(access, &expected))
            }
            ContextCommand::Aliases(access, id, expected, aliases, reply) => {
                respond!(reply, self.context_aliases(access, &id, &expected, aliases))
            }
            ContextCommand::Index(access, clear, reply) => {
                respond!(reply, self.context_index(access, clear))
            }
        }
    }

    fn freeze_story(&mut self, request: FreezeStory) -> CoreResult<FrozenContext> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        if request.basis != BasisKind::Working {
            return Err(CoreError::new(
                "BasisUnavailable",
                "Reviewed and explicit historical requests need their respective authority records.",
            ));
        }
        // No inferred character knowledge is installed by C1. A later reviewed
        // knowledge view must supply these grants before limited POV is enabled.
        if request.policy.character_id.is_some() || !request.policy.character_grants.is_empty() {
            return Err(CoreError::new(
                "CharacterPolicyUnavailable",
                "Character-specific disclosure needs reviewed knowledge grants.",
            ));
        }
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String)> = tx.query_row(
            "SELECT id,payload_hash FROM story_snapshots WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        if let Some((id, saved_payload)) = previous {
            if payload != saved_payload {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This context operation was already used for a different request.",
                ));
            }
            return load_snapshot(&tx, &request.access, &id);
        }
        let current_epochs = epochs(&tx)?;
        if request.policy.version != current_epochs.policy {
            return Err(CoreError::new(
                "ContextPolicyChanged",
                "Prepare a new request using the current source permissions.",
            ));
        }
        let frozen = freeze_story_at(&tx, &request, &payload)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(frozen)
    }

    fn freeze_reviewed_continuation(
        &mut self,
        request: FreezeReviewedContinuation,
    ) -> CoreResult<FrozenContext> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        check_id(&request.expected.document_id)?;
        parse_version(&request.expected.version)?;
        if request.policy.audience != Audience::RestrictedWriting {
            return Err(CoreError::new(
                "BoundaryConflict",
                "Reviewed continuation needs a restricted writing policy.",
            ));
        }
        if request.policy.character_id.is_some() || !request.policy.character_grants.is_empty() {
            return Err(CoreError::new(
                "CharacterPolicyUnavailable",
                "Character-specific reviewed continuation needs reviewed knowledge grants.",
            ));
        }
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String)> = tx.query_row(
            "SELECT id,payload_hash FROM story_snapshots WHERE operation_namespace=? AND operation_id=?",
            params![request.access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        if let Some((id, saved_payload)) = previous {
            if payload != saved_payload {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This context operation was already used for a different request.",
                ));
            }
            return load_snapshot(&tx, &request.access, &id);
        }
        let current_epochs = epochs(&tx)?;
        if request.policy.version != current_epochs.policy {
            return Err(CoreError::new(
                "ContextPolicyChanged",
                "Prepare a new request using the current source permissions.",
            ));
        }
        let frozen = freeze_reviewed_continuation_at(&tx, &request, &payload)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(frozen)
    }

    fn context_snapshot(&self, access: &ProjectAccess, id: &str) -> CoreResult<FrozenContext> {
        self.check_access(access)?;
        load_snapshot(self.db()?, access, id)
    }

    fn context_read(
        &self,
        access: &ProjectAccess,
        id: &str,
        handle: &str,
    ) -> CoreResult<SourceRead> {
        let frozen = self.context_snapshot(access, id)?;
        read_source(self.db()?, &frozen, handle)
    }

    fn context_search(&self, request: SearchStory) -> CoreResult<SearchResult> {
        let query = request.query.trim();
        if query.is_empty() || query.len() > 500 || !(1..=100).contains(&request.limit) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Search needs 1–500 bytes and a result limit between 1 and 100.",
            ));
        }
        let frozen = self.context_snapshot(&request.access, &request.snapshot_id)?;
        search_frozen(self.db()?, &frozen, query, request.mode, request.limit)
    }

    fn context_revoke(
        &mut self,
        access: ProjectAccess,
        expected: &str,
    ) -> CoreResult<ContextEpochs> {
        self.check_access(&access)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE project SET disclosure_policy_epoch=disclosure_policy_epoch+1,context_source_epoch=context_source_epoch+1 WHERE singleton=1 AND disclosure_policy_epoch=?", [parse_version(expected)?])?;
        if changed != 1 {
            return Err(CoreError::new(
                "ContextPolicyChanged",
                "Source permissions already changed. Read the current version before retrying.",
            ));
        }
        let result = epochs(&tx)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    fn context_aliases(
        &mut self,
        access: ProjectAccess,
        id: &str,
        expected: &str,
        aliases: Vec<String>,
    ) -> CoreResult<ContextEpochs> {
        self.check_access(&access)?;
        if aliases.len() > 64
            || aliases.iter().any(|name| {
                name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control)
            })
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Use at most 64 nonempty names of up to 256 bytes.",
            ));
        }
        let mut aliases: Vec<_> = aliases
            .into_iter()
            .map(|name| name.trim().to_owned())
            .collect();
        aliases.sort();
        aliases.dedup();
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        read_document(&tx, id)?;
        let current = epochs(&tx)?;
        if current.source != expected {
            return Err(CoreError::new(
                "ContextChanged",
                "The story changed. Read its current version before changing aliases.",
            ));
        }
        if read_aliases(&tx, id)? == aliases {
            return Ok(current);
        }
        tx.execute("DELETE FROM document_aliases WHERE document_id=?", [id])?;
        for alias in aliases {
            tx.execute(
                "INSERT INTO document_aliases(document_id,alias) VALUES(?,?)",
                params![id, alias],
            )?;
        }
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        let result = epochs(&tx)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    fn context_document_aliases(
        &self,
        access: ProjectAccess,
        id: &str,
    ) -> CoreResult<DocumentAliases> {
        self.check_access(&access)?;
        let connection = self.db()?;
        let document = read_document(connection, id)?;
        Ok(DocumentAliases {
            document_id: document.head.document_id,
            aliases: read_aliases(connection, id)?,
            source_epoch: epochs(connection)?.source,
        })
    }

    fn context_index(
        &mut self,
        access: ProjectAccess,
        document_id: Option<String>,
    ) -> CoreResult<u32> {
        self.check_access(&access)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut count = 0;
        if let Some(id) = document_id {
            let document = read_document(&tx, &id)?;
            let revision = checkpoint_at(&tx, &document, "index")?;
            tx.execute(
                "DELETE FROM passage_projections WHERE revision_id=?",
                [&revision.id],
            )?;
            let source = SourceRef {
                project_id: access.project_id.clone(),
                document_id: id.clone(),
                revision_id: revision.id.clone(),
                body_hash: revision.head.body_hash.clone(),
            };
            for passage in passages(&revision, &revision.id, &source)? {
                tx.execute("INSERT INTO passage_projections(revision_id,block_id,block_order,body_hash,text) VALUES(?,?,?,?,?)", params![revision.id, passage.block_id, passage.block_order, revision.head.body_hash, passage.text])?;
                count += 1;
            }
            tx.execute("UPDATE documents SET projection_dirty=0 WHERE id=?", [id])?;
        } else {
            tx.execute("DELETE FROM passage_projections", [])?;
            tx.execute(
                "UPDATE documents SET projection_dirty=1 WHERE role='ordinary'",
                [],
            )?;
        }
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(count)
    }
}

/// Build and persist one immutable story snapshot inside an existing actor
/// transaction. Callers that need to compose another durable record (for
/// example a discussion run and its packet) can therefore freeze the same
/// source basis without opening a nested transaction.
pub(super) fn freeze_story_at(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
) -> CoreResult<FrozenContext> {
    freeze_story_impl(tx, request, payload_hash, false, None, None)
}

/// Freeze the explicit reviewed-continuation basis without treating the
/// current target chapter as reviewed authority. The target is the saved
/// working location for the eventual continuation; every earlier source is
/// an exact member of the currently selected author-reviewed prefix.
pub(super) fn freeze_reviewed_continuation_at(
    tx: &Connection,
    request: &FreezeReviewedContinuation,
    payload_hash: &str,
) -> CoreResult<FrozenContext> {
    let current_epochs = epochs(tx)?;
    if request.policy.version != current_epochs.policy {
        return Err(CoreError::new(
            "ContextPolicyChanged",
            "Prepare a new request using the current source permissions.",
        ));
    }
    let document = read_document(tx, &request.expected.document_id)?;
    if document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidDocument",
            "Reviewed continuation needs an active chapter target.",
        ));
    }
    require_head(&document.head, &request.expected)?;
    let target_position: i64 = tx.query_row(
        "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0 AND role='ordinary'",
        [&request.expected.document_id],
        |row| row.get(0),
    )?;
    if target_position < 0 {
        return Err(CoreError::new(
            "InvalidProject",
            "The chapter target has an invalid ordering position.",
        ));
    }
    let frontier = request.policy.reader_frontier.as_deref().ok_or_else(|| {
        CoreError::new(
            "InvalidPolicy",
            "Reviewed continuation needs a reader frontier equal to its target chapter.",
        )
    })?;
    if parse_version(frontier)? != target_position {
        return Err(CoreError::new(
            "BoundaryConflict",
            "Reviewed continuation cannot use a broader future reader frontier.",
        ));
    }
    let prefix = reviewed_story::selected_prefix(
        tx,
        &request.access,
        &request.expected.document_id,
        parse_version(&current_epochs.policy)?,
    )?;
    if prefix.is_empty() {
        return Err(CoreError::new(
            "BasisUnavailable",
            "There is no earlier reviewed chapter. Use working-draft continuation for the first chapter.",
        ));
    }
    for item in &prefix {
        if active_review_fence(tx, &request.access, &item.document_id, &item.bundle_id)? {
            return Err(CoreError::new(
                "BasisUnavailable",
                "An earlier selected reviewed chapter has an unresolved review fence.",
            ));
        }
    }

    let target_revision = checkpoint_at(tx, &document, "reviewedContext")?;
    let source_ref = |revision: &Revision| SourceRef {
        project_id: request.access.project_id.clone(),
        document_id: revision.head.document_id.clone(),
        revision_id: revision.id.clone(),
        body_hash: revision.head.body_hash.clone(),
    };
    let target_ref = source_ref(&target_revision);
    let mut reviewed_members = Vec::with_capacity(prefix.len());
    let mut reviewed_sources = Vec::with_capacity(prefix.len());
    let mut reviewed_source_refs = Vec::with_capacity(prefix.len());
    let mut reviewed_evidence = Vec::new();
    let mut reviewed_promises = Vec::new();
    let mut reviewed_knowledge = Vec::new();
    let mut reviewed_summaries = Vec::new();
    for item in &prefix {
        let revision = read_revision(tx, &item.revision_id)?;
        if revision.head != item.head {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier reviewed chapter no longer matches its selected revision.",
            ));
        }
        let position: i64 = tx.query_row(
            "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0 AND role='ordinary'",
            [&item.document_id],
            |row| row.get(0),
        )?;
        if position < 0
            || position > target_position
            || (position == target_position && item.document_id >= request.expected.document_id)
        {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "The reviewed prefix is not ordered before its continuation target.",
            ));
        }
        let source = source_ref(&revision);
        let source_handle = format!("reviewed-{}", item.bundle_id);
        reviewed_source_refs.push(source.clone());
        reviewed_members.push(ReviewedBasisMember {
            document_id: item.document_id.clone(),
            bundle_id: item.bundle_id.clone(),
            revision_id: item.revision_id.clone(),
            version: item.head.version.clone(),
            body_hash: item.head.body_hash.clone(),
        });
        reviewed_sources.push(SourceDescriptor {
            handle: source_handle,
            source,
            display_name: item.title.clone(),
            kind: SourceKind::ReviewedAuthority,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: Some(position.to_string()),
                visible_to_characters: Vec::new(),
                author_only: false,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        });
    }
    let record_sets =
        reviewed_story::current_records_for_sources(tx, &request.access, &reviewed_source_refs)?;
    for record_set in record_sets {
        let source = reviewed_source_refs
            .iter()
            .find(|source| {
                source.document_id == record_set.target.document_id
                    && source.revision_id == record_set.revision.id
                    && source.body_hash == record_set.target.body_hash
            })
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidReviewedRecords",
                    "The reviewed evidence source could not be matched to its frozen descriptor.",
                )
            })?;
        let source_handle = format!("reviewed-{}", record_set.bundle_id);
        if let Some(summary) = record_set.summary.clone() {
            let summary_hash = record_set.summary_hash.clone().ok_or_else(|| {
                CoreError::new(
                    "InvalidReviewedSummary",
                    "An accepted summary is missing its fingerprint.",
                )
            })?;
            reviewed_summaries.push(ReviewedSummarySet {
                project_id: record_set.project_id.clone(),
                operation_namespace: record_set.operation_namespace.clone(),
                bundle_id: record_set.bundle_id.clone(),
                summary_hash,
                source_handle: source_handle.clone(),
                summary,
            });
        }
        if !record_set.records.is_empty() {
            let records_hash = record_set.records_hash.ok_or_else(|| {
                CoreError::new(
                    "InvalidReviewedRecords",
                    "A nonempty reviewed evidence set has no canonical hash.",
                )
            })?;
            reviewed_evidence.push(from_storage_parts(
                record_set.project_id.clone(),
                record_set.operation_namespace.clone(),
                record_set.bundle_id.clone(),
                records_hash,
                source_handle.clone(),
                source.clone(),
                record_set.records.clone(),
            )?);
        }
        if let Some(knowledge) = record_set.knowledge
            && !knowledge.is_empty()
        {
            let knowledge_hash = record_set.knowledge_hash.ok_or_else(|| {
                CoreError::new(
                    "InvalidReviewedKnowledge",
                    "A nonempty reviewed knowledge set has no canonical hash.",
                )
            })?;
            reviewed_knowledge.push(knowledge_from_storage_parts(
                record_set.project_id.clone(),
                record_set.operation_namespace.clone(),
                record_set.bundle_id.clone(),
                knowledge_hash,
                source_handle.clone(),
                source.clone(),
                knowledge,
            )?);
        }
        if let Some(promises) = record_set.promises
            && !promises.is_empty()
        {
            let promises_hash = record_set.promises_hash.ok_or_else(|| {
                CoreError::new(
                    "InvalidReviewedPromises",
                    "A nonempty reviewed promise set has no canonical hash.",
                )
            })?;
            reviewed_promises.push(promise_from_storage_parts(
                record_set.project_id,
                record_set.operation_namespace,
                record_set.bundle_id,
                promises_hash,
                source_handle,
                source.clone(),
                promises,
            )?);
        }
    }
    let mut sources = Vec::with_capacity(prefix.len() + 1);
    sources.push(SourceDescriptor {
        handle: target_revision.id.clone(),
        source: target_ref.clone(),
        display_name: document.title.clone(),
        kind: SourceKind::CurrentDraft,
        current: true,
        coverage: CoverageLabel::Verbatim,
        disclosure: Disclosure {
            reader_position: Some(target_position.to_string()),
            visible_to_characters: Vec::new(),
            author_only: false,
            future_private: false,
        },
        story_time: None,
        // The target is working prose, not a derived digest of the entire
        // prefix. Reviewed authority is recorded in the basis manifest and
        // remains eligible for optional packing under the request budget.
        dependencies: Vec::new(),
    });
    sources.extend(reviewed_sources);
    let snapshot = StorySnapshot {
        snapshot_id: new_id(),
        project_id: request.access.project_id.clone(),
        basis: BasisKind::Reviewed,
        target: target_ref,
        context_source_epoch: current_epochs.source.clone(),
        ordering_epoch: current_epochs.source,
        disclosure_policy_version: current_epochs.policy,
        sources,
        reviewed_basis: Some(ReviewedBasisManifest {
            project_id: request.access.project_id.clone(),
            operation_namespace: request.access.operation_namespace.clone(),
            prefix: reviewed_members,
        }),
    };
    let handles = snapshot
        .sources
        .iter()
        .map(|source| source.handle.clone())
        .collect::<Vec<_>>();
    eligibility(
        &snapshot,
        &request.policy,
        ContextPurpose::Continue,
        &handles,
    )
    .map_err(eligibility_error)?;
    let frozen = FrozenContext {
        snapshot,
        policy: request.policy.clone(),
        purpose: ContextPurpose::Continue,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence,
        reviewed_promises,
        reviewed_knowledge,
        reviewed_summaries,
        project_chat: None,
    };
    for evidence in &frozen.reviewed_evidence {
        validate_frozen_evidence_set(evidence, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for promises in &frozen.reviewed_promises {
        validate_frozen_promise_set(promises, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for knowledge in &frozen.reviewed_knowledge {
        validate_frozen_knowledge_set(knowledge, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for summary in &frozen.reviewed_summaries {
        validate_frozen_summary(summary, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    let json = serde_json::to_string(&frozen)?;
    tx.execute(
        "INSERT INTO story_snapshots(id,project_id,operation_namespace,operation_id,payload_hash,context_source_epoch,disclosure_policy_epoch,manifest_json,manifest_hash) VALUES(?,?,?,?,?,?,?,?,?)",
        params![
            frozen.snapshot.snapshot_id,
            request.access.project_id,
            request.access.operation_namespace,
            request.operation_id,
            payload_hash,
            parse_version(&frozen.snapshot.context_source_epoch)?,
            parse_version(&frozen.policy.version)?,
            json,
            sha256_hex(json.as_bytes())
        ],
    )?;
    insert_snapshot_sources(tx, &frozen)?;
    Ok(frozen)
}

fn active_review_fence(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
    bundle_id: &str,
) -> CoreResult<bool> {
    db.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM review_fences f
            JOIN ready_bundles b ON b.id=f.affected_bundle_id
            JOIN ready_heads h ON h.project_id=f.project_id
                AND h.operation_namespace=f.operation_namespace
                AND h.document_id=b.document_id
                AND h.bundle_id=f.affected_bundle_id
            WHERE f.project_id=? AND f.operation_namespace=?
              AND f.affected_bundle_id=? AND h.document_id=?
        )",
        params![
            access.project_id,
            access.operation_namespace,
            bundle_id,
            document_id
        ],
        |row| row.get(0),
    )
    .map_err(CoreError::from)
}

pub(super) fn freeze_discussion_story_at(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
    retry_guidance: Option<&[crate::context::guidance::FrozenGuidance]>,
) -> CoreResult<FrozenContext> {
    freeze_story_impl(tx, request, payload_hash, true, retry_guidance, None)
}

/// Project-chat variant of the discussion freeze. It shares the ordinary
/// source compiler and attaches the authenticated project projection before
/// the immutable manifest is persisted.
pub(super) fn freeze_project_chat_at(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
    chat: &ProjectChatFreeze,
) -> CoreResult<FrozenContext> {
    freeze_story_impl(tx, request, payload_hash, true, None, Some(chat))
}

/// The first memory recipe reads exactly one saved chapter. Source freezing
/// never adds discussion, aliases, guidance, or neighboring story material.
pub(super) fn freeze_memory_story_at(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
) -> CoreResult<FrozenContext> {
    if request.purpose != ContextPurpose::MemoryAnalysis {
        return Err(CoreError::new(
            "InvalidMemoryRequest",
            "Use the chapter memory analysis recipe.",
        ));
    }
    freeze_story_impl(tx, request, payload_hash, false, None, None)
}

fn freeze_story_impl(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
    include_request_guidance: bool,
    retry_guidance: Option<&[crate::context::guidance::FrozenGuidance]>,
    chat: Option<&ProjectChatFreeze>,
) -> CoreResult<FrozenContext> {
    if request.basis != BasisKind::Working {
        return Err(CoreError::new(
            "BasisUnavailable",
            "Reviewed and explicit historical requests need their respective authority records.",
        ));
    }
    if request.policy.character_id.is_some() || !request.policy.character_grants.is_empty() {
        return Err(CoreError::new(
            "CharacterPolicyUnavailable",
            "Character-specific disclosure needs reviewed knowledge grants.",
        ));
    }
    check_id(&request.operation_id)?;
    let current_epochs = epochs(tx)?;
    if request.policy.version != current_epochs.policy {
        return Err(CoreError::new(
            "ContextPolicyChanged",
            "Prepare a new request using the current source permissions.",
        ));
    }
    let target_document = if chat.is_some() {
        read_document_with_role(
            tx,
            &request.expected.document_id,
            DocumentRole::ConversationAnchor,
        )?
    } else {
        read_document(tx, &request.expected.document_id)?
    };
    require_head(&target_document.head, &request.expected)?;
    if request.policy.audience == Audience::AuthorRoom
        && matches!(
            request.purpose,
            ContextPurpose::Revise | ContextPurpose::Continue
        )
        && !(request.purpose == ContextPurpose::Revise && target_document.kind != "chapter")
    {
        return Err(CoreError::new(
            "BoundaryConflict",
            "Prepare a separate writing request with an approved disclosure boundary.",
        ));
    }
    let memory_analysis = request.purpose == ContextPurpose::MemoryAnalysis;
    if chat.is_some() && request.expected.document_id.is_empty() {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project chat requires its blank conversation anchor as the run target.",
        ));
    }
    if memory_analysis && target_document.kind != "chapter" {
        return Err(CoreError::new(
            "InvalidMemoryRequest",
            "Story memory refresh needs a saved chapter.",
        ));
    }
    let target = checkpoint_at(tx, &target_document, "context")?;
    let source_ref = |revision: &Revision| SourceRef {
        project_id: request.access.project_id.clone(),
        document_id: revision.head.document_id.clone(),
        revision_id: revision.id.clone(),
        body_hash: revision.head.body_hash.clone(),
    };
    let mut snapshot = StorySnapshot {
        snapshot_id: new_id(),
        project_id: request.access.project_id.clone(),
        basis: request.basis,
        target: source_ref(&target),
        context_source_epoch: current_epochs.source.clone(),
        ordering_epoch: current_epochs.source,
        disclosure_policy_version: current_epochs.policy,
        sources: Vec::new(),
        reviewed_basis: None,
    };
    let ids = ordered_documents(tx)?;
    for (id, position) in ids {
        if memory_analysis && id != request.expected.document_id {
            continue;
        }
        let document = read_document(tx, &id)?;
        let revision = checkpoint_at(tx, &document, "context")?;
        let chapter = document.kind == "chapter";
        snapshot.sources.push(SourceDescriptor {
            handle: revision.id.clone(),
            source: source_ref(&revision),
            display_name: document.title,
            kind: SourceKind::CurrentDraft,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: chapter.then(|| position.to_string()),
                visible_to_characters: Vec::new(),
                author_only: !chapter,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        });
    }
    if chat.is_some() {
        snapshot.sources.push(SourceDescriptor {
            handle: target.id.clone(),
            source: source_ref(&target),
            display_name: target_document.title.clone(),
            kind: SourceKind::ConversationControl,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: None,
                visible_to_characters: Vec::new(),
                author_only: true,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        });
    }
    let target_handle = target.id;
    eligibility(
        &snapshot,
        &request.policy,
        request.purpose,
        std::slice::from_ref(&target_handle),
    )
    .map_err(eligibility_error)?;
    let total = snapshot.sources.len();
    // Test every candidate with the mandatory target. Only permitted
    // descriptors (including their titles) enter the public manifest.
    let mut eligible = HashSet::new();
    // C1 resolves only original drafts with no derived dependencies. Check
    // each against the target without repeatedly validating a thousand-
    // document manifest. Derived-source closure is handled by C0/C4.
    let mut candidate_snapshot = snapshot.clone();
    candidate_snapshot
        .sources
        .retain(|source| source.handle == target_handle);
    for source in &snapshot.sources {
        let selected = if source.handle == target_handle {
            vec![target_handle.clone()]
        } else {
            vec![target_handle.clone(), source.handle.clone()]
        };
        if source.handle != target_handle {
            candidate_snapshot.sources.push(source.clone());
        }
        if eligibility(
            &candidate_snapshot,
            &request.policy,
            request.purpose,
            &selected,
        )
        .is_ok()
        {
            eligible.insert(source.handle.clone());
        }
        candidate_snapshot.sources.truncate(1);
    }
    snapshot
        .sources
        .retain(|source| eligible.contains(&source.handle));
    let mut aliases = BTreeMap::new();
    for source in &snapshot.sources {
        // Author-entered aliases have no reader-disclosure provenance yet.
        // They remain author-room metadata until explicit safe grants exist.
        if request.policy.audience == Audience::AuthorRoom && !memory_analysis {
            aliases.insert(
                source.handle.clone(),
                read_aliases(tx, &source.source.document_id)?,
            );
        }
    }
    let mut frozen = FrozenContext {
        excluded_source_count: (total - snapshot.sources.len()) as u32,
        snapshot,
        policy: request.policy.clone(),
        purpose: request.purpose,
        aliases,
        guidance: if request.policy.audience == Audience::AuthorRoom && !memory_analysis {
            let mut selected = guidance::select_guidance_at(
                tx,
                &request.access.project_id,
                &request.expected.document_id,
                include_request_guidance && retry_guidance.is_none(),
            )?;
            if let Some(reused) = retry_guidance {
                selected.extend_from_slice(reused);
            }
            selected
        } else {
            Vec::new()
        },
        conversation: if include_request_guidance
            && request.policy.audience == Audience::AuthorRoom
            && request.purpose == ContextPurpose::Discuss
        {
            conversation_context::select_conversation_at(
                tx,
                &request.access,
                &request.expected.document_id,
                &request.policy.version,
            )?
        } else {
            None
        },
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_promises: Vec::new(),
        reviewed_knowledge: Vec::new(),
        reviewed_summaries: Vec::new(),
        project_chat: None,
    };
    frozen.navigation_views = select_navigation_views_at(tx, &frozen)?;
    if frozen.policy.audience == Audience::AuthorRoom
        && frozen.purpose != ContextPurpose::MemoryAnalysis
    {
        let eligible_sources = frozen
            .snapshot
            .sources
            .iter()
            .filter(|source| {
                matches!(
                    source.kind,
                    SourceKind::CurrentDraft | SourceKind::ReviewedAuthority
                )
            })
            .collect::<Vec<_>>();
        let source_refs = eligible_sources
            .iter()
            .map(|source| source.source.clone())
            .collect::<Vec<_>>();
        let record_sets =
            reviewed_story::current_records_for_sources(tx, &request.access, &source_refs)?;
        for record_set in record_sets {
            let source = eligible_sources
                .iter()
                .find(|source| {
                    source.source.document_id == record_set.target.document_id
                        && source.source.revision_id == record_set.revision.id
                        && source.source.body_hash == record_set.target.body_hash
                })
                .ok_or_else(|| {
                    CoreError::new(
                        "InvalidReviewedRecords",
                        "The reviewed evidence source could not be matched to its frozen descriptor.",
                    )
                })?;
            let source_handle = source.handle.clone();
            if let Some(summary) = record_set.summary.clone() {
                let summary_hash = record_set.summary_hash.clone().ok_or_else(|| {
                    CoreError::new(
                        "InvalidReviewedSummary",
                        "An accepted summary is missing its fingerprint.",
                    )
                })?;
                frozen.reviewed_summaries.push(ReviewedSummarySet {
                    project_id: record_set.project_id.clone(),
                    operation_namespace: record_set.operation_namespace.clone(),
                    bundle_id: record_set.bundle_id.clone(),
                    summary_hash,
                    source_handle: source_handle.clone(),
                    summary,
                });
            }
            if !record_set.records.is_empty() {
                let records_hash = record_set.records_hash.ok_or_else(|| {
                    CoreError::new(
                        "InvalidReviewedRecords",
                        "A nonempty reviewed evidence set has no canonical hash.",
                    )
                })?;
                frozen.reviewed_evidence.push(from_storage_parts(
                    record_set.project_id.clone(),
                    record_set.operation_namespace.clone(),
                    record_set.bundle_id.clone(),
                    records_hash,
                    source_handle.clone(),
                    source.source.clone(),
                    record_set.records.clone(),
                )?);
            }
            if let Some(knowledge) = record_set.knowledge
                && !knowledge.is_empty()
            {
                let knowledge_hash = record_set.knowledge_hash.ok_or_else(|| {
                    CoreError::new(
                        "InvalidReviewedKnowledge",
                        "A nonempty reviewed knowledge set has no canonical hash.",
                    )
                })?;
                frozen.reviewed_knowledge.push(knowledge_from_storage_parts(
                    record_set.project_id.clone(),
                    record_set.operation_namespace.clone(),
                    record_set.bundle_id.clone(),
                    knowledge_hash,
                    source_handle.clone(),
                    source.source.clone(),
                    knowledge,
                )?);
            }
            if let Some(promises) = record_set.promises
                && !promises.is_empty()
            {
                let promises_hash = record_set.promises_hash.ok_or_else(|| {
                    CoreError::new(
                        "InvalidReviewedPromises",
                        "A nonempty reviewed promise set has no canonical hash.",
                    )
                })?;
                frozen.reviewed_promises.push(promise_from_storage_parts(
                    record_set.project_id,
                    record_set.operation_namespace,
                    record_set.bundle_id,
                    promises_hash,
                    source_handle,
                    source.source.clone(),
                    promises,
                )?);
            }
        }
    }
    for evidence in &frozen.reviewed_evidence {
        validate_frozen_evidence_set(evidence, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for promises in &frozen.reviewed_promises {
        validate_frozen_promise_set(promises, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for knowledge in &frozen.reviewed_knowledge {
        validate_frozen_knowledge_set(knowledge, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for summary in &frozen.reviewed_summaries {
        validate_frozen_summary(summary, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    if let Some(chat) = chat {
        project_chat_context::augment_frozen_chat(tx, request, &mut frozen, chat)?;
    }
    validate_frozen_navigation_views(
        &frozen.navigation_views,
        &frozen.snapshot,
        &frozen.policy,
        frozen.purpose,
    )?;
    let json = serde_json::to_string(&frozen)?;
    tx.execute(
        "INSERT INTO story_snapshots(id,project_id,operation_namespace,operation_id,payload_hash,context_source_epoch,disclosure_policy_epoch,manifest_json,manifest_hash) VALUES(?,?,?,?,?,?,?,?,?)",
        params![frozen.snapshot.snapshot_id, request.access.project_id, request.access.operation_namespace, request.operation_id, payload_hash, parse_version(&frozen.snapshot.context_source_epoch)?, parse_version(&frozen.policy.version)?, json, sha256_hex(json.as_bytes())],
    )?;
    insert_snapshot_sources(tx, &frozen)?;
    insert_snapshot_navigation_views(tx, &frozen)?;
    guidance::pin_guidance_at(tx, &frozen.snapshot.snapshot_id, &frozen.guidance)?;
    Ok(frozen)
}

fn insert_snapshot_sources(db: &Connection, frozen: &FrozenContext) -> CoreResult<()> {
    for source in &frozen.snapshot.sources {
        let reader_position = if frozen.snapshot.basis == BasisKind::Reviewed {
            source
                .disclosure
                .reader_position
                .as_deref()
                .map(parse_version)
                .transpose()?
        } else {
            None
        };
        db.execute(
            "INSERT INTO snapshot_sources(snapshot_id,handle,document_id,revision_id,body_hash,reader_position) VALUES(?,?,?,?,?,?)",
            params![
                frozen.snapshot.snapshot_id,
                source.handle,
                source.source.document_id,
                source.source.revision_id,
                source.source.body_hash,
                reader_position,
            ],
        )?;
    }
    Ok(())
}

fn insert_snapshot_navigation_views(db: &Connection, frozen: &FrozenContext) -> CoreResult<()> {
    if frozen.navigation_views.is_empty() {
        return Ok(());
    }
    for view in &frozen.navigation_views {
        db.execute(
            "INSERT INTO snapshot_navigation_views(snapshot_id,view_id,content_hash) VALUES(?,?,?)",
            params![
                frozen.snapshot.snapshot_id,
                view.reference.view_id,
                view.reference.content_hash,
            ],
        )?;
    }
    Ok(())
}

/// Choose immutable, completed chapter views for a new author-room snapshot.
/// The view rows are deliberately inspected here instead of mutating their
/// `installed_current` flag: freezing a packet must never rewrite generated
/// memory history or advance the source epoch.
fn select_navigation_views_at(
    db: &Connection,
    frozen: &FrozenContext,
) -> CoreResult<Vec<FrozenNavigationView>> {
    if frozen.snapshot.basis != BasisKind::Working
        || frozen.policy.audience != Audience::AuthorRoom
        || !matches!(
            frozen.purpose,
            ContextPurpose::Discuss | ContextPurpose::Plan | ContextPurpose::StoryQuestion
        )
    {
        return Ok(Vec::new());
    }

    let (current_project, current_namespace): (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if current_project != frozen.snapshot.project_id {
        return Ok(Vec::new());
    }

    let mut selected = Vec::new();
    for source in &frozen.snapshot.sources {
        if selected.len() >= MAX_FROZEN_NAVIGATION_VIEWS
            || source.source.document_id == frozen.snapshot.target.document_id
            || source.kind != SourceKind::CurrentDraft
            || source.coverage != CoverageLabel::Verbatim
            || !source.current
            || source.disclosure.reader_position.is_none()
            || source.disclosure.author_only
            || source.disclosure.future_private
            || !source.dependencies.is_empty()
        {
            continue;
        }
        let is_chapter: bool = db.query_row(
            "SELECT kind='chapter' FROM documents WHERE id=? AND trashed=0 AND role='ordinary'",
            [&source.source.document_id],
            |row| row.get(0),
        )?;
        if !is_chapter {
            continue;
        }

        let source_epoch = parse_version(&frozen.snapshot.context_source_epoch)?;
        let policy_epoch = parse_version(&frozen.policy.version)?;
        let mut statement = db.prepare(
            "SELECT v.id FROM memory_views v
             JOIN memory_jobs j ON j.id=v.job_id
             JOIN memory_results r ON r.job_id=v.job_id
             WHERE v.project_id=? AND v.operation_namespace=? AND v.document_id=?
               AND v.source_revision_id=? AND v.source_body_hash=?
               AND v.context_source_epoch<=? AND v.disclosure_policy_epoch=?
               AND j.status='completed'
               AND r.outcome='completed' AND r.candidate_json IS NOT NULL
             ORDER BY v.created_at DESC,v.rowid DESC,v.id DESC",
        )?;
        let ids = statement
            .query_map(
                params![
                    &frozen.snapshot.project_id,
                    &current_namespace,
                    &source.source.document_id,
                    &source.source.revision_id,
                    &source.source.body_hash,
                    source_epoch,
                    policy_epoch,
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            continue;
        }
        // A source in the frozen manifest is already policy-eligible.  Read
        // the exact original revision once, then validate each candidate
        // against that retained prose before it can enter the packet.
        let source_read = read_source(db, frozen, &source.handle)?;
        for view_id in ids {
            let Some(view) = read_current_navigation_view(
                db,
                frozen,
                source,
                &source_read,
                &current_namespace,
                &view_id,
            )?
            else {
                continue;
            };
            selected.push(view);
            break;
        }
    }
    Ok(selected)
}

/// Read one candidate in its immutable job/result/view chain. Invalid or
/// stale current rows are ignored so an old aid cannot silently become a
/// current fallback for a new freeze.
fn read_current_navigation_view(
    db: &Connection,
    frozen: &FrozenContext,
    source: &SourceDescriptor,
    source_read: &SourceRead,
    current_namespace: &str,
    view_id: &str,
) -> CoreResult<Option<FrozenNavigationView>> {
    let view = match crate::projects::memory::validate_navigation_view_record(
        db,
        view_id,
        Some(&frozen.snapshot.snapshot_id),
    ) {
        Ok(view) => view,
        Err(_) => return Ok(None),
    };
    let Some(candidate) = view.candidate.clone() else {
        return Ok(None);
    };
    if view.project_id != frozen.snapshot.project_id
        || view.operation_namespace != current_namespace
        || view.document_id != source.source.document_id
        || view.source != source.source
        || parse_version(&view.context_source_epoch)?
            > parse_version(&frozen.snapshot.context_source_epoch)?
        || view.disclosure_policy_version != frozen.policy.version
        || candidate.source != source.source
    {
        return Ok(None);
    }
    let dependencies: Vec<SourceRef> = {
        let mut deps = db.prepare(
            "SELECT document_id,revision_id,body_hash FROM memory_view_sources
             WHERE view_id=? ORDER BY document_id,revision_id,body_hash",
        )?;
        deps.query_map([view_id], |row| {
            Ok(SourceRef {
                project_id: view.project_id.clone(),
                document_id: row.get(0)?,
                revision_id: row.get(1)?,
                body_hash: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    if dependencies != [source.source.clone()] {
        return Ok(None);
    }
    let view = FrozenNavigationView {
        reference: NavigationViewRef {
            view_id: view.id,
            project_id: view.project_id,
            operation_namespace: view.operation_namespace,
            content_hash: navigation_content_hash(&candidate)?,
        },
        source_context_epoch: view.context_source_epoch,
        disclosure_policy_version: view.disclosure_policy_version,
        dependencies,
        candidate,
    };
    if validate_navigation_view_payload(&view, source_read).is_err() {
        return Ok(None);
    }
    Ok(Some(view))
}

fn epochs(db: &Connection) -> CoreResult<ContextEpochs> {
    let (source, policy): (i64, i64) = db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(ContextEpochs {
        source: parse_stored_version(source)?,
        policy: parse_stored_version(policy)?,
    })
}

fn ordered_documents(db: &Connection) -> CoreResult<Vec<(String, i64)>> {
    let mut statement =
        db.prepare("SELECT id,position FROM documents WHERE trashed=0 AND role='ordinary' ORDER BY position,id")?;
    Ok(statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}

fn read_aliases(db: &Connection, id: &str) -> CoreResult<Vec<String>> {
    let mut statement =
        db.prepare("SELECT alias FROM document_aliases WHERE document_id=? ORDER BY alias")?;
    Ok(statement
        .query_map([id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}


pub(super) fn load_snapshot(
    db: &Connection,
    access: &ProjectAccess,
    id: &str,
) -> CoreResult<FrozenContext> {
    let (frozen, namespace) = validated_snapshot_record(db, id)?;
    if frozen.snapshot.project_id != access.project_id || namespace != access.operation_namespace {
        return Err(CoreError::new(
            "ContextProjectMismatch",
            "This snapshot belongs to another project or an independently recovered copy.",
        ));
    }
    if frozen.policy.version != epochs(db)?.policy {
        return Err(CoreError::new(
            "ContextPolicyChanged",
            "Source permissions changed. Prepare a new request before reading more evidence.",
        ));
    }
    Ok(frozen)
}

/// Check retained data integrity without granting access under today's policy.
/// Backup validation must retain valid historical records after revocation;
/// request-facing readers must additionally use `load_snapshot` above.
pub(super) fn validated_snapshot_record(
    db: &Connection,
    id: &str,
) -> CoreResult<(FrozenContext, String)> {
    check_id(id)?;
    navigation_pin_table_available(db)?;
    let row: Option<(String,String,String,String,i64,i64)> = db.query_row("SELECT project_id,operation_namespace,manifest_json,manifest_hash,context_source_epoch,disclosure_policy_epoch FROM story_snapshots WHERE id=?", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).optional()?;
    let (project, namespace, json, hash, source_epoch, policy_epoch) = row.ok_or_else(|| {
        CoreError::new(
            "ContextNotFound",
            "This snapshot is not available in this project.",
        )
    })?;
    let frozen = decode_snapshot(&json, &hash)?;
    if frozen.snapshot.snapshot_id != id
        || frozen.snapshot.project_id != project
        || frozen.snapshot.context_source_epoch != parse_stored_version(source_epoch)?
        || frozen.policy.version != parse_stored_version(policy_epoch)?
    {
        return Err(CoreError::new(
            "InvalidContext",
            "The context identity does not match its manifest.",
        ));
    }
    validate_pins(db, &frozen, &namespace)?;
    Ok((frozen, namespace))
}

fn validate_pins(
    db: &Connection,
    frozen: &FrozenContext,
    snapshot_namespace: &str,
) -> CoreResult<()> {
    validate_frozen_project_chat(db, frozen, snapshot_namespace)?;
    let mut review_validation = reviewed_story::ReviewValidationContext::new(db);
    let mut summary_handles = HashSet::new();
    for summary in &frozen.reviewed_summaries {
        validate_frozen_summary(summary, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        if summary.operation_namespace != snapshot_namespace
            || !summary_handles.insert(&summary.source_handle)
        {
            return Err(CoreError::new(
                "InvalidReviewedSummary",
                "Accepted summaries have an invalid namespace or duplicate source.",
            ));
        }
        review_validation.validate_reviewed_summary(
            &frozen.snapshot.project_id,
            snapshot_namespace,
            &summary.bundle_id,
            &summary.summary_hash,
            &summary.summary,
        )?;
    }

    for evidence in &frozen.reviewed_evidence {
        validate_frozen_evidence_set(evidence, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        let source = read_source(db, frozen, &evidence.source_handle)?;
        validate_evidence_payload(evidence, &source)?;
        review_validation.validate_reviewed_records(
            &frozen.snapshot.project_id,
            snapshot_namespace,
            &evidence.bundle_id,
            &evidence.source,
            &evidence.records_hash,
            &evidence.records,
        )?;
    }
    for promises in &frozen.reviewed_promises {
        validate_frozen_promise_set(promises, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        let source = read_source(db, frozen, &promises.source_handle)?;
        validate_promise_payload(promises, &source)?;
        review_validation.validate_reviewed_promises(
            &frozen.snapshot.project_id,
            snapshot_namespace,
            &promises.bundle_id,
            &promises.source,
            &promises.records_hash,
            &promises.records,
        )?;
    }
    let mut knowledge_handles = HashSet::new();
    for knowledge in &frozen.reviewed_knowledge {
        if knowledge.operation_namespace != snapshot_namespace
            || !knowledge_handles.insert(&knowledge.source_handle)
        {
            return Err(CoreError::new(
                "InvalidReviewedKnowledge",
                "Reviewed knowledge has an invalid namespace or duplicate source.",
            ));
        }
        validate_frozen_knowledge_set(knowledge, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        let source = read_source(db, frozen, &knowledge.source_handle)?;
        validate_knowledge_payload(knowledge, &source)?;
        review_validation.validate_reviewed_knowledge(
            &frozen.snapshot.project_id,
            snapshot_namespace,
            &knowledge.bundle_id,
            &knowledge.source,
            &knowledge.records_hash,
            &knowledge.records,
        )?;
    }
    if let Some(manifest) = frozen.snapshot.reviewed_basis.as_ref() {
        review_validation.validate_reviewed_snapshot_manifest(
            &frozen.snapshot.project_id,
            snapshot_namespace,
            &frozen.policy.version,
            manifest,
            &frozen.snapshot.sources,
        )?;
    }
    conversation_context::validate_conversation_at(db, frozen)?;
    let has_guidance: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='snapshot_guidance')", [], |row| row.get(0))?;
    if has_guidance {
        guidance::validate_guidance_at(
            db,
            &frozen.snapshot.snapshot_id,
            &frozen.snapshot.project_id,
            &frozen.guidance,
        )?;
    } else if !frozen.guidance.is_empty() {
        return Err(CoreError::new(
            "InvalidContext",
            "The pinned author guidance is missing.",
        ));
    }
    let has_reader_position: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('snapshot_sources') WHERE name='reader_position')",
        [],
        |row| row.get(0),
    )?;
    if !has_reader_position {
        return Err(CoreError::new(
            "InvalidContext",
            "The project is missing immutable reviewed reader-position pins.",
        ));
    }
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM snapshot_sources WHERE snapshot_id=?",
        [&frozen.snapshot.snapshot_id],
        |row| row.get(0),
    )?;
    if count != frozen.snapshot.sources.len() as i64 {
        return Err(CoreError::new(
            "InvalidContext",
            "The context source pins are incomplete.",
        ));
    }
    for source in &frozen.snapshot.sources {
        // The ordinary read path is role-aware.  Rechecking each pinned
        // source here prevents a forged manifest from relabeling a draft or
        // control anchor as ordinary evidence after it was persisted.
        let _ = read_source(db, frozen, &source.handle)?;
        let pin: Option<Option<i64>> = db
            .query_row(
                "SELECT s.reader_position FROM snapshot_sources s JOIN revisions r ON r.document_id=s.document_id AND r.id=s.revision_id WHERE s.snapshot_id=? AND s.handle=? AND s.document_id=? AND s.revision_id=? AND s.body_hash=? AND r.body_hash=s.body_hash",
                params![
                    frozen.snapshot.snapshot_id,
                    source.handle,
                    source.source.document_id,
                    source.source.revision_id,
                    source.source.body_hash
                ],
                |row| row.get(0),
            )
            .optional()?;
        let expected_reader_position = if frozen.snapshot.basis == BasisKind::Reviewed {
            source
                .disclosure
                .reader_position
                .as_deref()
                .map(parse_version)
                .transpose()?
        } else {
            None
        };
        if pin != Some(expected_reader_position) {
            return Err(CoreError::new(
                "InvalidContext",
                "A pinned source no longer matches its exact revision or reader position.",
            ));
        }
    }
    validate_navigation_pins(db, frozen, snapshot_namespace)?;
    Ok(())
}

fn validate_navigation_pins(
    db: &Connection,
    frozen: &FrozenContext,
    snapshot_namespace: &str,
) -> CoreResult<()> {
    let table_exists = navigation_pin_table_available(db)?;
    // Schema-16 manifests decode with an empty default field and remain
    // readable during migration. A non-empty field always requires schema 17.
    if !table_exists {
        if frozen.navigation_views.is_empty() {
            return Ok(());
        }
        return Err(CoreError::new(
            "InvalidContext",
            "The frozen navigation view pins are missing.",
        ));
    }
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM snapshot_navigation_views WHERE snapshot_id=?",
        [&frozen.snapshot.snapshot_id],
        |row| row.get(0),
    )?;
    if count != frozen.navigation_views.len() as i64 {
        return Err(CoreError::new(
            "InvalidContext",
            "The frozen navigation view pin set is incomplete or has extras.",
        ));
    }
    for view in &frozen.navigation_views {
        let stored: Option<String> = db
            .query_row(
                "SELECT content_hash FROM snapshot_navigation_views
                 WHERE snapshot_id=? AND view_id=?",
                params![frozen.snapshot.snapshot_id, view.reference.view_id],
                |row| row.get(0),
            )
            .optional()?;
        if stored.as_deref() != Some(view.reference.content_hash.as_str()) {
            return Err(CoreError::new(
                "InvalidContext",
                "A frozen navigation view pin does not match its content fingerprint.",
            ));
        }
        validate_navigation_view_record(db, frozen, snapshot_namespace, view)?;
    }
    Ok(())
}

/// Schema 17 makes generated-view pins part of the immutable project shape.
/// Keep the missing-table fallback only for pre-17 legacy readers, which are
/// migrated before normal project access; a tampered current database must
/// fail even when every retained manifest has an empty navigation field.
fn navigation_pin_table_available(db: &Connection) -> CoreResult<bool> {
    let schema: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='snapshot_navigation_views')",
        [],
        |row| row.get(0),
    )?;
    if schema >= 17 && !exists {
        return Err(CoreError::new(
            "InvalidContext",
            "The schema 17 project is missing immutable navigation view pins.",
        ));
    }
    Ok(exists)
}

/// Authenticate an immutable generated view and its original memory-analysis
/// snapshot. This intentionally does not compare the view's stored epoch with
/// today's epoch: a historical frozen packet remains valid after later edits
/// or disclosure revocation, while `load_snapshot` still enforces the live
/// request policy before exposing it to callers.
fn validate_navigation_view_record(
    db: &Connection,
    frozen: &FrozenContext,
    snapshot_namespace: &str,
    view: &FrozenNavigationView,
) -> CoreResult<()> {
    let stored = crate::projects::memory::validate_navigation_view_record(
        db,
        &view.reference.view_id,
        Some(&frozen.snapshot.snapshot_id),
    )?;
    let Some(stored_candidate) = stored.candidate.as_ref() else {
        return Err(CoreError::new(
            "InvalidContext",
            "A frozen navigation view has no validated candidate.",
        ));
    };
    if stored.id != view.reference.view_id
        || stored.project_id != view.reference.project_id
        || stored.operation_namespace != view.reference.operation_namespace
        || stored.project_id != frozen.snapshot.project_id
        || stored.operation_namespace != snapshot_namespace
        || stored.source != view.candidate.source
        || stored.context_source_epoch != view.source_context_epoch
        || stored.disclosure_policy_version != view.disclosure_policy_version
        || stored_candidate != &view.candidate
    {
        return Err(CoreError::new(
            "InvalidContext",
            "A frozen navigation view is not bound to its immutable memory record.",
        ));
    }
    let mut deps = db.prepare(
        "SELECT document_id,revision_id,body_hash FROM memory_view_sources
         WHERE view_id=? ORDER BY document_id,revision_id,body_hash",
    )?;
    let stored_deps = deps
        .query_map([&view.reference.view_id], |row| {
            Ok(SourceRef {
                project_id: stored.project_id.clone(),
                document_id: row.get(0)?,
                revision_id: row.get(1)?,
                body_hash: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if stored_deps != view.dependencies {
        return Err(CoreError::new(
            "InvalidContext",
            "A frozen navigation view does not retain its complete source dependency set.",
        ));
    }
    Ok(())
}

pub(super) fn read_source(
    db: &Connection,
    frozen: &FrozenContext,
    handle: &str,
) -> CoreResult<SourceRead> {
    let descriptor = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.handle == handle)
        .ok_or_else(|| {
            CoreError::new(
                "ContextSourceDisallowed",
                "This source is outside the frozen request.",
            )
        })?;
    let stored_role: String = db.query_row(
        "SELECT role FROM documents WHERE id=?",
        [&descriptor.source.document_id],
        |row| row.get(0),
    )?;
    let expected_role = match descriptor.kind {
        SourceKind::AssistantDraft => DocumentRole::AssistantDraft,
        SourceKind::ConversationControl => DocumentRole::ConversationAnchor,
        _ => DocumentRole::Ordinary,
    };
    if stored_role != expected_role.storage_name() {
        return Err(CoreError::new(
            "DocumentRoleMismatch",
            "The frozen source does not match its persisted authority role.",
        ));
    }
    let revision = read_revision(db, &descriptor.source.revision_id)?;
    if revision.head.document_id != descriptor.source.document_id
        || revision.head.body_hash != descriptor.source.body_hash
    {
        return Err(CoreError::new(
            "InvalidContext",
            "The source revision does not match its frozen fingerprint.",
        ));
    }
    let exact = passages(&revision, handle, &descriptor.source)?;
    let mut statement = db.prepare("SELECT block_id,block_order,text,body_hash FROM passage_projections WHERE revision_id=? AND projection_version=1 ORDER BY block_order")?;
    let cached = statement
        .query_map([&revision.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // Revalidate the entire projection, not just a claimed hash. Corruption or
    // interrupted maintenance cannot suppress a match in the original prose.
    let valid = cached.len() == exact.len()
        && cached
            .iter()
            .zip(&exact)
            .all(|((id, order, text, hash), source)| {
                id == &source.block_id
                    && order == &source.block_order
                    && text == &source.text
                    && hash == &descriptor.source.body_hash
            });
    Ok(SourceRead {
        descriptor: descriptor.clone(),
        passages: exact,
        body: revision.body,
        used_validated_projection: valid,
    })
}

fn passages(
    revision: &Revision,
    handle: &str,
    source: &SourceRef,
) -> CoreResult<Vec<SourcePassage>> {
    let blocks = revision.body["body"]["content"]
        .as_array()
        .ok_or_else(|| CoreError::new("InvalidDocument", "The revision has no blocks."))?;
    Ok(blocks
        .iter()
        .enumerate()
        .map(|(order, block)| {
            let mut text = String::new();
            if let Some(content) = block["content"].as_array() {
                for node in content {
                    if node["type"] == "hardBreak" {
                        text.push('\n');
                    } else if let Some(value) = node["text"].as_str() {
                        text.push_str(value);
                    }
                }
            }
            SourcePassage {
                handle: handle.into(),
                source: source.clone(),
                block_id: block["attrs"]["id"].as_str().unwrap_or_default().into(),
                block_order: order as u32,
                text,
            }
        })
        .collect())
}

/// Search only the already-authorized frozen sources. Both manual inspection
/// and model lookups use this exact matcher and coverage contract.
pub(super) fn search_frozen(
    db: &Connection,
    frozen: &FrozenContext,
    query: &str,
    mode: SearchMode,
    limit: u32,
) -> CoreResult<SearchResult> {
    search_saved_passages(frozen, query, mode, limit, |handle| {
        Ok(read_source(db, frozen, handle)?.passages)
    })
}

/// Deterministic search over caller-validated frozen evidence. Packet

/// Transfer validation keeps historical namespaces intact while verifying all
/// immutable manifests and revision pins before installing an independent copy.
pub(crate) fn validate_context_storage(db: &Connection) -> CoreResult<()> {
    epochs(db)?;
    let schema: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    navigation_pin_table_available(db)?;
    let has_reader_position: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('snapshot_sources') WHERE name='reader_position')",
        [],
        |row| row.get(0),
    )?;
    if schema >= 15 && !has_reader_position {
        return Err(CoreError::new(
            "InvalidContext",
            "The project is missing immutable reviewed reader-position pins.",
        ));
    }
    let mut statement = db.prepare("SELECT id,project_id,operation_namespace,operation_id,payload_hash,context_source_epoch,disclosure_policy_epoch,manifest_json,manifest_hash FROM story_snapshots")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (id, project, namespace, operation, payload, source_epoch, policy_epoch, json, hash) =
            row?;
        for value in [&id, &project, &namespace, &operation] {
            check_id(value)?;
        }
        if payload.len() != 64 || !payload.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(CoreError::new(
                "InvalidContext",
                "Invalid context request fingerprint.",
            ));
        }
        let frozen = decode_snapshot(&json, &hash)?;
        if frozen.snapshot.snapshot_id != id
            || frozen.snapshot.project_id != project
            || frozen.snapshot.context_source_epoch != parse_stored_version(source_epoch)?
            || frozen.policy.version != parse_stored_version(policy_epoch)?
        {
            return Err(CoreError::new(
                "InvalidContext",
                "The context manifest does not match its stored identity or epochs.",
            ));
        }
        validate_pins(db, &frozen, &namespace)?;
    }
    Ok(())
}
