//! Pure response contracts for bounded story continuation candidates.
//!
//! The provider supplies only plain paragraph text. Rust validates the
//! response before it becomes a durable candidate; document identities and
//! editor operations remain outside this response contract.

use serde::{Deserialize, Serialize};
use wns_kernel::{CoreError, CoreResult};

/// Versioned response contract for a continuation request.
pub const CONTINUATION_RESPONSE_CONTRACT: &str = "continuation-output.v1";
/// Maximum raw response size accepted by the continuation parser.
pub const MAX_CONTINUATION_RESPONSE_BYTES: usize = 64 * 1024;
/// Maximum number of paragraphs in one continuation candidate.
pub const MAX_CONTINUATION_PARAGRAPHS: usize = 128;
/// Maximum UTF-8 bytes in a candidate title.
pub const MAX_CONTINUATION_TITLE_BYTES: usize = 120;
/// Maximum UTF-8 bytes in a candidate explanation.
pub const MAX_CONTINUATION_EXPLANATION_BYTES: usize = 4096;
/// Maximum UTF-16 units in one generated paragraph.
pub const MAX_CONTINUATION_PARAGRAPH_UTF16: usize = 8192;
/// Maximum UTF-16 units across one generated candidate.
pub const MAX_CONTINUATION_TOTAL_UTF16: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContinuationCandidate {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContinuationOutput {
    pub schema_version: String,
    pub suggestions: Vec<ContinuationCandidate>,
}

/// Validate one complete provider response and return its typed candidate.
/// Exactly one candidate is required so the first continuation route has one
/// unambiguous append payload and does not need an additional selection step.
pub fn validate_continuation_output(raw: &str) -> CoreResult<ContinuationOutput> {
    if raw.len() > MAX_CONTINUATION_RESPONSE_BYTES {
        return Err(continuation_error(
            "InvalidContinuation",
            format!("The continuation response exceeds {MAX_CONTINUATION_RESPONSE_BYTES} bytes."),
        ));
    }
    let output: ContinuationOutput = serde_json::from_str(raw).map_err(|error| {
        continuation_error(
            "InvalidContinuation",
            format!("The continuation response is malformed: {error}"),
        )
    })?;
    if output.schema_version != CONTINUATION_RESPONSE_CONTRACT {
        return Err(continuation_error(
            "InvalidContinuation",
            format!(
                "The continuation response schemaVersion must be {CONTINUATION_RESPONSE_CONTRACT}."
            ),
        ));
    }
    if output.suggestions.len() != 1 {
        return Err(continuation_error(
            "InvalidContinuation",
            "The continuation response must contain exactly one suggestion.".to_owned(),
        ));
    }
    let candidate = &output.suggestions[0];
    if candidate.title.trim().is_empty() || candidate.title.len() > MAX_CONTINUATION_TITLE_BYTES {
        return Err(continuation_error(
            "InvalidContinuation",
            "The continuation title must be nonblank and at most 120 UTF-8 bytes.".to_owned(),
        ));
    }
    if candidate.explanation.len() > MAX_CONTINUATION_EXPLANATION_BYTES {
        return Err(continuation_error(
            "InvalidContinuation",
            "The continuation explanation exceeds 4,096 UTF-8 bytes.".to_owned(),
        ));
    }
    validate_continuation_paragraphs(&candidate.paragraphs)?;
    Ok(output)
}

/// Validate paragraph boundaries independently of the provider response.
/// Prepare and Apply can reuse this function after an author edits the
/// candidate without making another provider call.
pub fn validate_continuation_paragraphs(paragraphs: &[String]) -> CoreResult<()> {
    if paragraphs.is_empty() || paragraphs.len() > MAX_CONTINUATION_PARAGRAPHS {
        return Err(continuation_error(
            "InvalidContinuation",
            "A continuation must contain between 1 and 128 paragraphs.".to_owned(),
        ));
    }
    let mut total_utf16 = 0usize;
    for paragraph in paragraphs {
        if paragraph.trim().is_empty() || paragraph.contains(['\r', '\n']) {
            return Err(continuation_error(
                "InvalidContinuation",
                "Continuation paragraphs must be nonblank and single-line.".to_owned(),
            ));
        }
        let units = paragraph.encode_utf16().count();
        if units > MAX_CONTINUATION_PARAGRAPH_UTF16 {
            return Err(continuation_error(
                "InvalidContinuation",
                "A continuation paragraph exceeds 8,192 UTF-16 units.".to_owned(),
            ));
        }
        total_utf16 = total_utf16.checked_add(units).ok_or_else(|| {
            continuation_error(
                "InvalidContinuation",
                "The continuation UTF-16 length overflowed.".to_owned(),
            )
        })?;
        if total_utf16 > MAX_CONTINUATION_TOTAL_UTF16 {
            return Err(continuation_error(
                "InvalidContinuation",
                "The continuation exceeds 100,000 UTF-16 units.".to_owned(),
            ));
        }
    }
    Ok(())
}

fn continuation_error(code: &str, detail: String) -> CoreError {
    CoreError::new(code, &detail)
}
