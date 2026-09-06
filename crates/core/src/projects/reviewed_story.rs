//! Author-only reviewed prose basis.
//!
//! This slice records which exact chapter revisions an author has marked as
//! ready, with optional author-accepted summaries and passage-backed records.
//! Generated memory is never accepted implicitly. A ready head selects immutable bundles; the
//! bundles and stages remain historical evidence after the selection changes.

use super::*;
use crate::context::SourceRef;
use crate::context::{ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind};
use crate::projects::reviewed_summary::{
    SummaryChange, SummaryRevision, canonical_summary_json, summary_hash, validate_summary_binding,
};
use crate::projects::story_records::{
    PossessionRecord, PromiseRecord, canonical_promises_json, canonical_records_json,
    validate_promises, validate_records,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_REVIEW_CHAPTERS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageAuthorReview {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkReady {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub stage_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewPrefixItem {
    pub document_id: String,
    pub title: String,
    pub bundle_id: String,
    pub revision_id: String,
    pub head: Head,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStage {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub target: Head,
    pub revision: Revision,
    pub previous_bundle_id: Option<String>,
    pub prefix: Vec<ReviewPrefixItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub source_epoch: String,
    pub policy_epoch: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyBundle {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub stage_id: String,
    pub target: Head,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedRecordSet {
    pub bundle_id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub target: Head,
    pub revision: Revision,
    pub records: Vec<PossessionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStatus {
    pub document_id: String,
    pub title: String,
    pub head: Head,
    pub state: ReviewState,
    pub active_bundle_id: Option<String>,
    pub pending_stage_id: Option<String>,
    pub reason: Option<String>,
    pub can_stage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    NoReview,
    Ready,
    ChangedProse,
    EarlierBasisChanged,
    ReviewNeeded,
}

pub(super) enum ReviewCommand {
    Status(ProjectAccess, String, Reply<ReviewStatus>),
    ReadStage(ProjectAccess, String, Reply<ReviewStage>),
    Stage(StageAuthorReview, Reply<ReviewStage>),
    Mark(MarkReady, Reply<ReadyBundle>),
    ReadRecords(ProjectAccess, String, Reply<Option<ReviewedRecordSet>>),
}

#[derive(Debug, Clone)]
struct StageRow {
    id: String,
    project_id: String,
    operation_namespace: String,
    document_id: String,
    target: Head,
    revision_id: String,
    source_epoch: i64,
    policy_epoch: i64,
    previous_bundle_id: Option<String>,
    prefix: Vec<ReviewPrefixItem>,
    prefix_hash: String,
    records: Option<Vec<PossessionRecord>>,
    records_hash: Option<String>,
    promises: Option<Vec<PromiseRecord>>,
    promises_hash: Option<String>,
    summary: Option<SummaryRevision>,
    summary_hash: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct BundleRow {
    id: String,
    project_id: String,
    operation_namespace: String,
    stage_id: String,
    document_id: String,
    target: Head,
    revision_id: String,
    policy_epoch: i64,
    prefix: Vec<ReviewPrefixItem>,
    coverage: String,
    records: Option<Vec<PossessionRecord>>,
    records_hash: Option<String>,
    promises: Option<Vec<PromiseRecord>>,
    promises_hash: Option<String>,
    summary: Option<SummaryRevision>,
    summary_hash: Option<String>,
    created_at: String,
}

/// Read-scoped cache for immutable reviewed provenance.
///
/// A frozen snapshot can contain a growing prefix of reviewed bundles.  The
/// validators intentionally compare every exact persisted field, but they do
/// not need to query and decode the same immutable row once for every later
/// prefix.  This cache is created for one database read only; it never
/// consults mutable ready-head selections and is never shared between reads.
pub(super) struct ReviewValidationContext<'db> {
    db: &'db Connection,
    bundles: HashMap<String, Option<BundleRow>>,
    revisions: HashMap<String, Revision>,
}

impl<'db> ReviewValidationContext<'db> {
    pub(super) fn new(db: &'db Connection) -> Self {
        Self {
            db,
            bundles: HashMap::new(),
            revisions: HashMap::new(),
        }
    }

    fn read_bundle(&mut self, bundle_id: &str) -> CoreResult<Option<&BundleRow>> {
        if !self.bundles.contains_key(bundle_id) {
            let bundle = self::read_bundle(self.db, bundle_id)?;
            self.bundles.insert(bundle_id.to_owned(), bundle);
        }
        Ok(self.bundles.get(bundle_id).and_then(Option::as_ref))
    }

    fn read_revision(&mut self, revision_id: &str) -> CoreResult<&Revision> {
        if !self.revisions.contains_key(revision_id) {
            let revision = self::read_revision(self.db, revision_id)?;
            self.revisions.insert(revision_id.to_owned(), revision);
        }
        Ok(self
            .revisions
            .get(revision_id)
            .expect("revision inserted into validation cache"))
    }

    pub(super) fn validate_reviewed_records(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        bundle_id: &str,
        source: &SourceRef,
        records_hash: &str,
        records: &[PossessionRecord],
    ) -> CoreResult<()> {
        check_id(project_id)?;
        check_id(operation_namespace)?;
        check_id(bundle_id)?;
        if source.project_id != project_id {
            return Err(CoreError::new(
                "InvalidReviewedRecords",
                "Reviewed evidence source belongs to another project.",
            ));
        }
        let revision_id = {
            let bundle = self.read_bundle(bundle_id)?.ok_or_else(|| {
                CoreError::new(
                    "ReviewBundleNotFound",
                    "The reviewed evidence bundle is unavailable.",
                )
            })?;
            if bundle.project_id != project_id
                || bundle.operation_namespace != operation_namespace
                || bundle.target.document_id != source.document_id
                || bundle.target.body_hash != source.body_hash
                || bundle.revision_id != source.revision_id
                || bundle.coverage != "authorOnly"
            {
                return Err(CoreError::new(
                    "InvalidReviewedRecords",
                    "The reviewed evidence bundle does not match its immutable source.",
                ));
            }
            bundle.revision_id.clone()
        };
        let revision = self.read_revision(&revision_id)?;
        let computed = validate_records(records, revision).map_err(|error| {
            CoreError::new(
                "InvalidReviewedRecords",
                &format!("The frozen reviewed evidence is invalid: {}", error.detail),
            )
        })?;
        {
            let bundle = self
                .read_bundle(bundle_id)?
                .expect("bundle inserted into validation cache");
            if computed.as_deref().unwrap_or("") != records_hash
                || bundle.records_hash.as_deref().unwrap_or("") != records_hash
                || bundle.records.as_deref().unwrap_or(&[]) != records
            {
                return Err(CoreError::new(
                    "InvalidReviewedRecords",
                    "The frozen reviewed evidence does not match its immutable bundle.",
                ));
            }
        }
        self.validate_prefix_evidence(
            project_id,
            operation_namespace,
            &source.document_id,
            bundle_id,
        )?;
        Ok(())
    }

    pub(super) fn validate_reviewed_promises(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        bundle_id: &str,
        source: &SourceRef,
        promises_hash: &str,
        promises: &[PromiseRecord],
    ) -> CoreResult<()> {
        check_id(project_id)?;
        check_id(operation_namespace)?;
        check_id(bundle_id)?;
        if source.project_id != project_id {
            return Err(CoreError::new(
                "InvalidReviewedPromises",
                "Reviewed promise source belongs to another project.",
            ));
        }
        let revision_id = {
            let bundle = self.read_bundle(bundle_id)?.ok_or_else(|| {
                CoreError::new(
                    "ReviewBundleNotFound",
                    "The reviewed promise bundle is unavailable.",
                )
            })?;
            if bundle.project_id != project_id
                || bundle.operation_namespace != operation_namespace
                || bundle.target.document_id != source.document_id
                || bundle.target.body_hash != source.body_hash
                || bundle.revision_id != source.revision_id
                || bundle.coverage != "authorOnly"
            {
                return Err(CoreError::new(
                    "InvalidReviewedPromises",
                    "The reviewed promise bundle does not match its immutable source.",
                ));
            }
            bundle.revision_id.clone()
        };
        let revision = self.read_revision(&revision_id)?;
        let computed = validate_promises(promises, revision).map_err(|error| {
            CoreError::new(
                "InvalidReviewedPromises",
                &format!("The frozen reviewed promises are invalid: {}", error.detail),
            )
        })?;
        {
            let bundle = self
                .read_bundle(bundle_id)?
                .expect("bundle inserted into validation cache");
            if computed.as_deref().unwrap_or("") != promises_hash
                || bundle.promises_hash.as_deref().unwrap_or("") != promises_hash
                || bundle.promises.as_deref().unwrap_or(&[]) != promises
            {
                return Err(CoreError::new(
                    "InvalidReviewedPromises",
                    "The frozen reviewed promises do not match their immutable bundle.",
                ));
            }
        }
        self.validate_prefix_evidence(
            project_id,
            operation_namespace,
            &source.document_id,
            bundle_id,
        )?;
        Ok(())
    }

    pub(super) fn validate_reviewed_summary(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        bundle_id: &str,
        expected_summary_hash: &str,
        summary: &SummaryRevision,
    ) -> CoreResult<()> {
        check_id(project_id)?;
        check_id(operation_namespace)?;
        check_id(bundle_id)?;
        if !valid_hash(expected_summary_hash) {
            return Err(CoreError::new(
                "InvalidReviewedSummary",
                "The reviewed summary fingerprint is invalid.",
            ));
        }
        let (revision_id, prefix, target) = {
            let bundle = self.read_bundle(bundle_id)?.ok_or_else(|| {
                CoreError::new(
                    "ReviewBundleNotFound",
                    "The reviewed summary bundle is unavailable.",
                )
            })?;
            if bundle.project_id != project_id
                || bundle.operation_namespace != operation_namespace
                || bundle.coverage != "authorOnly"
            {
                return Err(CoreError::new(
                    "InvalidReviewedSummary",
                    "The reviewed summary bundle does not match its immutable source.",
                ));
            }
            (
                bundle.revision_id.clone(),
                bundle.prefix.clone(),
                bundle.target.clone(),
            )
        };
        let expected_source = SourceRef {
            project_id: project_id.to_owned(),
            document_id: target.document_id,
            revision_id,
            body_hash: target.body_hash,
        };
        validate_summary_binding(summary, project_id, &expected_source, &prefix)?;
        if summary_hash(summary)? != expected_summary_hash {
            return Err(CoreError::new(
                "InvalidReviewedSummary",
                "The reviewed summary fingerprint does not match its canonical payload.",
            ));
        }
        {
            let bundle = self
                .read_bundle(bundle_id)?
                .expect("bundle inserted into validation cache");
            if bundle.summary_hash.as_deref() != Some(expected_summary_hash)
                || bundle.summary.as_ref() != Some(summary)
            {
                return Err(CoreError::new(
                    "InvalidReviewedSummary",
                    "The reviewed summary does not match its immutable bundle.",
                ));
            }
        }
        self.validate_prefix_evidence(
            project_id,
            operation_namespace,
            &expected_source.document_id,
            bundle_id,
        )?;
        Ok(())
    }

    pub(super) fn validate_reviewed_snapshot_manifest(
        &mut self,
        snapshot_project_id: &str,
        snapshot_namespace: &str,
        snapshot_policy_epoch: &str,
        manifest: &ReviewedBasisManifest,
        sources: &[SourceDescriptor],
    ) -> CoreResult<()> {
        if manifest.project_id != snapshot_project_id
            || manifest.operation_namespace != snapshot_namespace
            || manifest.operation_namespace.is_empty()
            || manifest.prefix.is_empty()
            || manifest.prefix.len() > MAX_REVIEW_CHAPTERS
        {
            return Err(CoreError::new(
                "InvalidContext",
                "The reviewed snapshot has an invalid authority manifest.",
            ));
        }
        check_id(&manifest.project_id)?;
        check_id(&manifest.operation_namespace)?;
        let policy_epoch = parse_version(snapshot_policy_epoch)?;
        let mut documents = HashSet::new();
        let mut bundles = HashSet::new();
        let mut revisions = HashSet::new();
        for (index, member) in manifest.prefix.iter().enumerate() {
            check_id(&member.document_id)?;
            check_id(&member.bundle_id)?;
            check_id(&member.revision_id)?;
            parse_version(&member.version)?;
            if !valid_hash(&member.body_hash)
                || !documents.insert(&member.document_id)
                || !bundles.insert(&member.bundle_id)
                || !revisions.insert(&member.revision_id)
            {
                return Err(CoreError::new(
                    "InvalidContext",
                    "The reviewed snapshot authority manifest is not canonical.",
                ));
            }
            let revision_id = {
                let bundle = self.read_bundle(&member.bundle_id)?.ok_or_else(|| {
                    CoreError::new(
                        "InvalidContext",
                        "The reviewed snapshot references a missing immutable bundle.",
                    )
                })?;
                if bundle.project_id != manifest.project_id
                    || bundle.operation_namespace != manifest.operation_namespace
                    || bundle.policy_epoch != policy_epoch
                    || bundle.document_id != member.document_id
                    || bundle.revision_id != member.revision_id
                    || bundle.target.version != member.version
                    || bundle.target.body_hash != member.body_hash
                    || !same_manifest_prefix(&bundle.prefix, &manifest.prefix[..index])
                {
                    return Err(CoreError::new(
                        "InvalidContext",
                        "The reviewed snapshot bundle does not match its exact source.",
                    ));
                }
                bundle.revision_id.clone()
            };
            let revision = self.read_revision(&revision_id)?;
            if revision.head.document_id != member.document_id
                || revision.head.version != member.version
                || revision.head.body_hash != member.body_hash
            {
                return Err(CoreError::new(
                    "InvalidContext",
                    "The reviewed snapshot revision does not match its authority manifest.",
                ));
            }
            let matches_source = sources.iter().any(|source| {
                source.kind == SourceKind::ReviewedAuthority
                    && source.current
                    && source.source.project_id == manifest.project_id
                    && source.source.document_id == member.document_id
                    && source.source.revision_id == member.revision_id
                    && source.source.body_hash == member.body_hash
            });
            if !matches_source {
                return Err(CoreError::new(
                    "InvalidContext",
                    "The reviewed snapshot source manifest omits an authority member.",
                ));
            }
        }
        let authority_count = sources
            .iter()
            .filter(|source| source.kind == SourceKind::ReviewedAuthority)
            .count();
        if authority_count != manifest.prefix.len() {
            return Err(CoreError::new(
                "InvalidContext",
                "The reviewed snapshot has unbound authority sources.",
            ));
        }
        Ok(())
    }

    fn validate_prefix_evidence(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        owner_document_id: &str,
        bundle_id: &str,
    ) -> CoreResult<()> {
        let prefix_len = self
            .read_bundle(bundle_id)?
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A review prefix points to a missing bundle.",
                )
            })?
            .prefix
            .len();
        for index in 0..prefix_len {
            let item = self
                .bundles
                .get(bundle_id)
                .and_then(Option::as_ref)
                .expect("bundle inserted into validation cache")
                .prefix[index]
                .clone();
            self.read_bundle(&item.bundle_id)?;
            let prefix_owner = self
                .bundles
                .get(bundle_id)
                .and_then(Option::as_ref)
                .expect("bundle inserted into validation cache");
            let bundle = self.bundles.get(&item.bundle_id).and_then(Option::as_ref);
            self.validate_prefix_item(
                project_id,
                operation_namespace,
                owner_document_id,
                &item,
                bundle,
                &prefix_owner.prefix[..index],
            )?;
        }
        Ok(())
    }

    fn validate_prefix_evidence_from_prefix(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        owner_document_id: &str,
        prefix: &[ReviewPrefixItem],
    ) -> CoreResult<()> {
        for (index, item) in prefix.iter().enumerate() {
            self.read_bundle(&item.bundle_id)?;
            let bundle = self.bundles.get(&item.bundle_id).and_then(Option::as_ref);
            self.validate_prefix_item(
                project_id,
                operation_namespace,
                owner_document_id,
                item,
                bundle,
                &prefix[..index],
            )?;
        }
        Ok(())
    }

    fn validate_prefix_item(
        &self,
        project_id: &str,
        operation_namespace: &str,
        owner_document_id: &str,
        item: &ReviewPrefixItem,
        bundle: Option<&BundleRow>,
        expected_prefix: &[ReviewPrefixItem],
    ) -> CoreResult<()> {
        let bundle = bundle.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A review prefix points to a missing bundle.",
            )
        })?;
        if bundle.project_id != project_id
            || bundle.operation_namespace != operation_namespace
            || bundle.coverage != "authorOnly"
            || bundle.document_id != item.document_id
            || bundle.document_id == owner_document_id
            || bundle.revision_id != item.revision_id
            || bundle.target != item.head
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review prefix points to a foreign or mismatched bundle.",
            ));
        }
        if !same_prefix_basis(&bundle.prefix, expected_prefix) {
            return Err(CoreError::new(
                "InvalidProject",
                "A review prefix has inconsistent earlier ancestry.",
            ));
        }
        Ok(())
    }
}

type StageDbRow = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    i64,
    i64,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);
type BundleDbRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    i64,
    i64,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

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
    /// Resolve the current author-reviewed revision for a chapter export.
    ///
    /// The selected head and current review status are authoritative here;
    /// historical bundle validation alone is intentionally insufficient for a
    /// new export.  The returned revision is the exact immutable checkpoint
    /// recorded by the ready bundle.
    pub(super) fn resolve_reviewed_export_source(
        &self,
        access: &ProjectAccess,
        expected: &Head,
    ) -> CoreResult<(String, Revision)> {
        self.check_access(access)?;
        check_id(&expected.document_id)?;
        parse_version(&expected.version)?;

        let db = self.db()?;
        let document = read_document(db, &expected.document_id)?;
        if document.kind != "chapter" {
            return Err(CoreError::new(
                "InvalidDocument",
                "Reviewed export applies to chapter documents.",
            ));
        }
        require_head(&document.head, expected)?;

        let status = self.chapter_review_status(access.clone(), expected.document_id.as_str())?;
        if status.state == ReviewState::NoReview {
            return Err(CoreError::new(
                "ReviewRequired",
                "Mark this chapter reviewed before exporting the reviewed snapshot.",
            ));
        }
        if status.state != ReviewState::Ready {
            return Err(CoreError::new(
                "ReviewStale",
                "The reviewed chapter is no longer current; review it again before exporting.",
            ));
        }
        let bundle_id = status.active_bundle_id.ok_or_else(|| {
            CoreError::new(
                "ReviewRequired",
                "Mark this chapter reviewed before exporting the reviewed snapshot.",
            )
        })?;
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "ReviewSourceMismatch",
                "The current reviewed bundle is unavailable.",
            )
        })?;
        validate_reviewed_export_source(
            db,
            &access.project_id,
            &access.operation_namespace,
            &bundle_id,
            expected,
            &bundle.revision_id,
        )?;
        let revision = read_revision(db, &bundle.revision_id)?;
        Ok((bundle_id, revision))
    }

    pub(super) fn handle_review(&mut self, command: ReviewCommand) {
        match command {
            ReviewCommand::Status(access, document_id, reply) => {
                let _ = reply.send(self.chapter_review_status(access, &document_id));
            }
            ReviewCommand::ReadStage(access, stage_id, reply) => {
                let _ = reply.send(self.read_review_stage(access, &stage_id));
            }
            ReviewCommand::Stage(request, reply) => {
                let result = self.stage_author_review(request);
                self.fence_uncertain(&result);
                let _ = reply.send(result);
            }
            ReviewCommand::Mark(request, reply) => {
                let result = self.mark_ready(request);
                self.fence_uncertain(&result);
                let _ = reply.send(result);
            }
            ReviewCommand::ReadRecords(access, document_id, reply) => {
                let _ = reply.send(self.read_reviewed_record_set_internal(access, &document_id));
            }
        }
    }

    fn read_reviewed_record_set_internal(
        &self,
        access: ProjectAccess,
        document_id: &str,
    ) -> CoreResult<Option<ReviewedRecordSet>> {
        self.check_access(&access)?;
        check_id(document_id)?;
        let db = self.db()?;
        let document = read_document(db, document_id)?;
        if document.kind != "chapter" {
            return Err(CoreError::new(
                "InvalidDocument",
                "Reviewed evidence applies to chapter documents.",
            ));
        }
        let Some(bundle_id) = active_bundle_id(db, &access, document_id)? else {
            return Ok(None);
        };
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new("InvalidProject", "The selected reviewed bundle is missing.")
        })?;
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != document_id
            || bundle.coverage != "authorOnly"
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The selected reviewed bundle crosses project identity.",
            ));
        }
        let revision = read_revision(db, &bundle.revision_id)?;
        let status = self.chapter_review_status(access, document_id)?;
        Ok(Some(ReviewedRecordSet {
            bundle_id,
            project_id: bundle.project_id,
            operation_namespace: bundle.operation_namespace,
            target: bundle.target,
            revision,
            records: bundle.records.unwrap_or_default(),
            records_hash: bundle.records_hash,
            promises: bundle.promises,
            promises_hash: bundle.promises_hash,
            summary: bundle.summary,
            summary_hash: bundle.summary_hash,
            current: status.state == ReviewState::Ready,
        }))
    }

    fn chapter_review_status(
        &self,
        access: ProjectAccess,
        document_id: &str,
    ) -> CoreResult<ReviewStatus> {
        self.check_access(&access)?;
        check_id(document_id)?;
        let db = self.db()?;
        let document = read_document(db, document_id)?;
        if document.kind != "chapter" {
            return Err(CoreError::new(
                "InvalidDocument",
                "Author review applies to chapter documents.",
            ));
        }
        let pending_stage_id = pending_stage_id(db, &access, document_id)?;
        let current_policy = current_epochs(db)?.1;
        let active_id = active_bundle_id(db, &access, document_id)?;
        let Some(active_id) = active_id.clone() else {
            let (can_stage, reason) = stage_capability(db, &access, document_id, current_policy)?;
            return Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::NoReview,
                active_bundle_id: None,
                pending_stage_id,
                reason,
                can_stage,
            });
        };
        let Some(bundle) = read_bundle(db, &active_id)? else {
            let (can_stage, _) = stage_capability(db, &access, document_id, current_policy)?;
            return Ok(status_needs_review(
                &document,
                active_id,
                "The selected reviewed bundle is missing.",
                can_stage,
                pending_stage_id.clone(),
            ));
        };
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != document_id
        {
            let (can_stage, _) = stage_capability(db, &access, document_id, current_policy)?;
            return Ok(status_needs_review(
                &document,
                active_id,
                "The selected reviewed bundle belongs to another project namespace.",
                can_stage,
                pending_stage_id.clone(),
            ));
        }
        let (can_stage, _stage_reason) =
            stage_capability(db, &access, document_id, current_policy)?;
        if bundle.target != document.head {
            return Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::ChangedProse,
                active_bundle_id: Some(active_id),
                pending_stage_id: pending_stage_id.clone(),
                reason: Some("The chapter changed after it was marked reviewed.".into()),
                can_stage,
            });
        }
        if bundle.policy_epoch != current_policy {
            return Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::ReviewNeeded,
                active_bundle_id: Some(active_id),
                pending_stage_id: pending_stage_id.clone(),
                reason: Some("The disclosure policy changed; review this chapter again.".into()),
                can_stage,
            });
        }
        match selected_prefix(db, &access, document_id, current_policy) {
            Ok(prefix) if same_prefix_basis(&prefix, &bundle.prefix) => Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::Ready,
                active_bundle_id: Some(active_id),
                pending_stage_id: None,
                reason: None,
                can_stage,
            }),
            Ok(_) => Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::EarlierBasisChanged,
                active_bundle_id: Some(active_id),
                pending_stage_id: pending_stage_id.clone(),
                reason: Some("An earlier reviewed chapter changed or needs review.".into()),
                can_stage,
            }),
            Err(error) if error.code == "ReviewBasisUnavailable" => Ok(ReviewStatus {
                document_id: document_id.to_owned(),
                title: document.title,
                head: document.head,
                state: ReviewState::EarlierBasisChanged,
                active_bundle_id: Some(active_id),
                pending_stage_id,
                reason: Some(error.detail),
                can_stage: false,
            }),
            Err(error) => Err(error),
        }
    }

    fn read_review_stage(&self, access: ProjectAccess, stage_id: &str) -> CoreResult<ReviewStage> {
        self.check_access(&access)?;
        check_id(stage_id)?;
        let db = self.db()?;
        let stage = read_stage(db, &access, stage_id)?.ok_or_else(|| {
            CoreError::new("ReviewStageNotFound", "This review stage is not available.")
        })?;
        stage_to_dto(db, stage)
    }

    fn stage_author_review(&mut self, request: StageAuthorReview) -> CoreResult<ReviewStage> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        check_id(&request.expected.document_id)?;
        parse_version(&request.expected.version)?;
        let payload_hash = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = existing_stage(&tx, &request.access, &request.operation_id)? {
            if existing.0 != payload_hash {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This review operation was already used for another request.",
                ));
            }
            let stage = read_stage(&tx, &request.access, &existing.1)?.ok_or_else(|| {
                CoreError::new("InvalidProject", "The saved review stage is missing.")
            })?;
            let result = stage_to_dto(&tx, stage)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(result);
        }
        let document = read_document(&tx, &request.expected.document_id)?;
        if document.kind != "chapter" {
            return Err(CoreError::new(
                "InvalidDocument",
                "Author review applies to chapter documents.",
            ));
        }
        require_head(&document.head, &request.expected)?;
        let revision = checkpoint_at(&tx, &document, "authorReview")?;
        let (source_epoch, policy_epoch) = current_epochs(&tx)?;
        let prefix = selected_prefix(
            &tx,
            &request.access,
            &document.head.document_id,
            policy_epoch,
        )?;
        let previous_bundle_id =
            active_bundle_id(&tx, &request.access, &document.head.document_id)?;
        let records = match request.records {
            Some(records) => Some(records),
            None => match previous_bundle_id.as_deref() {
                Some(id) => read_bundle(&tx, id)?.and_then(|bundle| bundle.records),
                None => None,
            },
        };
        let records_hash = validate_records(&records.clone().unwrap_or_default(), &revision)
            .map_err(|error| {
                CoreError::new(
                    "InvalidReviewedRecords",
                    &format!("The reviewed evidence is invalid: {}", error.detail),
                )
            })?;
        let records_json = canonical_records_json(&records.clone().unwrap_or_default())?;
        let promises = match request.promises {
            Some(promises) => Some(promises),
            None => match previous_bundle_id.as_deref() {
                Some(id) => read_bundle(&tx, id)?.and_then(|bundle| bundle.promises),
                None => None,
            },
        };
        let promises_hash = validate_promises(&promises.clone().unwrap_or_default(), &revision)
            .map_err(|error| {
                CoreError::new(
                    "InvalidReviewedPromises",
                    &format!("The reviewed promises are invalid: {}", error.detail),
                )
            })?;
        let promises_json = canonical_promises_json(&promises.clone().unwrap_or_default())?;
        let summary = resolve_summary(
            &tx,
            &request.access,
            &document.head,
            &revision,
            &prefix,
            previous_bundle_id.as_deref(),
            request.summary.as_ref(),
        )?;
        let summary_json = summary.as_ref().map(canonical_summary_json).transpose()?;
        let summary_hash = summary.as_ref().map(summary_hash).transpose()?;
        let prefix_hash = hash_prefix(&prefix)?;
        let stage_id = new_id();
        tx.execute(
            "INSERT INTO review_stages(id,project_id,operation_namespace,operation_id,payload_hash,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash)
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                stage_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.operation_id,
                payload_hash,
                document.head.document_id,
                parse_version(&document.head.version)?,
                document.head.body_hash,
                revision.id,
                source_epoch,
                policy_epoch,
                previous_bundle_id,
                serde_json::to_string(&prefix)?,
                prefix_hash,
                records_json,
                records_hash,
                promises_json,
                promises_hash,
                summary_json,
                summary_hash,
            ],
        )?;
        let stage = read_stage(&tx, &request.access, &stage_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "The review stage was not readable after insert.",
            )
        })?;
        let result = stage_to_dto(&tx, stage)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    fn mark_ready(&mut self, request: MarkReady) -> CoreResult<ReadyBundle> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        check_id(&request.stage_id)?;
        let payload_hash = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = existing_bundle(&tx, &request.access, &request.operation_id)? {
            if existing.0 != payload_hash {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This ready operation was already used for another request.",
                ));
            }
            let bundle = read_bundle(&tx, &existing.1)?.ok_or_else(|| {
                CoreError::new("InvalidProject", "The saved ready bundle is missing.")
            })?;
            let result = bundle_to_dto(bundle);
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(result);
        }
        let stage = read_stage(&tx, &request.access, &request.stage_id)?.ok_or_else(|| {
            CoreError::new("ReviewStageNotFound", "This review stage is not available.")
        })?;
        let (source_epoch, policy_epoch) = current_epochs(&tx)?;
        if stage.source_epoch != source_epoch || stage.policy_epoch != policy_epoch {
            return Err(CoreError::new(
                "ReviewStageStale",
                "The story changed while this review was staged. Prepare it again.",
            ));
        }
        let document = read_document(&tx, &stage.document_id)?;
        require_head(&document.head, &stage.target)?;
        if document.last_checkpoint_id.as_deref() != Some(stage.revision_id.as_str()) {
            return Err(CoreError::new(
                "ReviewStageStale",
                "The staged revision is no longer the current saved revision.",
            ));
        }
        let stage_revision = read_revision(&tx, &stage.revision_id)?;
        validate_records(&stage.records.clone().unwrap_or_default(), &stage_revision).map_err(
            |error| {
                CoreError::new(
                    "InvalidReviewedRecords",
                    &format!("The staged reviewed evidence is invalid: {}", error.detail),
                )
            },
        )?;
        validate_promises(&stage.promises.clone().unwrap_or_default(), &stage_revision).map_err(
            |error| {
                CoreError::new(
                    "InvalidReviewedPromises",
                    &format!("The staged reviewed promises are invalid: {}", error.detail),
                )
            },
        )?;
        let prefix = selected_prefix(&tx, &request.access, &stage.document_id, policy_epoch)?;
        if !same_prefix_basis(&prefix, &stage.prefix) {
            return Err(CoreError::new(
                "ReviewStageStale",
                "An earlier reviewed chapter changed while this review was staged.",
            ));
        }
        let current_previous = active_bundle_id(&tx, &request.access, &stage.document_id)?;
        if current_previous != stage.previous_bundle_id {
            return Err(CoreError::new(
                "ReviewStageStale",
                "The selected reviewed head changed while this review was staged.",
            ));
        }
        let bundle_id = new_id();
        let prefix_json = serde_json::to_string(&stage.prefix)?;
        let summary_json = stage
            .summary
            .as_ref()
            .map(canonical_summary_json)
            .transpose()?;
        tx.execute(
            "INSERT INTO ready_bundles(id,project_id,operation_namespace,operation_id,payload_hash,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash)
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'authorOnly',?,?,?,?,?,?)",
            params![
                bundle_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.operation_id,
                payload_hash,
                stage.id,
                stage.document_id,
                parse_version(&stage.target.version)?,
                stage.target.body_hash,
                stage.revision_id,
                stage.source_epoch,
                stage.policy_epoch,
                stage.previous_bundle_id,
                prefix_json,
                stage.prefix_hash,
                canonical_records_json(&stage.records.clone().unwrap_or_default())?,
                stage.records_hash,
                canonical_promises_json(&stage.promises.clone().unwrap_or_default())?,
                stage.promises_hash,
                summary_json,
                stage.summary_hash,
            ],
        )?;
        let target_position: i64 = tx.query_row(
            "SELECT position FROM documents WHERE id=? AND trashed=0",
            [&stage.document_id],
            |row| row.get(0),
        )?;
        let mut later = tx.prepare(
            "SELECT h.bundle_id FROM ready_heads h JOIN documents d ON d.id=h.document_id
             WHERE h.project_id=? AND h.operation_namespace=? AND d.kind='chapter' AND d.trashed=0
             AND (d.position>? OR (d.position=? AND d.id>?)) ORDER BY d.position,d.id LIMIT ?",
        )?;
        let later_ids = later
            .query_map(
                params![
                    &request.access.project_id,
                    &request.access.operation_namespace,
                    target_position,
                    target_position,
                    &stage.document_id,
                    (MAX_REVIEW_CHAPTERS + 1) as i64,
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if later_ids.len() > MAX_REVIEW_CHAPTERS {
            return Err(CoreError::new(
                "ReviewLimitExceeded",
                "There are too many later reviewed chapters to fence safely.",
            ));
        }
        drop(later);
        for affected_bundle_id in later_ids {
            tx.execute(
                "INSERT INTO review_fences(id,project_id,operation_namespace,affected_bundle_id,changed_document_id,changed_version,changed_body_hash,superseding_bundle_id,reason)
                 VALUES(?,?,?,?,?,?,?,?,?)",
                params![
                    new_id(),
                    &request.access.project_id,
                    &request.access.operation_namespace,
                    affected_bundle_id,
                    &stage.document_id,
                    parse_version(&stage.target.version)?,
                    &stage.target.body_hash,
                    &bundle_id,
                    "An earlier chapter was superseded; reaffirm the later chapter.",
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO ready_heads(project_id,operation_namespace,document_id,bundle_id)
             VALUES(?,?,?,?)
             ON CONFLICT(project_id,operation_namespace,document_id) DO UPDATE SET bundle_id=excluded.bundle_id",
            params![
                &request.access.project_id,
                &request.access.operation_namespace,
                &stage.document_id,
                &bundle_id,
            ],
        )?;
        // Selecting a new authority bundle is a source change for future
        // stages. Existing bundles do not become stale merely because this
        // project epoch advances; their exact target and prefix decide that.
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        let result = read_bundle(&tx, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "The ready bundle was not readable after insert.",
            )
        })?;
        let result = bundle_to_dto(result);
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }
}

fn current_epochs(db: &Connection) -> CoreResult<(i64, i64)> {
    db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(CoreError::from)
}

fn active_bundle_id(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<String>> {
    db.query_row(
        "SELECT bundle_id FROM ready_heads WHERE project_id=? AND operation_namespace=? AND document_id=?",
        params![access.project_id, access.operation_namespace, document_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(CoreError::from)
}

fn existing_stage(
    db: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<Option<(String, String)>> {
    db.query_row(
        "SELECT payload_hash,id FROM review_stages WHERE project_id=? AND operation_namespace=? AND operation_id=?",
        params![access.project_id, access.operation_namespace, operation_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

fn existing_bundle(
    db: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<Option<(String, String)>> {
    db.query_row(
        "SELECT payload_hash,id FROM ready_bundles WHERE project_id=? AND operation_namespace=? AND operation_id=?",
        params![access.project_id, access.operation_namespace, operation_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

fn parse_prefix(json: &str, expected_hash: &str) -> CoreResult<Vec<ReviewPrefixItem>> {
    if json.len() > 4 * 1024 * 1024 {
        return Err(CoreError::new(
            "InvalidProject",
            "The review prefix is too large.",
        ));
    }
    let prefix: Vec<ReviewPrefixItem> = serde_json::from_str(json)
        .map_err(|_| CoreError::new("InvalidProject", "The saved review prefix is invalid."))?;
    validate_prefix(&prefix)?;
    if hash_prefix(&prefix)? != expected_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved review prefix fingerprint is invalid.",
        ));
    }
    Ok(prefix)
}

fn validate_prefix(prefix: &[ReviewPrefixItem]) -> CoreResult<()> {
    if prefix.len() > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "ReviewLimitExceeded",
            "The reviewed chapter prefix is too large.",
        ));
    }
    let mut ids = HashSet::new();
    for item in prefix {
        check_id(&item.document_id)?;
        check_id(&item.bundle_id)?;
        check_id(&item.revision_id)?;
        validate_title(&item.title)?;
        parse_version(&item.head.version)?;
        if item.head.document_id != item.document_id || !ids.insert(&item.document_id) {
            return Err(CoreError::new(
                "InvalidProject",
                "The reviewed chapter prefix is not canonical.",
            ));
        }
    }
    Ok(())
}

fn hash_prefix(prefix: &[ReviewPrefixItem]) -> CoreResult<String> {
    Ok(sha256_hex(serde_json::to_string(prefix)?.as_bytes()))
}

fn resolve_summary(
    db: &Connection,
    access: &ProjectAccess,
    target: &Head,
    revision: &Revision,
    prefix: &[ReviewPrefixItem],
    previous_bundle_id: Option<&str>,
    change: Option<&SummaryChange>,
) -> CoreResult<Option<SummaryRevision>> {
    let expected_source = SourceRef {
        project_id: access.project_id.clone(),
        document_id: target.document_id.clone(),
        revision_id: revision.id.clone(),
        body_hash: target.body_hash.clone(),
    };
    match change {
        Some(SummaryChange::Set { text, audience }) => {
            let summary = SummaryRevision {
                id: new_id(),
                text: text.clone(),
                audience: *audience,
                source: expected_source,
                dependencies: prefix.to_vec(),
            };
            validate_summary_binding(&summary, &access.project_id, &summary.source, prefix)?;
            Ok(Some(summary))
        }
        Some(SummaryChange::Clear) => Ok(None),
        None => {
            let Some(previous_bundle_id) = previous_bundle_id else {
                return Ok(None);
            };
            let previous = read_bundle(db, previous_bundle_id)?.ok_or_else(|| {
                CoreError::new("InvalidProject", "The previous reviewed bundle is missing.")
            })?;
            let Some(summary) = previous.summary else {
                return Ok(None);
            };
            if previous.target == *target
                && summary.source == expected_source
                && summary.dependencies == prefix
            {
                return Ok(Some(summary));
            }
            Err(CoreError::new(
                "ReviewSummaryRequired",
                "The reviewed summary basis changed; explicitly set or clear the summary.",
            ))
        }
    }
}

fn parse_record_set(
    records_json: Option<String>,
    records_hash: Option<String>,
    revision: &Revision,
) -> CoreResult<(Option<Vec<PossessionRecord>>, Option<String>)> {
    match (records_json, records_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed evidence JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            let records: Vec<PossessionRecord> = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed evidence is malformed: {error}"),
                )
            })?;
            if records.is_empty() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An empty reviewed evidence set must use the legacy null representation.",
                ));
            }
            let actual = validate_records(&records, revision).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed evidence is invalid: {}", error.detail),
                )
            })?;
            if actual.as_deref() != Some(hash.as_str())
                || canonical_records_json(&records)?.as_deref() != Some(json.as_str())
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed evidence hash or canonical JSON is invalid.",
                ));
            }
            Ok((Some(records), Some(hash)))
        }
    }
}

fn parse_promise_set(
    promises_json: Option<String>,
    promises_hash: Option<String>,
    revision: &Revision,
) -> CoreResult<(Option<Vec<PromiseRecord>>, Option<String>)> {
    match (promises_json, promises_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed promises JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            let promises: Vec<PromiseRecord> = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed promises are malformed: {error}"),
                )
            })?;
            if promises.is_empty() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An empty reviewed promise set must use the legacy null representation.",
                ));
            }
            let actual = validate_promises(&promises, revision).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed promises are invalid: {}", error.detail),
                )
            })?;
            if actual.as_deref() != Some(hash.as_str())
                || canonical_promises_json(&promises)?.as_deref() != Some(json.as_str())
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed promise hash or canonical JSON is invalid.",
                ));
            }
            Ok((Some(promises), Some(hash)))
        }
    }
}

fn parse_summary_set(
    summary_json: Option<String>,
    expected_hash: Option<String>,
    project_id: &str,
    target: &Head,
    revision_id: &str,
    prefix: &[ReviewPrefixItem],
) -> CoreResult<(Option<SummaryRevision>, Option<String>)> {
    match (summary_json, expected_hash) {
        (None, None) => Ok((None, None)),
        (Some(_), None) | (None, Some(_)) => Err(CoreError::new(
            "InvalidProject",
            "Reviewed summary JSON and hash must be present together.",
        )),
        (Some(json), Some(hash)) => {
            if json.len() > 4 * 1024 * 1024 || !valid_hash(&hash) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed summary is too large or has an invalid hash.",
                ));
            }
            let summary: SummaryRevision = serde_json::from_str(&json).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The saved reviewed summary is malformed: {error}"),
                )
            })?;
            if canonical_summary_json(&summary)? != json || summary_hash(&summary)? != hash {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The saved reviewed summary hash or canonical JSON is invalid.",
                ));
            }
            let source = SourceRef {
                project_id: project_id.to_owned(),
                document_id: target.document_id.clone(),
                revision_id: revision_id.to_owned(),
                body_hash: target.body_hash.clone(),
            };
            validate_summary_binding(&summary, project_id, &source, prefix)?;
            Ok((Some(summary), Some(hash)))
        }
    }
}

fn read_stage(
    db: &Connection,
    access: &ProjectAccess,
    stage_id: &str,
) -> CoreResult<Option<StageRow>> {
    let row: Option<StageDbRow> = db
        .query_row(
            "SELECT id,project_id,operation_namespace,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash,created_at FROM review_stages WHERE id=? AND project_id=? AND operation_namespace=?",
            params![stage_id, access.project_id, access.operation_namespace],
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?,
                row.get(13)?, row.get(14)?, row.get(15)?, row.get(16)?, row.get(17)?, row.get(18)?,
            )),
        )
        .optional()?;
    let Some((
        id,
        project_id,
        operation_namespace,
        document_id,
        target_version,
        target_body_hash,
        target_revision_id,
        source_epoch,
        policy_epoch,
        previous_bundle_id,
        prefix_json,
        prefix_hash,
        records_json,
        records_hash,
        promises_json,
        promises_hash,
        summary_json,
        summary_hash,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
    let revision = read_revision(db, &target_revision_id)?;
    if revision.head.document_id != document_id
        || revision.head.version != target_version.to_string()
        || revision.head.body_hash != target_body_hash
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The review stage revision does not match its target.",
        ));
    }
    let (records, records_hash) = parse_record_set(records_json, records_hash, &revision)?;
    let (promises, promises_hash) = parse_promise_set(promises_json, promises_hash, &revision)?;
    let (summary, summary_hash) = parse_summary_set(
        summary_json,
        summary_hash,
        &project_id,
        &Head {
            document_id: document_id.clone(),
            version: parse_stored_version(target_version)?,
            body_hash: target_body_hash.clone(),
        },
        &target_revision_id,
        &prefix,
    )?;
    Ok(Some(StageRow {
        id,
        project_id,
        operation_namespace,
        document_id: document_id.clone(),
        target: Head {
            document_id,
            version: parse_stored_version(target_version)?,
            body_hash: target_body_hash,
        },
        revision_id: target_revision_id,
        source_epoch,
        policy_epoch,
        previous_bundle_id,
        prefix,
        prefix_hash,
        records,
        records_hash,
        promises,
        promises_hash,
        summary,
        summary_hash,
        created_at,
    }))
}

fn stage_to_dto(db: &Connection, stage: StageRow) -> CoreResult<ReviewStage> {
    let revision = read_revision(db, &stage.revision_id)?;
    if revision.head != stage.target {
        return Err(CoreError::new(
            "InvalidProject",
            "The review stage revision does not match its target.",
        ));
    }
    Ok(ReviewStage {
        id: stage.id,
        project_id: stage.project_id,
        operation_namespace: stage.operation_namespace,
        target: stage.target,
        revision,
        previous_bundle_id: stage.previous_bundle_id,
        prefix: stage.prefix,
        records: stage.records,
        records_hash: stage.records_hash,
        promises: stage.promises,
        promises_hash: stage.promises_hash,
        summary: stage.summary,
        summary_hash: stage.summary_hash,
        source_epoch: parse_stored_version(stage.source_epoch)?,
        policy_epoch: parse_stored_version(stage.policy_epoch)?,
        created_at: stage.created_at,
    })
}

fn read_bundle(db: &Connection, bundle_id: &str) -> CoreResult<Option<BundleRow>> {
    let row: Option<BundleDbRow> = db
        .query_row(
            "SELECT id,project_id,operation_namespace,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash,created_at FROM ready_bundles WHERE id=?",
            [bundle_id],
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?,
                row.get(15)?, row.get(16)?, row.get(17)?, row.get(18)?, row.get(19)?, row.get(20)?,
            )),
        )
        .optional()?;
    let Some((
        id,
        project_id,
        operation_namespace,
        stage_id,
        document_id,
        target_version,
        target_body_hash,
        target_revision_id,
        _source_epoch,
        policy_epoch,
        _previous_bundle_id,
        prefix_json,
        prefix_hash,
        coverage,
        records_json,
        records_hash,
        promises_json,
        promises_hash,
        summary_json,
        summary_hash,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    let revision = read_revision(db, &target_revision_id)?;
    if revision.head.document_id != document_id
        || revision.head.version != target_version.to_string()
        || revision.head.body_hash != target_body_hash
    {
        return Err(CoreError::new(
            "InvalidProject",
            "A ready bundle revision does not match its target.",
        ));
    }
    let (records, records_hash) = parse_record_set(records_json, records_hash, &revision)?;
    let (promises, promises_hash) = parse_promise_set(promises_json, promises_hash, &revision)?;
    let target = Head {
        document_id: document_id.clone(),
        version: parse_stored_version(target_version)?,
        body_hash: target_body_hash.clone(),
    };
    let (summary, summary_hash) = parse_summary_set(
        summary_json,
        summary_hash,
        &project_id,
        &target,
        &target_revision_id,
        &parse_prefix(&prefix_json, &prefix_hash)?,
    )?;
    Ok(Some(BundleRow {
        id,
        project_id,
        operation_namespace,
        stage_id,
        document_id: document_id.clone(),
        target,
        revision_id: target_revision_id,
        policy_epoch,
        prefix: parse_prefix(&prefix_json, &prefix_hash)?,
        coverage,
        records,
        records_hash,
        promises,
        promises_hash,
        summary,
        summary_hash,
        created_at,
    }))
}

fn bundle_to_dto(bundle: BundleRow) -> ReadyBundle {
    ReadyBundle {
        id: bundle.id,
        project_id: bundle.project_id,
        operation_namespace: bundle.operation_namespace,
        stage_id: bundle.stage_id,
        target: bundle.target,
        records: bundle.records,
        records_hash: bundle.records_hash,
        promises: bundle.promises,
        promises_hash: bundle.promises_hash,
        summary: bundle.summary,
        summary_hash: bundle.summary_hash,
        created_at: bundle.created_at,
    }
}

pub(super) fn selected_prefix(
    db: &Connection,
    access: &ProjectAccess,
    target_document_id: &str,
    policy_epoch: i64,
) -> CoreResult<Vec<ReviewPrefixItem>> {
    let target_position: i64 = db.query_row(
        "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0",
        [target_document_id],
        |row| row.get(0),
    )?;
    let mut statement = db.prepare(
        "SELECT id,title,position FROM documents WHERE kind='chapter' AND trashed=0
         AND (position<? OR (position=? AND id<?)) ORDER BY position,id LIMIT ?",
    )?;
    let rows = statement
        .query_map(
            params![
                target_position,
                target_position,
                target_document_id,
                (MAX_REVIEW_CHAPTERS + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "ReviewLimitExceeded",
            "There are too many earlier chapters to review safely.",
        ));
    }
    let mut result = Vec::with_capacity(rows.len());
    for (document_id, title, _) in rows {
        let bundle_id = active_bundle_id(db, access, &document_id)?.ok_or_else(|| {
            CoreError::new(
                "ReviewBasisUnavailable",
                "Every earlier chapter must be marked reviewed first.",
            )
        })?;
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier selected reviewed bundle is missing.",
            )
        })?;
        let current = read_document(db, &document_id)?;
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.target != current.head
            || bundle.policy_epoch != policy_epoch
        {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter needs review before this chapter can be marked reviewed.",
            ));
        }
        if !same_prefix_basis(&bundle.prefix, &result) {
            return Err(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter's reviewed basis is no longer valid.",
            ));
        }
        result.push(ReviewPrefixItem {
            document_id,
            title,
            bundle_id,
            revision_id: bundle.revision_id,
            head: current.head,
        });
    }
    Ok(result)
}

/// Validate immutable reviewed provenance for an export or historical read.
///
/// This deliberately does not consult `ready_heads`, current policy, or the
/// current ordered prefix. Those are required by
/// `resolve_reviewed_export_source` for a new export, while an already-recorded
/// export must remain independently verifiable after a later review replaces
/// the selected head.
pub(super) fn validate_reviewed_export_source(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    expected: &Head,
    revision_id: &str,
) -> CoreResult<()> {
    check_id(project_id)?;
    check_id(operation_namespace)?;
    check_id(bundle_id)?;
    check_id(&expected.document_id)?;
    check_id(revision_id)?;
    parse_version(&expected.version)?;
    if !valid_hash(&expected.body_hash) {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export target has an invalid body fingerprint.",
        ));
    }

    let bundle = read_bundle(connection, bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "ReviewBundleNotFound",
            "The reviewed export bundle is not available.",
        )
    })?;
    if bundle.coverage != "authorOnly"
        || bundle.project_id != project_id
        || bundle.operation_namespace != operation_namespace
        || bundle.document_id != expected.document_id
        || bundle.target != *expected
        || bundle.revision_id != revision_id
    {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export bundle does not match its immutable source.",
        ));
    }

    validate_prefix_evidence(
        connection,
        project_id,
        operation_namespace,
        &expected.document_id,
        &bundle.prefix,
    )
    .map_err(|error| {
        CoreError::new(
            "ReviewSourceMismatch",
            &format!("The reviewed export prefix is invalid: {}", error.detail),
        )
    })?;

    let revision = read_revision(connection, revision_id).map_err(|error| {
        CoreError::new(
            "ReviewSourceMismatch",
            &format!("The reviewed export revision is invalid: {}", error.detail),
        )
    })?;
    if revision.id != revision_id || revision.head != *expected {
        return Err(CoreError::new(
            "ReviewSourceMismatch",
            "The reviewed export revision does not match its immutable source.",
        ));
    }
    Ok(())
}

/// Resolve the explicit reviewed evidence attached to a set of exact sources
/// in one ordered prefix validation pass.
///
/// The single-source helper historically called [`selected_prefix`] for every
/// source.  That made a working-story freeze reread and reparse every growing
/// reviewed prefix.  This batch form reads each ordered chapter's selected
/// bundle once, while retaining the same distinction between an unavailable
/// current basis (`None`) and malformed persistence (`Err`).  Returned sets
/// retain the order of `sources`; sources without a current reviewed record
/// set are omitted.
pub(super) fn current_records_for_sources(
    db: &Connection,
    access: &ProjectAccess,
    sources: &[SourceRef],
) -> CoreResult<Vec<ReviewedRecordSet>> {
    let policy_epoch = current_epochs(db)?.1;
    let mut candidates = Vec::new();
    for source in sources {
        check_id(&source.project_id)?;
        check_id(&source.document_id)?;
        check_id(&source.revision_id)?;
        if !valid_hash(&source.body_hash) || source.project_id != access.project_id {
            return Err(CoreError::new(
                "InvalidReviewedRecords",
                "The reviewed evidence source has invalid project or body identity.",
            ));
        }
        let Some(bundle_id) = active_bundle_id(db, access, &source.document_id)? else {
            continue;
        };
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new("InvalidProject", "The selected reviewed bundle is missing.")
        })?;
        let document = read_document(db, &source.document_id)?;
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != source.document_id
            || bundle.coverage != "authorOnly"
            || bundle.target != document.head
        {
            continue;
        }
        if bundle.policy_epoch != policy_epoch
            || bundle.target.body_hash != source.body_hash
            || bundle.revision_id != source.revision_id
        {
            continue;
        }
        let target_position: i64 = db.query_row(
            "SELECT position FROM documents WHERE id=? AND kind='chapter' AND trashed=0",
            [&source.document_id],
            |row| row.get(0),
        )?;
        candidates.push(CurrentRecordCandidate {
            source: source.clone(),
            bundle,
            target_position,
        });
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let prefixes = batch_selected_prefixes(db, access, policy_epoch, &candidates)?;
    let mut result = Vec::new();
    for candidate in candidates {
        let Some(outcome) = prefixes.get(&candidate.source.document_id) else {
            continue;
        };
        match outcome {
            PrefixOutcome::Valid {
                matches_bundle: true,
            } => {}
            PrefixOutcome::Unavailable => continue,
            PrefixOutcome::Error(error) => return Err(error.clone()),
            PrefixOutcome::Valid {
                matches_bundle: false,
            } => continue,
        };
        let revision = read_revision(db, &candidate.bundle.revision_id)?;
        if revision.head.document_id != candidate.source.document_id
            || revision.head.body_hash != candidate.source.body_hash
            || revision.id != candidate.source.revision_id
        {
            continue;
        }
        if candidate.bundle.records.is_none()
            && candidate.bundle.promises.is_none()
            && candidate.bundle.summary.is_none()
        {
            continue;
        }
        result.push(ReviewedRecordSet {
            bundle_id: candidate.bundle.id,
            project_id: candidate.bundle.project_id,
            operation_namespace: candidate.bundle.operation_namespace,
            target: candidate.bundle.target,
            revision,
            records: candidate.bundle.records.unwrap_or_default(),
            records_hash: candidate.bundle.records_hash,
            promises: candidate.bundle.promises,
            promises_hash: candidate.bundle.promises_hash,
            summary: candidate.bundle.summary,
            summary_hash: candidate.bundle.summary_hash,
            current: true,
        });
    }
    Ok(result)
}

#[derive(Debug)]
struct CurrentRecordCandidate {
    source: SourceRef,
    bundle: BundleRow,
    target_position: i64,
}

#[derive(Debug)]
enum PrefixOutcome {
    Valid { matches_bundle: bool },
    Unavailable,
    Error(CoreError),
}

/// Resolve selected prefixes for all current candidates in one ordered walk.
/// The result is keyed by document ID because a source's exact revision was
/// already checked while building `CurrentRecordCandidate`.
fn batch_selected_prefixes(
    db: &Connection,
    access: &ProjectAccess,
    policy_epoch: i64,
    candidates: &[CurrentRecordCandidate],
) -> CoreResult<HashMap<String, PrefixOutcome>> {
    let max_target = candidates
        .iter()
        .max_by(|left, right| {
            left.target_position
                .cmp(&right.target_position)
                .then_with(|| left.source.document_id.cmp(&right.source.document_id))
        })
        .expect("batch prefix validation requires a candidate");
    let mut statement = db.prepare(
        "SELECT id,title,position FROM documents WHERE kind='chapter' AND trashed=0
         AND (position<? OR (position=? AND id<?)) ORDER BY position,id LIMIT ?",
    )?;
    let rows = statement
        .query_map(
            params![
                max_target.target_position,
                max_target.target_position,
                max_target.source.document_id,
                (MAX_REVIEW_CHAPTERS + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    let mut target_by_key = HashMap::new();
    let mut candidate_by_document = HashMap::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        candidate_by_document.insert(candidate.source.document_id.clone(), candidate_index);
        target_by_key.insert(
            (
                candidate.target_position,
                candidate.source.document_id.clone(),
            ),
            candidate_index,
        );
    }
    let mut outcomes = HashMap::new();
    for candidate in candidates {
        // `rows` is ordered by position and ID.  Count exact predecessors so
        // same-position chapters retain selected_prefix's lexical ordering.
        let prior_count = rows
            .iter()
            .take_while(|(document_id, _, position)| {
                *position < candidate.target_position
                    || (*position == candidate.target_position
                        && document_id < &candidate.source.document_id)
            })
            .count();
        if prior_count > MAX_REVIEW_CHAPTERS {
            outcomes.insert(
                candidate.source.document_id.clone(),
                PrefixOutcome::Error(CoreError::new(
                    "ReviewLimitExceeded",
                    "There are too many earlier chapters to review safely.",
                )),
            );
        }
    }

    let active_heads = load_active_bundle_ids(db, access)?;
    let mut prefix = Vec::new();
    let mut broken: Option<CoreError> = None;
    for (index, (document_id, title, position)) in rows.iter().enumerate() {
        match target_by_key.get(&(*position, document_id.clone())) {
            Some(candidate_index)
                if !outcomes.contains_key(&candidates[*candidate_index].source.document_id) =>
            {
                let target_id = &candidates[*candidate_index].source.document_id;
                outcomes.insert(
                    target_id.clone(),
                    match broken.as_ref() {
                        Some(error) if error.code == "ReviewBasisUnavailable" => {
                            PrefixOutcome::Unavailable
                        }
                        Some(error) => PrefixOutcome::Error(error.clone()),
                        None => PrefixOutcome::Valid {
                            matches_bundle: same_prefix_basis(
                                &candidates[*candidate_index].bundle.prefix,
                                &prefix,
                            ),
                        },
                    },
                );
            }
            _ => {}
        }
        if index >= MAX_REVIEW_CHAPTERS {
            broken.get_or_insert_with(|| {
                CoreError::new(
                    "ReviewLimitExceeded",
                    "There are too many earlier chapters to review safely.",
                )
            });
            continue;
        }
        if broken.is_some() {
            continue;
        }
        let Some(bundle_id) = active_heads.get(document_id) else {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "Every earlier chapter must be marked reviewed first.",
            ));
            continue;
        };
        let bundle = if let Some(candidate_index) = candidate_by_document.get(document_id) {
            candidates[*candidate_index].bundle.clone()
        } else {
            match read_bundle(db, bundle_id) {
                Ok(Some(bundle)) => bundle,
                Ok(None) => {
                    broken = Some(CoreError::new(
                        "ReviewBasisUnavailable",
                        "An earlier selected reviewed bundle is missing.",
                    ));
                    continue;
                }
                Err(error) => {
                    broken = Some(error);
                    continue;
                }
            }
        };
        let current = match read_document(db, document_id) {
            Ok(current) => current,
            Err(error) => {
                broken = Some(error);
                continue;
            }
        };
        if bundle.project_id != access.project_id
            || bundle.operation_namespace != access.operation_namespace
            || bundle.document_id != *document_id
            || bundle.coverage != "authorOnly"
            || bundle.target != current.head
            || bundle.policy_epoch != policy_epoch
        {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter needs review before this chapter can be marked reviewed.",
            ));
            continue;
        }
        if !same_prefix_basis(&bundle.prefix, &prefix) {
            broken = Some(CoreError::new(
                "ReviewBasisUnavailable",
                "An earlier chapter's reviewed basis is no longer valid.",
            ));
            continue;
        }
        prefix.push(ReviewPrefixItem {
            document_id: document_id.clone(),
            title: title.clone(),
            bundle_id: bundle.id,
            revision_id: bundle.revision_id,
            head: current.head,
        });
    }

    for candidate in candidates {
        if outcomes.contains_key(&candidate.source.document_id) {
            continue;
        }
        outcomes.insert(
            candidate.source.document_id.clone(),
            match broken.as_ref() {
                Some(error) if error.code == "ReviewBasisUnavailable" => PrefixOutcome::Unavailable,
                Some(error) => PrefixOutcome::Error(error.clone()),
                None => PrefixOutcome::Valid {
                    matches_bundle: same_prefix_basis(&candidate.bundle.prefix, &prefix),
                },
            },
        );
    }
    Ok(outcomes)
}

fn load_active_bundle_ids(
    db: &Connection,
    access: &ProjectAccess,
) -> CoreResult<HashMap<String, String>> {
    let mut statement = db.prepare(
        "SELECT document_id,bundle_id FROM ready_heads WHERE project_id=? AND operation_namespace=?",
    )?;
    Ok(statement
        .query_map(
            params![access.project_id, access.operation_namespace],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .collect::<Result<HashMap<_, _>, _>>()?)
}

/// Authenticate a complete reviewed evidence array retained in a frozen
/// historical packet.  This intentionally does not consult today's selected
/// head, policy epoch, or ordered prefix.
#[allow(dead_code)]
pub(super) fn validate_reviewed_records(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    source: &SourceRef,
    records_hash: &str,
    records: &[PossessionRecord],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_records(
        project_id,
        operation_namespace,
        bundle_id,
        source,
        records_hash,
        records,
    )?;
    Ok(())
}

/// Authenticate a complete reviewed promise array retained in a frozen
/// historical packet without consulting current selection or policy state.
#[allow(dead_code)]
pub(super) fn validate_reviewed_promises(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    bundle_id: &str,
    source: &SourceRef,
    promises_hash: &str,
    promises: &[PromiseRecord],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_promises(
        project_id,
        operation_namespace,
        bundle_id,
        source,
        promises_hash,
        promises,
    )?;
    Ok(())
}

/// Validate the immutable bundle provenance retained by a frozen reviewed
/// snapshot. This intentionally does not consult `ready_heads`: old
/// snapshots remain readable evidence after a later review supersedes a
/// selected head. New continuation requests use `selected_prefix` instead.
#[allow(dead_code)]
pub(super) fn validate_reviewed_snapshot_manifest(
    db: &Connection,
    snapshot_project_id: &str,
    snapshot_namespace: &str,
    snapshot_policy_epoch: &str,
    manifest: &ReviewedBasisManifest,
    sources: &[SourceDescriptor],
) -> CoreResult<()> {
    ReviewValidationContext::new(db).validate_reviewed_snapshot_manifest(
        snapshot_project_id,
        snapshot_namespace,
        snapshot_policy_epoch,
        manifest,
        sources,
    )?;
    Ok(())
}

fn same_manifest_prefix(actual: &[ReviewPrefixItem], expected: &[ReviewedBasisMember]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.document_id == expected.document_id
                && actual.bundle_id == expected.bundle_id
                && actual.revision_id == expected.revision_id
                && actual.head.version == expected.version
                && actual.head.body_hash == expected.body_hash
        })
}

fn same_prefix_basis(left: &[ReviewPrefixItem], right: &[ReviewPrefixItem]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.document_id == right.document_id
                && left.bundle_id == right.bundle_id
                && left.revision_id == right.revision_id
                && left.head == right.head
        })
}

fn stage_capability(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
    policy_epoch: i64,
) -> CoreResult<(bool, Option<String>)> {
    match selected_prefix(db, access, document_id, policy_epoch) {
        Ok(_) => Ok((true, None)),
        Err(error) if error.code == "ReviewBasisUnavailable" => Ok((false, Some(error.detail))),
        Err(error) => Err(error),
    }
}

fn validate_prefix_evidence(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    owner_document_id: &str,
    prefix: &[ReviewPrefixItem],
) -> CoreResult<()> {
    let mut validation = ReviewValidationContext::new(db);
    validation.validate_prefix_evidence_from_prefix(
        project_id,
        operation_namespace,
        owner_document_id,
        prefix,
    )?;
    Ok(())
}

fn validate_previous_bundle(
    db: &Connection,
    project_id: &str,
    operation_namespace: &str,
    owner_document_id: &str,
    previous_bundle_id: Option<&str>,
) -> CoreResult<()> {
    let Some(previous_bundle_id) = previous_bundle_id else {
        return Ok(());
    };
    let previous = read_bundle(db, previous_bundle_id)?.ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "A review record points to a missing previous bundle.",
        )
    })?;
    if previous.project_id != project_id
        || previous.operation_namespace != operation_namespace
        || previous.document_id != owner_document_id
    {
        return Err(CoreError::new(
            "InvalidProject",
            "A review record points to a previous bundle from another chapter or identity.",
        ));
    }
    Ok(())
}

fn status_needs_review(
    document: &DocumentRecord,
    bundle_id: String,
    reason: &str,
    can_stage: bool,
    pending_stage_id: Option<String>,
) -> ReviewStatus {
    ReviewStatus {
        document_id: document.head.document_id.clone(),
        title: document.title.clone(),
        head: document.head.clone(),
        state: ReviewState::ReviewNeeded,
        active_bundle_id: Some(bundle_id),
        pending_stage_id,
        reason: Some(reason.to_owned()),
        can_stage,
    }
}

fn pending_stage_id(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<String>> {
    db.query_row(
        "SELECT s.id FROM review_stages s
         WHERE s.project_id=? AND s.operation_namespace=? AND s.document_id=?
         AND NOT EXISTS (SELECT 1 FROM ready_bundles b WHERE b.stage_id=s.id)
         ORDER BY s.rowid DESC LIMIT 1",
        params![access.project_id, access.operation_namespace, document_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(CoreError::from)
}

/// Validate the durable review tables without rejecting historical bundles
/// retained after recovery. Stale prose and stale prefixes are ordinary
/// historical states; only a current selected head can authorize future work.
pub(crate) fn validate_review_storage(db: &Connection) -> CoreResult<()> {
    let (project_id, namespace): (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let count: i64 = db.query_row("SELECT COUNT(*) FROM review_stages", [], |row| row.get(0))?;
    if count < 0 || count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical review stages.",
        ));
    }
    let count: i64 = db.query_row("SELECT COUNT(*) FROM ready_bundles", [], |row| row.get(0))?;
    if count < 0 || count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical ready bundles.",
        ));
    }
    let count: i64 = db.query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row.get(0))?;
    if count < 0 || count as usize > MAX_REVIEW_CHAPTERS {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many selected reviewed heads.",
        ));
    }
    let mut stages = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash FROM review_stages ORDER BY id",
    )?;
    let stage_rows = stages.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, Option<String>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
        ))
    })?;
    for row in stage_rows {
        let (
            id,
            stage_project,
            stage_namespace,
            operation_id,
            document_id,
            version,
            body_hash,
            revision_id,
            source_epoch,
            policy_epoch,
            previous,
            prefix_json,
            prefix_hash,
            records_json,
            records_hash,
            promises_json,
            promises_hash,
            summary_json,
            summary_hash,
        ) = row?;
        check_id(&id)?;
        check_id(&stage_project)?;
        check_id(&stage_namespace)?;
        check_id(&operation_id)?;
        check_id(&document_id)?;
        check_id(&revision_id)?;
        let _ = (
            parse_stored_version(version)?,
            parse_stored_version(source_epoch)?,
            parse_stored_version(policy_epoch)?,
        );
        if let Some(previous) = previous.as_deref() {
            check_id(previous)?;
            validate_previous_bundle(
                db,
                &stage_project,
                &stage_namespace,
                &document_id,
                Some(previous),
            )?;
        }
        let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
        validate_prefix_evidence(db, &stage_project, &stage_namespace, &document_id, &prefix)?;
        let revision = read_revision(db, &revision_id)?;
        if revision.head.document_id != document_id
            || revision.head.version != version.to_string()
            || revision.head.body_hash != body_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review stage revision does not match its target.",
            ));
        }
        let _ = parse_record_set(records_json, records_hash, &revision)?;
        let _ = parse_promise_set(promises_json, promises_hash, &revision)?;
        let _ = parse_summary_set(
            summary_json,
            summary_hash,
            &stage_project,
            &Head {
                document_id: document_id.clone(),
                version: parse_stored_version(version)?,
                body_hash: body_hash.clone(),
            },
            &revision_id,
            &prefix,
        )?;
    }
    let mut bundles = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,stage_id,document_id,target_version,target_body_hash,target_revision_id,source_epoch,policy_epoch,previous_bundle_id,prefix_json,prefix_hash,coverage,records_json,records_hash,promises_json,promises_hash,summary_json,summary_hash FROM ready_bundles ORDER BY id",
    )?;
    let bundle_rows = bundles.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
            row.get::<_, Option<String>>(21)?,
        ))
    })?;
    for row in bundle_rows {
        let (
            id,
            bundle_project,
            bundle_namespace,
            operation_id,
            payload_hash,
            stage_id,
            document_id,
            version,
            body_hash,
            revision_id,
            source_epoch,
            policy_epoch,
            previous,
            prefix_json,
            prefix_hash,
            coverage,
            records_json,
            records_hash,
            promises_json,
            promises_hash,
            summary_json,
            summary_hash,
        ) = row?;
        for id in [
            &id,
            &bundle_project,
            &bundle_namespace,
            &operation_id,
            &stage_id,
            &document_id,
            &revision_id,
        ] {
            check_id(id)?;
        }
        if payload_hash.is_empty() || coverage != "authorOnly" {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle has invalid metadata.",
            ));
        }
        let _ = (
            parse_stored_version(version)?,
            parse_stored_version(source_epoch)?,
            parse_stored_version(policy_epoch)?,
        );
        if let Some(previous) = previous.as_deref() {
            check_id(previous)?;
            validate_previous_bundle(
                db,
                &bundle_project,
                &bundle_namespace,
                &document_id,
                Some(previous),
            )?;
        }
        let prefix = parse_prefix(&prefix_json, &prefix_hash)?;
        validate_prefix_evidence(
            db,
            &bundle_project,
            &bundle_namespace,
            &document_id,
            &prefix,
        )?;
        let revision = read_revision(db, &revision_id)?;
        if revision.head.document_id != document_id
            || revision.head.version != version.to_string()
            || revision.head.body_hash != body_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle revision does not match its target.",
            ));
        }
        let (bundle_records, bundle_records_hash) =
            parse_record_set(records_json, records_hash, &revision)?;
        let (bundle_promises, bundle_promises_hash) =
            parse_promise_set(promises_json, promises_hash, &revision)?;
        let (bundle_summary, bundle_summary_hash) = parse_summary_set(
            summary_json,
            summary_hash,
            &bundle_project,
            &Head {
                document_id: document_id.clone(),
                version: parse_stored_version(version)?,
                body_hash: body_hash.clone(),
            },
            &revision_id,
            &prefix,
        )?;
        let stage_identity: Option<(String, String)> = db
            .query_row(
                "SELECT project_id,operation_namespace FROM review_stages WHERE id=?",
                [&stage_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if stage_identity.as_ref() != Some(&(bundle_project.clone(), bundle_namespace.clone())) {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle references a foreign review stage.",
            ));
        }
        let stage_access = ProjectAccess {
            project_id: bundle_project.clone(),
            operation_namespace: bundle_namespace.clone(),
            session: "validation".into(),
            writer_lease: "validation".into(),
        };
        let stage = read_stage(db, &stage_access, &stage_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A ready bundle references a missing review stage.",
            )
        })?;
        if stage.target.document_id != document_id
            || stage.target.version != version.to_string()
            || stage.target.body_hash != body_hash
            || stage.revision_id != revision_id
            || stage.source_epoch != source_epoch
            || stage.policy_epoch != policy_epoch
            || stage.previous_bundle_id != previous
            || !same_prefix_basis(&stage.prefix, &prefix)
            || stage.prefix_hash != prefix_hash
            || stage.records != bundle_records
            || stage.records_hash != bundle_records_hash
            || stage.promises != bundle_promises
            || stage.promises_hash != bundle_promises_hash
            || stage.summary != bundle_summary
            || stage.summary_hash != bundle_summary_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A ready bundle does not match its immutable review stage.",
            ));
        }
    }
    let mut heads = db.prepare("SELECT project_id,operation_namespace,document_id,bundle_id FROM ready_heads ORDER BY document_id")?;
    let head_rows = heads.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in head_rows {
        let (head_project, head_namespace, document_id, bundle_id) = row?;
        if head_project != project_id || head_namespace != namespace {
            return Err(CoreError::new(
                "InvalidProject",
                "A selected reviewed head belongs to a retired identity.",
            ));
        }
        check_id(&document_id)?;
        check_id(&bundle_id)?;
        let bundle = read_bundle(db, &bundle_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A selected reviewed head points to a missing bundle.",
            )
        })?;
        if bundle.project_id != project_id
            || bundle.operation_namespace != namespace
            || bundle.document_id != document_id
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A selected reviewed head crosses project identity.",
            ));
        }
    }
    let fence_count: i64 =
        db.query_row("SELECT COUNT(*) FROM review_fences", [], |row| row.get(0))?;
    if fence_count < 0 || fence_count as usize > 100_000 {
        return Err(CoreError::new(
            "InvalidProject",
            "Too many historical review fences.",
        ));
    }
    let mut fences = db.prepare("SELECT id,project_id,operation_namespace,affected_bundle_id,changed_document_id,changed_version,changed_body_hash,superseding_bundle_id FROM review_fences ORDER BY id")?;
    let fence_rows = fences.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    for row in fence_rows {
        let (
            id,
            fence_project,
            fence_namespace,
            affected,
            changed_document,
            changed_version,
            changed_hash,
            superseding,
        ) = row?;
        for value in [
            &id,
            &fence_project,
            &fence_namespace,
            &affected,
            &changed_document,
            &superseding,
        ] {
            check_id(value)?;
        }
        let _ = parse_stored_version(changed_version)?;
        if changed_hash.is_empty()
            || read_bundle(db, &affected)?.is_none()
            || read_bundle(db, &superseding)?.is_none()
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review fence references invalid bundle evidence.",
            ));
        }
        let affected_bundle = read_bundle(db, &affected)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A review fence references a missing affected bundle.",
            )
        })?;
        let superseding_bundle = read_bundle(db, &superseding)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A review fence references a missing superseding bundle.",
            )
        })?;
        if affected_bundle.project_id != fence_project
            || affected_bundle.operation_namespace != fence_namespace
            || superseding_bundle.project_id != fence_project
            || superseding_bundle.operation_namespace != fence_namespace
            || superseding_bundle.target.document_id != changed_document
            || superseding_bundle.target.version != changed_version.to_string()
            || superseding_bundle.target.body_hash != changed_hash
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A review fence does not match its bundle evidence.",
            ));
        }
    }
    Ok(())
}
