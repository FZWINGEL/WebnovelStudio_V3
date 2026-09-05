//! Frozen, project-owned evidence. Revisions remain the only text authority;
//! passage projections can be deleted without losing story material.
use super::*;
use crate::context::conversation::{FrozenConversation, validate_conversation};
use crate::context::guidance::{FrozenGuidance, validate_frozen_guidance};
use crate::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind, SourceRef,
    StorySnapshot, evaluate_sources,
};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextEpochs {
    pub source: String,
    pub policy: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenContext {
    pub snapshot: StorySnapshot,
    pub policy: InformationPolicy,
    pub purpose: ContextPurpose,
    pub aliases: BTreeMap<String, Vec<String>>,
    /// No titles or text from excluded material are exposed to a writing packet.
    pub excluded_source_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<FrozenGuidance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<FrozenConversation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePassage {
    pub handle: String,
    pub source: SourceRef,
    pub block_id: String,
    pub block_order: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRead {
    pub descriptor: SourceDescriptor,
    pub passages: Vec<SourcePassage>,
    pub body: Value,
    pub used_validated_projection: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Literal,
    Lexical,
    ExactAlias,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchStory {
    pub access: ProjectAccess,
    pub snapshot_id: String,
    pub query: String,
    pub mode: SearchMode,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHit {
    pub passage: SourcePassage,
    pub start_utf16: u32,
    pub end_utf16: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResult {
    pub snapshot_id: String,
    pub hits: Vec<SearchHit>,
    /// Alias/title matches identify a source, not an occurrence in its prose.
    pub source_matches: Vec<SourceDescriptor>,
    pub searched_sources: u32,
    pub has_more: bool,
    /// Retrieval reports where it looked, never that an event did not happen.
    pub coverage: String,
}

pub(super) enum ContextCommand {
    Epochs(ProjectAccess, Reply<ContextEpochs>),
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
        let documents = self.documents(access.clone())?;
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
        if request.policy.audience == Audience::AuthorRoom
            && matches!(
                request.purpose,
                ContextPurpose::Revise | ContextPurpose::Continue
            )
        {
            return Err(CoreError::new(
                "BoundaryConflict",
                "Prepare a separate writing request with an approved disclosure boundary.",
            ));
        }
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
        let mut hits = Vec::new();
        let mut source_matches = Vec::new();
        let normalized = query.to_lowercase();
        let mut has_more = false;
        for source in &frozen.snapshot.sources {
            let alias = source.display_name.to_lowercase() == normalized
                || frozen.aliases.get(&source.handle).is_some_and(|names| {
                    names.iter().any(|name| name.to_lowercase() == normalized)
                });
            if matches!(request.mode, SearchMode::ExactAlias) {
                if alias {
                    if source_matches.len() == request.limit as usize {
                        has_more = true;
                    } else {
                        source_matches.push(source.clone());
                    }
                }
                continue;
            }
            let read = read_source(self.db()?, &frozen, &source.handle)?;
            for passage in read.passages {
                let spans = match request.mode {
                    SearchMode::ExactAlias => {
                        unreachable!("alias matching returns source descriptors")
                    }
                    SearchMode::Literal => literal_spans(&passage.text, &normalized),
                    SearchMode::Lexical => {
                        let terms: Vec<_> = normalized.split_whitespace().collect();
                        if terms
                            .iter()
                            .all(|term| passage.text.to_lowercase().contains(term))
                        {
                            literal_spans(&passage.text, terms[0])
                        } else {
                            Vec::new()
                        }
                    }
                };
                for (start_utf16, end_utf16) in spans {
                    if hits.len() == request.limit as usize {
                        has_more = true;
                        break;
                    }
                    hits.push(SearchHit {
                        passage: passage.clone(),
                        start_utf16,
                        end_utf16,
                    });
                }
            }
        }
        Ok(SearchResult { snapshot_id: request.snapshot_id, hits, source_matches, searched_sources: frozen.snapshot.sources.len() as u32, has_more, coverage: "Exact eligible saved sources; a missing match does not establish that an event never happened.".into() })
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
            tx.execute("UPDATE documents SET projection_dirty=1", [])?;
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
    freeze_story_impl(tx, request, payload_hash, false, None)
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
        "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0",
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
    for item in &prefix {
        let revision = read_revision(tx, &item.revision_id)?;
        if revision.head != item.head {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier reviewed chapter no longer matches its selected revision.",
            ));
        }
        let position: i64 = tx.query_row(
            "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0",
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
        reviewed_members.push(ReviewedBasisMember {
            document_id: item.document_id.clone(),
            bundle_id: item.bundle_id.clone(),
            revision_id: item.revision_id.clone(),
            version: item.head.version.clone(),
            body_hash: item.head.body_hash.clone(),
        });
        reviewed_sources.push(SourceDescriptor {
            handle: format!("reviewed-{}", item.bundle_id),
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
    };
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
    freeze_story_impl(tx, request, payload_hash, true, retry_guidance)
}

fn freeze_story_impl(
    tx: &Connection,
    request: &FreezeStory,
    payload_hash: &str,
    include_request_guidance: bool,
    retry_guidance: Option<&[crate::context::guidance::FrozenGuidance]>,
) -> CoreResult<FrozenContext> {
    if request.basis != BasisKind::Working {
        return Err(CoreError::new(
            "BasisUnavailable",
            "Reviewed and explicit historical requests need their respective authority records.",
        ));
    }
    if request.policy.audience == Audience::AuthorRoom
        && matches!(
            request.purpose,
            ContextPurpose::Revise | ContextPurpose::Continue
        )
    {
        return Err(CoreError::new(
            "BoundaryConflict",
            "Prepare a separate writing request with an approved disclosure boundary.",
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
    let target_document = read_document(tx, &request.expected.document_id)?;
    require_head(&target_document.head, &request.expected)?;
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
        if request.policy.audience == Audience::AuthorRoom {
            aliases.insert(
                source.handle.clone(),
                read_aliases(tx, &source.source.document_id)?,
            );
        }
    }
    let frozen = FrozenContext {
        excluded_source_count: (total - snapshot.sources.len()) as u32,
        snapshot,
        policy: request.policy.clone(),
        purpose: request.purpose,
        aliases,
        guidance: if request.policy.audience == Audience::AuthorRoom {
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
    };
    let json = serde_json::to_string(&frozen)?;
    tx.execute(
        "INSERT INTO story_snapshots(id,project_id,operation_namespace,operation_id,payload_hash,context_source_epoch,disclosure_policy_epoch,manifest_json,manifest_hash) VALUES(?,?,?,?,?,?,?,?,?)",
        params![frozen.snapshot.snapshot_id, request.access.project_id, request.access.operation_namespace, request.operation_id, payload_hash, parse_version(&frozen.snapshot.context_source_epoch)?, parse_version(&frozen.policy.version)?, json, sha256_hex(json.as_bytes())],
    )?;
    insert_snapshot_sources(tx, &frozen)?;
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
        db.prepare("SELECT id,position FROM documents WHERE trashed=0 ORDER BY position,id")?;
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

fn eligibility_error(error: crate::context::EligibilityError) -> CoreError {
    CoreError::new("ContextSourceDisallowed", &error.to_string())
}

fn eligibility(
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
    handles: &[String],
) -> Result<crate::context::EligibilityReceipt, crate::context::EligibilityError> {
    evaluate_sources(snapshot, policy, purpose, handles)
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

pub(super) fn decode_snapshot(json: &str, hash: &str) -> CoreResult<FrozenContext> {
    if sha256_hex(json.as_bytes()) != hash {
        return Err(CoreError::new(
            "InvalidContext",
            "The context manifest failed its fingerprint check.",
        ));
    }
    let frozen: FrozenContext =
        serde_json::from_str(json).map_err(|e| CoreError::new("InvalidContext", &e.to_string()))?;
    validate_conversation(
        frozen.conversation.as_ref(),
        &frozen.snapshot.project_id,
        &frozen.snapshot.target.document_id,
        &frozen.policy.version,
        frozen.policy.audience,
        frozen.purpose,
    )
    .map_err(|message| CoreError::new("InvalidConversationContext", &message))?;
    if frozen.snapshot.ordering_epoch != frozen.snapshot.context_source_epoch
        || frozen.snapshot.disclosure_policy_version != frozen.policy.version
    {
        return Err(CoreError::new(
            "InvalidContext",
            "The frozen ordering or disclosure epoch is inconsistent.",
        ));
    }
    if frozen.policy.audience == Audience::RestrictedWriting && !frozen.aliases.is_empty() {
        return Err(CoreError::new(
            "InvalidContext",
            "Unclassified aliases cannot enter restricted writing context.",
        ));
    }
    validate_frozen_guidance(
        &frozen.guidance,
        &frozen.snapshot.project_id,
        &frozen.snapshot.target.document_id,
        frozen.policy.audience,
    )
    .map_err(|message| CoreError::new("InvalidContext", &message))?;
    let selected: Vec<_> = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| source.handle.clone())
        .collect();
    eligibility(&frozen.snapshot, &frozen.policy, frozen.purpose, &selected)
        .map_err(eligibility_error)?;
    if frozen
        .aliases
        .keys()
        .any(|handle| !selected.contains(handle))
    {
        return Err(CoreError::new(
            "InvalidContext",
            "An alias refers to a source outside the manifest.",
        ));
    }
    Ok(frozen)
}

fn validate_pins(
    db: &Connection,
    frozen: &FrozenContext,
    snapshot_namespace: &str,
) -> CoreResult<()> {
    if let Some(manifest) = frozen.snapshot.reviewed_basis.as_ref() {
        reviewed_story::validate_reviewed_snapshot_manifest(
            db,
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

/// Unicode lowercase can expand a character, so retain an original UTF-16 map
/// instead of treating normalized UTF-8 byte positions as editor offsets.
fn literal_spans(text: &str, query: &str) -> Vec<(u32, u32)> {
    let mut normalized = String::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    for character in text.chars() {
        let next = offset + character.len_utf16() as u32;
        let lowered: String = character.to_lowercase().collect();
        starts.extend(std::iter::repeat_n(offset, lowered.len()));
        ends.extend(std::iter::repeat_n(next, lowered.len()));
        normalized.push_str(&lowered);
        offset = next;
    }
    normalized
        .match_indices(query)
        .map(|(at, matched)| (starts[at], ends[at + matched.len() - 1]))
        .collect()
}

/// Transfer validation keeps historical namespaces intact while verifying all
/// immutable manifests and revision pins before installing an independent copy.
pub(crate) fn validate_context_storage(db: &Connection) -> CoreResult<()> {
    epochs(db)?;
    let schema: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
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
