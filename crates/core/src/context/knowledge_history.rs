//! Pure, frozen-snapshot queries over reviewed character-knowledge records.
//!
//! The result is intentionally a bounded observation history.  It preserves
//! attitudes and exact evidence, reports disclosure/coverage uncertainty, and
//! never turns missing observations or differing statements into a definitive
//! mental state.

use super::reviewed_knowledge::{
    ReviewedKnowledgeSet, eligible_records, validate_frozen_knowledge_set,
};
use super::{SourceRef, StorySnapshot};
use crate::projects::story_context::FrozenContext;
use crate::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, KnowledgeAttitude, KnowledgeRecord, PossessionTiming,
    StoryEntityRef,
};
use crate::projects::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeHistory {
    pub character_id: String,
    pub topic_id: Option<String>,
    pub label_variants: Vec<String>,
    pub observations: Vec<KnowledgeHistoryObservation>,
    pub uncertainty: Vec<KnowledgeHistoryUncertainty>,
    pub incomplete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeHistoryObservation {
    pub record_id: String,
    pub source_handle: String,
    pub source: SourceRef,
    pub source_display_name: String,
    pub source_order: u32,
    pub character: StoryEntityRef,
    pub topic: StoryEntityRef,
    pub attitude: KnowledgeAttitude,
    pub statement: String,
    pub timing: PossessionTiming,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeHistoryUncertainty {
    NoEligibleObservations,
    EarlierOrUnknownTiming,
    MultipleRecordedAttitudes,
    DisclosureLimited,
}

fn invalid_query(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidKnowledgeHistoryQuery", &detail.into())
}

fn validate_id(value: &str, field: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid_query(format!(
            "The knowledge-history {field} must be a 1–64 character ASCII identifier."
        )));
    }
    Ok(())
}

/// Query one character identity, optionally narrowed to one topic identity,
/// from authenticated frozen reviewed knowledge.
pub fn query_knowledge_history(
    frozen: &FrozenContext,
    character_id: &str,
    topic_id: Option<&str>,
) -> CoreResult<KnowledgeHistory> {
    validate_id(character_id, "characterId")?;
    if let Some(topic_id) = topic_id {
        validate_id(topic_id, "topicId")?;
    }

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
    for set in &frozen.reviewed_knowledge {
        validate_frozen_knowledge_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
        let (source_index, descriptor) = sources
            .get(set.source_handle.as_str())
            .copied()
            .ok_or_else(|| invalid_query("The knowledge source is outside the frozen manifest."))?;
        let source_order = source_order(frozen, set, descriptor, source_index)?;
        let permitted = eligible_records(&set.records, frozen.policy.audience);
        disclosure_limited |= permitted.len() != set.records.len();
        for (record_order, record) in permitted.into_iter().enumerate() {
            if record.character.id != character_id
                || topic_id.is_some_and(|topic| record.topic.id != topic)
            {
                continue;
            }
            observations.push((
                source_order.key.clone(),
                record_order,
                KnowledgeHistoryObservation {
                    record_id: record.id.clone(),
                    source_handle: set.source_handle.clone(),
                    source: set.source.clone(),
                    source_display_name: descriptor.display_name.clone(),
                    source_order: source_order.output,
                    character: record.character.clone(),
                    topic: record.topic.clone(),
                    attitude: record.attitude,
                    statement: record.statement.clone(),
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
    let mut attitudes = HashSet::new();
    let mut uncertainty = Vec::new();
    for observation in &observations {
        if labels.insert(observation.character.label.clone()) {
            label_variants.push(observation.character.label.clone());
        }
        attitudes.insert(observation.attitude);
        if !matches!(observation.timing, PossessionTiming::AtPassage) {
            push_uncertainty(
                &mut uncertainty,
                KnowledgeHistoryUncertainty::EarlierOrUnknownTiming,
            );
        }
    }
    if attitudes.len() > 1 {
        push_uncertainty(
            &mut uncertainty,
            KnowledgeHistoryUncertainty::MultipleRecordedAttitudes,
        );
    }
    if observations.is_empty() {
        push_uncertainty(
            &mut uncertainty,
            KnowledgeHistoryUncertainty::NoEligibleObservations,
        );
    }
    if disclosure_limited {
        push_uncertainty(
            &mut uncertainty,
            KnowledgeHistoryUncertainty::DisclosureLimited,
        );
    }

    Ok(KnowledgeHistory {
        character_id: character_id.to_owned(),
        topic_id: topic_id.map(str::to_owned),
        label_variants,
        observations,
        uncertainty,
        incomplete: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SourceOrder {
    key: (u8, u64, String, usize),
    output: u32,
}

fn source_order(
    frozen: &FrozenContext,
    set: &ReviewedKnowledgeSet,
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
        return Ok(SourceOrder {
            key: (
                0,
                u64::try_from(prefix_index)
                    .map_err(|_| invalid_query("The reviewed prefix is too large."))?,
                descriptor.source.document_id.clone(),
                source_index,
            ),
            output: u32::try_from(prefix_index)
                .map_err(|_| invalid_query("The reviewed prefix is too large."))?,
        });
    }
    if let Some(position) = descriptor
        .disclosure
        .reader_position
        .as_deref()
        .and_then(parse_position)
    {
        return Ok(SourceOrder {
            key: (
                0,
                position,
                descriptor.source.document_id.clone(),
                source_index,
            ),
            output: u32::try_from(position)
                .unwrap_or(u32::try_from(source_index).unwrap_or(u32::MAX)),
        });
    }
    Ok(SourceOrder {
        key: (1, 0, descriptor.source.document_id.clone(), source_index),
        output: u32::try_from(source_index)
            .map_err(|_| invalid_query("The frozen source manifest is too large."))?,
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
    uncertainty: &mut Vec<KnowledgeHistoryUncertainty>,
    value: KnowledgeHistoryUncertainty,
) {
    if !uncertainty.contains(&value) {
        uncertainty.push(value);
    }
}

// Keep the source imports in this module explicit.  They make accidental
// future use of a mutable project or a generated digest conspicuous.
#[allow(dead_code)]
fn _source_contract(_: &StorySnapshot, _: &KnowledgeRecord) {}
