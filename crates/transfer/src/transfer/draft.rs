use super::*;

pub(crate) fn plain_projection(body: &Value) -> CoreResult<String> {
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The checkpoint body has no document blocks.",
            )
        })?;
    let mut rendered = Vec::with_capacity(blocks.len());
    for block in blocks {
        let object = block.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The checkpoint contains an invalid block.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("sceneBreak") => rendered.push("[Scene break]".to_owned()),
            Some("paragraph") | Some("heading") => {
                let mut text = String::new();
                if let Some(content) = object.get("content").and_then(Value::as_array) {
                    for inline in content {
                        let inline = inline.as_object().ok_or_else(|| {
                            transfer_error(
                                "InvalidDocument",
                                "The checkpoint contains an invalid inline node.",
                            )
                        })?;
                        match inline.get("type").and_then(Value::as_str) {
                            Some("text") => text.push_str(
                                inline.get("text").and_then(Value::as_str).ok_or_else(|| {
                                    transfer_error("InvalidDocument", "A text node has no text.")
                                })?,
                            ),
                            Some("hardBreak") => text.push('\n'),
                            _ => {
                                return Err(transfer_error(
                                    "InvalidDocument",
                                    "The checkpoint contains an unsupported inline node.",
                                ));
                            }
                        }
                    }
                }
                rendered.push(text);
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The checkpoint contains an unsupported block.",
                ));
            }
        }
    }
    Ok(rendered.join("\n\n"))
}

pub(crate) fn draft_format_loss(format: DraftFormat) -> &'static str {
    match format {
        DraftFormat::PlainText => {
            "Rich marks and links are omitted; hard breaks remain newlines and scene breaks become [Scene break]."
        }
        DraftFormat::Markdown => {
            "Markdown represents headings, bold, italic, safe links, and scene breaks. Spacing and end-of-paragraph breaks depend on the reader. Comments, history, and other project material are omitted."
        }
    }
}

/// Validate a complete immutable snapshot before projecting it into an export
/// format.  The returned bytes are the exact UTF-8 content installed on disk.
pub(crate) fn project_draft(body: &Value, format: DraftFormat) -> CoreResult<ProjectedDraft> {
    let encoded = serde_json::to_string(body)?;
    let validated = validate_snapshot_json(&encoded).map_err(|error| {
        transfer_error(
            "InvalidDocument",
            format!("The export source is invalid: {error}"),
        )
    })?;
    let text = match format {
        DraftFormat::PlainText => plain_projection(&validated.snapshot)?,
        DraftFormat::Markdown => markdown_projection(&validated.snapshot)?,
    };
    let utf8_bytes = text.len() as u64;
    let sha256 = sha256_bytes(text.as_bytes());
    Ok(ProjectedDraft {
        text,
        utf8_bytes,
        sha256,
    })
}

pub(crate) fn markdown_projection(body: &Value) -> CoreResult<String> {
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source has no document blocks.",
            )
        })?;
    let mut rendered = Vec::with_capacity(blocks.len());
    for block in blocks {
        let object = block.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source contains an invalid block.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("sceneBreak") => rendered.push("---".to_owned()),
            Some("paragraph") => rendered.push(markdown_block_text(object)?),
            Some("heading") => {
                let level = object
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("level"))
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        transfer_error("InvalidDocument", "The heading has no valid level.")
                    })?;
                let text = markdown_block_text(object)?;
                let prefix = "#".repeat(usize::try_from(level).map_err(|_| {
                    transfer_error("InvalidDocument", "The heading level is too large.")
                })?);
                rendered.push(if text.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {text}")
                });
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported block.",
                ));
            }
        }
    }
    // Markdown export has one deterministic line ending and never appends a
    // newline after the final block.  Source-backed trailing empty blocks and
    // hard breaks remain part of the projected content.
    let joined = rendered.join("\n\n");
    Ok(normalize_lf(&joined))
}

pub(crate) fn markdown_block_text(block: &serde_json::Map<String, Value>) -> CoreResult<String> {
    let Some(content) = block.get("content").and_then(Value::as_array) else {
        return Ok(String::new());
    };
    let mut result = String::new();
    for inline in content {
        let object = inline.as_object().ok_or_else(|| {
            transfer_error(
                "InvalidDocument",
                "The export source contains an invalid inline node.",
            )
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("hardBreak") => result.push_str("  \n"),
            Some("text") => {
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| transfer_error("InvalidDocument", "A text node has no text."))?;
                result.push_str(&markdown_text(text, object.get("marks"))?);
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported inline node.",
                ));
            }
        }
    }
    Ok(result)
}

pub(crate) fn markdown_text(text: &str, marks: Option<&Value>) -> CoreResult<String> {
    let text = normalize_lf(text);
    let marks = marks.and_then(Value::as_array).cloned().unwrap_or_default();
    let (leading, core, trailing) = boundary_whitespace(&text);
    if core.is_empty() {
        return Ok(markdown_leading_whitespace(&text));
    }
    let leading = markdown_leading_whitespace(leading);
    let mut rendered = markdown_escape(core);
    let mut link: Option<String> = None;
    for mark in marks {
        let object = mark
            .as_object()
            .ok_or_else(|| transfer_error("InvalidDocument", "A text mark is not an object."))?;
        match object.get("type").and_then(Value::as_str) {
            Some("bold") => rendered = format!("**{rendered}**"),
            Some("italic") => rendered = format!("*{rendered}*"),
            Some("link") => {
                let href = object
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("href"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| transfer_error("InvalidDocument", "A link mark has no href."))?;
                link = Some(href.to_owned());
            }
            _ => {
                return Err(transfer_error(
                    "InvalidDocument",
                    "The export source contains an unsupported mark.",
                ));
            }
        }
    }
    if let Some(href) = link {
        // Angle-bracket destinations preserve punctuation such as parentheses
        // and query entities.  Character references keep raw angle brackets
        // from terminating the CommonMark destination while preserving the
        // URL seen by a Markdown parser.
        let href = markdown_href(&href);
        rendered = format!("[{rendered}](<{href}>)");
    }
    Ok(format!("{leading}{rendered}{trailing}"))
}

pub(crate) fn markdown_leading_whitespace(value: &str) -> String {
    let indent = value
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .fold(0, |column, ch| {
            if ch == '\t' {
                column + 4 - column % 4
            } else {
                column + 1
            }
        });
    if indent < 4 {
        return value.to_owned();
    }
    let first = value.as_bytes()[0];
    let entity = match first {
        b' ' => "&#32;",
        b'\t' => "&#9;",
        _ => unreachable!("the leading character must be an ASCII indent"),
    };
    format!("{entity}{}", &value[1..])
}

pub(crate) fn boundary_whitespace(value: &str) -> (&str, &str, &str) {
    let leading_end = value
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    let trailing_start = value[leading_end..]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, ch)| leading_end + index + ch.len_utf8())
        .unwrap_or(leading_end);
    (
        &value[..leading_end],
        &value[leading_end..trailing_start],
        &value[trailing_start..],
    )
}

pub(crate) fn markdown_escape(value: &str) -> String {
    const ESCAPED: &str = r#"\\`*_{}[]()#+-!<>|~&"#;
    let mut result = String::with_capacity(value.len());
    let mut line_start = 0;
    for (index, ch) in value.char_indices() {
        if ch == '\n' {
            result.push_str("  \n");
            line_start = index + 1;
        } else {
            // Ordinary sentence/decimal periods are readable as-is. Escape a
            // possible ordered-list marker at a line's start (CommonMark 5.2).
            let ordered_marker = if ch == '.' {
                let prefix = value[line_start..index].trim_start_matches([' ', '\t']);
                (1..=9).contains(&prefix.len())
                    && prefix.bytes().all(|byte| byte.is_ascii_digit())
                    && value[index + 1..]
                        .chars()
                        .next()
                        .is_none_or(|next| matches!(next, ' ' | '\t' | '\n'))
            } else {
                false
            };
            if ESCAPED.contains(ch) || ordered_marker {
                result.push('\\');
            }
            result.push(ch);
        }
    }
    result
}

pub(crate) fn markdown_href(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(ch),
        }
    }
    result
}

pub(crate) fn normalize_lf(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            result.push('\n');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Freeze the requested current head and return a complete, tamper-detecting
/// preview.  The preview refers to this immutable revision even if the
/// working document changes before the file is installed.
pub fn prepare_draft_export(
    project: &impl TransferSource,
    access: &ProjectAccess,
    expected: Head,
    format: DraftFormat,
) -> CoreResult<DraftExportPreview> {
    if !format.is_supported() {
        return Err(transfer_error(
            "InvalidRequest",
            "The requested export format is unsupported.",
        ));
    }
    let revision = project.checkpoint(CheckpointRequest {
        access: access.clone(),
        expected,
        reason: CheckpointReason::Export,
    })?;
    let projected = project_draft(&revision.body, format)?;
    let metadata = project.metadata()?;
    Ok(DraftExportPreview {
        id: Uuid::new_v4().to_string(),
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        source_head: revision.head,
        revision_id: revision.id,
        format,
        format_version: 1,
        utf8_bytes: projected.utf8_bytes,
        sha256: projected.sha256,
        format_loss: draft_format_loss(format).into(),
        preview_text: projected.text,
        review_bundle_id: None,
    })
}

/// Freeze the current author-reviewed chapter source without creating a
/// checkpoint.  The project actor resolves the active ReadyBundle and returns
/// its immutable target revision; the caller-supplied head is only evidence for
/// that resolution.
pub fn prepare_reviewed_draft_export(
    project: &impl TransferSource,
    access: &ProjectAccess,
    expected: Head,
    format: DraftFormat,
) -> CoreResult<DraftExportPreview> {
    if !format.is_supported() {
        return Err(transfer_error(
            "InvalidRequest",
            "The requested export format is unsupported.",
        ));
    }
    let (review_bundle_id, revision) =
        project.resolve_reviewed_export_source(access.clone(), expected)?;
    let projected = project_draft(&revision.body, format)?;
    let metadata = project.metadata()?;
    Ok(DraftExportPreview {
        id: Uuid::new_v4().to_string(),
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        source_head: revision.head,
        revision_id: revision.id,
        format,
        format_version: 1,
        utf8_bytes: projected.utf8_bytes,
        sha256: projected.sha256,
        format_loss: draft_format_loss(format).into(),
        preview_text: projected.text,
        review_bundle_id: Some(review_bundle_id),
    })
}

/// Install one prepared whole-document export and then record its immutable
/// metadata.  Filesystem installation and SQLite recording intentionally have
/// separate failure boundaries; a record failure leaves the installed output
/// in place and asks the caller to prepare a new explicit export.
pub fn export_prepared_draft(
    project: &impl TransferSource,
    access: ProjectAccess,
    preview: DraftExportPreview,
    target: &Path,
) -> CoreResult<ExportRecord> {
    let candidate = output_path(target)?;
    let basename = candidate
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            transfer_error(
                "InvalidRequest",
                "The destination must have a UTF-8 basename.",
            )
        })?
        .to_owned();
    crate::exports::validate_basename(&basename)?;
    project.install_export(access, preview, candidate, basename)
}
