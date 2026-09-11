//! Explicit, passage-backed reviewed story evidence.
//!
//! These records are author-entered observations.  They are never inferred
//! from prose, generated memory, or a display label, and they do not replace
//! the immutable source revision that supports them.

use wns_kernel::{CoreError, CoreResult, Revision, validate_snapshot_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_RECORDS: usize = 64;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_LABEL_BYTES: usize = 160;
pub const MAX_QUOTE_BYTES: usize = 4096;
pub const MAX_PROMISE_NOTE_BYTES: usize = 1024;
pub const MAX_KNOWLEDGE_STATEMENT_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryEntityRef {
    /// Project-local opaque identity.  Labels never merge two identities.
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PossessionTiming {
    AtPassage,
    Earlier,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceAudience {
    AuthorRoom,
    Reader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceAnchor {
    pub block_id: String,
    pub from_utf16: u32,
    pub to_utf16: u32,
    pub quote: String,
    pub quote_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PossessionRecord {
    pub id: String,
    pub object: StoryEntityRef,
    pub holder: Option<StoryEntityRef>,
    pub timing: PossessionTiming,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

/// Explicit author-entered promise observations. A phase is an observation at
/// the cited passage; it is never interpreted as a current truth or inferred
/// transfer/state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromisePhase {
    Setup,
    Payoff,
    Cancelled,
    Unclear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromiseRecord {
    pub id: String,
    pub promise: StoryEntityRef,
    pub phase: PromisePhase,
    pub timing: PossessionTiming,
    pub note: String,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

/// The author's recorded relationship between a character and a topic at an
/// exact prose passage.  This is an observation about the character's mental
/// state, never an inferred fact about the world or a permission to read the
/// surrounding chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeAttitude {
    Knows,
    Believes,
    Suspects,
    Rejects,
    Unaware,
    Unclear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRecord {
    pub id: String,
    pub character: StoryEntityRef,
    pub topic: StoryEntityRef,
    pub attitude: KnowledgeAttitude,
    pub statement: String,
    pub timing: PossessionTiming,
    pub audience: EvidenceAudience,
    pub evidence: EvidenceAnchor,
}

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidReviewedEvidence", &detail.into())
}

fn valid_id(value: &str, label: &str) -> CoreResult<()> {
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

fn valid_label(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty()
        || value.len() > MAX_LABEL_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "{label} must be nonblank, at most {MAX_LABEL_BYTES} UTF-8 bytes, and contain no control characters."
        )));
    }
    Ok(())
}

fn valid_sha256(value: &str, label: &str) -> CoreResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{label} must be a SHA-256 fingerprint.")));
    }
    Ok(())
}

fn block_text(block: &Value) -> Option<(&str, String)> {
    let object = block.as_object()?;
    let block_type = object.get("type")?.as_str()?;
    if block_type != "paragraph" && block_type != "heading" {
        return None;
    }
    let id = object.get("attrs")?.get("id")?.as_str()?;
    let mut text = String::new();
    if let Some(content) = object.get("content").and_then(Value::as_array) {
        for node in content {
            match node.get("type").and_then(|value| value.as_str()) {
                Some("text") => text.push_str(node.get("text")?.as_str()?),
                Some("hardBreak") => text.push('\n'),
                _ => return None,
            }
        }
    }
    Some((id, text))
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

fn grapheme_boundary(text: &str, target: u32) -> CoreResult<bool> {
    if target == 0 {
        return Ok(true);
    }
    let mut units = 0_u32;
    for grapheme in text.graphemes(true) {
        units = units
            .checked_add(
                u32::try_from(grapheme.encode_utf16().count())
                    .map_err(|_| invalid("Evidence grapheme is too large."))?,
            )
            .ok_or_else(|| invalid("Evidence UTF-16 range overflowed."))?;
        if units == target {
            return Ok(true);
        }
        if units > target {
            return Ok(false);
        }
    }
    Ok(false)
}

fn validate_anchor(anchor: &EvidenceAnchor, revision: &Revision) -> CoreResult<()> {
    valid_id(&anchor.block_id, "Evidence block ID")?;
    if anchor.quote.is_empty() || anchor.quote.len() > MAX_QUOTE_BYTES {
        return Err(invalid(format!(
            "Evidence quotations must be nonempty and at most {MAX_QUOTE_BYTES} UTF-8 bytes."
        )));
    }
    valid_sha256(&anchor.quote_hash, "Evidence quotation hash")?;

    let blocks = revision
        .body
        .get("body")
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("The reviewed revision has no canonical block array."))?;
    let (_, text) = blocks
        .iter()
        .filter_map(block_text)
        .find(|(id, _)| *id == anchor.block_id)
        .ok_or_else(|| invalid("Evidence must point to one text-bearing block."))?;
    if anchor.from_utf16 >= anchor.to_utf16 {
        return Err(invalid("Evidence range must be nonempty and ordered."));
    }
    let end = u32::try_from(text.encode_utf16().count())
        .map_err(|_| invalid("Evidence block is too large for UTF-16 offsets."))?;
    if anchor.to_utf16 > end {
        return Err(invalid("Evidence range exceeds its source block."));
    }
    if !grapheme_boundary(&text, anchor.from_utf16)? || !grapheme_boundary(&text, anchor.to_utf16)?
    {
        return Err(invalid(
            "Evidence range must begin and end at a grapheme boundary.",
        ));
    }
    let start = utf16_boundary(&text, anchor.from_utf16)
        .ok_or_else(|| invalid("Evidence start is not a UTF-16 boundary."))?;
    let finish = utf16_boundary(&text, anchor.to_utf16)
        .ok_or_else(|| invalid("Evidence end is not a UTF-16 boundary."))?;
    if text[start..finish] != anchor.quote {
        return Err(invalid(
            "Evidence quotation does not match the exact reviewed revision.",
        ));
    }
    if wns_kernel::sha256_hex(anchor.quote.as_bytes()) != anchor.quote_hash.to_ascii_lowercase() {
        return Err(invalid("Evidence quotation hash does not match its text."));
    }
    Ok(())
}

fn validate_entity(entity: &StoryEntityRef, label: &str) -> CoreResult<()> {
    valid_id(&entity.id, &format!("{label} ID"))?;
    valid_label(&entity.label, &format!("{label} label"))
}

/// Validate a complete reviewed evidence set and return its canonical hash.
/// Empty sets are represented by `None` so legacy serialized rows remain
/// unchanged.  The hash covers the complete ordered array, including every
/// evidence field.
pub fn validate_records(
    records: &[PossessionRecord],
    revision: &Revision,
) -> CoreResult<Option<String>> {
    if records.is_empty() {
        return Ok(None);
    }
    if records.len() > MAX_RECORDS {
        return Err(invalid(format!(
            "A reviewed evidence set may contain at most {MAX_RECORDS} records."
        )));
    }
    let encoded_revision = serde_json::to_string(&revision.body)?;
    let receipt = validate_snapshot_json(&encoded_revision)
        .map_err(|error| invalid(format!("The reviewed revision is invalid: {error}")))?;
    if receipt.snapshot != revision.body || receipt.hash != revision.head.body_hash {
        return Err(invalid(
            "The reviewed revision body is not canonical or its hash does not match.",
        ));
    }
    let mut ids = HashSet::with_capacity(records.len());
    for record in records {
        valid_id(&record.id, "Reviewed evidence ID")?;
        if !ids.insert(&record.id) {
            return Err(invalid("Reviewed evidence IDs must be unique."));
        }
        validate_entity(&record.object, "Object")?;
        if let Some(holder) = &record.holder {
            validate_entity(holder, "Holder")?;
        }
        validate_anchor(&record.evidence, revision)?;
    }
    let encoded = serde_json::to_vec(records)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed evidence set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(wns_kernel::sha256_hex(&encoded)))
}

/// Return the canonical JSON representation used for persistence and hashing.
pub fn canonical_records_json(records: &[PossessionRecord]) -> CoreResult<Option<String>> {
    if records.is_empty() {
        return Ok(None);
    }
    let encoded = serde_json::to_string(records)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed evidence set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(encoded))
}

fn validate_promise_note(note: &str) -> CoreResult<()> {
    if note.trim().is_empty()
        || note.len() > MAX_PROMISE_NOTE_BYTES
        || note.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "Promise notes must be nonblank, at most {MAX_PROMISE_NOTE_BYTES} UTF-8 bytes, and contain no control characters."
        )));
    }
    Ok(())
}

/// Validate and fingerprint a complete ordered promise observation set.
/// Promise hashes are deliberately independent from possession hashes so old
/// evidence rows and their receipts remain byte-compatible.
pub fn validate_promises(
    promises: &[PromiseRecord],
    revision: &Revision,
) -> CoreResult<Option<String>> {
    if promises.is_empty() {
        return Ok(None);
    }
    if promises.len() > MAX_RECORDS {
        return Err(invalid(format!(
            "A reviewed promise set may contain at most {MAX_RECORDS} records."
        )));
    }
    let encoded_revision = serde_json::to_string(&revision.body)?;
    let receipt = validate_snapshot_json(&encoded_revision)
        .map_err(|error| invalid(format!("The reviewed revision is invalid: {error}")))?;
    if receipt.snapshot != revision.body || receipt.hash != revision.head.body_hash {
        return Err(invalid(
            "The reviewed revision body is not canonical or its hash does not match.",
        ));
    }
    let mut ids = HashSet::with_capacity(promises.len());
    for promise in promises {
        valid_id(&promise.id, "Reviewed promise ID")?;
        if !ids.insert(&promise.id) {
            return Err(invalid("Reviewed promise IDs must be unique."));
        }
        validate_entity(&promise.promise, "Promise")?;
        validate_promise_note(&promise.note)?;
        validate_anchor(&promise.evidence, revision)?;
    }
    let encoded = serde_json::to_vec(promises)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed promise set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(wns_kernel::sha256_hex(&encoded)))
}

/// Return canonical persistence JSON for a promise set. Empty sets use the
/// legacy null representation, matching possession evidence inheritance.
pub fn canonical_promises_json(promises: &[PromiseRecord]) -> CoreResult<Option<String>> {
    if promises.is_empty() {
        return Ok(None);
    }
    let encoded = serde_json::to_string(promises)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed promise set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(encoded))
}

/// Compatibility aliases for context/storage callers that name the set
/// after its record type.
pub fn validate_promise_records(
    promises: &[PromiseRecord],
    revision: &Revision,
) -> CoreResult<Option<String>> {
    validate_promises(promises, revision)
}

pub fn canonical_promise_json(promises: &[PromiseRecord]) -> CoreResult<Option<String>> {
    canonical_promises_json(promises)
}

fn validate_knowledge_statement(statement: &str) -> CoreResult<()> {
    if statement.trim().is_empty()
        || statement.len() > MAX_KNOWLEDGE_STATEMENT_BYTES
        || statement.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "Knowledge statements must be nonblank, at most {MAX_KNOWLEDGE_STATEMENT_BYTES} UTF-8 bytes, and contain no control characters."
        )));
    }
    Ok(())
}

/// Validate and fingerprint a complete ordered author-reviewed knowledge set.
/// An empty set uses the nullable legacy representation, so callers can
/// distinguish explicit clear at the request boundary from persisted absence.
pub fn validate_knowledge(
    knowledge: &[KnowledgeRecord],
    revision: &Revision,
) -> CoreResult<Option<String>> {
    if knowledge.is_empty() {
        return Ok(None);
    }
    if knowledge.len() > MAX_RECORDS {
        return Err(invalid(format!(
            "A reviewed knowledge set may contain at most {MAX_RECORDS} records."
        )));
    }
    let encoded_revision = serde_json::to_string(&revision.body)?;
    let receipt = validate_snapshot_json(&encoded_revision)
        .map_err(|error| invalid(format!("The reviewed revision is invalid: {error}")))?;
    if receipt.snapshot != revision.body || receipt.hash != revision.head.body_hash {
        return Err(invalid(
            "The reviewed revision body is not canonical or its hash does not match.",
        ));
    }
    let mut ids = HashSet::with_capacity(knowledge.len());
    for record in knowledge {
        valid_id(&record.id, "Reviewed knowledge ID")?;
        if !ids.insert(&record.id) {
            return Err(invalid("Reviewed knowledge IDs must be unique."));
        }
        validate_entity(&record.character, "Character")?;
        validate_entity(&record.topic, "Topic")?;
        validate_knowledge_statement(&record.statement)?;
        validate_anchor(&record.evidence, revision)?;
    }
    let encoded = serde_json::to_vec(knowledge)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed knowledge set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(wns_kernel::sha256_hex(&encoded)))
}

/// Return canonical persistence JSON for a reviewed knowledge set.
pub fn canonical_knowledge_json(knowledge: &[KnowledgeRecord]) -> CoreResult<Option<String>> {
    if knowledge.is_empty() {
        return Ok(None);
    }
    let encoded = serde_json::to_string(knowledge)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(invalid(format!(
            "The reviewed knowledge set exceeds {MAX_RECORD_BYTES} canonical UTF-8 bytes."
        )));
    }
    Ok(Some(encoded))
}
