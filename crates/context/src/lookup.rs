//! Pure validation for the bounded C6 story lookup response protocol.
//!
//! This module deliberately does not search, read, call a provider, or grant
//! any source access. It accepts only a small provider response describing
//! application-executed reads, or a final discussion response. The project
//! owner remains responsible for authorization, snapshot identity, budgets,
//! and execution of each requested read.

use crate::SourceRef;
use crate::evidence_history::EvidenceHistory;
use crate::knowledge_history::KnowledgeHistory;
use crate::promise_history::PromiseHistory;
use crate::frozen::{FrozenContext, SearchMode, SearchResult, SourcePassage};
use wns_kernel::{CoreError, CoreResult};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;

pub const LOOKUP_SCHEMA_VERSION: &str = "story-lookup.v1";
pub const MAX_LOOKUP_ENVELOPE_BYTES: usize = 64 * 1024;
pub const MAX_LOOKUP_READS: usize = 8;
pub const MAX_LOOKUP_ID_BYTES: usize = 64;
pub const MAX_LOOKUP_QUERY_BYTES: usize = 512;
pub const MAX_LOOKUP_HANDLE_BYTES: usize = 256;
pub const MAX_LOOKUP_BLOCK_IDS: usize = 32;
pub const MAX_LOOKUP_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_LOOKUP_READ_LIMIT: u32 = 20;
pub const MAX_LOOKUP_TOTAL_INPUT_BYTES: u128 = 3 * 24_576;
pub const MAX_LOOKUP_TOTAL_OUTPUT_BYTES: u128 = 3 * 65_536;
pub const LOOKUP_SOURCE_PROJECTION_SCHEMA: &str = "story-lookup-source.v1";
pub const REVIEWED_MEMORY_CAPABILITY: &str = "reviewed-memory.v1";
pub const MAX_MEMORY_OFFSET: u32 = 100_000;
pub const MAX_MEMORY_LIMIT: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LookupErrorCode {
    InvalidJson,
    DuplicateKey,
    InvalidEnvelope,
    InvalidField,
    InvalidAllowance,
    InvalidCapability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupError {
    pub code: LookupErrorCode,
    pub message: String,
}

impl LookupError {
    fn new(code: LookupErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LookupError {}

impl fmt::Display for LookupErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidJson => "invalidJson",
            Self::DuplicateKey => "duplicateKey",
            Self::InvalidEnvelope => "invalidEnvelope",
            Self::InvalidField => "invalidField",
            Self::InvalidAllowance => "invalidAllowance",
            Self::InvalidCapability => "invalidCapability",
        })
    }
}

/// Application byte allowances for the initial lookup request and its
/// bounded context-expansion invocations. These are serialized decimal byte
/// caps, not provider token or billing limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupAllowance {
    pub max_additional_invocations: u8,
    pub total_input_bytes: String,
    pub total_output_bytes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryEntityKind {
    Character,
    Topic,
    Object,
    Promise,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryEntityEntry {
    pub entity: crate::story_records::StoryEntityRef,
    pub label_variants: Vec<String>,
    pub source_handle: String,
    pub source: SourceRef,
}

impl Default for LookupAllowance {
    fn default() -> Self {
        Self {
            max_additional_invocations: 2,
            total_input_bytes: "73728".to_owned(),
            total_output_bytes: "196608".to_owned(),
        }
    }
}

impl LookupAllowance {
    pub fn new(
        max_additional_invocations: u8,
        total_input_bytes: impl Into<String>,
        total_output_bytes: impl Into<String>,
    ) -> Result<Self, LookupError> {
        let allowance = Self {
            max_additional_invocations,
            total_input_bytes: total_input_bytes.into(),
            total_output_bytes: total_output_bytes.into(),
        };
        allowance.validate()?;
        Ok(allowance)
    }

    pub fn validate(&self) -> Result<(), LookupError> {
        if self.max_additional_invocations > 2 {
            return Err(LookupError::new(
                LookupErrorCode::InvalidAllowance,
                "maxAdditionalInvocations must be between 0 and 2.",
            ));
        }
        validate_positive_decimal(
            &self.total_input_bytes,
            MAX_LOOKUP_TOTAL_INPUT_BYTES,
            "totalInputBytes",
        )?;
        validate_positive_decimal(
            &self.total_output_bytes,
            MAX_LOOKUP_TOTAL_OUTPUT_BYTES,
            "totalOutputBytes",
        )?;
        Ok(())
    }
}

/// One application-executed lookup request. The provider cannot supply a
/// path, command, source body, or arbitrary tool arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LookupRead {
    Search {
        id: String,
        query: String,
        mode: SearchMode,
        limit: u32,
    },
    Read {
        id: String,
        handle: String,
        #[serde(
            rename = "blockIds",
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_block_ids"
        )]
        block_ids: Option<Vec<String>>,
    },
    FindEntities {
        id: String,
        entity_kind: MemoryEntityKind,
        query: String,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        offset: u32,
        limit: u32,
    },
    KnowledgeHistory {
        id: String,
        character_id: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_non_null_string"
        )]
        topic_id: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        offset: u32,
        limit: u32,
    },
    PromiseHistory {
        id: String,
        promise_id: String,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        offset: u32,
        limit: u32,
    },
    PossessionHistory {
        id: String,
        object_id: String,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        offset: u32,
        limit: u32,
    },
}

/// The request half of one application-executed exchange. This alias keeps
/// the wire shape identical to the request descriptors in a `needsContext`
/// envelope while giving packet code a descriptive contract name.
pub type LookupReadRequest = LookupRead;

/// The application result for one previously authorized lookup request.
/// Search results and passages retain their Rust-owned source identities; a
/// provider cannot manufacture an arbitrary path, title, or source body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LookupReadResult {
    Search {
        result: SearchResult,
    },
    Read {
        handle: String,
        source: SourceRef,
        passages: Vec<SourcePassage>,
        complete: bool,
    },
    FindEntities {
        entity_kind: MemoryEntityKind,
        query: String,
        entries: Vec<MemoryEntityEntry>,
        offset: u32,
        total_matches: u32,
        next_offset: Option<u32>,
        incomplete: bool,
    },
    KnowledgeHistory {
        history: KnowledgeHistory,
        offset: u32,
        total_observations: u32,
        next_offset: Option<u32>,
    },
    PromiseHistory {
        history: PromiseHistory,
        offset: u32,
        total_observations: u32,
        next_offset: Option<u32>,
    },
    PossessionHistory {
        history: EvidenceHistory,
        offset: u32,
        total_observations: u32,
        next_offset: Option<u32>,
    },
    Unavailable {
        code: String,
        detail: String,
    },
}

/// The bounded lookup state carried into packet compilation. The initial
/// invocation has zero completed expansions and no exchanges; each
/// application-executed expansion adds one authenticated exchange. The
/// packet compiler owns semantic validation of request/result correspondence,
/// source identity, and byte budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupPacketInput {
    pub allowance: LookupAllowance,
    pub completed_invocations: u8,
    pub exchanges: Vec<LookupExchange>,
    /// Author-room source labels for evidence returned by this lookup chain.
    /// Historical packets omit this optional field; newly created child
    /// packets populate it from the frozen Rust-owned descriptors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_projection: Option<LookupSourceProjection>,
    /// Rust-owned capability for typed reads over reviewed story memory.
    /// Historical packets omit this field; fresh lookup packets include the
    /// exact capability string when the author enabled reviewed-memory reads.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_non_null_string"
    )]
    pub reviewed_memory: Option<String>,
}

impl LookupPacketInput {
    pub fn validate_capability(&self) -> Result<(), LookupError> {
        validate_reviewed_memory_capability(self.reviewed_memory.as_deref())
    }

    pub fn authorize_read(&self, read: &LookupRead) -> Result<(), LookupError> {
        validate_lookup_read(read)?;
        if read.is_memory() && self.reviewed_memory.as_deref() != Some(REVIEWED_MEMORY_CAPABILITY) {
            return Err(LookupError::new(
                LookupErrorCode::InvalidCapability,
                "Reviewed-memory reads require the reviewed-memory.v1 capability.",
            ));
        }
        Ok(())
    }
}

pub fn validate_reviewed_memory_capability(capability: Option<&str>) -> Result<(), LookupError> {
    if let Some(capability) = capability
        && capability != REVIEWED_MEMORY_CAPABILITY
    {
        return Err(LookupError::new(
            LookupErrorCode::InvalidCapability,
            "reviewedMemory must be reviewed-memory.v1 when present.",
        ));
    }
    Ok(())
}

pub fn reviewed_memory_enabled(lookup: Option<&LookupPacketInput>) -> bool {
    lookup
        .is_some_and(|lookup| lookup.reviewed_memory.as_deref() == Some(REVIEWED_MEMORY_CAPABILITY))
}

/// Exact, author-room-only labels for sources that actually appear in lookup
/// read or search evidence. This is intentionally separate from the provider
/// `story-lookup.v1` response contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupSourceProjection {
    pub schema_version: String,
    pub sources: Vec<LookupSourceProjectionSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupSourceProjectionSource {
    pub handle: String,
    pub source: SourceRef,
    pub display_name: String,
}

impl LookupSourceProjection {
    /// Build the deterministic projection for a newly created child packet.
    /// References are collected only from successful read/search evidence and
    /// labels are copied from the exact frozen descriptors.
    pub fn from_exchanges(
        frozen: &FrozenContext,
        exchanges: &[LookupExchange],
    ) -> CoreResult<Self> {
        let mut returned = HashSet::new();
        for exchange in exchanges {
            match &exchange.result {
                LookupReadResult::Read { handle, source, .. } => {
                    returned.insert((handle.clone(), source.clone()));
                }
                LookupReadResult::Search { result } => {
                    for descriptor in &result.source_matches {
                        returned.insert((descriptor.handle.clone(), descriptor.source.clone()));
                    }
                    for hit in &result.hits {
                        returned.insert((hit.passage.handle.clone(), hit.passage.source.clone()));
                    }
                }
                LookupReadResult::FindEntities { entries, .. } => {
                    for entry in entries {
                        returned.insert((entry.source_handle.clone(), entry.source.clone()));
                    }
                }
                LookupReadResult::KnowledgeHistory { history, .. } => {
                    for observation in &history.observations {
                        returned.insert((
                            observation.source_handle.clone(),
                            observation.source.clone(),
                        ));
                    }
                }
                LookupReadResult::PromiseHistory { history, .. } => {
                    for observation in &history.observations {
                        returned.insert((
                            observation.source_handle.clone(),
                            observation.source.clone(),
                        ));
                    }
                }
                LookupReadResult::PossessionHistory { history, .. } => {
                    for observation in &history.observations {
                        returned.insert((
                            observation.source_handle.clone(),
                            observation.source.clone(),
                        ));
                    }
                }
                LookupReadResult::Unavailable { .. } => {}
            }
        }

        let mut sources = Vec::new();
        for descriptor in &frozen.snapshot.sources {
            if returned.remove(&(descriptor.handle.clone(), descriptor.source.clone())) {
                sources.push(LookupSourceProjectionSource {
                    handle: descriptor.handle.clone(),
                    source: descriptor.source.clone(),
                    display_name: descriptor.display_name.clone(),
                });
            }
        }
        if let Some((handle, _)) = returned.into_iter().next() {
            return Err(CoreError::new(
                "InvalidLookupEvidence",
                &format!("Lookup evidence returned an unknown source handle {handle:?}."),
            ));
        }
        Ok(Self {
            schema_version: LOOKUP_SOURCE_PROJECTION_SCHEMA.to_owned(),
            sources,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookupExchange {
    pub request: LookupReadRequest,
    pub result: LookupReadResult,
}

/// Strict provider response. A `needsContext` response contains only bounded
/// read descriptors; a `discussion` response is final text and requests no
/// application action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LookupEnvelope {
    NeedsContext {
        #[serde(rename = "schemaVersion")]
        schema_version: String,
        reads: Vec<LookupRead>,
    },
    Discussion {
        #[serde(rename = "schemaVersion")]
        schema_version: String,
        text: String,
    },
}

impl LookupEnvelope {
    pub fn schema_version(&self) -> &str {
        match self {
            Self::NeedsContext { schema_version, .. } | Self::Discussion { schema_version, .. } => {
                schema_version
            }
        }
    }
}

impl LookupRead {
    /// Stable request identity used to correlate an application result with
    /// the exact read the provider requested.
    pub fn id(&self) -> &str {
        match self {
            Self::Search { id, .. }
            | Self::Read { id, .. }
            | Self::FindEntities { id, .. }
            | Self::KnowledgeHistory { id, .. }
            | Self::PromiseHistory { id, .. }
            | Self::PossessionHistory { id, .. } => id,
        }
    }

    pub fn is_memory(&self) -> bool {
        matches!(
            self,
            Self::FindEntities { .. }
                | Self::KnowledgeHistory { .. }
                | Self::PromiseHistory { .. }
                | Self::PossessionHistory { .. }
        )
    }
}

/// Parse and validate one complete provider response. The raw UTF-8 byte cap
/// is applied before JSON parsing, and duplicate object keys are rejected
/// before typed deserialization can silently take the last value.
pub fn parse_lookup_envelope(raw: &str) -> Result<LookupEnvelope, LookupError> {
    if raw.len() > MAX_LOOKUP_ENVELOPE_BYTES {
        return Err(LookupError::new(
            LookupErrorCode::InvalidEnvelope,
            "The lookup envelope exceeds the 64 KiB UTF-8 byte limit.",
        ));
    }
    let value = parse_strict_json(raw)?;
    let envelope: LookupEnvelope = serde_json::from_value(value).map_err(|error| {
        LookupError::new(
            LookupErrorCode::InvalidEnvelope,
            format!("The lookup envelope does not match story-lookup.v1: {error}"),
        )
    })?;
    validate_lookup_envelope(&envelope)?;
    Ok(envelope)
}

/// Validate an already decoded envelope. Callers that accept external JSON
/// should prefer [`parse_lookup_envelope`] so duplicate keys are also caught.
pub fn validate_lookup_envelope(envelope: &LookupEnvelope) -> Result<(), LookupError> {
    if envelope.schema_version() != LOOKUP_SCHEMA_VERSION {
        return Err(LookupError::new(
            LookupErrorCode::InvalidEnvelope,
            format!(
                "schemaVersion must be {LOOKUP_SCHEMA_VERSION}, not {:?}.",
                envelope.schema_version()
            ),
        ));
    }
    match envelope {
        LookupEnvelope::NeedsContext { reads, .. } => validate_reads(reads),
        LookupEnvelope::Discussion { text, .. } => validate_discussion_text(text),
    }
}

fn validate_reads(reads: &[LookupRead]) -> Result<(), LookupError> {
    if reads.is_empty() || reads.len() > MAX_LOOKUP_READS {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            "reads must contain between 1 and 8 requests.",
        ));
    }
    let mut ids = HashSet::with_capacity(reads.len());
    for read in reads {
        validate_lookup_read(read)?;
        if !ids.insert(read.id()) {
            return Err(LookupError::new(
                LookupErrorCode::InvalidField,
                format!("read id {:?} is duplicated.", read.id()),
            ));
        }
    }
    Ok(())
}

/// Validate one read descriptor independently of the per-envelope read
/// count. Packet compilation uses this when checking each bounded exchange;
/// the envelope validator additionally enforces the one-to-eight count and
/// unique IDs for that request round.
pub fn validate_lookup_read(read: &LookupRead) -> Result<(), LookupError> {
    validate_identifier(read.id(), "read id", MAX_LOOKUP_ID_BYTES)?;
    match read {
        LookupRead::Search { query, limit, .. } => {
            validate_text(query, "query", MAX_LOOKUP_QUERY_BYTES)?;
            if !(1..=MAX_LOOKUP_READ_LIMIT).contains(limit) {
                return Err(LookupError::new(
                    LookupErrorCode::InvalidField,
                    "search limit must be between 1 and 20.",
                ));
            }
        }
        LookupRead::Read {
            handle, block_ids, ..
        } => {
            validate_text(handle, "handle", MAX_LOOKUP_HANDLE_BYTES)?;
            if handle.chars().any(char::is_whitespace) {
                return Err(LookupError::new(
                    LookupErrorCode::InvalidField,
                    "A source handle cannot contain whitespace.",
                ));
            }
            if let Some(block_ids) = block_ids {
                validate_block_ids(block_ids)?;
            }
        }
        LookupRead::FindEntities {
            query,
            offset,
            limit,
            ..
        } => {
            validate_text(query, "query", MAX_LOOKUP_QUERY_BYTES)?;
            validate_memory_page(*offset, *limit)?;
        }
        LookupRead::KnowledgeHistory {
            character_id,
            topic_id,
            offset,
            limit,
            ..
        } => {
            validate_identifier(character_id, "characterId", MAX_LOOKUP_ID_BYTES)?;
            if let Some(topic_id) = topic_id {
                validate_identifier(topic_id, "topicId", MAX_LOOKUP_ID_BYTES)?;
            }
            validate_memory_page(*offset, *limit)?;
        }
        LookupRead::PromiseHistory {
            promise_id,
            offset,
            limit,
            ..
        } => {
            validate_identifier(promise_id, "promiseId", MAX_LOOKUP_ID_BYTES)?;
            validate_memory_page(*offset, *limit)?;
        }
        LookupRead::PossessionHistory {
            object_id,
            offset,
            limit,
            ..
        } => {
            validate_identifier(object_id, "objectId", MAX_LOOKUP_ID_BYTES)?;
            validate_memory_page(*offset, *limit)?;
        }
    }
    Ok(())
}

fn validate_memory_page(offset: u32, limit: u32) -> Result<(), LookupError> {
    if offset > MAX_MEMORY_OFFSET {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            format!("offset must be at most {MAX_MEMORY_OFFSET}."),
        ));
    }
    if !(1..=MAX_MEMORY_LIMIT).contains(&limit) {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            format!("limit must be between 1 and {MAX_MEMORY_LIMIT}."),
        ));
    }
    Ok(())
}

fn validate_block_ids(block_ids: &[String]) -> Result<(), LookupError> {
    if block_ids.len() > MAX_LOOKUP_BLOCK_IDS {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            "blockIds may contain at most 32 IDs.",
        ));
    }
    if block_ids.is_empty() {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            "blockIds, when supplied, must contain at least one ID.",
        ));
    }
    let mut ids = HashSet::with_capacity(block_ids.len());
    for block_id in block_ids {
        validate_identifier(block_id, "block id", MAX_LOOKUP_ID_BYTES)?;
        if !ids.insert(block_id.as_str()) {
            return Err(LookupError::new(
                LookupErrorCode::InvalidField,
                format!("block id {block_id:?} is duplicated."),
            ));
        }
    }
    Ok(())
}

fn deserialize_optional_block_ids<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(|value| {
                serde_json::from_value(value).map_err(|error| de::Error::custom(error.to_string()))
            })
            .collect::<Result<Vec<String>, D::Error>>()
            .map(Some),
        Value::Null => Err(de::Error::custom("blockIds must be an array when supplied")),
        _ => Err(de::Error::custom("blockIds must be an array")),
    }
}

fn deserialize_optional_non_null_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(value) => Ok(Some(value)),
        Value::Null => Err(de::Error::custom("field must be omitted, not null")),
        _ => Err(de::Error::custom("field must be a string when supplied")),
    }
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn validate_identifier(value: &str, label: &str, max_bytes: usize) -> Result<(), LookupError> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            format!(
                "{label} must be a nonempty ASCII identifier using letters, digits, '_' or '-', at most {max_bytes} bytes."
            ),
        ));
    }
    Ok(())
}

fn validate_text(value: &str, label: &str, max_bytes: usize) -> Result<(), LookupError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            format!("{label} must be nonblank, control-free, and at most {max_bytes} UTF-8 bytes."),
        ));
    }
    Ok(())
}

fn validate_discussion_text(value: &str) -> Result<(), LookupError> {
    if value.trim().is_empty()
        || value.len() > MAX_LOOKUP_TEXT_BYTES
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(LookupError::new(
            LookupErrorCode::InvalidField,
            "text must be nonblank, contain only printable characters plus normal whitespace, and be at most 64 KiB UTF-8 bytes.",
        ));
    }
    Ok(())
}

fn validate_positive_decimal(value: &str, maximum: u128, label: &str) -> Result<(), LookupError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(LookupError::new(
            LookupErrorCode::InvalidAllowance,
            format!("{label} must be a canonical decimal string."),
        ));
    }
    let parsed = value.parse::<u128>().map_err(|_| {
        LookupError::new(
            LookupErrorCode::InvalidAllowance,
            format!("{label} is too large."),
        )
    })?;
    if parsed == 0 || parsed > maximum {
        return Err(LookupError::new(
            LookupErrorCode::InvalidAllowance,
            format!("{label} must be positive and at most {maximum} bytes."),
        ));
    }
    Ok(())
}

fn parse_strict_json(raw: &str) -> Result<Value, LookupError> {
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    let value = StrictValue::deserialize(&mut deserializer)
        .map_err(|error| map_decode_error(error.to_string()))?
        .0;
    deserializer
        .end()
        .map_err(|error| map_decode_error(error.to_string()))?;
    Ok(value)
}

fn map_decode_error(message: String) -> LookupError {
    let code = if message.contains("duplicate object key") {
        LookupErrorCode::DuplicateKey
    } else {
        LookupErrorCode::InvalidJson
    };
    LookupError::new(code, message)
}

/// A serde-backed JSON value that rejects duplicate keys while retaining the
/// normal serde_json parser and number/string semantics.
struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictVisitor;

        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let number = serde_json::Number::from_f64(value)
                    .ok_or_else(|| E::custom("JSON number is not finite"))?;
                Ok(StrictValue(Value::Number(number)))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value.to_owned())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictValue>()? {
                    values.push(value.0);
                }
                Ok(StrictValue(Value::Array(values)))
            }

            fn visit_map<A>(self, mut map_access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = Map::new();
                let mut keys = HashSet::new();
                while let Some(key) = map_access.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(de::Error::custom(format!("duplicate object key {key:?}")));
                    }
                    let value = map_access.next_value::<StrictValue>()?;
                    values.insert(key, value.0);
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(StrictVisitor)
    }
}
