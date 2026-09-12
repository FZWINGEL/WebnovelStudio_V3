//! Bounded, nonsecret Codex model discovery data.
//!
//! The native runtime owns the app-server transport.  This module only parses
//! a completed `model/list` page and validates the small cache that can be
//! persisted in the app library.  A cache is descriptive evidence, not proof
//! that a model is currently ready to dispatch.

use super::catalog::ServiceTier;
use super::preferences::ModelSelection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use wns_kernel::{CoreError, CoreResult};

pub const CODEX_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const CODEX_CATALOG_KEY: &str = "codex-model-catalog-v1";
pub const MAX_MODELS: usize = 256;
pub const MAX_TRAITS: usize = 32;
pub const MAX_IDENTIFIER_CHARS: usize = 128;
pub const MAX_LABEL_CHARS: usize = 256;
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

/// A sanitized, persisted snapshot of one complete Codex model discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCatalog {
    pub cli_version: String,
    pub executable_sha256: String,
    /// Unix milliseconds represented as a decimal string to avoid platform
    /// integer-width and timestamp formatting differences.
    pub discovered_at: String,
    pub models: Vec<CodexCatalogModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCatalogModel {
    pub model_id: String,
    pub label: String,
    pub reasoning_levels: Vec<String>,
    #[serde(default)]
    pub default_reasoning: Option<String>,
    pub service_tiers: Vec<ServiceTier>,
    #[serde(default)]
    pub default_service_tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexModelPage {
    pub models: Vec<CodexCatalogModel>,
    pub next_cursor: Option<String>,
}

impl CodexCatalog {
    pub fn validate(&self) -> CoreResult<()> {
        validate_cli_version(&self.cli_version)?;
        validate_fingerprint(&self.executable_sha256)?;
        validate_timestamp(&self.discovered_at)?;
        if self.models.len() > MAX_MODELS {
            return Err(invalid("The Codex catalog contains too many models."));
        }
        let mut ids = HashSet::new();
        for model in &self.models {
            model.validate()?;
            if !ids.insert(&model.model_id) {
                return Err(invalid("The Codex catalog contains a duplicate model ID."));
            }
        }
        Ok(())
    }

    /// Return whether a saved selection is declared by this discovery.
    /// Omitted reasoning means provider default and is accepted only when the
    /// discovery supplied that default, so native binding can freeze it.
    pub fn supports(&self, selection: &ModelSelection) -> bool {
        if selection.provider_id != "codex" {
            return false;
        }
        let Some(model) = self
            .models
            .iter()
            .find(|model| model.model_id == selection.model_id)
        else {
            return false;
        };
        selection.reasoning.as_ref().map_or_else(
            || model.default_reasoning.is_some(),
            |value| model.reasoning_levels.iter().any(|level| level == value),
        ) && selection
            .service_tier
            .as_ref()
            .is_none_or(|value| model.service_tiers.iter().any(|tier| tier.id == *value))
    }

    pub fn model(&self, model_id: &str) -> Option<&CodexCatalogModel> {
        self.models.iter().find(|model| model.model_id == model_id)
    }
}

impl CodexCatalogModel {
    /// Exact launch metadata identity. A refresh cannot change an accepted
    /// request's defaults or capabilities while preserving this fingerprint.
    pub fn fingerprint(&self) -> CoreResult<String> {
        use sha2::{Digest, Sha256};
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| invalid("The Codex model metadata could not be encoded."))?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn validate(&self) -> CoreResult<()> {
        validate_identifier(&self.model_id, "Codex model ID")?;
        validate_label(&self.label, "Codex model label")?;
        validate_identifiers(&self.reasoning_levels, "reasoning level")?;
        if self.reasoning_levels.len() > MAX_TRAITS {
            return Err(invalid(
                "The Codex model declares too many reasoning levels.",
            ));
        }
        let mut tiers = HashSet::new();
        if self.service_tiers.len() > MAX_TRAITS {
            return Err(invalid("The Codex model declares too many service tiers."));
        }
        for tier in &self.service_tiers {
            validate_identifier(&tier.id, "service tier ID")?;
            validate_label(&tier.label, "service tier label")?;
            if !tiers.insert(&tier.id) {
                return Err(invalid(
                    "The Codex model declares a duplicate service tier.",
                ));
            }
        }
        if let Some(default) = &self.default_reasoning {
            validate_identifier(default, "default reasoning level")?;
            if !self.reasoning_levels.iter().any(|value| value == default) {
                return Err(invalid(
                    "The Codex default reasoning level is not declared.",
                ));
            }
        }
        if let Some(default) = &self.default_service_tier {
            validate_identifier(default, "default service tier")?;
            if !self.service_tiers.iter().any(|tier| &tier.id == default) {
                return Err(invalid("The Codex default service tier is not declared."));
            }
        }
        Ok(())
    }
}

/// Parse the `result` value returned by one `model/list` JSON-RPC response.
/// Unknown upstream fields are ignored.  A top-level JSON-RPC envelope is
/// accepted as a convenience for callers that have not peeled off `result`.
pub fn parse_model_page(bytes: &[u8]) -> CoreResult<CodexModelPage> {
    if bytes.len() > MAX_PAGE_BYTES {
        return Err(invalid("The Codex model page exceeds the supported size."));
    }
    let parsed: Value = serde_json::from_slice(bytes).map_err(|error| {
        CoreError::new(
            "InvalidCodexCatalog",
            &format!("Invalid model page: {error}"),
        )
    })?;
    let object = parsed
        .as_object()
        .ok_or_else(|| invalid("The Codex model page must be an object."))?;
    let result = object
        .get("result")
        .and_then(Value::as_object)
        .unwrap_or(object);
    let data = result
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("The Codex model page has no data array."))?;
    if data.len() > MAX_MODELS {
        return Err(invalid("The Codex model page contains too many models."));
    }

    let next_cursor = match result
        .get("nextCursor")
        .or_else(|| result.get("next_cursor"))
    {
        None | Some(Value::Null) => None,
        Some(Value::String(cursor)) => {
            validate_text(cursor, "Codex pagination cursor", 128)?;
            Some(cursor.clone())
        }
        Some(_) => {
            return Err(invalid(
                "The Codex pagination cursor must be a nonempty string or null.",
            ));
        }
    };

    let mut models = Vec::with_capacity(data.len());
    let mut ids = HashSet::new();
    for item in data {
        let object = item
            .as_object()
            .ok_or_else(|| invalid("The Codex model page contains a non-object model row."))?;
        let model_id = required_text(object, &["model", "id"], "Codex model ID")?;
        if !ids.insert(model_id.clone()) {
            return Err(invalid(
                "The Codex model page contains a duplicate model ID.",
            ));
        }
        let hidden = match object.get("hidden") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| invalid("The Codex model hidden flag must be a boolean."))?,
        };
        if hidden {
            continue;
        }
        if !parse_input_modalities(object)? {
            continue;
        }
        let label = text_from(object, &["displayName", "display_name", "name"])
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| model_id.clone());
        let reasoning_levels = parse_reasoning(object)?;
        let default_reasoning = declared_identifier(
            object,
            &[
                "defaultReasoningEffort",
                "defaultReasoning",
                "default_reasoning",
            ],
            "Codex default reasoning level",
            false,
        )?;
        if default_reasoning
            .as_ref()
            .is_some_and(|default| !reasoning_levels.iter().any(|level| level == default))
        {
            return Err(invalid(
                "The Codex default reasoning level is not declared.",
            ));
        }
        let service_tiers = parse_service_tiers(object)?;
        let default_service_tier = declared_identifier(
            object,
            &["defaultServiceTier", "default_service_tier"],
            "Codex default service tier",
            true,
        )?;
        if default_service_tier
            .as_ref()
            .is_some_and(|default| !service_tiers.iter().any(|tier| tier.id == *default))
        {
            return Err(invalid("The Codex default service tier is not declared."));
        }
        let model = CodexCatalogModel {
            model_id,
            label,
            reasoning_levels,
            default_reasoning,
            service_tiers,
            default_service_tier,
        };
        model.validate()?;
        models.push(model);
    }
    Ok(CodexModelPage {
        models,
        next_cursor,
    })
}

fn parse_reasoning(object: &Map<String, Value>) -> CoreResult<Vec<String>> {
    let Some(values) = object
        .get("supportedReasoningEfforts")
        .or_else(|| object.get("supported_reasoning_efforts"))
        .or_else(|| object.get("reasoningLevels"))
    else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| invalid("The Codex reasoning levels must be an array."))?;
    if values.len() > MAX_TRAITS {
        return Err(invalid(
            "The Codex model declares too many reasoning levels.",
        ));
    }
    let mut result = Vec::new();
    for value in values {
        let candidate = value.as_str().map(str::to_owned).or_else(|| {
            value.as_object().and_then(|entry| {
                text_from(
                    entry,
                    &["reasoningEffort", "reasoning_effort", "id", "name"],
                )
            })
        });
        let Some(candidate) = candidate else {
            return Err(invalid(
                "The Codex reasoning levels contain a malformed entry.",
            ));
        };
        if result.iter().any(|item| item == &candidate) {
            continue;
        }
        validate_identifier(&candidate, "reasoning level")?;
        result.push(candidate);
    }
    Ok(result)
}

fn parse_service_tiers(object: &Map<String, Value>) -> CoreResult<Vec<ServiceTier>> {
    let Some(values) = object
        .get("serviceTiers")
        .or_else(|| object.get("service_tiers"))
    else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| invalid("The Codex service tiers must be an array."))?;
    if values.len() > MAX_TRAITS {
        return Err(invalid("The Codex model declares too many service tiers."));
    }
    let mut result = Vec::new();
    for value in values {
        let (id, label) = if let Some(id) = value.as_str() {
            (id.to_owned(), id.to_owned())
        } else if let Some(entry) = value.as_object() {
            let id = required_text(
                entry,
                &["id", "serviceTier", "service_tier", "name"],
                "Codex service tier ID",
            )?;
            let label =
                text_from(entry, &["label", "name", "description"]).unwrap_or_else(|| id.clone());
            (id, label)
        } else {
            return Err(invalid(
                "The Codex service tiers contain a malformed entry.",
            ));
        };
        if result.iter().any(|item: &ServiceTier| item.id == id) {
            continue;
        }
        validate_identifier(&id, "service tier ID")?;
        validate_label(&label, "service tier label")?;
        result.push(ServiceTier { id, label });
    }
    Ok(result)
}

fn text_from(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        object.get(*name).and_then(Value::as_str).and_then(|value| {
            if value.is_empty()
                || value.chars().any(char::is_control)
                || value.chars().count() > MAX_LABEL_CHARS
            {
                None
            } else {
                Some(value.to_owned())
            }
        })
    })
}

fn required_text(object: &Map<String, Value>, names: &[&str], label: &str) -> CoreResult<String> {
    let Some(name) = names.iter().find(|name| object.contains_key(**name)) else {
        return Err(invalid(&format!("The {label} is missing.")));
    };
    let value = object
        .get(*name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("The {label} must be a string.")))?;
    validate_text(value, label, MAX_LABEL_CHARS)?;
    Ok(value.to_owned())
}

fn declared_identifier(
    object: &Map<String, Value>,
    names: &[&str],
    label: &str,
    allow_null: bool,
) -> CoreResult<Option<String>> {
    let Some(name) = names.iter().find(|name| object.contains_key(**name)) else {
        return Ok(None);
    };
    let value = object.get(*name).expect("name came from object keys");
    if allow_null && value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| invalid(&format!("The {label} must be a string or null.")))?;
    validate_identifier(value, label)?;
    Ok(Some(value.to_owned()))
}

/// Return whether the row accepts text input.  The generated schema currently
/// defaults absent modalities to text+image, so an absent field remains
/// eligible.  A present image-only row is well-formed but unsuitable for the
/// text authoring adapter; unknown or malformed modality values invalidate the
/// complete page rather than silently changing the model's meaning.
fn parse_input_modalities(object: &Map<String, Value>) -> CoreResult<bool> {
    let Some(name) = ["inputModalities", "input_modalities"]
        .iter()
        .find(|name| object.contains_key(**name))
    else {
        return Ok(true);
    };
    let values = object
        .get(*name)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("The Codex input modalities must be an array."))?;
    let mut accepts_text = false;
    for value in values {
        match value.as_str() {
            Some("text") => accepts_text = true,
            Some("image" | "audio") => {}
            Some(_) => {
                return Err(invalid(
                    "The Codex input modalities contain an unknown value.",
                ));
            }
            None => {
                return Err(invalid(
                    "The Codex input modalities contain a malformed value.",
                ));
            }
        }
    }
    Ok(accepts_text)
}

fn validate_text(value: &str, label: &str, max_chars: usize) -> CoreResult<()> {
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        return Err(invalid(&format!(
            "The {label} is empty, too long, or contains control characters."
        )));
    }
    Ok(())
}

fn validate_cli_version(value: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.chars().count() > MAX_IDENTIFIER_CHARS
        || !value.as_bytes()[0].is_ascii_digit()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(invalid(
            "Codex CLI version is not a valid observed version token.",
        ));
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> CoreResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid(
            "Codex executable fingerprint must be 64 lowercase hexadecimal characters.",
        ));
    }
    Ok(())
}

fn validate_label(value: &str, label: &str) -> CoreResult<()> {
    validate_text(value, label, MAX_LABEL_CHARS)
}

fn validate_identifier(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.chars().count() > MAX_IDENTIFIER_CHARS
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/".contains(&byte))
    {
        return Err(invalid(&format!(
            "The {label} must be 1..128 ASCII identifier characters."
        )));
    }
    Ok(())
}

fn validate_identifiers(values: &[String], label: &str) -> CoreResult<()> {
    let mut seen = HashSet::new();
    for value in values {
        validate_identifier(value, label)?;
        if !seen.insert(value) {
            return Err(invalid(&format!(
                "The Codex catalog contains a duplicate {label}."
            )));
        }
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> CoreResult<()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(
            "Codex discovery time must be a nonnegative Unix millisecond string.",
        ));
    }
    value
        .parse::<u128>()
        .map_err(|_| invalid("Codex discovery time is out of range."))?;
    Ok(())
}

fn invalid(detail: &str) -> CoreError {
    CoreError::new("InvalidCodexCatalog", detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_unknown_fields_defaults_and_hidden_models() {
        let page = json!({
            "data": [
                {"model": "hidden", "displayName": "Hidden", "hidden": true},
                {"model": "writer-v3", "displayName": "Writer V3", "supportedReasoningEfforts": [{"reasoningEffort": "high"}, "xhigh"], "defaultReasoningEffort": "high", "serviceTiers": [{"id": "fast", "name": "Fast"}], "defaultServiceTier": "fast", "futureField": {"ignored": true}}
            ],
            "nextCursor": "next-1"
        });
        let parsed = parse_model_page(page.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.next_cursor.as_deref(), Some("next-1"));
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].model_id, "writer-v3");
        assert_eq!(parsed.models[0].default_reasoning.as_deref(), Some("high"));
        assert_eq!(
            parsed.models[0].default_service_tier.as_deref(),
            Some("fast")
        );
    }

    #[test]
    fn duplicate_traits_are_deduplicated_but_duplicate_models_are_rejected() {
        let page = json!({"data": [
            {"id": "one", "supportedReasoningEfforts": ["high", "high"], "serviceTiers": ["fast", "fast"]}
        ]});
        let parsed = parse_model_page(page.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].reasoning_levels, vec!["high"]);
        assert_eq!(parsed.models[0].service_tiers[0].id, "fast");

        let duplicate = json!({"data": [
            {"id": "one"},
            {"id": "one", "displayName": "later"}
        ]});
        assert!(parse_model_page(duplicate.to_string().as_bytes()).is_err());
    }

    #[test]
    fn text_eligibility_defaults_absent_modalities_and_excludes_image_only_rows() {
        let page = json!({"data": [
            {"id": "default-modalities"},
            {"id": "text-and-image", "inputModalities": ["text", "image"]},
            {"id": "image-only", "inputModalities": ["image"]},
            {"id": "audio-only", "inputModalities": ["audio"]}
        ]});
        let parsed = parse_model_page(page.to_string().as_bytes()).unwrap();
        assert_eq!(
            parsed
                .models
                .iter()
                .map(|model| model.model_id.as_str())
                .collect::<Vec<_>>(),
            vec!["default-modalities", "text-and-image"]
        );

        for modalities in [json!("text"), json!(["unknown"]), json!([null])] {
            let malformed = json!({"data": [{"id": "bad", "inputModalities": modalities}]});
            assert!(parse_model_page(malformed.to_string().as_bytes()).is_err());
        }
    }

    #[test]
    fn parses_sanitized_codex_01534_model_list_shape_with_optional_null_tiers() {
        fn row(id: &str, default: &str, efforts: &[&str], has_priority: bool) -> Value {
            let reasoning = efforts
                .iter()
                .map(|effort| json!({"reasoningEffort": effort, "description": ""}))
                .collect::<Vec<_>>();
            let tiers = if has_priority {
                json!([{"id": "priority", "name": "Fast", "description": "Increased usage"}])
            } else {
                json!([])
            };
            json!({
                "id": id,
                "model": id,
                "displayName": id,
                "defaultReasoningEffort": default,
                "supportedReasoningEfforts": reasoning,
                "defaultServiceTier": null,
                "serviceTiers": tiers,
                "inputModalities": ["text", "image"],
                "hidden": false,
                "isDefault": false,
                "supportsPersonality": false,
                "multiAgentVersion": "v1",
                "unknownFutureField": {"ignored": true}
            })
        }

        let page = json!({
            "data": [
                row("gpt-6-astra", "medium", &["low", "medium", "high", "xhigh", "max", "ultra"], true),
                row("gpt-5.6-sol", "low", &["low", "medium", "high", "xhigh", "max", "ultra"], true),
                row("gpt-5.6-terra", "medium", &["low", "medium", "high", "xhigh", "max", "ultra"], true),
                row("gpt-5.6-luna", "medium", &["low", "medium", "high", "xhigh", "max"], true),
                row("gpt-5.5", "medium", &["low", "medium", "high", "xhigh"], true),
                row("gpt-5.4-mini", "medium", &["low", "medium", "high", "xhigh"], false),
                row("gpt-5.3-codex-spark", "high", &["low", "medium", "high", "xhigh"], false)
            ],
            "nextCursor": null
        });
        let parsed = parse_model_page(page.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.models.len(), 7);
        assert_eq!(parsed.models[3].model_id, "gpt-5.6-luna");
        assert_eq!(
            parsed.models[3].default_reasoning.as_deref(),
            Some("medium")
        );
        assert_eq!(parsed.models[3].default_service_tier, None);
        assert_eq!(parsed.models[6].model_id, "gpt-5.3-codex-spark");
    }

    #[test]
    fn invalid_defaults_and_rows_reject_the_complete_page() {
        for row in [
            json!({"id": "one", "supportedReasoningEfforts": ["high"], "defaultReasoningEffort": "max"}),
            json!({"id": "one", "serviceTiers": ["fast"], "defaultServiceTier": "priority"}),
            json!({"id": "one", "serviceTiers": ["fast"], "defaultServiceTier": 1}),
            json!({"id": "one", "supportedReasoningEfforts": "high"}),
            json!({"id": "one", "serviceTiers": "fast"}),
        ] {
            let malformed = json!({"data": [row]});
            assert!(parse_model_page(malformed.to_string().as_bytes()).is_err());
        }
        assert!(parse_model_page(br#"{"data":[null]}"#).is_err());
        assert!(parse_model_page(br#"{"data":[{}]}"#).is_err());
    }

    #[test]
    fn malformed_page_is_rejected_and_selection_traits_are_checked() {
        assert!(parse_model_page(br#"{"data":"bad"}"#).is_err());
        assert_eq!(
            parse_model_page(br#"{"data":[],"nextCursor":null}"#)
                .unwrap()
                .next_cursor,
            None
        );
        for cursor in [
            br#"{"data":[],"nextCursor":1}"#.as_slice(),
            br#"{"data":[],"nextCursor":{}}"#.as_slice(),
            br#"{"data":[],"nextCursor":""}"#.as_slice(),
            br#"{"data":[],"nextCursor":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.as_slice(),
        ] {
            assert!(parse_model_page(cursor).is_err());
        }
        let catalog = CodexCatalog {
            cli_version: "1.0.0".into(),
            executable_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .into(),
            discovered_at: "1700000000000".into(),
            models: vec![CodexCatalogModel {
                model_id: "one".into(),
                label: "One".into(),
                reasoning_levels: vec!["high".into()],
                default_reasoning: Some("high".into()),
                service_tiers: vec![],
                default_service_tier: None,
            }],
        };
        catalog.validate().unwrap();
        assert!(catalog.supports(&ModelSelection {
            provider_id: "codex".into(),
            model_id: "one".into(),
            reasoning: None,
            service_tier: None
        }));
        assert!(!catalog.supports(&ModelSelection {
            provider_id: "codex".into(),
            model_id: "one".into(),
            reasoning: Some("max".into()),
            service_tier: None
        }));
    }
}
