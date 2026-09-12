//! Frozen, passage-backed author-reviewed character knowledge.
//!
//! A knowledge set is an authenticated sidecar selected from one reviewed
//! bundle.  It records what a character was marked as knowing, believing,
//! suspecting, rejecting, or not knowing at a cited passage.  It is never
//! treated as a world-fact table and never grants access to the surrounding
//! prose.

use super::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, InformationPolicy, SourceKind, SourceRef,
    StorySnapshot,
};
use crate::frozen::SourceRead;
use crate::story_records::{EvidenceAudience, KnowledgeRecord, MAX_KNOWLEDGE_STATEMENT_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use wns_kernel::sha256_hex;
use wns_kernel::{CoreError, CoreResult};

pub const MAX_REVIEWED_KNOWLEDGE_RECORDS: usize = 64;
pub const MAX_REVIEWED_KNOWLEDGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedKnowledgeSet {
    pub project_id: String,
    pub operation_namespace: String,
    pub bundle_id: String,
    pub records_hash: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub records: Vec<KnowledgeRecord>,
}

pub type ReviewedKnowledgeCoverage = super::reviewed_evidence::ReviewedEvidenceCoverage;
pub type ReviewedKnowledgeOmission = super::reviewed_evidence::ReviewedEvidenceOmission;
pub type ReviewedKnowledgeOmissionReason = super::reviewed_evidence::ReviewedEvidenceOmissionReason;

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidReviewedKnowledge", &detail.into())
}

fn valid_id(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid(format!(
            "{label} must be a 1–256 character ASCII identifier."
        )));
    }
    Ok(())
}

fn valid_record_id(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid(format!(
            "{label} must be a 1–64 character ASCII identifier."
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

fn validate_record_shape(records: &[KnowledgeRecord]) -> CoreResult<()> {
    if records.is_empty() || records.len() > MAX_REVIEWED_KNOWLEDGE_RECORDS {
        return Err(invalid(
            "A frozen reviewed knowledge set must contain 1 to 64 records.",
        ));
    }
    let mut ids = HashSet::with_capacity(records.len());
    for record in records {
        valid_record_id(&record.id, "Reviewed knowledge recordId")?;
        if !ids.insert(&record.id) {
            return Err(invalid("Reviewed knowledge record IDs must be unique."));
        }
        valid_id(&record.character.id, "Knowledge character identity")?;
        valid_id(&record.topic.id, "Knowledge topic identity")?;
        if record.character.label.trim().is_empty()
            || record.topic.label.trim().is_empty()
            || record.character.label.len() > 160
            || record.topic.label.len() > 160
            || record.character.label.chars().any(char::is_control)
            || record.topic.label.chars().any(char::is_control)
        {
            return Err(invalid("Knowledge entity labels are invalid."));
        }
        if record.statement.trim().is_empty()
            || record.statement.len() > MAX_KNOWLEDGE_STATEMENT_BYTES
            || record.statement.chars().any(char::is_control)
        {
            return Err(invalid(
                "Knowledge statements must be nonblank, at most 1024 UTF-8 bytes, and contain no control characters.",
            ));
        }
    }
    Ok(())
}

pub fn from_storage_parts(
    project_id: String,
    operation_namespace: String,
    bundle_id: String,
    records_hash: String,
    source_handle: String,
    source: SourceRef,
    records: Vec<KnowledgeRecord>,
) -> CoreResult<ReviewedKnowledgeSet> {
    let set = ReviewedKnowledgeSet {
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

pub fn records_hash(records: &[KnowledgeRecord]) -> CoreResult<String> {
    let encoded = serde_json::to_vec(records)?;
    if encoded.len() > MAX_REVIEWED_KNOWLEDGE_BYTES {
        return Err(invalid("The reviewed knowledge set exceeds 64 KiB."));
    }
    Ok(sha256_hex(&encoded))
}

pub fn validate_frozen_knowledge_set(
    set: &ReviewedKnowledgeSet,
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> CoreResult<()> {
    valid_id(&set.project_id, "Knowledge projectId")?;
    valid_id(&set.operation_namespace, "Knowledge operationNamespace")?;
    valid_id(&set.bundle_id, "Knowledge bundleId")?;
    valid_id(&set.source_handle, "Knowledge sourceHandle")?;
    valid_hash(&set.records_hash, "Knowledge recordsHash")?;
    if set.project_id != snapshot.project_id || set.source.project_id != snapshot.project_id {
        return Err(invalid("Reviewed knowledge belongs to another project."));
    }
    validate_record_shape(&set.records)?;
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The reviewed knowledge record-set fingerprint is invalid.",
        ));
    }
    let descriptor = snapshot
        .sources
        .iter()
        .find(|source| source.handle == set.source_handle)
        .ok_or_else(|| invalid("The reviewed knowledge source is outside the frozen manifest."))?;
    if descriptor.source != set.source
        || !descriptor.current
        || descriptor.coverage != CoverageLabel::Verbatim
    {
        return Err(invalid(
            "Reviewed knowledge is not bound to its exact current source.",
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
                invalid("Reviewed knowledge needs an exact reviewed basis manifest.")
            })?;
            if set.operation_namespace != manifest.operation_namespace {
                return Err(invalid(
                    "Reviewed knowledge belongs to another operation namespace.",
                ));
            }
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
                    invalid("Reviewed knowledge is not a member of the frozen reviewed prefix.")
                })?;
        }
        _ => {
            return Err(invalid(
                "Reviewed knowledge is unavailable for this basis, audience, or context purpose.",
            ));
        }
    }
    Ok(())
}

/// Re-check every quotation against the exact source read before packet
/// budgeting or history rendering.  This deliberately does not infer the
/// statement from the quotation or merge labels into identities.
pub fn validate_knowledge_payload(
    set: &ReviewedKnowledgeSet,
    source: &SourceRead,
) -> CoreResult<()> {
    if set.source_handle != source.descriptor.handle || set.source != source.descriptor.source {
        return Err(invalid(
            "Reviewed knowledge does not match its source read.",
        ));
    }
    validate_record_shape(&set.records)?;
    let blocks: std::collections::HashMap<&str, &str> = source
        .passages
        .iter()
        .map(|passage| (passage.block_id.as_str(), passage.text.as_str()))
        .collect();
    for record in &set.records {
        let text = blocks
            .get(record.evidence.block_id.as_str())
            .ok_or_else(|| invalid("Reviewed knowledge points outside exact source blocks."))?;
        let quote = utf16_slice(text, record.evidence.from_utf16, record.evidence.to_utf16)
            .ok_or_else(|| invalid("Reviewed knowledge has an invalid UTF-16 range."))?;
        if quote != record.evidence.quote
            || sha256_hex(record.evidence.quote.as_bytes())
                != record.evidence.quote_hash.to_ascii_lowercase()
        {
            return Err(invalid(
                "Reviewed knowledge quotation does not match its source.",
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

pub fn eligible_records(records: &[KnowledgeRecord], audience: Audience) -> Vec<&KnowledgeRecord> {
    if audience == Audience::RestrictedWriting {
        records
            .iter()
            .filter(|record| record.audience == EvidenceAudience::Reader)
            .collect()
    } else {
        records.iter().collect()
    }
}

pub fn source_ref(set: &ReviewedKnowledgeSet) -> &SourceRef {
    &set.source
}

pub fn validate_complete_set(set: &ReviewedKnowledgeSet) -> CoreResult<()> {
    if set.records.is_empty() {
        return Err(invalid(
            "An empty knowledge set must be represented by omission.",
        ));
    }
    validate_record_shape(&set.records)?;
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The complete reviewed knowledge set failed its hash check.",
        ));
    }
    Ok(())
}
