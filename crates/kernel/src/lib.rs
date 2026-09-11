//! L0 — foundation types shared by every layer.
//!
//! Two things live here and nothing else does:
//!
//! * [`CoreError`] / [`CoreResult`] / [`Head`] — the error and identity types
//!   every layer must be able to name. They live at L0 so that `wns-storage`
//!   can be extracted without depending on the crate that holds the document
//!   model, which is what today's storage↔projects cycle is made of.
//! * W0 snapshot validation and canonicalization — the editor contract's Rust
//!   half. The validator deliberately accepts only the small document vocabulary
//!   used by the first slice. It constructs a canonical document while parsing
//!   instead of accepting arbitrary ProseMirror extensions and trying to strip
//!   them later.
//!
//! This crate depends on nothing else in the workspace. Everything depends on it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;

pub type CoreResult<T> = Result<T, CoreError>;

/// The reply channel one actor command answers on.
///
/// Moved down from `projects/session.rs`: a command enum cannot leave
/// `webnovel-core` while its variants name this type, and every module's
/// command enum names it.
pub type Reply<T> = std::sync::mpsc::SyncSender<CoreResult<T>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreError {
    pub code: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_head: Option<Head>,
}
impl CoreError {
    pub fn new(code: &str, detail: &str) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
            current_head: None,
        }
    }
    /// Generalized from `rusqlite::Error` to any `Display` so the kernel does
    /// not have to name a persistence error type. Every existing call site is a
    /// `map_err(CoreError::uncertain)` over a `rusqlite::Error`, which still
    /// infers unchanged.
    pub fn uncertain(error: impl std::fmt::Display) -> Self {
        Self::new(
            "UncertainOutcome",
            &format!("The commit outcome must be reconciled: {error}"),
        )
    }
    pub fn disconnected() -> Self {
        Self::new(
            "UncertainOutcome",
            "The project connection stopped. Keep your text and reopen the project to reconcile.",
        )
    }
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for CoreError {}
impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new("PersistenceUnavailable", &error.to_string())
    }
}
impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        Self::new("PersistenceUnavailable", &error.to_string())
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("InvalidDocument", &error.to_string())
    }
}

/// A document identity pinned to an exact version and body hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Head {
    pub document_id: String,
    pub version: String,
    pub body_hash: String,
}

/// Validate the shape of an identifier used across project records.
///
/// Moved down from `projects.rs`, where it was a 12-line private helper with
/// 300 call sites. It is an identity primitive of the same class as [`Head`],
/// and record vocabulary at L2 needs it to validate its own shapes.
pub fn check_id(id: &str) -> CoreResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Identifiers must contain 1–64 ASCII letters, digits, dashes or underscores.",
        ));
    }
    Ok(())
}

/// One renderer's explicit lease on an open project.
///
/// Moved down from `projects/records.rs` because the packet vocabulary embeds
/// it — `SearchStory` and `FreezeStory` carry an `access` — so it has to sit at
/// or below the compiler. It is four strings and no behaviour.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAccess {
    pub project_id: String,
    pub session: String,
    pub writer_lease: String,
    pub operation_namespace: String,
}

/// Parse a canonical nonnegative decimal version string.
///
/// Moved down from `projects.rs` with its two siblings below. All three are used
/// by eleven to twenty modules apiece, which is what makes them foundation
/// rather than project logic: a module cannot leave the crate while the helpers
/// its bodies call stay behind.
pub fn parse_version(value: &str) -> CoreResult<i64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Versions must be canonical nonnegative decimal strings.",
        ));
    }
    value
        .parse()
        .map_err(|_| CoreError::new("InvalidRequest", "Version exceeds the supported range."))
}

/// Render a stored integer version as its canonical decimal string.
pub fn parse_stored_version(value: i64) -> CoreResult<String> {
    if value < 0 {
        return Err(CoreError::new(
            "InvalidProject",
            "The project contains a negative metadata version.",
        ));
    }
    Ok(value.to_string())
}

/// The hash a request is deduplicated on: its canonical JSON with the renderer
/// lease fields removed, so the same logical request from a new session hashes
/// the same.
pub fn logical_hash<T: Serialize>(request: &T) -> CoreResult<String> {
    let mut value = serde_json::to_value(request)?;
    if let Some(access) = value.get_mut("access").and_then(Value::as_object_mut) {
        access.remove("session");
        access.remove("writerLease");
    }
    Ok(sha256_hex(
        serde_json::to_string(&canonicalize_value(value))?.as_bytes(),
    ))
}

/// One immutable saved revision of a document.
///
/// Moved down from `projects/records.rs` so the packet compiler's input
/// vocabulary can name it: `story_records` — the shapes a compiled packet
/// carries — depends on this type, and a layer-2 crate may not reach up to the
/// crate under decomposition for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub head: Head,
    pub body: Value,
    pub reason: String,
    pub parent_id: Option<String>,
}

/// One document row, typed.
///
/// Moved down from `webnovel-core::projects::records` so that the row readers
/// in `wns-storage` can name what they return without reaching up into the
/// crate under decomposition. The serde attributes are unchanged: they are what
/// keeps historical serialized records byte-compatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentRecord {
    pub head: Head,
    pub title: String,
    pub kind: String,
    pub metadata_version: String,
    pub body: Value,
    pub last_checkpoint_id: Option<String>,
    /// Ordinary documents are the only records exposed through the generic
    /// editor and story-context APIs.  Assistant drafts and conversation
    /// anchors use explicit, typed paths and are omitted from legacy JSON so
    /// historical previews and hashes remain byte-compatible.
    #[serde(default, skip_serializing_if = "DocumentRole::is_ordinary")]
    pub role: DocumentRole,
}

/// Authority role for a document row.  This is deliberately an enum rather
/// than a title/ID convention so every source consumer can apply the same
/// fence.  New roles must be added with a reader-floor migration.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum DocumentRole {
    #[default]
    Ordinary,
    AssistantDraft,
    ConversationAnchor,
}

impl DocumentRole {
    /// Public because the row readers in `wns-storage` decode with it.
    pub fn storage_name(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::AssistantDraft => "assistantDraft",
            Self::ConversationAnchor => "conversationAnchor",
        }
    }

    /// Public because the row readers in `wns-storage` decode with it.
    pub fn from_storage(value: &str) -> CoreResult<Self> {
        match value {
            "ordinary" => Ok(Self::Ordinary),
            "assistantDraft" => Ok(Self::AssistantDraft),
            "conversationAnchor" => Ok(Self::ConversationAnchor),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The document contains an unknown authority role.",
            )),
        }
    }

    fn is_ordinary(&self) -> bool {
        matches!(self, Self::Ordinary)
    }
}

/// The restore half of a command receipt's stored result.
///
/// Receipt vocabulary: [`StoredResult`] names it, so it has to sit at or below
/// every crate that stores or reads a receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoredDecision {
    pub revision_id: String,
    pub before_revision_id: String,
    pub after_revision_id: String,
}

/// The apply half of a command receipt's stored result.
///
/// Receipt vocabulary by the same argument as [`RestoredDecision`]: it moved
/// down as a leaf type, five strings and no behaviour, so that `StoredResult`
/// can follow it without dragging `proposals` along.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedDecision {
    pub decision_id: String,
    pub proposal_id: String,
    pub prepared_id: String,
    pub before_revision_id: String,
    pub after_revision_id: String,
}

/// The immutable decision a command receipt records.
///
/// This is the type `wns-storage::existing_receipt` returns and
/// `wns-storage::insert_receipt` writes, so it lives at L0 with them rather
/// than beside any one command that produces one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredResult {
    pub head: Head,
    pub saved_generation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<AppliedDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored: Option<RestoredDecision>,
}

/// A fresh identifier. An id is an opaque string to every layer above this one,
/// and to the database.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A body fingerprint is exactly 64 hex digits, and nothing else is one.
/// Callers write this predicate as a validity fence over stored rows, so it
/// belongs beside the hashes it checks rather than in any one reader.
pub fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A title is at most 512 bytes, non-blank and free of control characters.
///
/// A ninth helper of the same kind as [`valid_hash`], found the same way —
/// by a module moving and the compiler naming what it could no longer see.
/// Nine callers, and none of them a reason for it to live above the schema.
pub fn validate_title(title: &str) -> CoreResult<()> {
    if title.trim().is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
        return Err(CoreError::new(
            "InvalidRequest",
            "Enter a title of at most 512 bytes without control characters.",
        ));
    }
    Ok(())
}

/// Require a document to be exactly at an expected head, or report a conflict
/// carrying the head the caller actually found.
///
/// The `current_head` field is what the editor reconciles against, so this is
/// the one place a version conflict is constructed.
pub fn require_head(current: &Head, expected: &Head) -> CoreResult<()> {
    parse_version(&expected.version)?;
    if current != expected {
        let mut error = CoreError::new(
            "VersionConflict",
            "This document has a newer saved version. Keep your text and reconcile.",
        );
        error.current_head = Some(current.clone());
        return Err(error);
    }
    Ok(())
}

const MAX_RAW_BYTES: usize = 2 * 1024 * 1024;
const MAX_UTF16_UNITS: u64 = 1_000_000;
const MAX_BLOCKS: usize = 10_000;

/// The result of validating and canonicalizing a W0 snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReceipt {
    pub snapshot: Value,
    pub canonical_json: String,
    pub hash: String,
    pub utf16_units: u32,
    pub block_count: u32,
}

#[derive(Debug, Clone)]
enum InlineNode {
    Text { text: String, marks: Vec<Value> },
    HardBreak,
}

/// Validate a W0 `WnsDocument` JSON string and return its canonical receipt.
pub fn validate_snapshot_json(input: &str) -> Result<SnapshotReceipt, String> {
    if input.len() > MAX_RAW_BYTES {
        return Err(format!("document exceeds {} byte limit", MAX_RAW_BYTES));
    }

    let parsed: Value =
        serde_json::from_str(input).map_err(|error| format!("invalid JSON: {error}"))?;
    let root = as_object(&parsed, "document")?;
    check_fields(
        root,
        &["schemaVersion", "body"],
        &["schemaVersion", "body"],
        "document",
    )?;
    if root.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("document.schemaVersion must be 1".to_owned());
    }

    let body = as_object(required(root, "body", "document")?, "document.body")?;
    check_fields(
        body,
        &["type", "content"],
        &["type", "content"],
        "document.body",
    )?;
    if string_field(body, "type", "document.body")? != "doc" {
        return Err("document.body.type must be doc".to_owned());
    }
    let blocks = as_array(
        required(body, "content", "document.body")?,
        "document.body.content",
    )?;
    if blocks.is_empty() {
        return Err("document.body.content must not be empty".to_owned());
    }
    if blocks.len() > MAX_BLOCKS {
        return Err(format!("document has more than {MAX_BLOCKS} blocks"));
    }

    let mut ids = HashSet::with_capacity(blocks.len());
    let mut utf16_units = 0_u64;
    let mut canonical_blocks = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        canonical_blocks.push(parse_block(block, index, &mut ids, &mut utf16_units)?);
    }

    let mut canonical_body = Map::new();
    canonical_body.insert("content".to_owned(), Value::Array(canonical_blocks));
    canonical_body.insert("type".to_owned(), Value::String("doc".to_owned()));
    let mut snapshot = Map::new();
    snapshot.insert("body".to_owned(), Value::Object(canonical_body));
    snapshot.insert("schemaVersion".to_owned(), Value::from(1));
    let snapshot = canonicalize_value(Value::Object(snapshot));
    let canonical_json = serde_json::to_string(&snapshot)
        .map_err(|error| format!("failed to serialize canonical snapshot: {error}"))?;
    let hash = sha256_hex(canonical_json.as_bytes());

    Ok(SnapshotReceipt {
        snapshot,
        canonical_json,
        hash,
        utf16_units: u32::try_from(utf16_units)
            .map_err(|_| "UTF-16 unit count exceeds u32".to_owned())?,
        block_count: u32::try_from(blocks.len())
            .map_err(|_| "block count exceeds u32".to_owned())?,
    })
}

fn parse_block(
    value: &Value,
    index: usize,
    ids: &mut HashSet<String>,
    utf16_units: &mut u64,
) -> Result<Value, String> {
    let path = format!("document.body.content[{index}]");
    let object = as_object(value, &path)?;
    let node_type = string_field(object, "type", &path)?;
    match node_type {
        "paragraph" | "heading" => {
            check_fields(
                object,
                &["type", "attrs", "content"],
                &["type", "attrs"],
                &path,
            )?;
            let attrs = as_object(required(object, "attrs", &path)?, &format!("{path}.attrs"))?;
            let attrs_path = format!("{path}.attrs");
            if node_type == "heading" {
                check_fields(attrs, &["id", "level"], &["id", "level"], &attrs_path)?;
            } else {
                check_fields(attrs, &["id"], &["id"], &attrs_path)?;
            }
            let id = string_field(attrs, "id", &attrs_path)?.to_owned();
            validate_id(&id, &attrs_path)?;
            if !ids.insert(id.clone()) {
                return Err(format!("{attrs_path}.id is duplicated"));
            }

            let mut canonical_attrs = Map::new();
            canonical_attrs.insert("id".to_owned(), Value::String(id));
            if node_type == "heading" {
                let level = attrs
                    .get("level")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| format!("{attrs_path}.level must be an integer"))?;
                if !(1..=3).contains(&level) {
                    return Err(format!("{attrs_path}.level must be between 1 and 3"));
                }
                canonical_attrs.insert("level".to_owned(), Value::from(level));
            }

            let canonical_content = match object.get("content") {
                None => None,
                Some(content) => {
                    let content_path = format!("{path}.content");
                    let content = as_array(content, &content_path)?;
                    let mut canonical = Vec::with_capacity(content.len());
                    for (inline_index, inline) in content.iter().enumerate() {
                        canonical.push(parse_inline(
                            inline,
                            &format!("{content_path}[{inline_index}]"),
                            utf16_units,
                        )?);
                    }
                    Some(merge_adjacent_text(canonical))
                }
            };

            let mut result = Map::new();
            result.insert("attrs".to_owned(), Value::Object(canonical_attrs));
            if let Some(content) = canonical_content.filter(|content| !content.is_empty()) {
                result.insert(
                    "content".to_owned(),
                    Value::Array(content.into_iter().map(inline_to_value).collect()),
                );
            }
            result.insert("type".to_owned(), Value::String(node_type.to_owned()));
            Ok(canonicalize_value(Value::Object(result)))
        }
        "sceneBreak" => {
            check_fields(object, &["type", "attrs"], &["type", "attrs"], &path)?;
            let attrs = as_object(required(object, "attrs", &path)?, &format!("{path}.attrs"))?;
            let attrs_path = format!("{path}.attrs");
            check_fields(attrs, &["id"], &["id"], &attrs_path)?;
            let id = string_field(attrs, "id", &attrs_path)?.to_owned();
            validate_id(&id, &attrs_path)?;
            if !ids.insert(id.clone()) {
                return Err(format!("{attrs_path}.id is duplicated"));
            }
            let mut canonical_attrs = Map::new();
            canonical_attrs.insert("id".to_owned(), Value::String(id));
            let mut result = Map::new();
            result.insert("attrs".to_owned(), Value::Object(canonical_attrs));
            result.insert("type".to_owned(), Value::String("sceneBreak".to_owned()));
            Ok(canonicalize_value(Value::Object(result)))
        }
        other => Err(format!("{path}.type has unsupported node {other:?}")),
    }
}

fn parse_inline(value: &Value, path: &str, utf16_units: &mut u64) -> Result<InlineNode, String> {
    let object = as_object(value, path)?;
    let node_type = string_field(object, "type", path)?;
    match node_type {
        "text" => {
            check_fields(object, &["type", "text", "marks"], &["type", "text"], path)?;
            let text = string_field(object, "text", path)?;
            if text.is_empty() {
                return Err(format!("{path}.text must not be empty"));
            }
            *utf16_units = utf16_units
                .checked_add(text.encode_utf16().count() as u64)
                .ok_or_else(|| "UTF-16 unit count overflowed".to_owned())?;
            if *utf16_units > MAX_UTF16_UNITS {
                return Err(format!("document exceeds {MAX_UTF16_UNITS} UTF-16 units"));
            }

            let marks = match object.get("marks") {
                None => Vec::new(),
                Some(marks) => {
                    let marks = as_array(marks, &format!("{path}.marks"))?;
                    parse_marks(marks, &format!("{path}.marks"))?
                }
            };
            Ok(InlineNode::Text {
                text: text.to_owned(),
                marks,
            })
        }
        "hardBreak" => {
            check_fields(object, &["type"], &["type"], path)?;
            *utf16_units = utf16_units
                .checked_add(1)
                .ok_or_else(|| "UTF-16 unit count overflowed".to_owned())?;
            if *utf16_units > MAX_UTF16_UNITS {
                return Err(format!("document exceeds {MAX_UTF16_UNITS} UTF-16 units"));
            }
            Ok(InlineNode::HardBreak)
        }
        other => Err(format!("{path}.type has unsupported inline node {other:?}")),
    }
}

fn parse_marks(marks: &[Value], path: &str) -> Result<Vec<Value>, String> {
    let mut seen = HashSet::new();
    let mut parsed = Vec::with_capacity(marks.len());
    for (index, mark) in marks.iter().enumerate() {
        let mark_path = format!("{path}[{index}]");
        let object = as_object(mark, &mark_path)?;
        let mark_type = string_field(object, "type", &mark_path)?;
        if !seen.insert(mark_type.to_owned()) {
            return Err(format!("{mark_path}.type is duplicated"));
        }

        let canonical = match mark_type {
            "bold" | "italic" => {
                check_fields(object, &["type"], &["type"], &mark_path)?;
                let mut result = Map::new();
                result.insert("type".to_owned(), Value::String(mark_type.to_owned()));
                Value::Object(result)
            }
            "link" => {
                check_fields(object, &["type", "attrs"], &["type", "attrs"], &mark_path)?;
                let attrs_path = format!("{mark_path}.attrs");
                let attrs = as_object(required(object, "attrs", &mark_path)?, &attrs_path)?;
                check_fields(attrs, &["href"], &["href"], &attrs_path)?;
                let href = string_field(attrs, "href", &attrs_path)?.to_owned();
                validate_href(&href, &attrs_path)?;
                let mut canonical_attrs = Map::new();
                canonical_attrs.insert("href".to_owned(), Value::String(href));
                let mut result = Map::new();
                result.insert("attrs".to_owned(), Value::Object(canonical_attrs));
                result.insert("type".to_owned(), Value::String("link".to_owned()));
                Value::Object(result)
            }
            other => return Err(format!("{mark_path}.type has unsupported mark {other:?}")),
        };
        parsed.push(canonical);
    }

    parsed.sort_by_key(
        |mark| match mark.get("type").and_then(Value::as_str).unwrap_or_default() {
            "bold" => 0_u8,
            "italic" => 1,
            "link" => 2,
            _ => 3,
        },
    );
    Ok(parsed)
}

fn merge_adjacent_text(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut merged = Vec::with_capacity(nodes.len());
    for node in nodes {
        match (merged.last_mut(), node) {
            (
                Some(InlineNode::Text {
                    text: previous_text,
                    marks: previous_marks,
                }),
                InlineNode::Text { text, marks },
            ) if *previous_marks == marks => previous_text.push_str(&text),
            (_, node) => merged.push(node),
        }
    }
    merged
}

fn inline_to_value(node: InlineNode) -> Value {
    match node {
        InlineNode::HardBreak => {
            let mut object = Map::new();
            object.insert("type".to_owned(), Value::String("hardBreak".to_owned()));
            Value::Object(object)
        }
        InlineNode::Text { text, marks } => {
            let mut object = Map::new();
            object.insert("text".to_owned(), Value::String(text));
            object.insert("type".to_owned(), Value::String("text".to_owned()));
            if !marks.is_empty() {
                object.insert("marks".to_owned(), Value::Array(marks));
            }
            canonicalize_value(Value::Object(object))
        }
    }
}

fn validate_id(id: &str, path: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(format!(
            "{path}.id must be 1..64 ASCII letters, digits, '_' or '-'",
        ));
    }
    Ok(())
}

fn validate_href(href: &str, path: &str) -> Result<(), String> {
    if href.is_empty()
        || href.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == '\\'
        })
    {
        return Err(format!(
            "{path}.href contains unsafe whitespace or control characters"
        ));
    }

    let lower = href.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let url = Url::parse(href)
            .map_err(|_| format!("{path}.href must be an absolute http or https URL"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
            return Err(format!("{path}.href must have a nonempty host"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(format!("{path}.href must have a host without credentials"));
        }
        return Ok(());
    }

    if lower.starts_with("mailto:") {
        if href.contains('%') {
            return Err(format!("{path}.href must be a valid mailto address"));
        }
        let url =
            Url::parse(href).map_err(|_| format!("{path}.href must be a valid mailto address"))?;
        if url.scheme() != "mailto" || url.query().is_some() || url.fragment().is_some() {
            return Err(format!("{path}.href must be a valid mailto address"));
        }
        let address = url.path();
        let mut parts = address.split('@');
        let local = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        if parts.next().is_some()
            || local.is_empty()
            || domain.is_empty()
            || !domain.contains('.')
            || address.contains(['/', '#', '\\'])
            || address.contains('?')
            || local.starts_with('.')
            || local.ends_with('.')
            || domain.starts_with('.')
            || domain.ends_with('.')
        {
            return Err(format!("{path}.href must be a valid mailto address"));
        }
        return Ok(());
    }

    Err(format!(
        "{path}.href must use an absolute http, https, or mailto URL",
    ))
}

fn as_object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{path} must be an object"))
}

fn as_array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path} must be an array"))
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{path}.{key} is required"))
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a str, String> {
    required(object, key, path)?
        .as_str()
        .ok_or_else(|| format!("{path}.{key} must be a string"))
}

fn check_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    required_fields: &[&str],
    path: &str,
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{path} contains unknown field {key:?}"));
        }
    }
    for key in required_fields {
        if !object.contains_key(*key) {
            return Err(format!("{path}.{key} is required"));
        }
    }
    Ok(())
}

pub fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            let mut canonical = Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize_value(value));
            }
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
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
    use super::validate_snapshot_json;
    use rusqlite::Connection;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;

    #[derive(Debug, Deserialize)]
    struct FixtureFile {
        cases: Vec<FixtureCase>,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureCase {
        name: String,
        input: String,
        expected: Option<ExpectedReceipt>,
        error: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedReceipt {
        canonical_json: String,
        hash: String,
        utf16_units: u32,
        block_count: u32,
        snapshot: Option<Value>,
    }

    #[test]
    fn shared_snapshot_fixtures_match() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/w0_snapshot_golden.json");
        let fixture = fs::read_to_string(fixture_path).expect("read shared fixture");
        let fixture: FixtureFile = serde_json::from_str(&fixture).expect("parse shared fixture");
        for case in fixture.cases {
            let result = validate_snapshot_json(&case.input);
            match (case.expected, case.error) {
                (Some(expected), None) => {
                    let receipt = result.unwrap_or_else(|error| {
                        panic!("fixture {} unexpectedly failed: {error}", case.name)
                    });
                    assert_eq!(
                        receipt.canonical_json, expected.canonical_json,
                        "{}",
                        case.name
                    );
                    assert_eq!(receipt.hash, expected.hash, "{}", case.name);
                    assert_eq!(receipt.utf16_units, expected.utf16_units, "{}", case.name);
                    assert_eq!(receipt.block_count, expected.block_count, "{}", case.name);
                    if let Some(snapshot) = expected.snapshot {
                        assert_eq!(receipt.snapshot, snapshot, "{}", case.name);
                    }
                }
                (None, Some(expected_error)) => {
                    let error = result.expect_err(&case.name);
                    assert!(
                        error.contains(&expected_error),
                        "fixture {} error {error:?} does not contain {expected_error:?}",
                        case.name
                    );
                }
                _ => panic!("fixture {} must specify exactly one expectation", case.name),
            }
        }
    }

    #[test]
    fn links_bundled_sqlite() {
        let connection = Connection::open_in_memory().expect("open bundled SQLite");
        let version: String = connection
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))
            .expect("query linked SQLite version");
        assert!(!version.is_empty());
    }

    #[test]
    fn rejects_raw_documents_over_two_megabytes() {
        let input = json!({
            "schemaVersion": 1,
            "body": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "attrs": {"id": "large"},
                    "content": [{"type": "text", "text": "a".repeat(2 * 1024 * 1024)}]
                }]
            }
        });
        let input = serde_json::to_string(&input).expect("serialize oversized document");
        let error = validate_snapshot_json(&input).expect_err("raw byte limit should reject");
        assert!(error.contains("byte limit"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_documents_over_ten_thousand_blocks() {
        let blocks: Vec<Value> = (0..=10_000)
            .map(|index| json!({"type": "paragraph", "attrs": {"id": format!("p{index}")}}))
            .collect();
        let input = json!({"schemaVersion": 1, "body": {"type": "doc", "content": blocks}});
        let input = serde_json::to_string(&input).expect("serialize oversized block list");
        let error = validate_snapshot_json(&input).expect_err("block limit should reject");
        assert!(
            error.contains("more than 10000 blocks"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_documents_over_one_million_utf16_units() {
        let input = json!({
            "schemaVersion": 1,
            "body": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "attrs": {"id": "utf16"},
                    "content": [{"type": "text", "text": "a".repeat(1_000_001)}]
                }]
            }
        });
        let input = serde_json::to_string(&input).expect("serialize oversized UTF-16 document");
        let error = validate_snapshot_json(&input).expect_err("UTF-16 limit should reject");
        assert!(error.contains("UTF-16 units"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_unknown_fields() {
        let input = r#"{"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p","extra":1}}]}}"#;
        let error = validate_snapshot_json(input).expect_err("unknown fields should reject");
        assert!(error.contains("unknown field"), "unexpected error: {error}");
    }
}
