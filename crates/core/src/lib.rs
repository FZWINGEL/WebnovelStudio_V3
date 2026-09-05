//! W0 snapshot validation and canonicalization.
//!
//! The validator deliberately accepts only the small document vocabulary used by
//! the first slice.  It constructs a canonical document while parsing instead of
//! accepting arbitrary ProseMirror extensions and trying to strip them later.

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;

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

fn canonicalize_value(value: Value) -> Value {
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
