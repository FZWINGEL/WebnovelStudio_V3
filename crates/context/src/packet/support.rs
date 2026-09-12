//! Part of `packet`, split out of the file §3.4 measured at 4,149 lines.
//!
//! The items are `pub(super)` rather than private because a child module's
//! private items are not visible to its parent, and this module exists to be
//! called by it.

use super::*;

/// Serialize the exact provider input used by the compiler. Storage can use
/// this same function when revalidating a persisted packet receipt.
pub fn serialized_input(
    messages: &[PacketMessage],
    options: &PacketOptions,
) -> Result<String, PacketError> {
    serde_json::to_string(&(messages, options)).map_err(|error| PacketError::InvalidRequest {
        message: format!("failed to serialize packet input: {error}"),
    })
}

/// Hash the exact serialized provider input with the packet's deterministic
/// SHA-256 receipt rule.
pub fn packet_input_hash(
    messages: &[PacketMessage],
    options: &PacketOptions,
) -> Result<String, PacketError> {
    Ok(sha256_hex(serialized_input(messages, options)?.as_bytes()))
}

pub(super) fn packet_source(source: &SelectedSource, include_display_name: bool) -> PacketSource {
    PacketSource {
        handle: source.read.read.descriptor.handle.clone(),
        source: source.read.read.descriptor.source.clone(),
        display_name: include_display_name
            .then(|| source.read.read.descriptor.display_name.clone()),
        mandatory: source.mandatory,
        kind: source.read.read.descriptor.kind,
        coverage: source.read.read.descriptor.coverage,
        reader_position: source
            .read
            .read
            .descriptor
            .disclosure
            .reader_position
            .clone(),
        author_only: source.read.read.descriptor.disclosure.author_only,
        story_time: source.read.read.descriptor.story_time.clone(),
        representation: if source.passages.is_some() {
            "wholeBlocks".to_owned()
        } else {
            "fullText".to_owned()
        },
        body: source.passages.is_none().then(|| source.read.body.clone()),
        passages: source
            .passages
            .as_ref()
            .map(|passages| {
                passages
                    .iter()
                    .map(|passage| PacketPassage {
                        block_id: passage.block_id.clone(),
                        block_order: passage.block_order,
                        text: passage.text.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(super) fn canonicalize_read(read: &SourceRead) -> Result<CanonicalRead, PacketError> {
    let serialized = serde_json::to_string(&read.body).map_err(|error| {
        source_binding(
            "InvalidSourceBody",
            format!("The source body could not be serialized: {error}"),
            Some(read.descriptor.handle.clone()),
        )
    })?;
    let receipt = validate_snapshot_json(&serialized).map_err(|message| {
        source_binding(
            "InvalidSourceBody",
            format!("The source body is not a canonical document: {message}"),
            Some(read.descriptor.handle.clone()),
        )
    })?;
    if receipt.hash != read.descriptor.source.body_hash {
        return Err(source_binding(
            "SourceBodyHashMismatch",
            "The source body does not match its frozen body hash.",
            Some(read.descriptor.handle.clone()),
        ));
    }
    let blocks = receipt.snapshot["body"]["content"]
        .as_array()
        .ok_or_else(|| {
            source_binding(
                "InvalidSourceBody",
                "The canonical source body has no block content.",
                Some(read.descriptor.handle.clone()),
            )
        })?;
    if blocks.len() != read.passages.len() {
        return Err(source_binding(
            "PassageProjectionMismatch",
            "The source passage projection does not cover the exact body.",
            Some(read.descriptor.handle.clone()),
        ));
    }
    for (order, (block, passage)) in blocks.iter().zip(&read.passages).enumerate() {
        let block_id = block["attrs"]["id"].as_str().unwrap_or_default();
        let text = block_text(block);
        if passage.handle != read.descriptor.handle
            || passage.source != read.descriptor.source
            || passage.block_id != block_id
            || passage.block_order != order as u32
            || passage.text != text
        {
            return Err(source_binding(
                "PassageProjectionMismatch",
                "A source passage is not an exact projection of its frozen body.",
                Some(read.descriptor.handle.clone()),
            ));
        }
    }
    Ok(CanonicalRead {
        read: read.clone(),
        body: receipt.snapshot,
        passages: read.passages.clone(),
    })
}

pub(super) fn block_text(block: &Value) -> String {
    block["content"]
        .as_array()
        .map(|content| {
            content
                .iter()
                .map(|inline| {
                    if inline["type"] == "hardBreak" {
                        "\n".to_owned()
                    } else {
                        inline["text"].as_str().unwrap_or_default().to_owned()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn validate_request_identity(request: &PacketRequest) -> Result<(), PacketError> {
    for (label, value) in [
        ("packetId", request.packet_id.as_str()),
        ("sessionId", request.session_id.as_str()),
        ("invocationOrdinal", request.invocation_ordinal.as_str()),
    ] {
        if value.is_empty() {
            return Err(PacketError::InvalidRequest {
                message: format!("{label} must not be empty"),
            });
        }
    }
    parse_decimal(&request.invocation_ordinal).map_err(|message| PacketError::InvalidRequest {
        message: format!("invocationOrdinal is invalid: {message}"),
    })?;
    validate_safe_brief(request)?;
    if request.budget.model_id != MOCK_MODEL_ID {
        return Err(PacketError::Budget(budget_error(
            BudgetErrorCode::InvalidBudget,
            0,
            0,
            Vec::new(),
            "Only the deterministic mock-story-context budget profile is supported.",
        )));
    }
    if let Some(binding) = request.provider_binding.as_ref() {
        binding
            .validate()
            .map_err(|message| PacketError::InvalidRequest { message })?;
    }
    Ok(())
}

pub(super) fn validate_safe_brief(request: &PacketRequest) -> Result<(), PacketError> {
    let Some(brief) = request.safe_brief.as_ref() else {
        return Ok(());
    };
    if brief.text.is_empty() || brief.text.trim().is_empty() {
        return Err(PacketError::InvalidRequest {
            message: "The approved writing brief must be nonempty.".to_owned(),
        });
    }
    if brief.text.len() > MAX_SAFE_BRIEF_BYTES {
        return Err(PacketError::InvalidRequest {
            message: "The approved writing brief exceeds 16 KiB.".to_owned(),
        });
    }
    if !brief.confirmed {
        return Err(PacketError::InvalidRequest {
            message: "The approved writing brief must be explicitly confirmed.".to_owned(),
        });
    }
    let valid_scope = match request.frozen.purpose {
        ContextPurpose::Revise => request.scope.as_ref().is_some_and(|scope| {
            matches!(
                scope.kind,
                ScopeKind::Passage | ScopeKind::Blocks | ScopeKind::WholeDocument
            )
        }),
        ContextPurpose::Continue => request
            .scope
            .as_ref()
            .is_some_and(|scope| scope.kind == ScopeKind::Append),
        _ => false,
    };
    if request.frozen.policy.audience != Audience::RestrictedWriting || !valid_scope {
        return Err(PacketError::InvalidRequest {
            message: if request.frozen.purpose == ContextPurpose::Continue {
                "An approved writing brief requires a restricted continuation append scope."
                    .to_owned()
            } else {
                "An approved writing brief requires a restricted scoped revision.".to_owned()
            },
        });
    }
    if let Some(origin) = brief.origin_message_id.as_deref()
        && (origin.is_empty()
            || origin.len() > 64
            || !origin
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    {
        return Err(PacketError::InvalidRequest {
            message: "The approved writing brief origin message ID is invalid.".to_owned(),
        });
    }
    if let Some(origin) = brief.project_origin.as_ref() {
        if origin.version != "project-conversation-brief.v1"
            || origin.project_id.is_empty()
            || origin.operation_namespace.is_empty()
            || origin.conversation_id.is_empty()
            || origin.message_id.is_empty()
            || origin.scope_hash.len() != 64
            || origin.text_hash != sha256_hex(brief.text.as_bytes())
            || !origin
                .scope_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(PacketError::InvalidRequest {
                message: "The project conversation brief provenance is malformed or does not match its text.".to_owned(),
            });
        }
        // The project-origin hash is over the renderer-facing
        // DiscussionScopeInput shape.  Continuation has no author-selected
        // scope (`StartDiscussion.scope` is None); its compiled Append grant
        // is derived from the exact frozen target solely to constrain the
        // provider.  Keep the author-confirmed null provenance in that case,
        // while still requiring the derived Append grant above.  For scoped
        // revisions, normalize the trusted ScopeGrant back to the input shape
        // before hashing so the same author-confirmed digest survives
        // compilation.
        let scope_input = if request.frozen.purpose == ContextPurpose::Continue {
            None
        } else {
            request.scope.as_ref().map(|scope| BriefScopeInput {
                kind: scope.kind,
                start: scope.start.clone(),
                end: scope.end.clone(),
                quote: scope.quote.clone(),
                source_body_hash: scope.source_hash.clone(),
            })
        };
        let scope_hash = sha256_hex(&serde_json::to_vec(&scope_input).map_err(|error| {
            PacketError::InvalidRequest {
                message: format!("failed to hash the approved brief scope: {error}"),
            }
        })?);
        if origin.scope_hash != scope_hash
            || origin.target.document_id != request.frozen.snapshot.target.document_id
            || origin.target.body_hash != request.frozen.snapshot.target.body_hash
        {
            return Err(PacketError::InvalidRequest {
                message: "The project conversation brief is bound to another target or scope."
                    .to_owned(),
            });
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BriefScopeInput {
    pub(super) kind: ScopeKind,
    pub(super) start: Option<Endpoint>,
    pub(super) end: Option<Endpoint>,
    pub(super) quote: String,
    pub(super) source_body_hash: String,
}

pub(super) fn mandatory_handles(request: &PacketRequest) -> Result<Vec<String>, PacketError> {
    let target = request
        .frozen
        .snapshot
        .sources
        .iter()
        .find(|descriptor| descriptor.source == request.frozen.snapshot.target)
        .map(|descriptor| descriptor.handle.clone())
        .ok_or_else(|| {
            source_binding(
                "TargetNotInManifest",
                "The frozen target is not in the source manifest.",
                None,
            )
        })?;
    let mut result = vec![target];
    let mut seen = HashSet::new();
    for handle in &request.mandatory_handles {
        if !seen.insert(handle) {
            return Err(PacketError::InvalidRequest {
                message: format!("mandatoryHandles contains duplicate handle {handle:?}"),
            });
        }
        if result.iter().any(|existing| existing == handle) {
            return Err(PacketError::InvalidRequest {
                message: format!("mandatoryHandles repeats the target handle {handle:?}"),
            });
        }
        result.push(handle.clone());
    }
    Ok(result)
}

pub(super) fn manifest_by_handle(
    frozen: &FrozenContext,
) -> Result<HashMap<String, &crate::contracts::SourceDescriptor>, PacketError> {
    let mut result = HashMap::with_capacity(frozen.snapshot.sources.len());
    for descriptor in &frozen.snapshot.sources {
        if result
            .insert(descriptor.handle.clone(), descriptor)
            .is_some()
        {
            return Err(PacketError::Eligibility(EligibilityError {
                code: crate::eligibility::EligibilityErrorCode::DuplicateSource,
                message: "The frozen source manifest contains duplicate handles.".to_owned(),
                handle: Some(descriptor.handle.clone()),
                dependency: None,
            }));
        }
    }
    Ok(result)
}

pub(super) fn stable_source_order(
    frozen: &FrozenContext,
    target: &str,
    mandatory: &[String],
    eligible: &HashSet<&str>,
) -> Vec<String> {
    let mut result = Vec::with_capacity(eligible.len());
    result.push(target.to_owned());
    for handle in mandatory {
        if eligible.contains(handle.as_str()) && !result.iter().any(|item| item == handle) {
            result.push(handle.clone());
        }
    }
    for descriptor in &frozen.snapshot.sources {
        if eligible.contains(descriptor.handle.as_str())
            && !result.iter().any(|item| item == &descriptor.handle)
        {
            result.push(descriptor.handle.clone());
        }
    }
    result
}

pub(super) fn available_input_tokens(budget: &MockContextBudget) -> Result<usize, PacketError> {
    let window = parse_decimal(&budget.context_window_tokens);
    let output = parse_decimal(&budget.reserved_output_tokens);
    let protocol = parse_decimal(&budget.reserved_protocol_tokens);
    let (window, output, protocol) = match (window, output, protocol) {
        (Ok(window), Ok(output), Ok(protocol))
            if output
                .checked_add(protocol)
                .is_some_and(|reserved| reserved <= window) =>
        {
            (window, output, protocol)
        }
        _ => {
            return Err(PacketError::Budget(budget_error(
                BudgetErrorCode::InvalidBudget,
                0,
                0,
                Vec::new(),
                "Context window and output/protocol reservations must be canonical decimal strings with reservations within the window.",
            )));
        }
    };
    let available = window
        .checked_sub(output)
        .and_then(|remaining| remaining.checked_sub(protocol))
        .ok_or_else(|| {
            PacketError::Budget(budget_error(
                BudgetErrorCode::InvalidBudget,
                0,
                0,
                Vec::new(),
                "The reserved output and protocol counters exceed the context window.",
            ))
        })?;
    usize::try_from(available).map_err(|_| {
        PacketError::Budget(budget_error(
            BudgetErrorCode::InvalidBudget,
            0,
            0,
            Vec::new(),
            "The available mock input budget does not fit the host counter.",
        ))
    })
}

pub(super) fn budget_error(
    code: BudgetErrorCode,
    required: usize,
    available: usize,
    mandatory_handles: Vec<String>,
    message: &str,
) -> BudgetError {
    BudgetError {
        code,
        message: message.to_owned(),
        required_input_tokens: required.to_string(),
        available_input_tokens: available.to_string(),
        mandatory_handles,
    }
}

pub(super) fn omission(handle: &str, reason: &str) -> String {
    format!("handle:{handle};reason:{reason}")
}

pub(super) fn optional_omissions(
    optional_handles: &[String],
    reads: &HashMap<String, CanonicalRead>,
    selected_block_counts: &HashMap<String, usize>,
    directory_omissions: &[String],
) -> Vec<String> {
    let mut omissions = Vec::new();
    for handle in optional_handles {
        let total = reads
            .get(handle.as_str())
            .map_or(0, |read| read.passages.len());
        let selected = selected_block_counts.get(handle).copied().unwrap_or(0);
        if selected == 0 {
            omissions.push(omission(
                handle,
                &format!("optional source omitted by input budget;blocks:{total}"),
            ));
        } else if selected < total {
            omissions.push(omission(
                handle,
                &format!(
                    "optional blocks omitted by input budget;remaining:{}",
                    total - selected
                ),
            ));
        }
    }
    omissions.extend(directory_omissions.iter().cloned());
    omissions
}

pub(super) fn source_binding(code: &str, message: impl Into<String>, handle: Option<String>) -> PacketError {
    PacketError::SourceBinding {
        code: code.to_owned(),
        message: message.into(),
        handle,
    }
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

pub(super) fn summary_is_smaller(summary: &ReviewedSummarySet, request: &PacketRequest) -> bool {
    let summary_bytes =
        serde_json::to_vec(&summary_payload(summary)).map_or(usize::MAX, |bytes| bytes.len());
    request
        .sources
        .iter()
        .find(|source| source.descriptor.handle == summary.source_handle)
        .is_some_and(|source| {
            serde_json::to_vec(&source.body).is_ok_and(|bytes| summary_bytes < bytes.len())
        })
}

pub(super) fn summary_source_omissions(original: &[String], summaries: &[ReviewedSummarySet]) -> Vec<String> {
    let mut omissions: Vec<String> = original
        .iter()
        .filter(|entry| {
            !summaries.iter().any(|summary| {
                entry.starts_with(&format!("handle:{};reason:", summary.source_handle))
            })
        })
        .cloned()
        .collect();
    omissions.extend(summaries.iter().map(|summary| {
        omission(
            &summary.source_handle,
            "accepted narrative summary delivered;original source omitted",
        )
    }));
    omissions
}

/// The provider-facing options a compiled packet carries.
///
/// One input and one output: the only block in the pipeline that carries
/// nothing else across its boundary.
pub(super) fn packet_options(request: &PacketRequest) -> Result<PacketOptions, PacketError> {
    Ok(match request.provider_binding.as_ref() {
    Some(binding) => {
        binding
            .validate()
            .map_err(|message| PacketError::InvalidRequest { message })?;
        PacketOptions {
            model_id: binding.model_id.clone(),
            // This boundary has no qualified provider token limit. The
            // retained output cap is an application byte limit instead.
            max_output_tokens: String::new(),
            token_accounting_method: binding.accounting_method.clone(),
            provider_binding: Some(binding.clone()),
        }
    }
    None => PacketOptions {
        model_id: request.budget.model_id.clone(),
        max_output_tokens: request.budget.reserved_output_tokens.clone(),
        token_accounting_method: MOCK_TOKEN_ACCOUNTING_METHOD.to_owned(),
        provider_binding: None,
    },
    })
}

pub(super) fn canonical_by_handle(reads: Vec<CanonicalRead>) -> HashMap<String, CanonicalRead> {
    reads
        .into_iter()
        .map(|read| (read.read.descriptor.handle.clone(), read))
        .collect()
}

pub(super) fn navigation_by_handle(
    views: &[ValidatedNavigationView],
) -> HashMap<String, ValidatedNavigationView> {
    views
        .iter()
        .cloned()
        .map(|view| (view.source_handle.clone(), view))
        .collect()
}
