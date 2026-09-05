//! Independent validation of a prepared replacement against a saved document.
//!
//! This module intentionally validates snapshots and structural tokens. It does
//! not deserialize or execute ProseMirror steps. JavaScript remains the live
//! editor and prepares a complete result snapshot; Rust checks that result is a
//! valid document and that every token outside the explicit scope is unchanged.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use unicode_segmentation::UnicodeSegmentation;

use crate::{SnapshotReceipt, validate_snapshot_json};

/// A UTF-16 endpoint inside one block's inline content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub block_id: String,
    pub utf16_offset: u32,
}

/// The kind of structural authority granted to a prepared replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ScopeKind {
    Passage,
    Blocks,
    WholeDocument,
}

/// A source-bound scope grant. Endpoints are required for passage and blocks;
/// whole-document grants cover the canonical document token stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeGrant {
    pub kind: ScopeKind,
    #[serde(default)]
    pub start: Option<Endpoint>,
    #[serde(default)]
    pub end: Option<Endpoint>,
    pub source_hash: String,
    pub quote: String,
    pub quote_hash: String,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
}

/// A complete source/result validation request.
///
/// The request has one explicit source and one explicit prepared result. The
/// serialized names are the camelCase IPC contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeValidationRequest {
    pub source_snapshot: Value,

    pub result_snapshot: Value,
    pub scope: ScopeGrant,
}

/// A machine-readable validation failure. `validate_scope` returns this as a
/// string for parity with the W0 snapshot validator; callers that need a typed
/// IPC response can serialize this value directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeValidationError {
    pub code: String,
    pub message: String,
}

impl fmt::Display for ScopeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ScopeValidationError {}

/// The canonical token stream used for structural comparison.
///
/// `Scalar` and `HardBreak` carry the enclosing block style so a replacement
/// cannot alter an unselected suffix by changing its paragraph/heading type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum StructuralToken {
    OpenBlock {
        id: String,
        #[serde(rename = "type")]
        block_type: String,
        attrs: Value,
    },
    Scalar {
        scalar: String,
        marks: Vec<Value>,
        style: BlockStyle,
    },
    HardBreak {
        style: BlockStyle,
    },
    CloseBlock,
    SceneBreak {
        id: String,
        attrs: Value,
    },
}

/// Block rendering style carried by inline tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockStyle {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub level: Option<u8>,
}

/// A successful source/result scope check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeReceipt {
    pub accepted: bool,
    pub scope: ScopeKind,
    pub source_hash: String,
    pub result_hash: String,
    pub quote: String,
    pub quote_hash: String,
    pub prefix: String,
    pub suffix: String,
    pub source_token_count: u32,
    pub result_token_count: u32,
    pub replacement_token_count: u32,
}

#[derive(Debug, Clone)]
struct BlockSpan {
    id: String,
    block_type: String,
    attrs: Value,
    style: Option<BlockStyle>,
    token_start: usize,
    content_start: usize,
    content_end: usize,
    token_end: usize,
    utf16_units: u32,
    text: String,
}

#[derive(Debug, Clone)]
struct TokenDocument {
    tokens: Vec<StructuralToken>,
    blocks: Vec<BlockSpan>,
    by_id: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy)]
struct TokenRange {
    start: usize,
    end: usize,
    first_block: usize,
    last_block: usize,
    selected_ids: usize,
}

/// Validate a JSON-encoded scope request.
pub fn validate_scope_json(input: &str) -> Result<ScopeReceipt, String> {
    let request: ScopeValidationRequest = serde_json::from_str(input)
        .map_err(|error| format!("invalid scope request JSON: {error}"))?;
    validate_scope(&request)
}

/// Capture the source-bound values for a new grant.
///
/// A caller should use this when the author selects a passage, then persist
/// the returned grant alongside its source revision. Validation still requires
/// all three values and never silently fills them from an untrusted request.
pub fn capture_scope(source_snapshot: &Value, mut scope: ScopeGrant) -> Result<ScopeGrant, String> {
    let source = canonical_snapshot(source_snapshot)
        .map_err(|error| format!("source snapshot is invalid: {error}"))?;
    let document = token_document(&source.snapshot)
        .map_err(|error| format!("source tokenization failed: {error}"))?;
    let range = scope_range(&document, &scope)?;
    scope.source_hash = source.hash;
    scope.quote = plain_text(&document.tokens[range.start..range.end]);
    scope.quote_hash = structured_hash(&document.tokens[range.start..range.end])?;
    if scope.prefix.is_none() {
        scope.prefix = Some(context_prefix(&document, range.first_block, range.start));
    }
    if scope.suffix.is_none() {
        scope.suffix = Some(context_suffix(&document, range.last_block, range.end));
    }
    Ok(scope)
}

/// Validate a prepared result against its canonical source and explicit grant.
pub fn validate_scope(request: &ScopeValidationRequest) -> Result<ScopeReceipt, String> {
    let source = canonical_snapshot(&request.source_snapshot)
        .map_err(|error| format!("source snapshot is invalid: {error}"))?;
    let result = canonical_snapshot(&request.result_snapshot)
        .map_err(|error| format!("result snapshot is invalid: {error}"))?;

    let source_doc = token_document(&source.snapshot)
        .map_err(|error| format!("source tokenization failed: {error}"))?;
    let result_doc = token_document(&result.snapshot)
        .map_err(|error| format!("result tokenization failed: {error}"))?;
    let range = scope_range(&source_doc, &request.scope)?;

    let source_hash = source.hash.clone();
    let quote = plain_text(&source_doc.tokens[range.start..range.end]);
    let quote_hash = structured_hash(&source_doc.tokens[range.start..range.end])?;
    let prefix = context_prefix(&source_doc, range.first_block, range.start);
    let suffix = context_suffix(&source_doc, range.last_block, range.end);
    if request.scope.source_hash != source_hash {
        return Err(format!(
            "scope sourceHash does not match source snapshot: expected {}, got {source_hash}",
            request.scope.source_hash
        ));
    }
    if request.scope.quote != quote {
        return Err("scope quote does not match the source endpoints".to_owned());
    }
    if request.scope.quote_hash != quote_hash {
        return Err("scope quoteHash does not match the source fragment".to_owned());
    }
    if let Some(expected) = request.scope.prefix.as_deref()
        && expected != prefix
    {
        return Err("scope prefix does not match the source endpoint context".to_owned());
    }
    if let Some(expected) = request.scope.suffix.as_deref()
        && expected != suffix
    {
        return Err("scope suffix does not match the source endpoint context".to_owned());
    }

    let source_prefix = &source_doc.tokens[..range.start];
    let source_suffix = &source_doc.tokens[range.end..];
    if result_doc.tokens.len() < source_prefix.len() + source_suffix.len() {
        return Err("result removes unselected document structure".to_owned());
    }
    if result_doc.tokens[..source_prefix.len()] != *source_prefix {
        return Err("result changes tokens before the granted scope".to_owned());
    }
    let suffix_start = result_doc.tokens.len() - source_suffix.len();
    if result_doc.tokens[suffix_start..] != *source_suffix {
        return Err("result changes tokens after the granted scope".to_owned());
    }
    if suffix_start < source_prefix.len() {
        return Err("result scope boundaries overlap".to_owned());
    }
    let replacement = &result_doc.tokens[source_prefix.len()..suffix_start];

    if request.scope.kind == ScopeKind::Passage
        && range.first_block == range.last_block
        && replacement.iter().any(StructuralToken::is_boundary)
    {
        return Err("passage scope cannot add or remove block or scene boundaries".to_owned());
    }

    validate_ids_and_merge_policy(
        &source_doc,
        &result_doc,
        replacement,
        &range,
        request.scope.kind,
    )?;

    Ok(ScopeReceipt {
        accepted: true,
        scope: request.scope.kind,
        source_hash,
        result_hash: result.hash,
        quote,
        quote_hash,
        prefix,
        suffix,
        source_token_count: source_doc
            .tokens
            .len()
            .try_into()
            .map_err(|_| "source token count exceeds u32".to_owned())?,
        result_token_count: result_doc
            .tokens
            .len()
            .try_into()
            .map_err(|_| "result token count exceeds u32".to_owned())?,
        replacement_token_count: replacement
            .len()
            .try_into()
            .map_err(|_| "replacement token count exceeds u32".to_owned())?,
    })
}

/// Produce the canonical structural token stream for a validated snapshot.
pub fn structural_tokens(snapshot: &Value) -> Result<Vec<StructuralToken>, String> {
    Ok(token_document(snapshot)?.tokens)
}

/// Produce an owning iterator over canonical structural tokens.
pub fn structural_token_iter(
    snapshot: &Value,
) -> Result<std::vec::IntoIter<StructuralToken>, String> {
    Ok(structural_tokens(snapshot)?.into_iter())
}

fn canonical_snapshot(value: &Value) -> Result<SnapshotReceipt, String> {
    let input = match value {
        Value::String(json) => json.clone(),
        other => serde_json::to_string(other)
            .map_err(|error| format!("failed to serialize snapshot input: {error}"))?,
    };
    validate_snapshot_json(&input)
}

fn token_document(snapshot: &Value) -> Result<TokenDocument, String> {
    let body = snapshot
        .get("body")
        .and_then(Value::as_object)
        .ok_or_else(|| "snapshot.body must be an object".to_owned())?;
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "snapshot.body.content must be an array".to_owned())?;

    let mut tokens = Vec::new();
    let mut spans = Vec::with_capacity(blocks.len());
    let mut by_id = HashMap::with_capacity(blocks.len());
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "document block must be an object".to_owned())?;
        let block_type = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "document block type must be a string".to_owned())?;
        let attrs = object
            .get("attrs")
            .cloned()
            .ok_or_else(|| "document block attrs are required".to_owned())?;
        let attrs_object = attrs
            .as_object()
            .ok_or_else(|| "document block attrs must be an object".to_owned())?;
        let id = attrs_object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "document block attrs.id must be a string".to_owned())?
            .to_owned();
        if by_id.contains_key(&id) {
            return Err(format!("duplicate block id {id:?}"));
        }
        let block_index = spans.len();
        by_id.insert(id.clone(), block_index);
        let token_start = tokens.len();
        let (style, content_start, content_end, token_end, utf16_units, text) = match block_type {
            "paragraph" | "heading" => {
                let level = attrs_object
                    .get("level")
                    .and_then(Value::as_u64)
                    .map(|value| value as u8);
                let style = BlockStyle {
                    block_type: block_type.to_owned(),
                    level,
                };
                tokens.push(StructuralToken::OpenBlock {
                    id: id.clone(),
                    block_type: block_type.to_owned(),
                    attrs: attrs.clone(),
                });
                let content_start = tokens.len();
                let mut text = String::new();
                let mut utf16_units = 0_u32;
                if let Some(content) = object.get("content") {
                    let content = content
                        .as_array()
                        .ok_or_else(|| "block content must be an array".to_owned())?;
                    for inline in content {
                        let inline_object = inline
                            .as_object()
                            .ok_or_else(|| "inline node must be an object".to_owned())?;
                        match inline_object.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let inline_text = inline_object
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .ok_or_else(|| "text node text must be a string".to_owned())?;
                                let marks = inline_object
                                    .get("marks")
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default();
                                for scalar in inline_text.chars() {
                                    utf16_units = utf16_units
                                        .checked_add(scalar.len_utf16() as u32)
                                        .ok_or_else(|| {
                                            "block UTF-16 length overflowed".to_owned()
                                        })?;
                                    text.push(scalar);
                                    tokens.push(StructuralToken::Scalar {
                                        scalar: scalar.to_string(),
                                        marks: marks.clone(),
                                        style: style.clone(),
                                    });
                                }
                            }
                            Some("hardBreak") => {
                                utf16_units = utf16_units
                                    .checked_add(1)
                                    .ok_or_else(|| "block UTF-16 length overflowed".to_owned())?;
                                text.push('\n');
                                tokens.push(StructuralToken::HardBreak {
                                    style: style.clone(),
                                });
                            }
                            Some(other) => {
                                return Err(format!("unsupported inline node type {other:?}"));
                            }
                            None => return Err("inline node type is required".to_owned()),
                        }
                    }
                }
                let content_end = tokens.len();
                tokens.push(StructuralToken::CloseBlock);
                (
                    Some(style),
                    content_start,
                    content_end,
                    tokens.len(),
                    utf16_units,
                    text,
                )
            }
            "sceneBreak" => {
                tokens.push(StructuralToken::SceneBreak {
                    id: id.clone(),
                    attrs: attrs.clone(),
                });
                (
                    None,
                    token_start,
                    token_start + 1,
                    tokens.len(),
                    0,
                    String::new(),
                )
            }
            other => return Err(format!("unsupported block type {other:?}")),
        };
        spans.push(BlockSpan {
            id,
            block_type: block_type.to_owned(),
            attrs,
            style,
            token_start,
            content_start,
            content_end,
            token_end,
            utf16_units,
            text,
        });
    }
    Ok(TokenDocument {
        tokens,
        blocks: spans,
        by_id,
    })
}

fn scope_range(document: &TokenDocument, scope: &ScopeGrant) -> Result<TokenRange, String> {
    match scope.kind {
        ScopeKind::WholeDocument => Ok(TokenRange {
            start: 0,
            end: document.tokens.len(),
            first_block: 0,
            last_block: document.blocks.len().saturating_sub(1),
            selected_ids: document.blocks.len(),
        }),
        ScopeKind::Passage => {
            let start = scope
                .start
                .as_ref()
                .ok_or_else(|| "passage scope requires start".to_owned())?;
            let end = scope
                .end
                .as_ref()
                .ok_or_else(|| "passage scope requires end".to_owned())?;
            let first_block = block_index(document, &start.block_id)?;
            let last_block = block_index(document, &end.block_id)?;
            if first_block > last_block {
                return Err("scope start must not be after scope end".to_owned());
            }
            let start_block = &document.blocks[first_block];
            if first_block == last_block && start_block.style.is_none() {
                return Err("passage endpoints must be inside a text block".to_owned());
            }
            let start_token = endpoint_token(document, first_block, start)?;
            let end_token = endpoint_token(document, last_block, end)?;
            if start_token > end_token {
                return Err("scope start must not be after scope end".to_owned());
            }
            Ok(TokenRange {
                start: start_token,
                end: end_token,
                first_block,
                last_block,
                selected_ids: last_block - first_block + 1,
            })
        }
        ScopeKind::Blocks => {
            let start = scope
                .start
                .as_ref()
                .ok_or_else(|| "blocks scope requires start".to_owned())?;
            let end = scope
                .end
                .as_ref()
                .ok_or_else(|| "blocks scope requires end".to_owned())?;
            let first = block_index(document, &start.block_id)?;
            let last = block_index(document, &end.block_id)?;
            if first > last {
                return Err("scope start must not be after scope end".to_owned());
            }
            let first_span = &document.blocks[first];
            let last_span = &document.blocks[last];
            validate_endpoint_boundary(document, first, start.utf16_offset)?;
            validate_endpoint_boundary(document, last, end.utf16_offset)?;
            if start.utf16_offset != 0 {
                return Err(
                    "blocks scope must start at the beginning of its first block".to_owned(),
                );
            }
            if end.utf16_offset != last_span.utf16_units {
                return Err("blocks scope must end at the end of its last block".to_owned());
            }
            Ok(TokenRange {
                start: first_span.token_start,
                end: last_span.token_end,
                first_block: first,
                last_block: last,
                selected_ids: last - first + 1,
            })
        }
    }
}

fn block_index(document: &TokenDocument, id: &str) -> Result<usize, String> {
    document
        .by_id
        .get(id)
        .copied()
        .ok_or_else(|| format!("scope endpoint blockId {id:?} does not exist"))
}

fn endpoint_token(
    document: &TokenDocument,
    block_index: usize,
    endpoint: &Endpoint,
) -> Result<usize, String> {
    validate_endpoint_boundary(document, block_index, endpoint.utf16_offset)?;
    let block = &document.blocks[block_index];
    let mut offset = 0_u32;
    for (index, token) in document.tokens[block.content_start..block.content_end]
        .iter()
        .enumerate()
    {
        if offset == endpoint.utf16_offset {
            return Ok(block.content_start + index);
        }
        offset += token_width(token);
    }
    Ok(block.content_end)
}

fn token_width(token: &StructuralToken) -> u32 {
    match token {
        StructuralToken::Scalar { scalar, .. } => scalar.encode_utf16().count() as u32,
        StructuralToken::HardBreak { .. } => 1,
        StructuralToken::OpenBlock { .. }
        | StructuralToken::CloseBlock
        | StructuralToken::SceneBreak { .. } => 0,
    }
}

fn validate_endpoint_boundary(
    document: &TokenDocument,
    block_index: usize,
    utf16_offset: u32,
) -> Result<(), String> {
    let block = &document.blocks[block_index];
    if utf16_offset > block.utf16_units {
        return Err(format!(
            "endpoint utf16Offset {} is outside block {} (length {})",
            utf16_offset, block.id, block.utf16_units
        ));
    }
    if block.style.is_none() {
        if utf16_offset != 0 {
            return Err("sceneBreak endpoint must use utf16Offset 0".to_owned());
        }
        return Ok(());
    }
    let mut boundaries = HashSet::new();
    boundaries.insert(0_u32);
    let mut utf16_by_byte = HashMap::new();
    let mut utf16 = 0_u32;
    for (byte_index, scalar) in block.text.char_indices() {
        utf16_by_byte.insert(byte_index, utf16);
        utf16 += scalar.len_utf16() as u32;
    }
    utf16_by_byte.insert(block.text.len(), utf16);
    for (byte_index, grapheme) in block.text.grapheme_indices(true) {
        let start = *utf16_by_byte
            .get(&byte_index)
            .ok_or_else(|| "failed to map grapheme start to UTF-16".to_owned())?;
        let end_byte = byte_index + grapheme.len();
        let end = *utf16_by_byte
            .get(&end_byte)
            .ok_or_else(|| "failed to map grapheme end to UTF-16".to_owned())?;
        boundaries.insert(start);
        boundaries.insert(end);
    }
    if !boundaries.contains(&utf16_offset) {
        return Err(format!(
            "endpoint utf16Offset {utf16_offset} splits a grapheme cluster or surrogate pair"
        ));
    }
    Ok(())
}

fn validate_ids_and_merge_policy(
    source: &TokenDocument,
    result: &TokenDocument,
    replacement: &[StructuralToken],
    range: &TokenRange,
    kind: ScopeKind,
) -> Result<(), String> {
    let source_ids: HashSet<&str> = source
        .blocks
        .iter()
        .map(|block| block.id.as_str())
        .collect();
    let selected_ids: HashSet<&str> = source.blocks[range.first_block..=range.last_block]
        .iter()
        .map(|block| block.id.as_str())
        .collect();
    let replacement_ids: HashSet<&str> = replacement
        .iter()
        .filter_map(|token| match token {
            StructuralToken::OpenBlock { id, .. } | StructuralToken::SceneBreak { id, .. } => {
                Some(id.as_str())
            }
            _ => None,
        })
        .collect();
    let result_selected_ids: HashSet<&str> = result
        .blocks
        .iter()
        .filter_map(|block| {
            if selected_ids.contains(block.id.as_str()) {
                Some(block.id.as_str())
            } else {
                None
            }
        })
        .collect();

    for id in &replacement_ids {
        if source_ids.contains(id) && !selected_ids.contains(id) {
            return Err(format!(
                "replacement reuses unselected source block id {id:?}"
            ));
        }
    }

    let source_block_by_id: HashMap<&str, &BlockSpan> = source
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let result_block_by_id: HashMap<&str, &BlockSpan> = result
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    for id in replacement_ids
        .iter()
        .filter(|id| kind == ScopeKind::Passage && source_ids.contains(*id))
    {
        let old = source_block_by_id[*id];
        let new = result_block_by_id[*id];
        if old.block_type != new.block_type || old.attrs != new.attrs {
            return Err(format!(
                "retained block id {id:?} changes its block style or attributes"
            ));
        }
    }

    if kind != ScopeKind::WholeDocument && range.selected_ids > 1 {
        let selected_styles: Vec<_> = source.blocks[range.first_block..=range.last_block]
            .iter()
            .map(|block| block.style.clone())
            .collect();
        let unselected_block_count = source.blocks.len() - range.selected_ids;
        let result_selected_block_count =
            result.blocks.len().saturating_sub(unselected_block_count);
        let merge = kind == ScopeKind::Passage
            || (result_selected_block_count == 1 && !result_selected_ids.is_empty());
        if merge && result_selected_block_count < range.selected_ids {
            if !result_selected_ids.contains(source.blocks[range.first_block].id.as_str()) {
                return Err("a cross-block merge must retain the left block id".to_owned());
            }
            for block in &source.blocks[range.first_block + 1..=range.last_block] {
                if result_selected_ids.contains(block.id.as_str()) {
                    return Err(format!(
                        "a cross-block merge must retire the right block id {:?}",
                        block.id
                    ));
                }
            }
            let first_style = &selected_styles[0];
            if kind == ScopeKind::Passage
                && selected_styles.iter().any(|style| style != first_style)
            {
                return Err("cross-block merge must preserve one compatible block style".to_owned());
            }
        }
    }

    // Every result block is either an explicitly selected source identity or a
    // fresh ID. This rejects accidental reuse of retired/source IDs even when
    // the replacement segment happens to have the same plain text.
    for block in &result.blocks {
        if !source_ids.contains(block.id.as_str()) && !replacement_ids.contains(block.id.as_str()) {
            return Err(format!(
                "result contains an unaccounted block id {:?}",
                block.id
            ));
        }
    }

    Ok(())
}

impl StructuralToken {
    fn is_boundary(&self) -> bool {
        matches!(
            self,
            Self::OpenBlock { .. } | Self::CloseBlock | Self::SceneBreak { .. }
        )
    }
}

fn plain_text(tokens: &[StructuralToken]) -> String {
    let mut text = String::new();
    for token in tokens {
        match token {
            StructuralToken::Scalar { scalar, .. } => text.push_str(scalar),
            StructuralToken::HardBreak { .. } => text.push('\n'),
            StructuralToken::CloseBlock => text.push_str("\n\n"),
            StructuralToken::OpenBlock { .. } => {}
            StructuralToken::SceneBreak { .. } => text.push_str("\n\n"),
        }
    }
    // Omit only the final outer block separator, never literal selected line
    // breaks. A quotation ending in two hardBreaks must retain those units.
    let trailing_separators = tokens
        .iter()
        .rev()
        .take_while(|token| token.is_boundary())
        .filter(|token| {
            matches!(
                token,
                StructuralToken::CloseBlock | StructuralToken::SceneBreak { .. }
            )
        })
        .count();
    text.truncate(text.len().saturating_sub(trailing_separators * 2));
    text
}

fn context_prefix(document: &TokenDocument, block_index: usize, token_index: usize) -> String {
    let block = &document.blocks[block_index];
    if token_index <= block.content_start {
        return String::new();
    }
    plain_text(&document.tokens[block.content_start..token_index])
}

fn context_suffix(document: &TokenDocument, block_index: usize, token_index: usize) -> String {
    let block = &document.blocks[block_index];
    if token_index >= block.content_end {
        return String::new();
    }
    plain_text(&document.tokens[token_index..block.content_end])
}

fn structured_hash(tokens: &[StructuralToken]) -> Result<String, String> {
    let value = serde_json::to_vec(tokens)
        .map_err(|error| format!("failed to serialize scope quote: {error}"))?;
    Ok(sha256_hex(&value))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn snapshot(content: Value) -> Value {
        json!({
            "schemaVersion": 1,
            "body": {"type": "doc", "content": content}
        })
    }

    fn paragraph(id: &str, text: &str) -> Value {
        json!({"type":"paragraph","attrs":{"id":id},"content":[{"type":"text","text":text}]})
    }

    fn request(source: Value, result: Value, scope: ScopeGrant) -> ScopeValidationRequest {
        ScopeValidationRequest {
            source_snapshot: source,
            result_snapshot: result,
            scope,
        }
    }

    fn bound_scope(source: &Value, mut scope: ScopeGrant) -> ScopeGrant {
        let receipt = canonical_snapshot(source).expect("canonical source");
        let document = token_document(&receipt.snapshot).expect("source tokens");
        let range = scope_range(&document, &scope).expect("scope range");
        scope.source_hash = receipt.hash;
        scope.quote = plain_text(&document.tokens[range.start..range.end]);
        scope.quote_hash =
            structured_hash(&document.tokens[range.start..range.end]).expect("quote hash");
        scope
    }

    #[test]
    fn complete_block_grant_authorizes_style_change_and_block_deletion() {
        let source = snapshot(json!([
            paragraph("a", "First"),
            paragraph("b", "Second"),
            paragraph("outside", "Keep me")
        ]));
        let scope = ScopeGrant {
            kind: ScopeKind::Blocks,
            start: Some(Endpoint {
                block_id: "a".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "b".into(),
                utf16_offset: 6,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        };
        let grant = capture_scope(&source, scope).unwrap();
        let deleted = snapshot(json!([paragraph("outside", "Keep me")]));
        validate_scope(&request(source.clone(), deleted, grant.clone())).unwrap();
        let mut styled = source.clone();
        styled["body"]["content"][0]["type"] = json!("heading");
        styled["body"]["content"][0]["attrs"]["level"] = json!(2);
        validate_scope(&request(source, styled, grant)).unwrap();
    }

    #[test]
    fn exact_quote_preserves_selected_trailing_hard_breaks() {
        let source = snapshot(
            json!([{"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":"A"},{"type":"hardBreak"},{"type":"hardBreak"}]}]),
        );
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 3,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        };
        let grant = capture_scope(&source, scope).unwrap();
        assert_eq!(grant.quote, "A\n\n");
        validate_scope(&request(source.clone(), source, grant)).unwrap();
    }

    #[test]
    fn passage_preserves_unselected_tokens() {
        let source = snapshot(json!([paragraph("p", "before middle after")]));
        let result = snapshot(json!([paragraph("p", "before changed after")]));
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 7,
            }),
            end: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 13,
            }),
            source_hash: String::new(),
            quote: "middle".into(),
            quote_hash: String::new(),
            prefix: Some("before ".into()),
            suffix: Some(" after".into()),
        };
        let receipt = validate_scope(&request(
            source.clone(),
            result,
            bound_scope(&source, scope),
        ))
        .expect("valid passage");
        assert_eq!(receipt.quote, "middle");
        assert_eq!(receipt.prefix, "before ");
        assert_eq!(receipt.suffix, " after");
    }

    #[test]
    fn structural_tokens_carry_marks_and_block_style() {
        let source = snapshot(json!([{
            "type": "heading",
            "attrs": {"id": "h", "level": 2},
            "content": [
                {"type": "text", "text": "A", "marks": [{"type": "bold"}]},
                {"type": "hardBreak"},
                {"type": "text", "text": "B"}
            ]
        }]));
        let receipt = canonical_snapshot(&source).expect("canonical source");
        let tokens = structural_tokens(&receipt.snapshot).expect("tokens");
        assert!(matches!(
            &tokens[1],
            StructuralToken::Scalar { scalar, marks, style }
                if scalar == "A"
                    && marks == &vec![json!({"type": "bold"})]
                    && style.block_type == "heading"
                    && style.level == Some(2)
        ));
        assert!(matches!(
            &tokens[2],
            StructuralToken::HardBreak { style }
                if style.block_type == "heading" && style.level == Some(2)
        ));
        assert!(matches!(&tokens[4], StructuralToken::CloseBlock));
    }

    #[test]
    fn combining_and_zwj_boundaries_are_checked() {
        let source = snapshot(json!([paragraph("p", "a\u{301} 👩‍💻 z")]));
        let result = source.clone();
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 1,
            }),
            end: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 2,
            }),
            ..ScopeGrant {
                kind: ScopeKind::Passage,
                start: None,
                end: None,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            }
        };
        let error =
            validate_scope(&request(source.clone(), result, scope)).expect_err("inside cluster");
        assert!(error.contains("grapheme"), "{error}");
    }

    #[test]
    fn cross_block_merge_retains_left_and_retires_right() {
        let source = snapshot(json!([paragraph("left", "a"), paragraph("right", "b")]));
        let result = snapshot(json!([paragraph("left", "a b")]));
        let scope = ScopeGrant {
            kind: ScopeKind::Blocks,
            start: Some(Endpoint {
                block_id: "left".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "right".into(),
                utf16_offset: 1,
            }),
            ..ScopeGrant {
                kind: ScopeKind::Blocks,
                start: None,
                end: None,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            }
        };
        validate_scope(&request(
            source.clone(),
            result,
            bound_scope(&source, scope),
        ))
        .expect("valid merge");
    }

    #[test]
    fn passage_can_cross_paragraphs_and_merge_boundaries() {
        let source = snapshot(json!([paragraph("left", "a"), paragraph("right", "b")]));
        let result = snapshot(json!([paragraph("left", "a b")]));
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "left".into(),
                utf16_offset: 1,
            }),
            end: Some(Endpoint {
                block_id: "right".into(),
                utf16_offset: 1,
            }),
            ..ScopeGrant {
                kind: ScopeKind::Passage,
                start: None,
                end: None,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            }
        };
        validate_scope(&request(
            source.clone(),
            result,
            bound_scope(&source, scope),
        ))
        .expect("valid cross-paragraph passage merge");
    }

    #[test]
    fn passage_can_cross_paragraphs_without_merging_them() {
        let source = snapshot(json!([paragraph("left", "a"), paragraph("right", "b")]));
        let result = snapshot(json!([paragraph("left", "x"), paragraph("right", "y")]));
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "left".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "right".into(),
                utf16_offset: 1,
            }),
            ..ScopeGrant {
                kind: ScopeKind::Passage,
                start: None,
                end: None,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            }
        };
        validate_scope(&request(
            source.clone(),
            result,
            bound_scope(&source, scope),
        ))
        .expect("valid cross-paragraph passage replacement");
    }

    #[test]
    fn passage_cannot_insert_boundary() {
        let source = snapshot(json!([paragraph("p", "a b")]));
        let result = snapshot(json!([paragraph("p", "a"), paragraph("new", "b")]));
        let scope = ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 1,
            }),
            end: Some(Endpoint {
                block_id: "p".into(),
                utf16_offset: 2,
            }),
            ..ScopeGrant {
                kind: ScopeKind::Passage,
                start: None,
                end: None,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            }
        };
        let error = validate_scope(&request(
            source.clone(),
            result,
            bound_scope(&source, scope),
        ))
        .expect_err("boundary insertion");
        assert!(
            error.contains("unselected") || error.contains("boundar"),
            "{error}"
        );
    }
}
