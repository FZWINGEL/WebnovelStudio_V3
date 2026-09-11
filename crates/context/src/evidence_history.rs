//! Pure, frozen-snapshot queries over author-reviewed possession evidence.
//!
//! This module reports observations that were explicitly recorded against a
//! source passage. It never turns labels into identities, orders records into
//! an inferred transfer, or claims a current holder. Restricted writing uses
//! the reviewed-evidence projection before matching an object so private
//! records cannot affect object labels, observations, or counts.

use super::SourceRef;
use super::reviewed_evidence::{eligible_records, validate_frozen_evidence_set};
use crate::frozen::FrozenContext;
use crate::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionTiming, StoryEntityRef,
};
use wns_kernel::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The bounded result of looking up one project-local object identity in a
/// frozen context. An empty result means that this frozen evidence did not
/// contain an observation; it does not mean the object never existed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceHistory {
    pub object_id: String,
    pub label_variants: Vec<String>,
    pub observations: Vec<EvidenceHistoryObservation>,
    pub uncertainty: Vec<EvidenceHistoryUncertainty>,
    /// Always true: reviewed observations are bounded evidence, never an
    /// exhaustive history. Excluded or private material is also reflected in
    /// this status and in the corresponding uncertainty variants.
    pub incomplete: bool,
}

/// One exact author-reviewed observation, retained in frozen source order.
/// The holder is the recorded value and is intentionally not a current-owner
/// assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceHistoryObservation {
    pub record_id: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub source_display_name: String,
    pub source_order: u32,
    pub object: StoryEntityRef,
    pub holder: Option<StoryEntityRef>,
    pub timing: PossessionTiming,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

/// Conditions that keep the observation list from supporting a stronger
/// conclusion. These are status markers only; no variant asserts a current
/// holder or an inferred transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceHistoryUncertainty {
    DisclosureLimited,
    ExcludedSources,
    EarlierTiming,
    UnknownTiming,
    UnknownHolder,
    DifferingHolders,
}

fn invalid_query(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidEvidenceHistoryQuery", &detail.into())
}

fn validate_object_id(object_id: &str) -> CoreResult<()> {
    if object_id.is_empty()
        || object_id.len() > 64
        || !object_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid_query(
            "The evidence-history objectId must be a 1–64 character ASCII identifier.",
        ));
    }
    Ok(())
}

/// Query one explicit object identity from the authenticated frozen evidence
/// sidecar. The source manifest determines the order; set insertion order is
/// never treated as story chronology.
pub fn query_evidence_history(
    frozen: &FrozenContext,
    object_id: &str,
) -> CoreResult<EvidenceHistory> {
    validate_object_id(object_id)?;

    let mut sources = HashMap::with_capacity(frozen.snapshot.sources.len());
    for (index, descriptor) in frozen.snapshot.sources.iter().enumerate() {
        if sources
            .insert(descriptor.handle.as_str(), (index, descriptor))
            .is_some()
        {
            return Err(invalid_query(
                "The frozen source manifest contains duplicate source handles.",
            ));
        }
    }

    let mut observations = Vec::new();
    let mut disclosure_limited = false;
    for set in &frozen.reviewed_evidence {
        // Validate every set, including one that does not match this object,
        // so an invalid unrelated sidecar cannot be used as a query oracle.
        validate_frozen_evidence_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;

        let (source_index, descriptor) = sources
            .get(set.source_handle.as_str())
            .copied()
            .ok_or_else(|| invalid_query("The evidence source is outside the frozen manifest."))?;
        let source_order = source_order(frozen, set, descriptor, source_index)?;
        let permitted = eligible_records(&set.records, frozen.policy.audience);
        // This is a snapshot-wide disclosure status. We intentionally do not
        // inspect private rows before the audience projection or associate
        // their count with the requested object.
        disclosure_limited |= permitted.len() != set.records.len();

        for (record_order, record) in permitted.into_iter().enumerate() {
            if record.object.id != object_id {
                continue;
            }
            observations.push((
                source_order.key.clone(),
                record_order,
                EvidenceHistoryObservation {
                    record_id: record.id.clone(),
                    source_handle: set.source_handle.clone(),
                    source: set.source.clone(),
                    source_display_name: descriptor.display_name.clone(),
                    source_order: source_order.output,
                    object: record.object.clone(),
                    holder: record.holder.clone(),
                    timing: record.timing,
                    audience: record.audience,
                    evidence: record.evidence.clone(),
                },
            ));
        }
    }

    observations
        .sort_by_key(|(source_order, record_order, _)| (source_order.clone(), *record_order));
    let observations: Vec<_> = observations
        .into_iter()
        .map(|(_, _, observation)| observation)
        .collect();

    let mut label_variants = Vec::new();
    let mut labels = HashSet::new();
    let mut holder_ids = HashSet::new();
    let mut uncertainty = Vec::new();
    for observation in &observations {
        if labels.insert(observation.object.label.clone()) {
            label_variants.push(observation.object.label.clone());
        }
        match observation.timing {
            PossessionTiming::AtPassage => {}
            PossessionTiming::Earlier => {
                push_uncertainty(&mut uncertainty, EvidenceHistoryUncertainty::EarlierTiming)
            }
            PossessionTiming::Unknown => {
                push_uncertainty(&mut uncertainty, EvidenceHistoryUncertainty::UnknownTiming)
            }
        }
        if let Some(holder) = &observation.holder {
            holder_ids.insert(holder.id.as_str());
        } else {
            push_uncertainty(&mut uncertainty, EvidenceHistoryUncertainty::UnknownHolder);
        }
    }
    if holder_ids.len() > 1 {
        push_uncertainty(
            &mut uncertainty,
            EvidenceHistoryUncertainty::DifferingHolders,
        );
    }
    if disclosure_limited {
        push_uncertainty(
            &mut uncertainty,
            EvidenceHistoryUncertainty::DisclosureLimited,
        );
    }
    if frozen.excluded_source_count > 0 {
        push_uncertainty(
            &mut uncertainty,
            EvidenceHistoryUncertainty::ExcludedSources,
        );
    }

    Ok(EvidenceHistory {
        object_id: object_id.to_owned(),
        label_variants,
        observations,
        uncertainty,
        incomplete: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SourceOrder {
    /// Reviewed prefixes use their immutable manifest position. Other
    /// working snapshots use the canonical reader position when present.
    key: (u8, u64, String, usize),
    output: u32,
}

fn source_order(
    frozen: &FrozenContext,
    set: &super::reviewed_evidence::ReviewedEvidenceSet,
    descriptor: &super::SourceDescriptor,
    source_index: usize,
) -> CoreResult<SourceOrder> {
    if let Some(manifest) = &frozen.snapshot.reviewed_basis
        && let Some((prefix_index, _)) = manifest.prefix.iter().enumerate().find(|(_, member)| {
            member.bundle_id == set.bundle_id
                && member.document_id == descriptor.source.document_id
                && member.revision_id == descriptor.source.revision_id
                && member.body_hash == descriptor.source.body_hash
        })
    {
        let ordinal = u64::try_from(prefix_index)
            .map_err(|_| invalid_query("The reviewed prefix is too large."))?;
        let output = u32::try_from(prefix_index)
            .map_err(|_| invalid_query("The reviewed prefix is too large."))?;
        return Ok(SourceOrder {
            key: (
                0,
                ordinal,
                descriptor.source.document_id.clone(),
                source_index,
            ),
            output,
        });
    }

    if let Some(position) = descriptor
        .disclosure
        .reader_position
        .as_deref()
        .and_then(parse_position)
    {
        let output =
            u32::try_from(position).unwrap_or(u32::try_from(source_index).unwrap_or(u32::MAX));
        return Ok(SourceOrder {
            key: (
                0,
                position,
                descriptor.source.document_id.clone(),
                source_index,
            ),
            output,
        });
    }

    let output = u32::try_from(source_index)
        .map_err(|_| invalid_query("The frozen source manifest is too large."))?;
    Ok(SourceOrder {
        key: (1, 0, descriptor.source.document_id.clone(), source_index),
        output,
    })
}

fn parse_position(value: &str) -> Option<u64> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn push_uncertainty(
    uncertainty: &mut Vec<EvidenceHistoryUncertainty>,
    value: EvidenceHistoryUncertainty,
) {
    if !uncertainty.contains(&value) {
        uncertainty.push(value);
    }
}
