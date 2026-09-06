//! Typed replacement blocks for broad, explicitly scoped author proposals.
//!
//! A provider may propose block content, but it never assigns editor block
//! identities. JavaScript prepares the complete result snapshot and supplies
//! the fresh IDs. Rust validates both the typed candidate and that complete
//! snapshot against the source-bound scope.

use super::{ScopeKind, ScopeValidationRequest, validate_scope};
use crate::validate_snapshot_json;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

pub const STRUCTURED_PROPOSAL_RESPONSE_CONTRACT: &str = "structured-proposal-output.v1";
pub const MAX_STRUCTURED_BLOCKS: usize = 128;
pub const MAX_STRUCTURED_UTF16_UNITS: usize = 100_000;
pub const MAX_STRUCTURED_EXPLANATION_BYTES: usize = 4096;

/// A replacement block has no editor ID. The application assigns one while
/// preparing the complete result document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TypedReplacementBlock {
    Paragraph {
        #[serde(default)]
        content: Vec<TypedReplacementInline>,
    },
    Heading {
        attrs: TypedReplacementHeadingAttrs,
        #[serde(default)]
        content: Vec<TypedReplacementInline>,
    },
    SceneBreak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypedReplacementHeadingAttrs {
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TypedReplacementInline {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<TypedReplacementMark>,
    },
    HardBreak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TypedReplacementMark {
    Bold,
    Italic,
    Link { attrs: TypedReplacementLinkAttrs },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypedReplacementLinkAttrs {
    pub href: String,
}

/// Validate the provider-owned replacement grammar without assigning IDs.
pub fn validate_typed_replacement_blocks(blocks: &[TypedReplacementBlock]) -> Result<(), String> {
    if blocks.len() > MAX_STRUCTURED_BLOCKS {
        return Err(format!(
            "structured replacement has more than {MAX_STRUCTURED_BLOCKS} blocks"
        ));
    }
    let mut total = 0_usize;
    for (index, block) in blocks.iter().enumerate() {
        let content = match block {
            TypedReplacementBlock::Paragraph { content }
            | TypedReplacementBlock::Heading { content, .. } => content,
            TypedReplacementBlock::SceneBreak => continue,
        };
        if let TypedReplacementBlock::Heading { attrs, .. } = block
            && !(1..=3).contains(&attrs.level)
        {
            return Err(format!(
                "structured replacement heading {index} level must be between 1 and 3"
            ));
        }
        for (inline_index, inline) in content.iter().enumerate() {
            match inline {
                TypedReplacementInline::Text { text, marks } => {
                    if text.is_empty() {
                        return Err(format!(
                            "structured replacement block {index} inline {inline_index} text must not be empty"
                        ));
                    }
                    if text.contains(['\r', '\n']) {
                        return Err(format!(
                            "structured replacement block {index} text must use hardBreak for line breaks"
                        ));
                    }
                    total = total
                        .checked_add(text.encode_utf16().count())
                        .ok_or_else(|| {
                            "structured replacement text length overflowed".to_owned()
                        })?;
                    let mut mark_types = HashSet::new();
                    for mark in marks {
                        let key = match mark {
                            TypedReplacementMark::Bold => "bold",
                            TypedReplacementMark::Italic => "italic",
                            TypedReplacementMark::Link { .. } => "link",
                        };
                        if matches!(mark, TypedReplacementMark::Link { .. }) {
                            let mark_snapshot = json!({
                                "schemaVersion": 1,
                                "body": {
                                    "type": "doc",
                                    "content": [{
                                        "type": "paragraph",
                                        "attrs": {"id": "link-check"},
                                        "content": [{
                                            "type": "text",
                                            "text": "x",
                                            "marks": [mark_value(mark)]
                                        }]
                                    }]
                                }
                            });
                            validate_snapshot_json(
                                &serde_json::to_string(&mark_snapshot)
                                    .map_err(|error| error.to_string())?,
                            )
                            .map_err(|error| {
                                format!("structured replacement link is invalid: {error}")
                            })?;
                        }
                        if !mark_types.insert(key) {
                            return Err(format!(
                                "structured replacement block {index} repeats {key} formatting"
                            ));
                        }
                    }
                }
                TypedReplacementInline::HardBreak => {
                    total = total.checked_add(1).ok_or_else(|| {
                        "structured replacement text length overflowed".to_owned()
                    })?;
                }
            }
        }
    }
    if total > MAX_STRUCTURED_UTF16_UNITS {
        return Err(format!(
            "structured replacement exceeds {MAX_STRUCTURED_UTF16_UNITS} UTF-16 units"
        ));
    }
    Ok(())
}

/// Convert typed blocks to the canonical snapshot grammar using application
/// supplied IDs. The number and order of IDs must exactly match the blocks.
pub fn typed_replacement_snapshot(
    blocks: &[TypedReplacementBlock],
    ids: &[String],
) -> Result<Value, String> {
    validate_typed_replacement_blocks(blocks)?;
    if blocks.len() != ids.len() {
        return Err("structured replacement IDs must match the block count".to_owned());
    }
    let mut seen = HashSet::with_capacity(ids.len());
    let mut content = Vec::with_capacity(blocks.len());
    for (index, (block, id)) in blocks.iter().zip(ids).enumerate() {
        validate_id(id).map_err(|error| format!("replacement block {index}: {error}"))?;
        if !seen.insert(id) {
            return Err(format!("replacement block ID {id:?} is duplicated"));
        }
        content.push(block_value(block, id));
    }
    let snapshot = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": content},
    });
    validate_snapshot_json(&serde_json::to_string(&snapshot).map_err(|error| error.to_string())?)
        .map(|receipt| receipt.snapshot)
}

/// Validate a structured candidate against a complete application-prepared
/// result. The source and every unselected block remain protected by the
/// shared scope validator; the candidate must exactly describe the inserted
/// block sequence while all inserted IDs must be fresh.
pub fn validate_structured_replacement(
    request: &ScopeValidationRequest,
    blocks: &[TypedReplacementBlock],
) -> Result<super::ScopeReceipt, String> {
    if !matches!(
        request.scope.kind,
        ScopeKind::Blocks | ScopeKind::WholeDocument
    ) {
        return Err("structured replacement requires a blocks or whole-document scope".to_owned());
    }
    validate_typed_replacement_blocks(blocks)?;
    let source = canonical_snapshot(&request.source_snapshot)?;
    let result = canonical_snapshot(&request.result_snapshot)?;
    let source_blocks = document_blocks(&source.snapshot)?;
    let result_blocks = document_blocks(&result.snapshot)?;
    let (first, last) = selected_range(&source_blocks, &request.scope)?;
    let suffix_count = source_blocks.len().saturating_sub(last + 1);
    let result_first = first.min(result_blocks.len());
    if result_blocks.len() < first + suffix_count {
        return Err("structured result removes unselected document blocks".to_owned());
    }
    let result_last_exclusive = result_blocks.len() - suffix_count;
    if result_first > result_last_exclusive {
        return Err("structured result scope boundaries overlap".to_owned());
    }
    let inserted = &result_blocks[result_first..result_last_exclusive];
    if inserted.len() != blocks.len() {
        return Err("prepared structured blocks do not match the result block range".to_owned());
    }

    let source_ids: HashSet<&str> = source_blocks.iter().filter_map(block_id).collect();
    let mut replacement_ids = HashSet::new();
    for (index, (block, result_block)) in blocks.iter().zip(inserted).enumerate() {
        let result_id = block_id(result_block)
            .ok_or_else(|| format!("result replacement block {index} has no ID"))?;
        if !replacement_ids.insert(result_id) {
            return Err(format!(
                "result replacement block ID {result_id:?} is duplicated"
            ));
        }
        if source_ids.contains(result_id) {
            return Err(format!(
                "result replacement block reuses source ID {result_id:?}"
            ));
        }
        let expected = block_value(block, result_id);
        let expected = canonical_snapshot(&json!({
            "schemaVersion": 1,
            "body": {"type": "doc", "content": [expected]},
        }))?;
        let expected_block = document_blocks(&expected.snapshot)?
            .into_iter()
            .next()
            .ok_or_else(|| "structured replacement block vanished during validation".to_owned())?;
        if expected_block != *result_block {
            return Err(format!(
                "prepared result block {index} does not match its typed candidate"
            ));
        }
    }

    let receipt = validate_scope(request)?;
    if receipt.scope == ScopeKind::WholeDocument && inserted.is_empty() {
        return Err("a whole-document replacement must leave one valid document block".to_owned());
    }
    Ok(receipt)
}

fn block_value(block: &TypedReplacementBlock, id: &str) -> Value {
    let mut object = Map::new();
    let (block_type, attrs, content) = match block {
        TypedReplacementBlock::Paragraph { content } => {
            ("paragraph", json!({"id": id}), Some(content.as_slice()))
        }
        TypedReplacementBlock::Heading { attrs, content } => (
            "heading",
            json!({"id": id, "level": attrs.level}),
            Some(content.as_slice()),
        ),
        TypedReplacementBlock::SceneBreak => ("sceneBreak", json!({"id": id}), None),
    };
    object.insert("attrs".to_owned(), attrs);
    object.insert("type".to_owned(), Value::String(block_type.to_owned()));
    if let Some(content) = content {
        let values = content.iter().map(inline_value).collect::<Vec<_>>();
        if !values.is_empty() {
            object.insert("content".to_owned(), Value::Array(values));
        }
    }
    Value::Object(object)
}

fn inline_value(inline: &TypedReplacementInline) -> Value {
    match inline {
        TypedReplacementInline::Text { text, marks } => {
            let mut object = Map::new();
            object.insert("text".to_owned(), Value::String(text.clone()));
            object.insert("type".to_owned(), Value::String("text".to_owned()));
            if !marks.is_empty() {
                object.insert(
                    "marks".to_owned(),
                    Value::Array(marks.iter().map(mark_value).collect()),
                );
            }
            Value::Object(object)
        }
        TypedReplacementInline::HardBreak => json!({"type": "hardBreak"}),
    }
}

fn mark_value(mark: &TypedReplacementMark) -> Value {
    match mark {
        TypedReplacementMark::Bold => json!({"type": "bold"}),
        TypedReplacementMark::Italic => json!({"type": "italic"}),
        TypedReplacementMark::Link { attrs } => {
            json!({"type": "link", "attrs": {"href": attrs.href}})
        }
    }
}

fn canonical_snapshot(value: &Value) -> Result<crate::SnapshotReceipt, String> {
    validate_snapshot_json(
        &serde_json::to_string(value)
            .map_err(|error| format!("snapshot serialization failed: {error}"))?,
    )
}

fn document_blocks(value: &Value) -> Result<Vec<Value>, String> {
    value
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "snapshot body content is missing".to_owned())
}

fn selected_range(blocks: &[Value], scope: &super::ScopeGrant) -> Result<(usize, usize), String> {
    if scope.kind == ScopeKind::WholeDocument {
        return if blocks.is_empty() {
            Err("source document has no blocks".to_owned())
        } else {
            Ok((0, blocks.len() - 1))
        };
    }
    let start = scope
        .start
        .as_ref()
        .ok_or_else(|| "blocks scope requires a start endpoint".to_owned())?;
    let end = scope
        .end
        .as_ref()
        .ok_or_else(|| "blocks scope requires an end endpoint".to_owned())?;
    let first = blocks
        .iter()
        .position(|block| block_id(block) == Some(start.block_id.as_str()))
        .ok_or_else(|| "structured scope start block is missing".to_owned())?;
    let last = blocks
        .iter()
        .position(|block| block_id(block) == Some(end.block_id.as_str()))
        .ok_or_else(|| "structured scope end block is missing".to_owned())?;
    if first > last {
        return Err("structured scope start must not be after end".to_owned());
    }
    Ok((first, last))
}

fn block_id(block: &Value) -> Option<&str> {
    block
        .get("attrs")
        .and_then(Value::as_object)
        .and_then(|attrs| attrs.get("id"))
        .and_then(Value::as_str)
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("block IDs must be 1..64 ASCII letters, digits, '_' or '-'".to_owned());
    }
    Ok(())
}
