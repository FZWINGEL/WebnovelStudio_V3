//! Deterministic, read-only lookup over authenticated frozen story memory.
//!
//! This module is deliberately independent of storage and providers.  The
//! packet owner authenticates the frozen snapshot and grants the lookup
//! capability; this engine then validates the complete reviewed sidecars and
//! derives one bounded, reproducible result from them.

use super::SourceRef;
use super::evidence_history::query_evidence_history;
use super::knowledge_history::query_knowledge_history;
use super::lookup::{
    LookupRead, LookupReadResult, MAX_MEMORY_LIMIT, MAX_MEMORY_OFFSET, MemoryEntityEntry,
    MemoryEntityKind, validate_lookup_read,
};
use super::promise_history::query_promise_history;
use super::reviewed_evidence::{ReviewedEvidenceSet, validate_frozen_evidence_set};
use super::reviewed_knowledge::{ReviewedKnowledgeSet, validate_frozen_knowledge_set};
use super::reviewed_promises::{ReviewedPromiseSet, validate_frozen_promise_set};
use crate::projects::story_context::FrozenContext;
use crate::projects::story_records::StoryEntityRef;
use crate::projects::{CoreError, CoreResult};
use std::collections::HashMap;

/// Execute one typed memory read against the immutable snapshot.
///
/// Ordinary `Search` and `Read` requests belong to the existing source
/// lookup path and return `None` here. Catalog reads validate all reviewed
/// sidecar families before returning even an empty page; history reads use
/// the typed query that validates every set in their relevant family. Packet
/// and storage callers validate the complete frozen snapshot before running
/// this engine.
pub fn execute_memory_lookup(
    frozen: &FrozenContext,
    read: &LookupRead,
) -> CoreResult<Option<LookupReadResult>> {
    validate_lookup_read(read)
        .map_err(|error| CoreError::new("InvalidLookupRead", &error.to_string()))?;

    let result = match read {
        LookupRead::Search { .. } | LookupRead::Read { .. } => return Ok(None),
        LookupRead::FindEntities {
            entity_kind,
            query,
            offset,
            limit,
            ..
        } => {
            let (entries, total_matches, next_offset) =
                find_entities(frozen, *entity_kind, query, *offset, *limit)?;
            LookupReadResult::FindEntities {
                entity_kind: *entity_kind,
                query: query.clone(),
                entries,
                offset: *offset,
                total_matches,
                next_offset,
                incomplete: true,
            }
        }
        LookupRead::KnowledgeHistory {
            character_id,
            topic_id,
            offset,
            limit,
            ..
        } => {
            let history = query_knowledge_history(frozen, character_id, topic_id.as_deref())?;
            let total = history.observations.len();
            let (start, end, next_offset) = page_bounds(*offset, *limit, total)?;
            let mut page = history;
            page.observations = page.observations[start..end].to_vec();
            LookupReadResult::KnowledgeHistory {
                history: page,
                offset: *offset,
                total_observations: checked_count(total, "knowledge observations")?,
                next_offset,
            }
        }
        LookupRead::PromiseHistory {
            promise_id,
            offset,
            limit,
            ..
        } => {
            let history = query_promise_history(frozen, promise_id)?;
            let total = history.observations.len();
            let (start, end, next_offset) = page_bounds(*offset, *limit, total)?;
            let mut page = history;
            page.observations = page.observations[start..end].to_vec();
            LookupReadResult::PromiseHistory {
                history: page,
                offset: *offset,
                total_observations: checked_count(total, "promise observations")?,
                next_offset,
            }
        }
        LookupRead::PossessionHistory {
            object_id,
            offset,
            limit,
            ..
        } => {
            let history = query_evidence_history(frozen, object_id)?;
            let total = history.observations.len();
            let (start, end, next_offset) = page_bounds(*offset, *limit, total)?;
            let mut page = history;
            page.observations = page.observations[start..end].to_vec();
            LookupReadResult::PossessionHistory {
                history: page,
                offset: *offset,
                total_observations: checked_count(total, "possession observations")?,
                next_offset,
            }
        }
    };

    Ok(Some(result))
}

fn find_entities(
    frozen: &FrozenContext,
    kind: MemoryEntityKind,
    query: &str,
    offset: u32,
    limit: u32,
) -> CoreResult<(Vec<MemoryEntityEntry>, u32, Option<u32>)> {
    let mut candidates = collect_entities(frozen, kind)?;
    let needle = query.to_lowercase();
    candidates.retain(|candidate| {
        candidate
            .labels
            .iter()
            .any(|label| label.to_lowercase().contains(&needle))
    });
    let total_matches = checked_count(candidates.len(), "entity matches")?;
    let (start, end, next_offset) = page_bounds(offset, limit, candidates.len())?;
    let entries = candidates[start..end]
        .iter()
        .map(|candidate| {
            let mut entry = candidate.entry.clone();
            entry.label_variants = candidate.labels.clone();
            entry
        })
        .collect();
    Ok((entries, total_matches, next_offset))
}

#[derive(Debug, Clone)]
struct EntityCandidate {
    entry: MemoryEntityEntry,
    labels: Vec<String>,
}

fn collect_entities(
    frozen: &FrozenContext,
    kind: MemoryEntityKind,
) -> CoreResult<Vec<EntityCandidate>> {
    let mut occurrences = Vec::new();

    // Validate every family before exposing even an empty catalog.  A query
    // must not become an oracle for a malformed unrelated sidecar.
    for set in &frozen.reviewed_evidence {
        validate_frozen_evidence_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for set in &frozen.reviewed_knowledge {
        validate_frozen_knowledge_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for set in &frozen.reviewed_promises {
        validate_frozen_promise_set(set, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }

    for (set_index, set) in frozen.reviewed_evidence.iter().enumerate() {
        let source_index = source_for_set(frozen, set)?;
        for (record_order, record) in set.records.iter().enumerate().filter(|(_, record)| {
            frozen.policy.audience != super::Audience::RestrictedWriting
                || record.audience == crate::projects::story_records::EvidenceAudience::Reader
        }) {
            let entity = match kind {
                MemoryEntityKind::Character => record.holder.as_ref(),
                MemoryEntityKind::Object => Some(&record.object),
                MemoryEntityKind::Topic | MemoryEntityKind::Promise => None,
            };
            if let Some(entity) = entity {
                occurrences.push(EntityOccurrence {
                    entity,
                    source_handle: &set.source_handle,
                    source: &set.source,
                    source_index,
                    family_order: 0,
                    set_index,
                    record_order,
                });
            }
        }
    }

    for (set_index, set) in frozen.reviewed_knowledge.iter().enumerate() {
        let source_index = source_for_set(frozen, set)?;
        for (record_order, record) in set.records.iter().enumerate().filter(|(_, record)| {
            frozen.policy.audience != super::Audience::RestrictedWriting
                || record.audience == crate::projects::story_records::EvidenceAudience::Reader
        }) {
            let entity = match kind {
                MemoryEntityKind::Character => &record.character,
                MemoryEntityKind::Topic => &record.topic,
                MemoryEntityKind::Object | MemoryEntityKind::Promise => continue,
            };
            occurrences.push(EntityOccurrence {
                entity,
                source_handle: &set.source_handle,
                source: &set.source,
                source_index,
                family_order: 1,
                set_index,
                record_order,
            });
        }
    }

    for (set_index, set) in frozen.reviewed_promises.iter().enumerate() {
        let source_index = source_for_set(frozen, set)?;
        for (record_order, record) in set.records.iter().enumerate().filter(|(_, record)| {
            frozen.policy.audience != super::Audience::RestrictedWriting
                || record.audience == crate::projects::story_records::EvidenceAudience::Reader
        }) {
            let entity = match kind {
                MemoryEntityKind::Promise => &record.promise,
                MemoryEntityKind::Character
                | MemoryEntityKind::Topic
                | MemoryEntityKind::Object => continue,
            };
            occurrences.push(EntityOccurrence {
                entity,
                source_handle: &set.source_handle,
                source: &set.source,
                source_index,
                family_order: 2,
                set_index,
                record_order,
            });
        }
    }

    occurrences.sort_by(|left, right| {
        left.source_index
            .cmp(&right.source_index)
            .then(left.family_order.cmp(&right.family_order))
            .then(left.set_index.cmp(&right.set_index))
            .then(left.record_order.cmp(&right.record_order))
            .then(left.entity.id.cmp(&right.entity.id))
    });

    let mut candidates = Vec::new();
    let mut indexes: HashMap<String, usize> = HashMap::new();
    for occurrence in occurrences {
        add_entity(&mut candidates, &mut indexes, occurrence);
    }
    Ok(candidates)
}

struct EntityOccurrence<'a> {
    entity: &'a StoryEntityRef,
    source_handle: &'a str,
    source: &'a SourceRef,
    source_index: usize,
    family_order: u8,
    set_index: usize,
    record_order: usize,
}

fn add_entity(
    candidates: &mut Vec<EntityCandidate>,
    indexes: &mut HashMap<String, usize>,
    occurrence: EntityOccurrence<'_>,
) {
    let entity = occurrence.entity;
    if let Some(index) = indexes.get(&entity.id).copied() {
        let candidate = &mut candidates[index];
        if !candidate.labels.iter().any(|label| label == &entity.label) {
            candidate.labels.push(entity.label.clone());
        }
        return;
    }
    let index = candidates.len();
    indexes.insert(entity.id.clone(), index);
    candidates.push(EntityCandidate {
        entry: MemoryEntityEntry {
            entity: entity.clone(),
            label_variants: vec![entity.label.clone()],
            source_handle: occurrence.source_handle.to_owned(),
            source: occurrence.source.to_owned(),
        },
        labels: vec![entity.label.clone()],
    });
}

fn source_for_set<S>(frozen: &FrozenContext, set: &S) -> CoreResult<usize>
where
    S: MemorySetIdentity,
{
    let (source_index, descriptor) = frozen
        .snapshot
        .sources
        .iter()
        .enumerate()
        .find(|(_, descriptor)| descriptor.handle == set.source_handle())
        .ok_or_else(|| invalid_memory("Memory source is outside the frozen manifest."))?;
    if descriptor.source != *set.source() {
        return Err(invalid_memory(
            "Memory source identity does not match its handle.",
        ));
    }
    Ok(source_index)
}

trait MemorySetIdentity {
    fn source_handle(&self) -> &str;
    fn source(&self) -> &SourceRef;
}

impl MemorySetIdentity for ReviewedEvidenceSet {
    fn source_handle(&self) -> &str {
        &self.source_handle
    }
    fn source(&self) -> &SourceRef {
        &self.source
    }
}

impl MemorySetIdentity for ReviewedKnowledgeSet {
    fn source_handle(&self) -> &str {
        &self.source_handle
    }
    fn source(&self) -> &SourceRef {
        &self.source
    }
}

impl MemorySetIdentity for ReviewedPromiseSet {
    fn source_handle(&self) -> &str {
        &self.source_handle
    }
    fn source(&self) -> &SourceRef {
        &self.source
    }
}

fn page_bounds(offset: u32, limit: u32, total: usize) -> CoreResult<(usize, usize, Option<u32>)> {
    if offset > MAX_MEMORY_OFFSET || !(1..=MAX_MEMORY_LIMIT).contains(&limit) {
        return Err(invalid_memory(
            "Memory page bounds are outside the validated range.",
        ));
    }
    let start = usize::try_from(offset)
        .map_err(|_| invalid_memory("Memory page offset cannot be represented."))?;
    let end = start.saturating_add(usize::try_from(limit).unwrap_or(usize::MAX));
    let end = end.min(total);
    let next = (end < total).then(|| u32::try_from(end).ok()).flatten();
    Ok((start.min(total), end, next))
}

fn checked_count(count: usize, label: &str) -> CoreResult<u32> {
    u32::try_from(count)
        .map_err(|_| invalid_memory(format!("The number of {label} exceeds the wire limit.")))
}

fn invalid_memory(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidMemoryLookup", &detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::reviewed_evidence::ReviewedEvidenceSet;
    use crate::context::reviewed_knowledge::records_hash;
    use crate::context::{
        Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
        SourceDescriptor, SourceKind, StorySnapshot,
    };
    use crate::projects::story_records::{
        EvidenceAnchor, EvidenceAudience, KnowledgeAttitude, KnowledgeRecord, PossessionRecord,
        PossessionTiming,
    };

    fn source() -> (SourceRef, SourceDescriptor) {
        let source = SourceRef {
            project_id: "project".into(),
            document_id: "chapter-1".into(),
            revision_id: "revision-1".into(),
            body_hash: "a".repeat(64),
        };
        let descriptor = SourceDescriptor {
            handle: "chapter-1".into(),
            source: source.clone(),
            display_name: "Chapter 1".into(),
            kind: SourceKind::CurrentDraft,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: Some("1".into()),
                visible_to_characters: Vec::new(),
                author_only: false,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        };
        (source, descriptor)
    }

    fn record(id: &str, character_id: &str, attitude: KnowledgeAttitude) -> KnowledgeRecord {
        KnowledgeRecord {
            id: id.into(),
            character: StoryEntityRef {
                id: character_id.into(),
                label: "Mei".into(),
            },
            topic: StoryEntityRef {
                id: "topic-key".into(),
                label: "The key".into(),
            },
            attitude,
            statement: format!("statement {id}"),
            timing: PossessionTiming::AtPassage,
            audience: EvidenceAudience::AuthorRoom,
            evidence: EvidenceAnchor {
                block_id: "paragraph-1".into(),
                from_utf16: 0,
                to_utf16: 4,
                quote: "Mei ".into(),
                quote_hash: "b".repeat(64),
            },
        }
    }

    fn frozen(records: Vec<KnowledgeRecord>) -> FrozenContext {
        let (source, descriptor) = source();
        let records_hash = records_hash(&records).expect("hash records");
        FrozenContext {
            snapshot: StorySnapshot {
                snapshot_id: "snapshot-1".into(),
                project_id: "project".into(),
                basis: BasisKind::Working,
                target: source.clone(),
                context_source_epoch: "1".into(),
                ordering_epoch: "1".into(),
                disclosure_policy_version: "1".into(),
                sources: vec![descriptor],
                reviewed_basis: None,
            },
            policy: InformationPolicy {
                version: "1".into(),
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
            purpose: ContextPurpose::Discuss,
            aliases: Default::default(),
            excluded_source_count: 0,
            guidance: Vec::new(),
            conversation: None,
            navigation_views: Vec::new(),
            reviewed_evidence: Vec::new(),
            reviewed_promises: Vec::new(),
            reviewed_knowledge: vec![ReviewedKnowledgeSet {
                project_id: "project".into(),
                operation_namespace: "operation".into(),
                bundle_id: "bundle-1".into(),
                records_hash,
                source_handle: "chapter-1".into(),
                source,
                records,
            }],
            reviewed_summaries: Vec::new(),
        }
    }

    #[test]
    fn entity_pages_keep_equal_labels_as_distinct_ids_and_stable_order() {
        let frozen = frozen(vec![
            record("knowledge-1", "character-1", KnowledgeAttitude::Believes),
            record("knowledge-2", "character-2", KnowledgeAttitude::Knows),
        ]);
        let first = execute_memory_lookup(
            &frozen,
            &LookupRead::FindEntities {
                id: "entities".into(),
                entity_kind: MemoryEntityKind::Character,
                query: "mei".into(),
                offset: 0,
                limit: 1,
            },
        )
        .expect("lookup succeeds")
        .expect("memory result");
        let LookupReadResult::FindEntities {
            entries,
            total_matches,
            next_offset,
            ..
        } = first
        else {
            panic!("expected entity result")
        };
        assert_eq!(total_matches, 2);
        assert_eq!(next_offset, Some(1));
        assert_eq!(entries[0].entity.id, "character-1");

        let second = execute_memory_lookup(
            &frozen,
            &LookupRead::FindEntities {
                id: "entities".into(),
                entity_kind: MemoryEntityKind::Character,
                query: "mei".into(),
                offset: 1,
                limit: 1,
            },
        )
        .expect("lookup succeeds")
        .expect("memory result");
        let LookupReadResult::FindEntities {
            entries,
            next_offset,
            ..
        } = second
        else {
            panic!("expected entity result")
        };
        assert_eq!(entries[0].entity.id, "character-2");
        assert_eq!(next_offset, None);
    }

    #[test]
    fn history_page_preserves_complete_uncertainty_metadata() {
        let frozen = frozen(vec![
            record("knowledge-1", "character-1", KnowledgeAttitude::Believes),
            record("knowledge-2", "character-1", KnowledgeAttitude::Knows),
        ]);
        let result = execute_memory_lookup(
            &frozen,
            &LookupRead::KnowledgeHistory {
                id: "history".into(),
                character_id: "character-1".into(),
                topic_id: Some("topic-key".into()),
                offset: 0,
                limit: 1,
            },
        )
        .expect("lookup succeeds")
        .expect("memory result");
        let LookupReadResult::KnowledgeHistory {
            history,
            total_observations,
            next_offset,
            ..
        } = result
        else {
            panic!("expected knowledge history")
        };
        assert_eq!(history.observations.len(), 1);
        assert_eq!(history.label_variants, vec!["Mei"]);
        assert!(history.uncertainty.contains(
            &crate::context::knowledge_history::KnowledgeHistoryUncertainty::MultipleRecordedAttitudes
        ));
        assert_eq!(total_observations, 2);
        assert_eq!(next_offset, Some(1));
    }

    #[test]
    fn catalog_uses_earliest_frozen_source_across_memory_families() {
        let mut frozen = frozen(vec![record(
            "knowledge-1",
            "character-1",
            KnowledgeAttitude::Knows,
        )]);
        let (source, mut descriptor) = source();
        let source = SourceRef {
            document_id: "chapter-2".into(),
            revision_id: "revision-2".into(),
            ..source
        };
        descriptor.handle = "chapter-2".into();
        descriptor.source = source.clone();
        descriptor.display_name = "Chapter 2".into();
        // Catalog ordering follows the frozen source manifest.  Deliberately
        // give the later descriptor a lower reader position so a catalog that
        // accidentally reuses disclosure chronology would choose chapter 2.
        descriptor.disclosure.reader_position = Some("0".into());
        frozen.snapshot.sources.push(descriptor);
        let possession = PossessionRecord {
            id: "possession-1".into(),
            object: StoryEntityRef {
                id: "object-key".into(),
                label: "The key".into(),
            },
            holder: Some(StoryEntityRef {
                id: "character-1".into(),
                label: "Mei later".into(),
            }),
            timing: PossessionTiming::AtPassage,
            audience: EvidenceAudience::AuthorRoom,
            evidence: EvidenceAnchor {
                block_id: "paragraph-1".into(),
                from_utf16: 0,
                to_utf16: 4,
                quote: "Mei ".into(),
                quote_hash: "b".repeat(64),
            },
        };
        let records_hash =
            crate::context::reviewed_evidence::records_hash(std::slice::from_ref(&possession))
                .expect("hash possession");
        frozen.reviewed_evidence.push(ReviewedEvidenceSet {
            project_id: "project".into(),
            operation_namespace: "operation".into(),
            bundle_id: "bundle-2".into(),
            records_hash,
            source_handle: "chapter-2".into(),
            source,
            records: vec![possession],
        });

        let result = execute_memory_lookup(
            &frozen,
            &LookupRead::FindEntities {
                id: "entities".into(),
                entity_kind: MemoryEntityKind::Character,
                query: "mei".into(),
                offset: 0,
                limit: 20,
            },
        )
        .expect("lookup succeeds")
        .expect("memory result");
        let LookupReadResult::FindEntities { entries, .. } = result else {
            panic!("expected entity result")
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_handle, "chapter-1");
        assert_eq!(entries[0].label_variants, vec!["Mei", "Mei later"]);
    }
}
