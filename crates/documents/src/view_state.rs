//! The renderer's saved caret, and its validation against a stored document.
//!
//! A view state is a document id, a head, and two endpoints into that head's
//! blocks. It belongs here rather than with the project actor because `transfer`
//! validates one before accepting a backup, and two of its three readers are
//! document operations. Everything it needs — `read_document`, `Endpoint`,
//! `Head` — is at L2 or below.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use wns_kernel::{CoreError, CoreResult, Head, check_id, parse_stored_version, valid_hash};
use wns_storage::read_document;

use crate::scope::Endpoint;

/// Where the renderer's caret, anchor and focus sat in a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewState {
    pub document_id: String,
    pub head: Head,
    pub anchor: Endpoint,
    pub focus: Endpoint,
}

pub fn read_view_state(connection: &Connection) -> CoreResult<Option<ViewState>> {
    let row: Option<(String, i64, String, String, i64, String, i64)> = connection
        .query_row(
            "SELECT document_id,head_version,head_body_hash,anchor_block_id,anchor_utf16_offset,focus_block_id,focus_utf16_offset FROM view_state WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((document_id, version, hash, anchor_block, anchor_offset, focus_block, focus_offset)) =
        row
    else {
        return Ok(None);
    };
    check_id(&document_id)?;
    let version = parse_stored_version(version)?;
    if !valid_hash(&hash) || anchor_offset < 0 || focus_offset < 0 {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved view state contains an invalid head or endpoint.",
        ));
    }
    let anchor_offset = u32::try_from(anchor_offset).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved view anchor exceeds the supported UTF-16 range.",
        )
    })?;
    let focus_offset = u32::try_from(focus_offset).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved view focus exceeds the supported UTF-16 range.",
        )
    })?;
    check_id(&anchor_block)?;
    check_id(&focus_block)?;
    Ok(Some(ViewState {
        document_id: document_id.clone(),
        head: Head {
            document_id,
            version,
            body_hash: hash,
        },
        anchor: Endpoint {
            block_id: anchor_block,
            utf16_offset: anchor_offset,
        },
        focus: Endpoint {
            block_id: focus_block,
            utf16_offset: focus_offset,
        },
    }))
}

pub fn validate_stored_view_state(connection: &Connection) -> CoreResult<()> {
    let Some(state) = read_view_state(connection)? else {
        return Ok(());
    };
    let current = match read_document(connection, &state.document_id) {
        Ok(current) => current,
        Err(error) if error.code == "DocumentNotFound" => return Ok(()),
        Err(error) => return Err(error),
    };
    if current.head == state.head {
        validate_endpoint(&current.body, &state.anchor)?;
        validate_endpoint(&current.body, &state.focus)?;
    }
    Ok(())
}

pub fn validate_endpoint(body: &Value, endpoint: &Endpoint) -> CoreResult<()> {
    check_id(&endpoint.block_id)?;
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::new("InvalidDocument", "The document has no block content."))?;
    let block = blocks.iter().find(|block| {
        block
            .get("attrs")
            .and_then(Value::as_object)
            .and_then(|attrs| attrs.get("id"))
            .and_then(Value::as_str)
            == Some(endpoint.block_id.as_str())
    });
    let block = block.ok_or_else(|| {
        CoreError::new(
            "InvalidRequest",
            "The saved view endpoint refers to an unknown block.",
        )
    })?;
    let block_type = block
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if block_type == "sceneBreak" && endpoint.utf16_offset == 0 {
        return Ok(());
    }
    if !matches!(block_type, "paragraph" | "heading") {
        return Err(CoreError::new(
            "InvalidRequest",
            "View endpoints must be inside a paragraph or heading.",
        ));
    }
    let content = block
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut offset = 0_u32;
    let mut boundaries = HashSet::from([0_u32]);
    for inline in content {
        match inline.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = inline
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| CoreError::new("InvalidDocument", "A text node has no text."))?;
                for (byte_offset, _) in
                    unicode_segmentation::UnicodeSegmentation::grapheme_indices(text, true)
                {
                    let units = u32::try_from(text[..byte_offset].encode_utf16().count())
                        .map_err(|_| CoreError::new("InvalidDocument", "Text is too long."))?;
                    boundaries.insert(offset + units);
                }
                offset = offset
                    .checked_add(
                        u32::try_from(text.encode_utf16().count())
                            .map_err(|_| CoreError::new("InvalidDocument", "Text is too long."))?,
                    )
                    .ok_or_else(|| CoreError::new("InvalidDocument", "Text is too long."))?;
                boundaries.insert(offset);
            }
            Some("hardBreak") => {
                offset = offset
                    .checked_add(1)
                    .ok_or_else(|| CoreError::new("InvalidDocument", "Text is too long."))?;
                boundaries.insert(offset);
            }
            _ => {
                return Err(CoreError::new(
                    "InvalidDocument",
                    "The document contains an unsupported inline node.",
                ));
            }
        }
    }
    if !boundaries.contains(&endpoint.utf16_offset) {
        return Err(CoreError::new(
            "InvalidRequest",
            "The saved view endpoint is not at a grapheme boundary.",
        ));
    }
    Ok(())
}
