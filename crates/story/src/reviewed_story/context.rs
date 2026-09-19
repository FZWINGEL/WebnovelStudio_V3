use super::*;

/// Read-scoped cache for immutable reviewed provenance.
///
/// A frozen snapshot can contain a growing prefix of reviewed bundles.  The
/// validators intentionally compare every exact persisted field, but they do
/// not need to query and decode the same immutable row once for every later
/// prefix.  This cache is created for one database read only; it never
/// consults mutable ready-head selections and is never shared between reads.
// Public because `story_context` constructs one; it is the batching read side
// of review validation and travels with the module that owns the validation.
pub struct ReviewValidationContext<'db> {
    db: &'db Connection,
    bundles: HashMap<String, Option<BundleRow>>,
    revisions: HashMap<String, Revision>,
}

impl<'db> ReviewValidationContext<'db> {
    pub fn new(db: &'db Connection) -> Self {
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

    pub fn validate_reviewed_records(
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
        validate_ordinary_document_role(self.db, &source.document_id)?;
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

    pub fn validate_reviewed_promises(
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
        validate_ordinary_document_role(self.db, &source.document_id)?;
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

    pub fn validate_reviewed_knowledge(
        &mut self,
        project_id: &str,
        operation_namespace: &str,
        bundle_id: &str,
        source: &SourceRef,
        knowledge_hash: &str,
        knowledge: &[KnowledgeRecord],
    ) -> CoreResult<()> {
        check_id(project_id)?;
        check_id(operation_namespace)?;
        check_id(bundle_id)?;
        if source.project_id != project_id {
            return Err(CoreError::new(
                "InvalidReviewedKnowledge",
                "Reviewed knowledge source belongs to another project.",
            ));
        }
        validate_ordinary_document_role(self.db, &source.document_id)?;
        let revision_id = {
            let bundle = self.read_bundle(bundle_id)?.ok_or_else(|| {
                CoreError::new(
                    "ReviewBundleNotFound",
                    "The reviewed knowledge bundle is unavailable.",
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
                    "InvalidReviewedKnowledge",
                    "The reviewed knowledge bundle does not match its immutable source.",
                ));
            }
            bundle.revision_id.clone()
        };
        let revision = self.read_revision(&revision_id)?;
        let computed = validate_knowledge(knowledge, revision).map_err(|error| {
            CoreError::new(
                "InvalidReviewedKnowledge",
                &format!("The frozen reviewed knowledge is invalid: {}", error.detail),
            )
        })?;
        {
            let bundle = self
                .read_bundle(bundle_id)?
                .expect("bundle inserted into validation cache");
            if computed.as_deref().unwrap_or("") != knowledge_hash
                || bundle.knowledge_hash.as_deref().unwrap_or("") != knowledge_hash
                || bundle.knowledge.as_deref().unwrap_or(&[]) != knowledge
            {
                return Err(CoreError::new(
                    "InvalidReviewedKnowledge",
                    "The frozen reviewed knowledge does not match its immutable bundle.",
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

    pub fn validate_reviewed_summary(
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

    pub fn validate_reviewed_snapshot_manifest(
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
            validate_ordinary_document_role(self.db, &member.document_id)?;
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

    pub(crate) fn validate_prefix_evidence_from_prefix(
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
        validate_ordinary_document_role(self.db, &item.document_id)?;
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

pub(crate) type StageDbRow = (
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
    Option<String>,
    Option<String>,
    String,
);
pub(crate) type BundleDbRow = (
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
    Option<String>,
    Option<String>,
    String,
);
