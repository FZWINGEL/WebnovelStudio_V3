//! Immutable author-reviewed narrative summaries.
//!
//! A summary is a small, source-bound author-room/reader aid.  It is never a
//! replacement for the reviewed chapter revision and it is never mutable in
//! place: a new author decision creates a new immutable summary revision.

use super::reviewed_story::ReviewPrefixItem;
use super::{CoreError, CoreResult, check_id};
use crate::context::SourceRef;
use crate::sha256_hex;
use serde::{Deserialize, Serialize};

pub const MAX_SUMMARY_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SummaryAudience {
    AuthorRoom,
    Reader,
}

/// An explicit author decision about the summary attached to a staged review.
/// The optional field on [`super::reviewed_story::StageAuthorReview`] is
/// omitted when absent so legacy request payload hashes remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SummaryChange {
    Set {
        text: String,
        audience: SummaryAudience,
    },
    Clear,
}

/// Immutable summary text bound to one exact staged chapter revision and its
/// exact earlier reviewed prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SummaryRevision {
    pub id: String,
    pub text: String,
    pub audience: SummaryAudience,
    pub source: SourceRef,
    pub dependencies: Vec<ReviewPrefixItem>,
}

pub(crate) fn validate_summary_text(text: &str) -> CoreResult<()> {
    if text.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidReviewedSummary",
            "A reviewed summary must contain nonblank text.",
        ));
    }
    if text.len() > MAX_SUMMARY_BYTES {
        return Err(CoreError::new(
            "InvalidReviewedSummary",
            "A reviewed summary must be at most 16 KiB of UTF-8 text.",
        ));
    }
    if text
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(CoreError::new(
            "InvalidReviewedSummary",
            "A reviewed summary may contain newlines and tabs but no other control characters.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_summary_binding(
    summary: &SummaryRevision,
    expected_project_id: &str,
    expected_source: &SourceRef,
    expected_dependencies: &[ReviewPrefixItem],
) -> CoreResult<()> {
    check_id(&summary.id)?;
    validate_summary_text(&summary.text)?;
    check_id(&summary.source.project_id)?;
    check_id(&summary.source.document_id)?;
    check_id(&summary.source.revision_id)?;
    if summary.source.project_id != expected_project_id
        || summary.source != *expected_source
        || summary.dependencies != expected_dependencies
    {
        return Err(CoreError::new(
            "InvalidReviewedSummary",
            "The reviewed summary does not match its immutable source or prefix.",
        ));
    }
    Ok(())
}

pub(crate) fn canonical_summary_json(summary: &SummaryRevision) -> CoreResult<String> {
    serde_json::to_string(summary).map_err(CoreError::from)
}

pub(crate) fn summary_hash(summary: &SummaryRevision) -> CoreResult<String> {
    Ok(sha256_hex(canonical_summary_json(summary)?.as_bytes()))
}
