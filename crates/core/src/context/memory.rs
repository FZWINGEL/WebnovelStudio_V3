//! Pure, source-bound response contracts for the first navigation-memory cut.
//!
//! This module validates a bounded evidence-linked response. It does not infer
//! semantic truth, install a generated view, or authorize manuscript edits.

use crate::context::{CoverageLabel, SourceKind, SourceRef};
use crate::projects::story_context::{SourcePassage, SourceRead};
use wns_kernel::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire version for the single-chapter navigation response.
pub const DIGEST_SCHEMA_VERSION: &str = "navigation-digest.v1";
/// Maximum raw provider response size accepted by this pure contract.
pub const MAX_RAW_BYTES: usize = 64 * 1024;
/// Maximum number of extractive navigation items in one response.
pub const MAX_ITEMS: usize = 16;
/// Maximum UTF-8 bytes in one item label.
pub const MAX_ITEM_TEXT_BYTES: usize = 2048;
/// Maximum evidence anchors attached to one item.
pub const MAX_EVIDENCE_PER_ITEM: usize = 4;
/// Maximum UTF-8 bytes in one quoted evidence span.
pub const MAX_EVIDENCE_QUOTE_BYTES: usize = 4096;
/// Maximum UTF-8 bytes in an uncertainty note.
pub const MAX_UNCERTAINTY_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DigestCandidate {
    pub schema_version: String,
    pub source: SourceRef,
    pub items: Vec<DigestItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DigestItem {
    pub text: String,
    pub evidence: Vec<DigestEvidence>,
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DigestEvidence {
    pub block_id: String,
    pub from_utf16: u32,
    pub to_utf16: u32,
    pub quote: String,
}

/// Validate one complete saved chapter response against a trusted source read.
///
/// The trusted read is checked before response parsing. Its body must still be
/// canonical and hash to its exact `SourceRef`; its passage projection must
/// match that body. Every candidate citation then points into one of those
/// exact block strings at UTF-16-safe character boundaries.
pub fn validate_navigation_digest(raw: &[u8], trusted: &SourceRead) -> CoreResult<DigestCandidate> {
    if raw.len() > MAX_RAW_BYTES {
        return Err(memory_error(
            "InvalidMemoryCandidate",
            format!("The navigation response exceeds {MAX_RAW_BYTES} bytes."),
        ));
    }
    let passages = validate_trusted_source(trusted)?;
    let candidate: DigestCandidate = serde_json::from_slice(raw).map_err(|error| {
        memory_error(
            "InvalidMemoryCandidate",
            format!("The navigation response is malformed: {error}"),
        )
    })?;
    validate_candidate(candidate, &trusted.descriptor.source, &passages)
}

/// Produce deterministic, extractive mock output for offline contract tests.
///
/// A bounded, evenly distributed sample of non-empty source blocks produces
/// short verbatim navigation items. This exercises ordinary long chapters
/// without pretending that an extractive mock supplied a semantic summary.
pub fn mock_navigation_digest(trusted: &SourceRead) -> CoreResult<DigestCandidate> {
    let passages = validate_trusted_source(trusted)?;
    let nonempty: Vec<_> = passages
        .iter()
        .filter(|passage| !passage.text.trim().is_empty())
        .collect();
    let count = nonempty.len().min(MAX_ITEMS);
    let mut items = Vec::new();
    for index in 0..count {
        let position = if count == 1 {
            0
        } else {
            index * (nonempty.len() - 1) / (count - 1)
        };
        let passage = nonempty[position];
        let mut end = passage.text.len().min(256);
        while !passage.text.is_char_boundary(end) {
            end -= 1;
        }
        let excerpt = &passage.text[..end];
        let to_utf16 = utf16_len(excerpt)?;
        items.push(DigestItem {
            text: excerpt.to_owned(),
            evidence: vec![DigestEvidence {
                block_id: passage.block_id.clone(),
                from_utf16: 0,
                to_utf16,
                quote: excerpt.to_owned(),
            }],
            uncertainty: Some(
                "Extractive local test sample; it does not summarize every passage or verify meaning.".to_owned(),
            ),
        });
    }
    if items.is_empty() {
        return Err(memory_error(
            "EmptyMemorySource",
            "The saved chapter has no non-empty paragraph to summarize.",
        ));
    }
    Ok(DigestCandidate {
        schema_version: DIGEST_SCHEMA_VERSION.to_owned(),
        source: trusted.descriptor.source.clone(),
        items,
    })
}

fn validate_candidate(
    candidate: DigestCandidate,
    expected_source: &SourceRef,
    passages: &[SourcePassage],
) -> CoreResult<DigestCandidate> {
    if candidate.schema_version != DIGEST_SCHEMA_VERSION {
        return Err(memory_error(
            "InvalidMemoryCandidate",
            format!("schemaVersion must be {DIGEST_SCHEMA_VERSION}."),
        ));
    }
    if candidate.source != *expected_source {
        return Err(memory_error(
            "MemorySourceMismatch",
            "The navigation response cites a different project, document, revision, or body hash.",
        ));
    }
    if candidate.items.is_empty() || candidate.items.len() > MAX_ITEMS {
        return Err(memory_error(
            "InvalidMemoryCandidate",
            format!("The response must contain 1 to {MAX_ITEMS} items."),
        ));
    }
    for item in &candidate.items {
        if item.text.is_empty() || item.text.len() > MAX_ITEM_TEXT_BYTES {
            return Err(memory_error(
                "InvalidMemoryCandidate",
                format!("Each item text must be 1 to {MAX_ITEM_TEXT_BYTES} bytes."),
            ));
        }
        if let Some(uncertainty) = &item.uncertainty
            && (uncertainty.is_empty() || uncertainty.len() > MAX_UNCERTAINTY_BYTES)
        {
            return Err(memory_error(
                "InvalidMemoryCandidate",
                format!("Each uncertainty note must be 1 to {MAX_UNCERTAINTY_BYTES} bytes."),
            ));
        }
        if item.evidence.is_empty() || item.evidence.len() > MAX_EVIDENCE_PER_ITEM {
            return Err(memory_error(
                "InvalidMemoryCandidate",
                format!("Each item must contain 1 to {MAX_EVIDENCE_PER_ITEM} evidence anchors."),
            ));
        }
        for evidence in &item.evidence {
            let passage = passages
                .iter()
                .find(|passage| passage.block_id == evidence.block_id)
                .ok_or_else(|| {
                    memory_error(
                        "InvalidMemoryEvidence",
                        "An evidence anchor names no block in the trusted source.",
                    )
                })?;
            if evidence.quote.is_empty() || evidence.quote.len() > MAX_EVIDENCE_QUOTE_BYTES {
                return Err(memory_error(
                    "InvalidMemoryEvidence",
                    format!("Evidence quotes must be 1 to {MAX_EVIDENCE_QUOTE_BYTES} bytes."),
                ));
            }
            if evidence.from_utf16 >= evidence.to_utf16 {
                return Err(memory_error(
                    "InvalidMemoryEvidence",
                    "Evidence ranges must be non-empty and ordered.",
                ));
            }
            let start = utf16_boundary(&passage.text, evidence.from_utf16).ok_or_else(|| {
                memory_error(
                    "InvalidMemoryEvidence",
                    "An evidence range starts inside a UTF-16 surrogate pair or outside the block.",
                )
            })?;
            let end = utf16_boundary(&passage.text, evidence.to_utf16).ok_or_else(|| {
                memory_error(
                    "InvalidMemoryEvidence",
                    "An evidence range ends inside a UTF-16 surrogate pair or outside the block.",
                )
            })?;
            if start >= end || passage.text[start..end] != evidence.quote {
                return Err(memory_error(
                    "InvalidMemoryEvidence",
                    "The evidence quote does not match its exact source range.",
                ));
            }
        }
    }
    Ok(candidate)
}

fn validate_trusted_source(trusted: &SourceRead) -> CoreResult<Vec<SourcePassage>> {
    let source = &trusted.descriptor.source;
    if trusted.descriptor.kind != SourceKind::CurrentDraft
        || trusted.descriptor.coverage != CoverageLabel::Verbatim
        || !trusted.descriptor.dependencies.is_empty()
        || !trusted.descriptor.current
        || trusted.descriptor.handle.is_empty()
        || source.project_id.is_empty()
        || source.document_id.is_empty()
        || source.revision_id.is_empty()
        || !is_sha256(source.body_hash.as_str())
    {
        return Err(memory_error(
            "MemorySourceMismatch",
            "The trusted source has incomplete revision identity.",
        ));
    }
    let encoded = serde_json::to_string(&trusted.body).map_err(|error| {
        memory_error(
            "MemoryBodyMismatch",
            format!("The trusted source body cannot be serialized: {error}"),
        )
    })?;
    let receipt = crate::validate_snapshot_json(&encoded).map_err(|error| {
        memory_error(
            "MemoryBodyMismatch",
            format!("The trusted source body is not a valid canonical snapshot: {error}"),
        )
    })?;
    if receipt.hash != source.body_hash || receipt.canonical_json != encoded {
        return Err(memory_error(
            "MemoryBodyMismatch",
            "The trusted source body does not match its canonical SourceRef hash.",
        ));
    }
    let derived = passages_from_body(&receipt.snapshot, &trusted.descriptor.handle, source)?;
    if trusted.passages != derived
        || trusted
            .passages
            .iter()
            .any(|passage| passage.source != *source || passage.handle != trusted.descriptor.handle)
    {
        return Err(memory_error(
            "MemoryBodyMismatch",
            "The trusted source passage projection does not match its canonical body.",
        ));
    }
    Ok(derived)
}

fn passages_from_body(
    body: &Value,
    handle: &str,
    source: &SourceRef,
) -> CoreResult<Vec<SourcePassage>> {
    let blocks = body["body"]["content"].as_array().ok_or_else(|| {
        memory_error(
            "MemoryBodyMismatch",
            "The trusted source body has no canonical block array.",
        )
    })?;
    blocks
        .iter()
        .enumerate()
        .map(|(order, block)| {
            let mut text = String::new();
            if let Some(content) = block["content"].as_array() {
                for node in content {
                    if node["type"] == "hardBreak" {
                        text.push('\n');
                    } else if let Some(value) = node["text"].as_str() {
                        text.push_str(value);
                    }
                }
            }
            let block_id = block["attrs"]["id"].as_str().ok_or_else(|| {
                memory_error(
                    "MemoryBodyMismatch",
                    "The trusted source has a block without a canonical id.",
                )
            })?;
            Ok(SourcePassage {
                handle: handle.to_owned(),
                source: source.clone(),
                block_id: block_id.to_owned(),
                block_order: u32::try_from(order).map_err(|_| {
                    memory_error(
                        "MemoryBodyMismatch",
                        "The trusted source has too many blocks.",
                    )
                })?,
                text,
            })
        })
        .collect()
}

fn utf16_len(text: &str) -> CoreResult<u32> {
    text.encode_utf16().count().try_into().map_err(|_| {
        memory_error(
            "InvalidMemoryEvidence",
            "The source block is too long for a UTF-16 evidence range.",
        )
    })
}

/// Return the UTF-8 byte boundary corresponding to an editor UTF-16 offset.
/// Offsets inside a supplementary character are deliberately rejected.
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

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn memory_error(code: &str, detail: impl Into<String>) -> CoreError {
    CoreError::new(code, &detail.into())
}
