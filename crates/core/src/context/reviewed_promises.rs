//! Frozen, passage-backed author-reviewed promise observations.
//!
//! Promise observations are deliberately separate from possession records.
//! They share the same source, audience, and packet safety rules, but a
//! promise identity describes a narrative thread rather than an object.  The
//! complete record set remains authenticated by the project owner; this
//! module only validates the frozen representation and its exact evidence
//! before packet packing.

use super::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, InformationPolicy, SourceKind, SourceRef,
    StorySnapshot,
};
use crate::projects::story_context::SourceRead;
use crate::projects::story_records::{EvidenceAudience, PromiseRecord};
use crate::projects::{CoreError, CoreResult};
use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_REVIEWED_PROMISE_RECORDS: usize = 64;
pub const MAX_REVIEWED_PROMISE_BYTES: usize = 64 * 1024;
pub const MAX_PROMISE_NOTE_BYTES: usize = 1024;

/// A complete immutable promise-observation set selected from one reviewed
/// bundle and one exact saved source.  Restricted packets project this set to
/// reader-approved observations while retaining the complete hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedPromiseSet {
    pub project_id: String,
    pub operation_namespace: String,
    pub bundle_id: String,
    pub records_hash: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub records: Vec<PromiseRecord>,
}

/// Promise packets use the same receipt shape as possession evidence.  The
/// aliases keep the wire contract small while the distinct field names make
/// the two kinds of reviewed story material explicit to callers.
pub type ReviewedPromiseCoverage = super::reviewed_evidence::ReviewedEvidenceCoverage;
pub type ReviewedPromiseOmission = super::reviewed_evidence::ReviewedEvidenceOmission;
pub type ReviewedPromiseOmissionReason = super::reviewed_evidence::ReviewedEvidenceOmissionReason;

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidReviewedPromises", &detail.into())
}

fn valid_id(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty() || value.len() > 256 {
        return Err(invalid(format!(
            "{label} must be nonempty and at most 256 bytes."
        )));
    }
    Ok(())
}

fn valid_hash(value: &str, label: &str) -> CoreResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{label} must be a SHA-256 fingerprint.")));
    }
    Ok(())
}

fn valid_note(record: &PromiseRecord) -> CoreResult<()> {
    if record.note.trim().is_empty()
        || record.note.len() > MAX_PROMISE_NOTE_BYTES
        || record.note.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "Promise notes must be nonblank, at most {MAX_PROMISE_NOTE_BYTES} UTF-8 bytes, and contain no control characters."
        )));
    }
    Ok(())
}

/// Build the frozen sidecar from an authenticated storage row.
pub fn from_storage_parts(
    project_id: String,
    operation_namespace: String,
    bundle_id: String,
    records_hash: String,
    source_handle: String,
    source: SourceRef,
    records: Vec<PromiseRecord>,
) -> CoreResult<ReviewedPromiseSet> {
    let set = ReviewedPromiseSet {
        project_id,
        operation_namespace,
        bundle_id,
        records_hash,
        source_handle,
        source,
        records,
    };
    validate_complete_set(&set)?;
    Ok(set)
}

/// Hash the complete ordered promise array using its canonical serde form.
pub fn records_hash(records: &[PromiseRecord]) -> CoreResult<String> {
    let encoded = serde_json::to_vec(records)?;
    if encoded.len() > MAX_REVIEWED_PROMISE_BYTES {
        return Err(invalid("The reviewed promise set exceeds 64 KiB."));
    }
    Ok(sha256_hex(&encoded))
}

/// Validate frozen identity and policy relationships without consulting
/// mutable storage.  The owner authenticates the selected bundle separately.
pub fn validate_frozen_promise_set(
    set: &ReviewedPromiseSet,
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> CoreResult<()> {
    valid_id(&set.project_id, "Promise projectId")?;
    valid_id(&set.operation_namespace, "Promise operationNamespace")?;
    valid_id(&set.bundle_id, "Promise bundleId")?;
    valid_id(&set.source_handle, "Promise sourceHandle")?;
    valid_hash(&set.records_hash, "Promise recordsHash")?;
    if set.project_id != snapshot.project_id || set.source.project_id != snapshot.project_id {
        return Err(invalid("Reviewed promises belong to another project."));
    }
    if set.records.is_empty() || set.records.len() > MAX_REVIEWED_PROMISE_RECORDS {
        return Err(invalid(
            "A frozen reviewed promise set must contain 1 to 64 records.",
        ));
    }
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The reviewed promise record-set fingerprint is invalid.",
        ));
    }
    let descriptor = snapshot
        .sources
        .iter()
        .find(|source| source.handle == set.source_handle)
        .ok_or_else(|| invalid("The reviewed promise source is outside the frozen manifest."))?;
    if descriptor.source != set.source
        || !descriptor.current
        || descriptor.coverage != CoverageLabel::Verbatim
    {
        return Err(invalid(
            "Reviewed promises are not bound to their exact current source.",
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
            let manifest = snapshot.reviewed_basis.as_ref().ok_or_else(|| {
                invalid("Reviewed promises need an exact reviewed basis manifest.")
            })?;
            manifest
                .prefix
                .iter()
                .find(|member| {
                    member.bundle_id == set.bundle_id
                        && member.document_id == set.source.document_id
                        && member.revision_id == set.source.revision_id
                        && member.body_hash == set.source.body_hash
                })
                .ok_or_else(|| {
                    invalid("Reviewed promises are not a member of the frozen reviewed prefix.")
                })?;
        }
        _ => {
            return Err(invalid(
                "Reviewed promises are unavailable for this basis, audience, or context purpose.",
            ));
        }
    }
    validate_record_ids(&set.records)?;
    Ok(())
}

fn validate_record_ids(records: &[PromiseRecord]) -> CoreResult<()> {
    let mut ids = HashSet::with_capacity(records.len());
    for record in records {
        valid_id(&record.id, "Reviewed promise recordId")?;
        valid_id(&record.promise.id, "Promise identity")?;
        if !ids.insert(&record.id) {
            return Err(invalid("Reviewed promise record IDs must be unique."));
        }
        valid_note(record)?;
    }
    Ok(())
}

/// Re-check every promise anchor against the exact source read before any
/// budget branch is attempted.
pub fn validate_promise_payload(set: &ReviewedPromiseSet, source: &SourceRead) -> CoreResult<()> {
    if set.source_handle != source.descriptor.handle || set.source != source.descriptor.source {
        return Err(invalid("Reviewed promises do not match their source read."));
    }
    validate_record_ids(&set.records)?;
    let blocks: std::collections::HashMap<&str, &str> = source
        .passages
        .iter()
        .map(|passage| (passage.block_id.as_str(), passage.text.as_str()))
        .collect();
    for record in &set.records {
        let text = blocks
            .get(record.evidence.block_id.as_str())
            .ok_or_else(|| invalid("Reviewed promise points outside exact source blocks."))?;
        let quote = utf16_slice(text, record.evidence.from_utf16, record.evidence.to_utf16)
            .ok_or_else(|| invalid("Reviewed promise has an invalid UTF-16 range."))?;
        if quote != record.evidence.quote
            || sha256_hex(record.evidence.quote.as_bytes())
                != record.evidence.quote_hash.to_ascii_lowercase()
        {
            return Err(invalid(
                "Reviewed promise quotation does not match its source.",
            ));
        }
    }
    Ok(())
}

fn utf16_slice(text: &str, from: u32, to: u32) -> Option<&str> {
    if from >= to {
        return None;
    }
    let start = utf16_boundary(text, from)?;
    let end = utf16_boundary(text, to)?;
    text.get(start..end)
}

fn utf16_boundary(text: &str, target: u32) -> Option<usize> {
    let mut units = 0_u32;
    if target == 0 {
        return Some(0);
    }
    for (index, character) in text.char_indices() {
        if units == target {
            return Some(index);
        }
        units = units.checked_add(character.len_utf16() as u32)?;
        if units > target {
            return None;
        }
    }
    (units == target).then_some(text.len())
}

pub fn eligible_records(records: &[PromiseRecord], audience: Audience) -> Vec<&PromiseRecord> {
    if audience == Audience::RestrictedWriting {
        records
            .iter()
            .filter(|record| record.audience == EvidenceAudience::Reader)
            .collect()
    } else {
        records.iter().collect()
    }
}

pub fn source_ref(set: &ReviewedPromiseSet) -> &SourceRef {
    &set.source
}

/// Ensure a serialized promise set is complete before attaching it to a
/// frozen context. Empty sets are represented by omission.
pub fn validate_complete_set(set: &ReviewedPromiseSet) -> CoreResult<()> {
    if set.records.is_empty() {
        return Err(invalid(
            "An empty promise set must be represented by omission.",
        ));
    }
    if set.records.len() > MAX_REVIEWED_PROMISE_RECORDS {
        return Err(invalid(
            "A reviewed promise set may contain at most 64 records.",
        ));
    }
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The complete reviewed promise set failed its hash check.",
        ));
    }
    validate_record_ids(&set.records)
}
