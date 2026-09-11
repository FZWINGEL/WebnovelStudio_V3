//! Immutable author-accepted narrative summaries, separate from generated digests.
use super::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, InformationPolicy, SourceKind,
    StorySnapshot,
};
use crate::reviewed_summary::SummaryAudience;
use crate::reviewed_summary::SummaryRevision;
use wns_kernel::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedSummarySet {
    pub project_id: String,
    pub operation_namespace: String,
    pub bundle_id: String,
    pub summary_hash: String,
    pub source_handle: String,
    pub summary: SummaryRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedSummaryCoverage {
    pub source_handle: String,
    pub bundle_id: String,
    pub summary_id: String,
    pub summary_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewedSummaryOmissionReason {
    Budget,
    Disclosure,
    OriginalTextIncluded,
    NotSmaller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedSummaryOmission {
    // No summary text, revision ID or hash is disclosed for private summaries.
    pub source_handle: String,
    pub reason: ReviewedSummaryOmissionReason,
}

fn invalid(message: &str) -> CoreError {
    CoreError::new("InvalidReviewedSummary", message)
}

pub fn validate_frozen_set(
    set: &ReviewedSummarySet,
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> CoreResult<()> {
    if [
        &set.project_id,
        &set.operation_namespace,
        &set.bundle_id,
        &set.source_handle,
        &set.summary.id,
    ]
    .iter()
    .any(|id| id.is_empty() || id.len() > 256)
        || set.project_id != snapshot.project_id
        || set.summary.source.project_id != snapshot.project_id
    {
        return Err(invalid(
            "The accepted summary identity does not match this project.",
        ));
    }
    if crate::reviewed_summary::summary_hash(&set.summary)? != set.summary_hash {
        return Err(invalid("The accepted summary fingerprint is invalid."));
    }
    crate::reviewed_summary::validate_summary_text(&set.summary.text)?;
    let descriptor = snapshot
        .sources
        .iter()
        .find(|item| item.handle == set.source_handle)
        .ok_or_else(|| invalid("The accepted summary source is outside this snapshot."))?;
    if descriptor.source != set.summary.source
        || !descriptor.current
        || descriptor.coverage != CoverageLabel::Verbatim
    {
        return Err(invalid(
            "The accepted summary must match its exact current source.",
        ));
    }
    match (snapshot.basis, policy.audience, purpose) {
        (
            BasisKind::Working,
            Audience::AuthorRoom,
            ContextPurpose::Discuss | ContextPurpose::Plan | ContextPurpose::StoryQuestion,
        ) if matches!(
            descriptor.kind,
            SourceKind::CurrentDraft | SourceKind::ReviewedAuthority
        ) => {}
        (BasisKind::Reviewed, Audience::RestrictedWriting, ContextPurpose::Continue)
            if descriptor.kind == SourceKind::ReviewedAuthority =>
        {
            if !snapshot.reviewed_basis.as_ref().is_some_and(|basis| {
                basis.operation_namespace == set.operation_namespace
                    && basis.prefix.iter().any(|member| {
                        member.bundle_id == set.bundle_id
                            && member.document_id == set.summary.source.document_id
                            && member.revision_id == set.summary.source.revision_id
                            && member.body_hash == set.summary.source.body_hash
                    })
            }) {
                return Err(invalid(
                    "The accepted summary is outside the reviewed prefix.",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "Accepted summaries are unavailable for this request basis or purpose.",
            ));
        }
    }
    let mut dependencies = HashSet::new();
    for dependency in &set.summary.dependencies {
        if !dependencies.insert(&dependency.document_id)
            || !snapshot.sources.iter().any(|source| {
                source.current
                    && source.source.document_id == dependency.document_id
                    && source.source.revision_id == dependency.revision_id
                    && source.source.body_hash == dependency.head.body_hash
                    && dependency.head.document_id == dependency.document_id
            })
        {
            return Err(invalid(
                "An accepted summary dependency is missing from the eligible frozen sources.",
            ));
        }
        if let Some(basis) = &snapshot.reviewed_basis
            && !basis.prefix.iter().any(|member| {
                member.document_id == dependency.document_id
                    && member.bundle_id == dependency.bundle_id
                    && member.revision_id == dependency.revision_id
                    && member.version == dependency.head.version
                    && member.body_hash == dependency.head.body_hash
            })
        {
            return Err(invalid(
                "An accepted summary dependency differs from the reviewed basis.",
            ));
        }
    }
    Ok(())
}

pub fn eligible(set: &ReviewedSummarySet, audience: Audience) -> bool {
    audience == Audience::AuthorRoom || set.summary.audience == SummaryAudience::Reader
}

pub fn coverage(set: &ReviewedSummarySet) -> ReviewedSummaryCoverage {
    ReviewedSummaryCoverage {
        source_handle: set.source_handle.clone(),
        bundle_id: set.bundle_id.clone(),
        summary_id: set.summary.id.clone(),
        summary_hash: set.summary_hash.clone(),
    }
}
