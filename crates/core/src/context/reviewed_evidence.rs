//! Frozen, passage-backed author-reviewed story evidence.
//!
//! The project owner authenticates a complete record set against an immutable
//! reviewed bundle before it reaches this module.  These checks keep the
//! frozen source handle, policy boundary, canonical record hash, and exact
//! quotation relationship intact while the packet compiler chooses a bounded
//! representation.

use super::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, InformationPolicy, SourceKind, SourceRef,
    StorySnapshot,
};
use crate::projects::story_context::SourceRead;
use crate::projects::story_records::{EvidenceAudience, PossessionRecord};
use wns_kernel::{CoreError, CoreResult};
use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_REVIEWED_EVIDENCE_RECORDS: usize = 64;
pub const MAX_REVIEWED_EVIDENCE_BYTES: usize = 64 * 1024;

/// A complete immutable record set selected from one reviewed bundle and one
/// exact saved source. The array is provenance, not a claim that every record
/// is permitted in every audience. Restricted packets project it to reader
/// records before serialization and report only aggregate disclosure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEvidenceSet {
    pub project_id: String,
    pub operation_namespace: String,
    pub bundle_id: String,
    pub records_hash: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub records: Vec<PossessionRecord>,
}

/// Build the frozen sidecar from the storage owner's authenticated current or
/// historical record-set row.  The caller supplies the exact source handle
/// resolved into the same frozen source manifest.
pub fn from_storage_parts(
    project_id: String,
    operation_namespace: String,
    bundle_id: String,
    records_hash: String,
    source_handle: String,
    source: SourceRef,
    records: Vec<PossessionRecord>,
) -> CoreResult<ReviewedEvidenceSet> {
    let set = ReviewedEvidenceSet {
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

/// Receipt coverage for the records that actually reached the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEvidenceCoverage {
    pub source_handle: String,
    pub bundle_id: String,
    pub records_hash: String,
    /// Fingerprint of the complete policy-eligible projection before budget
    /// packing. AuthorRoom equals `records_hash`; Restricted uses the
    /// reader-only projection while retaining the complete-set fingerprint
    /// above as provenance.
    pub projection_hash: String,
    pub complete_record_set: bool,
    pub record_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ReviewedEvidenceOmissionReason {
    Budget,
    Disclosure,
}

/// Omitted records are aggregated per set and reason.  This keeps private
/// record identifiers out of RestrictedWriting packets and inspector data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEvidenceOmission {
    pub source_handle: String,
    pub bundle_id: String,
    pub records_hash: String,
    pub reason: ReviewedEvidenceOmissionReason,
    pub count: usize,
}

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidReviewedEvidence", &detail.into())
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

/// Hash the complete ordered record array using the same canonical serde
/// representation persisted by the reviewed-story owner.
pub fn records_hash(records: &[PossessionRecord]) -> CoreResult<String> {
    let encoded = serde_json::to_vec(records)?;
    if encoded.len() > MAX_REVIEWED_EVIDENCE_BYTES {
        return Err(invalid("The reviewed evidence set exceeds 64 KiB."));
    }
    Ok(sha256_hex(&encoded))
}

/// Validate frozen identity and policy relationships without consulting
/// mutable storage.  Storage separately authenticates that the bundle is the
/// selected current bundle (or the historical bundle of an old snapshot).
pub fn validate_frozen_evidence_set(
    set: &ReviewedEvidenceSet,
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> CoreResult<()> {
    valid_id(&set.project_id, "Evidence projectId")?;
    valid_id(&set.operation_namespace, "Evidence operationNamespace")?;
    valid_id(&set.bundle_id, "Evidence bundleId")?;
    valid_id(&set.source_handle, "Evidence sourceHandle")?;
    valid_hash(&set.records_hash, "Evidence recordsHash")?;
    if set.project_id != snapshot.project_id || set.source.project_id != snapshot.project_id {
        return Err(invalid("Reviewed evidence belongs to another project."));
    }
    if set.records.is_empty() || set.records.len() > MAX_REVIEWED_EVIDENCE_RECORDS {
        return Err(invalid(
            "A frozen reviewed evidence set must contain 1 to 64 records.",
        ));
    }
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The reviewed evidence record-set fingerprint is invalid.",
        ));
    }
    let descriptor = snapshot
        .sources
        .iter()
        .find(|source| source.handle == set.source_handle)
        .ok_or_else(|| invalid("The reviewed evidence source is outside the frozen manifest."))?;
    if descriptor.source != set.source
        || !descriptor.current
        || descriptor.coverage != CoverageLabel::Verbatim
    {
        return Err(invalid(
            "Reviewed evidence is not bound to its exact current source.",
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
                invalid("Reviewed evidence needs an exact reviewed basis manifest.")
            })?;
            let member = manifest
                .prefix
                .iter()
                .find(|member| {
                    member.bundle_id == set.bundle_id
                        && member.document_id == set.source.document_id
                        && member.revision_id == set.source.revision_id
                        && member.body_hash == set.source.body_hash
                })
                .ok_or_else(|| {
                    invalid("Reviewed evidence is not a member of the frozen reviewed prefix.")
                })?;
            let _ = member;
        }
        _ => {
            return Err(invalid(
                "Reviewed evidence is unavailable for this basis, audience, or context purpose.",
            ));
        }
    }
    validate_record_ids(&set.records)?;
    Ok(())
}

fn validate_record_ids(records: &[PossessionRecord]) -> CoreResult<()> {
    let mut ids = HashSet::with_capacity(records.len());
    for record in records {
        valid_id(&record.id, "Reviewed evidence recordId")?;
        if !ids.insert(&record.id) {
            return Err(invalid("Reviewed evidence record IDs must be unique."));
        }
    }
    Ok(())
}

/// Re-check every evidence anchor against the exact source read before any
/// budget branch.  This is a pure packet-side guard; storage performs the
/// stronger historical/current bundle authentication.
pub fn validate_evidence_payload(set: &ReviewedEvidenceSet, source: &SourceRead) -> CoreResult<()> {
    if set.source_handle != source.descriptor.handle || set.source != source.descriptor.source {
        return Err(invalid("Reviewed evidence does not match its source read."));
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
            .ok_or_else(|| invalid("Reviewed evidence points outside the exact source blocks."))?;
        let quote = utf16_slice(text, record.evidence.from_utf16, record.evidence.to_utf16)
            .ok_or_else(|| invalid("Reviewed evidence has an invalid UTF-16 range."))?;
        if quote != record.evidence.quote
            || sha256_hex(record.evidence.quote.as_bytes())
                != record.evidence.quote_hash.to_ascii_lowercase()
        {
            return Err(invalid(
                "Reviewed evidence quotation does not match its source.",
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

/// Extract the serialized evidence audience for packet-side diagnostics while
/// keeping the context module independent from storage's record-table schema.
pub fn record_audience(record: &PossessionRecord) -> Option<&'static str> {
    match record.audience {
        EvidenceAudience::AuthorRoom => Some("authorRoom"),
        EvidenceAudience::Reader => Some("reader"),
    }
}

/// Return a record identifier without exposing storage internals to packet
/// receipt construction.
pub fn record_id(record: &PossessionRecord) -> &str {
    &record.id
}

/// Return the records permitted in a model representation for one audience.
/// Callers retain the complete authenticated set and its full hash separately.
pub fn eligible_records(
    records: &[PossessionRecord],
    audience: Audience,
) -> Vec<&PossessionRecord> {
    if audience == Audience::RestrictedWriting {
        records
            .iter()
            .filter(|record| record.audience == EvidenceAudience::Reader)
            .collect()
    } else {
        records.iter().collect()
    }
}

/// Return the source descriptor's immutable identity for storage-side binding.
pub fn source_ref(set: &ReviewedEvidenceSet) -> &SourceRef {
    &set.source
}

/// Ensure a serialized evidence set is a complete candidate before it is
/// attached to a frozen context. This intentionally does not infer current
/// possession or claim exhaustive story coverage.
pub fn validate_complete_set(set: &ReviewedEvidenceSet) -> CoreResult<()> {
    if set.records.is_empty() {
        return Err(invalid(
            "An empty record set must be represented by omission.",
        ));
    }
    if records_hash(&set.records)? != set.records_hash.to_ascii_lowercase() {
        return Err(invalid(
            "The complete reviewed record set failed its hash check.",
        ));
    }
    validate_record_ids(&set.records)
}
