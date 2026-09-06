//! Frozen generated navigation is distinct from original manuscript evidence.
//! The project owner resolves these records; the compiler checks their exact
//! payload and source dependencies before supplying an unreviewed summary.
use crate::context::memory::{
    DIGEST_SCHEMA_VERSION, DigestCandidate, MAX_RAW_BYTES, validate_navigation_digest,
};
use crate::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, InformationPolicy, SourceKind, SourceRef,
    StorySnapshot, evaluate_sources,
};
use crate::projects::story_context::SourceRead;
use crate::projects::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MAX_FROZEN_NAVIGATION_VIEWS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationViewRef {
    pub view_id: String,
    pub project_id: String,
    pub operation_namespace: String,
    /// Fingerprint of generated candidate JSON, never a manuscript body hash.
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenNavigationView {
    pub reference: NavigationViewRef,
    pub source_context_epoch: String,
    pub disclosure_policy_version: String,
    /// Complete input dependencies, not merely the quotations displayed by UI.
    pub dependencies: Vec<SourceRef>,
    pub candidate: DigestCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NavigationOmissionReason {
    OriginalTextIncluded,
    Budget,
    NotSmaller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationViewOmission {
    pub view_id: String,
    pub reason: NavigationOmissionReason,
}

/// Candidate objects contain only typed integers, text, and arrays. Converting
/// through Value sorts object keys, so the hash does not depend on input JSON
/// whitespace or key order. Existing memory rows need not be rewritten.
pub fn navigation_content_hash(candidate: &DigestCandidate) -> CoreResult<String> {
    let bytes = serde_json::to_vec(&serde_json::to_value(candidate)?)?;
    if bytes.len() > MAX_RAW_BYTES {
        return Err(invalid("The generated navigation payload is too large."));
    }
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

fn invalid(detail: &str) -> CoreError {
    CoreError::new("InvalidNavigationContext", detail)
}

/// Validate frozen relationships without consulting today's mutable heads.
/// Storage separately authenticates namespace ownership and immutable view
/// records; this check remains valid for an already frozen historical packet.
pub fn validate_frozen_navigation_views(
    views: &[FrozenNavigationView],
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> CoreResult<()> {
    if views.is_empty() {
        return Ok(());
    }
    if views.len() > MAX_FROZEN_NAVIGATION_VIEWS
        || snapshot.basis != BasisKind::Working
        || policy.audience != Audience::AuthorRoom
        || !matches!(
            purpose,
            ContextPurpose::Discuss | ContextPurpose::Plan | ContextPurpose::StoryQuestion
        )
    {
        return Err(invalid(
            "Generated navigation is available only for bounded working-story discussions.",
        ));
    }
    let mut ids = HashSet::new();
    let mut documents = HashSet::new();
    for view in views {
        if !valid_id(&view.reference.view_id)
            || !valid_id(&view.reference.operation_namespace)
            || view.reference.project_id != snapshot.project_id
            || view.source_context_epoch != snapshot.context_source_epoch
            || view.disclosure_policy_version != snapshot.disclosure_policy_version
            || view.disclosure_policy_version != policy.version
            || view.candidate.schema_version != DIGEST_SCHEMA_VERSION
            || view.reference.content_hash != navigation_content_hash(&view.candidate)?
            || !ids.insert(&view.reference.view_id)
            || !documents.insert(&view.candidate.source.document_id)
        {
            return Err(invalid(
                "A navigation view does not match its frozen identity, fingerprint, or story basis.",
            ));
        }
        // C4-A analysis reads one complete chapter. Do not silently accept
        // richer recipes until their complete input dependencies are supported.
        if view.dependencies.as_slice() != std::slice::from_ref(&view.candidate.source)
            || view.candidate.source.document_id == snapshot.target.document_id
        {
            return Err(invalid(
                "A chapter navigation view needs its exact original chapter dependency.",
            ));
        }
        let descriptor = snapshot
            .sources
            .iter()
            .find(|source| source.source == view.candidate.source)
            .ok_or_else(|| invalid("The original navigation evidence is outside this snapshot."))?;
        if descriptor.kind != SourceKind::CurrentDraft
            || descriptor.coverage != CoverageLabel::Verbatim
            || !descriptor.current
            || descriptor.disclosure.reader_position.is_none()
            || descriptor.disclosure.author_only
            || descriptor.disclosure.future_private
            || !descriptor.dependencies.is_empty()
        {
            return Err(invalid(
                "The navigation view does not have an eligible original chapter source.",
            ));
        }
        let target = snapshot
            .sources
            .iter()
            .find(|source| source.source == snapshot.target)
            .ok_or_else(|| invalid("The frozen target is missing from the source manifest."))?;
        evaluate_sources(
            snapshot,
            policy,
            purpose,
            &[target.handle.clone(), descriptor.handle.clone()],
        )
        .map_err(|_| invalid("The navigation evidence is unavailable under this policy."))?;
    }
    Ok(())
}

/// Range validation authenticates citations against complete original prose.
/// It does not establish that the generated interpretation is true.
pub fn validate_navigation_view_payload(
    view: &FrozenNavigationView,
    source: &SourceRead,
) -> CoreResult<()> {
    if view.dependencies.as_slice() != std::slice::from_ref(&source.descriptor.source)
        || view.candidate.source != source.descriptor.source
        || view.reference.content_hash != navigation_content_hash(&view.candidate)?
    {
        return Err(invalid(
            "The navigation payload does not match its original evidence.",
        ));
    }
    let checked = validate_navigation_digest(&serde_json::to_vec(&view.candidate)?, source)?;
    if checked != view.candidate {
        return Err(invalid(
            "The generated navigation payload changed during validation.",
        ));
    }
    Ok(())
}
