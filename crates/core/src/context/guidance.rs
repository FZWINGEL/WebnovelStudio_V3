//! Explicit author instructions are immutable records, never manuscript revisions.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum GuidanceScope {
    Request,
    Document,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuidanceVersion {
    pub guidance_id: String,
    pub version_id: String,
    pub version: String,
    pub scope: GuidanceScope,
    pub document_id: Option<String>,
    pub text: String,
    pub text_hash: String,
    pub active: bool,
    pub origin_message_id: Option<String>,
    pub created_at: String,
}

/// An exact author-room instruction selected when a request freezes. The
/// referenced version also survives in its own authoritative local store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenGuidance {
    pub handle: String,
    pub project_id: String,
    pub version: GuidanceVersion,
}

pub(crate) fn validate_frozen_guidance(
    records: &[FrozenGuidance],
    project_id: &str,
    document_id: &str,
    audience: super::Audience,
) -> Result<(), String> {
    if !records.is_empty() && audience != super::Audience::AuthorRoom {
        return Err("Author-room guidance cannot enter a restricted writing request.".into());
    }
    let mut versions = std::collections::HashSet::new();
    let mut heads = std::collections::HashSet::new();
    for record in records {
        let version = &record.version;
        let counter = version.version.parse::<i64>().ok();
        if record.project_id != project_id
            || record.handle != format!("guidance-{}", version.version_id)
            || !versions.insert(&version.version_id)
            || !heads.insert(&version.guidance_id)
            || !version.active
            || !counter.is_some_and(|value| value > 0 && value.to_string() == version.version)
            || version.guidance_id.is_empty()
            || version.version_id.is_empty()
            || version.text.trim().is_empty()
            || version.text.len() > 16_384
            || crate::sha256_hex(version.text.as_bytes()) != version.text_hash
        {
            return Err(
                "The frozen author guidance has an invalid identity, version, or text hash.".into(),
            );
        }
        match version.scope {
            GuidanceScope::Project if version.document_id.is_none() => {}
            GuidanceScope::Document | GuidanceScope::Request
                if version.document_id.as_deref() == Some(document_id) => {}
            _ => return Err("The author guidance does not apply to this document.".into()),
        }
    }
    Ok(())
}
