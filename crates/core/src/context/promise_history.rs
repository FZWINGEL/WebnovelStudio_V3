//! Pure, frozen-snapshot queries over author-reviewed promise observations.
//!
//! This query reports exact recorded setup, payoff, cancellation, and
//! uncertainty observations.  It never infers that a promise is resolved,
//! current, or absent merely because a payoff was not found in the permitted
//! records.

use super::reviewed_promises::{ReviewedPromiseSet, validate_frozen_promise_set};
use super::{SourceDescriptor, SourceRef};
use crate::projects::story_context::FrozenContext;
use crate::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionTiming, PromisePhase, StoryEntityRef,
};
use crate::projects::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromiseHistory {
    pub promise_id: String,
    pub label_variants: Vec<String>,
    pub observations: Vec<PromiseHistoryObservation>,
    pub uncertainty: Vec<PromiseHistoryUncertainty>,
    /// Promise observations are bounded evidence, never an exhaustive index.
    pub incomplete: bool,
    /// True only when an eligible, exact record explicitly uses the Payoff
    /// phase.  A false value is not evidence that no payoff exists.
    pub has_recorded_payoff: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromiseHistoryObservation {
    pub record_id: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub source_display_name: String,
    pub source_order: u32,
    pub promise: StoryEntityRef,
    pub phase: PromisePhase,
    pub timing: PossessionTiming,
    pub note: String,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum PromiseHistoryUncertainty {
    DisclosureLimited,
    ExcludedSources,
    EarlierTiming,
    UnknownTiming,
    UnclearObservation,
    ConflictingOutcomes,
}

fn invalid_query(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidPromiseHistoryQuery", &detail.into())
}

fn validate_promise_id(promise_id: &str) -> CoreResult<()> {
    if promise_id.is_empty()
        || promise_id.len() > 64
        || !promise_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid_query(
            "The promise-history promiseId must be a 1–64 character ASCII identifier.",
        ));
    }
    Ok(())
}

/// Query one opaque promise identity from authenticated frozen evidence.
/// Source order is the frozen reviewed-prefix or reader-position order; the
/// order of records inside a chapter is retained and is never interpreted as
/// fictional chronology.
pub fn query_promise_history(
    frozen: &FrozenContext,
    promise_id: &str,
) -> CoreResult<PromiseHistory> {
    validate_promise_id(promise_id)?;

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
    let mut has_recorded_payoff = false;
    let mut has_cancelled = false;
    for set in &frozen.reviewed_promises {
        // Validate every set, including unrelated promises, so an invalid
        // sidecar cannot become a query oracle.
        validate_frozen_promise_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        let (source_index, descriptor) = sources
            .get(set.source_handle.as_str())
            .copied()
            .ok_or_else(|| invalid_query("The promise source is outside the frozen manifest."))?;
        let source_order = source_order(frozen, set, descriptor, source_index)?;
        let restricted = frozen.policy.audience == super::Audience::RestrictedWriting;
        let permitted_count = set
            .records
            .iter()
            .filter(|record| !restricted || record.audience == EvidenceAudience::Reader)
            .count();
        disclosure_limited |= permitted_count != set.records.len();

        // Keep the record's original position in the authenticated bundle.
        // Filtering private observations must not renumber later reader-visible
        // observations, because this order is part of the evidence trace.
        for (record_order, record) in set
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| !restricted || record.audience == EvidenceAudience::Reader)
        {
            if record.promise.id != promise_id {
                continue;
            }
            has_recorded_payoff |= record.phase == PromisePhase::Payoff;
            has_cancelled |= record.phase == PromisePhase::Cancelled;
            observations.push((
                source_order.key.clone(),
                record_order,
                PromiseHistoryObservation {
                    record_id: record.id.clone(),
                    source_handle: set.source_handle.clone(),
                    source: set.source.clone(),
                    source_display_name: descriptor.display_name.clone(),
                    source_order: source_order.output,
                    promise: record.promise.clone(),
                    phase: record.phase,
                    timing: record.timing,
                    note: record.note.clone(),
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
    let mut uncertainty = Vec::new();
    for observation in &observations {
        if labels.insert(observation.promise.label.clone()) {
            label_variants.push(observation.promise.label.clone());
        }
        match observation.timing {
            PossessionTiming::AtPassage => {}
            PossessionTiming::Earlier => {
                push_uncertainty(&mut uncertainty, PromiseHistoryUncertainty::EarlierTiming)
            }
            PossessionTiming::Unknown => {
                push_uncertainty(&mut uncertainty, PromiseHistoryUncertainty::UnknownTiming)
            }
        }
        if observation.phase == PromisePhase::Unclear {
            push_uncertainty(
                &mut uncertainty,
                PromiseHistoryUncertainty::UnclearObservation,
            );
        }
    }
    if has_recorded_payoff && has_cancelled {
        push_uncertainty(
            &mut uncertainty,
            PromiseHistoryUncertainty::ConflictingOutcomes,
        );
    }
    if disclosure_limited {
        push_uncertainty(
            &mut uncertainty,
            PromiseHistoryUncertainty::DisclosureLimited,
        );
    }
    if frozen.excluded_source_count > 0 {
        push_uncertainty(&mut uncertainty, PromiseHistoryUncertainty::ExcludedSources);
    }

    Ok(PromiseHistory {
        promise_id: promise_id.to_owned(),
        label_variants,
        observations,
        uncertainty,
        incomplete: true,
        has_recorded_payoff,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SourceOrder {
    key: (u8, u64, String, usize),
    output: u32,
}

fn source_order(
    frozen: &FrozenContext,
    set: &ReviewedPromiseSet,
    descriptor: &SourceDescriptor,
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
    uncertainty: &mut Vec<PromiseHistoryUncertainty>,
    value: PromiseHistoryUncertainty,
) {
    if !uncertainty.contains(&value) {
        uncertainty.push(value);
    }
}
