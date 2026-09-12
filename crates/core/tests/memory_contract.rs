use serde_json::{Value, json};
use webnovel_core::context::memory::{
    DIGEST_SCHEMA_VERSION, DigestCandidate, DigestEvidence, DigestItem, MAX_ITEM_TEXT_BYTES,
    MAX_RAW_BYTES, mock_navigation_digest, validate_navigation_digest,
};
use webnovel_core::context::{CoverageLabel, Disclosure, SourceDescriptor, SourceKind, SourceRef};
use webnovel_core::projects::story_context::{SourcePassage, SourceRead};

fn source_read() -> SourceRead {
    let input = json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "attrs": {"id": "p1"},
                    "content": [{"type": "text", "text": "Alpha 😀 beta"}]
                },
                {
                    "type": "paragraph",
                    "attrs": {"id": "p2"},
                    "content": [{"type": "text", "text": "Second passage."}]
                }
            ]
        }
    });
    let receipt = webnovel_core::validate_snapshot_json(&serde_json::to_string(&input).unwrap())
        .expect("fixture is a valid snapshot");
    let source = SourceRef {
        project_id: "project-1".into(),
        document_id: "chapter-1".into(),
        revision_id: "revision-1".into(),
        body_hash: receipt.hash,
    };
    let descriptor = SourceDescriptor {
        handle: "chapter-1".into(),
        source: source.clone(),
        display_name: "Chapter 1".into(),
        kind: SourceKind::CurrentDraft,
        current: true,
        coverage: CoverageLabel::Verbatim,
        disclosure: Disclosure {
            reader_position: None,
            visible_to_characters: Vec::new(),
            author_only: false,
            future_private: false,
        },
        story_time: None,
        dependencies: Vec::new(),
    };
    SourceRead {
        descriptor,
        passages: vec![
            SourcePassage {
                handle: "chapter-1".into(),
                source: source.clone(),
                block_id: "p1".into(),
                block_order: 0,
                text: "Alpha 😀 beta".into(),
            },
            SourcePassage {
                handle: "chapter-1".into(),
                source,
                block_id: "p2".into(),
                block_order: 1,
                text: "Second passage.".into(),
            },
        ],
        body: receipt.snapshot,
        used_validated_projection: true,
    }
}

fn valid_candidate(read: &SourceRead) -> DigestCandidate {
    DigestCandidate {
        schema_version: DIGEST_SCHEMA_VERSION.into(),
        source: read.descriptor.source.clone(),
        items: vec![DigestItem {
            text: "The emoji passage is anchored.".into(),
            evidence: vec![DigestEvidence {
                block_id: "p1".into(),
                from_utf16: 6,
                to_utf16: 8,
                quote: "😀".into(),
            }],
            uncertainty: Some("Navigation aid only.".into()),
        }],
    }
}

#[test]
fn deterministic_mock_is_extractively_valid_and_roundtrips() {
    let read = source_read();
    let candidate = mock_navigation_digest(&read).expect("mock digest");
    assert_eq!(candidate.items.len(), 2);
    assert_eq!(candidate.items[0].text, "Alpha 😀 beta");
    assert_eq!(candidate.items[0].evidence[0].to_utf16, 13);
    let raw = serde_json::to_vec(&candidate).expect("serialize candidate");
    assert_eq!(validate_navigation_digest(&raw, &read).unwrap(), candidate);
}

#[test]
fn mock_samples_long_chapters_with_valid_unicode_evidence() {
    let mut read = source_read();
    let texts: Vec<_> = (0..40)
        .map(|index| format!("Passage {index}: {}", "😀word ".repeat(300)))
        .collect();
    read.body["body"]["content"] = json!(texts.iter().enumerate().map(|(index, text)| {
        json!({"type":"paragraph","attrs":{"id":format!("p{index}")},"content":[{"type":"text","text":text}]})
    }).collect::<Vec<_>>());
    let canonical =
        webnovel_core::validate_snapshot_json(&serde_json::to_string(&read.body).unwrap()).unwrap();
    read.body = canonical.snapshot;
    read.descriptor.source.body_hash = canonical.hash;
    read.passages = texts
        .into_iter()
        .enumerate()
        .map(|(index, text)| SourcePassage {
            handle: read.descriptor.handle.clone(),
            source: read.descriptor.source.clone(),
            block_id: format!("p{index}"),
            block_order: index as u32,
            text,
        })
        .collect();
    let digest = mock_navigation_digest(&read).unwrap();
    assert_eq!(digest.items.len(), 16);
    assert_eq!(digest.items.first().unwrap().evidence[0].block_id, "p0");
    assert_eq!(digest.items.last().unwrap().evidence[0].block_id, "p39");
    assert_eq!(
        validate_navigation_digest(&serde_json::to_vec(&digest).unwrap(), &read).unwrap(),
        digest
    );
}

#[test]
fn source_identity_must_match_exact_project_revision_and_hash() {
    let read = source_read();
    let mut candidate = valid_candidate(&read);
    candidate.source.project_id = "other-project".into();
    let error = validate_navigation_digest(&serde_json::to_vec(&candidate).unwrap(), &read)
        .expect_err("foreign source must be rejected");
    assert_eq!(error.code, "MemorySourceMismatch");
}

#[test]
fn a_generated_view_cannot_be_relabelled_as_original_memory_input() {
    let mut read = source_read();
    let candidate = valid_candidate(&read);
    read.descriptor.kind = SourceKind::GeneratedDigest;
    read.descriptor.coverage = CoverageLabel::Digest;
    assert_eq!(
        validate_navigation_digest(&serde_json::to_vec(&candidate).unwrap(), &read)
            .unwrap_err()
            .code,
        "MemorySourceMismatch"
    );
    assert_eq!(
        mock_navigation_digest(&read).unwrap_err().code,
        "MemorySourceMismatch"
    );
}

#[test]
fn trusted_body_hash_and_projection_are_revalidated() {
    let mut read = source_read();
    read.body["body"]["content"][0]["content"][0]["text"] = Value::String("changed".into());
    let error =
        validate_navigation_digest(&serde_json::to_vec(&valid_candidate(&read)).unwrap(), &read)
            .expect_err("changed trusted body must be rejected");
    assert_eq!(error.code, "MemoryBodyMismatch");
}

#[test]
fn unknown_fields_are_rejected_before_acceptance() {
    let read = source_read();
    let mut raw = serde_json::to_value(valid_candidate(&read)).unwrap();
    raw.as_object_mut()
        .expect("candidate object")
        .insert("producerSecret".into(), Value::String("no".into()));
    let error = validate_navigation_digest(&serde_json::to_vec(&raw).unwrap(), &read)
        .expect_err("unknown wire fields must be rejected");
    assert_eq!(error.code, "InvalidMemoryCandidate");
}

#[test]
fn unicode_ranges_must_not_split_surrogates_or_forge_quotes() {
    let read = source_read();
    let mut split = valid_candidate(&read);
    split.items[0].evidence[0].from_utf16 = 7;
    split.items[0].evidence[0].to_utf16 = 8;
    let error = validate_navigation_digest(&serde_json::to_vec(&split).unwrap(), &read)
        .expect_err("surrogate split must be rejected");
    assert_eq!(error.code, "InvalidMemoryEvidence");

    let mut forged = valid_candidate(&read);
    forged.items[0].evidence[0].quote = "not the emoji".into();
    let error = validate_navigation_digest(&serde_json::to_vec(&forged).unwrap(), &read)
        .expect_err("forged quote must be rejected");
    assert_eq!(error.code, "InvalidMemoryEvidence");
}

#[test]
fn item_and_raw_response_limits_are_bounded() {
    let read = source_read();
    let mut oversized_item = valid_candidate(&read);
    oversized_item.items[0].text = "x".repeat(MAX_ITEM_TEXT_BYTES + 1);
    let error = validate_navigation_digest(&serde_json::to_vec(&oversized_item).unwrap(), &read)
        .expect_err("oversized item must be rejected");
    assert_eq!(error.code, "InvalidMemoryCandidate");

    let error = validate_navigation_digest(&vec![b'x'; MAX_RAW_BYTES + 1], &read)
        .expect_err("oversized raw response must be rejected");
    assert_eq!(error.code, "InvalidMemoryCandidate");
}

#[test]
fn empty_source_has_no_fake_digest() {
    let mut read = source_read();
    for passage in &mut read.passages {
        passage.text.clear();
    }
    read.body["body"]["content"] = json!([
        {"type": "paragraph", "attrs": {"id": "p1"}},
        {"type": "paragraph", "attrs": {"id": "p2"}}
    ]);
    let receipt =
        webnovel_core::validate_snapshot_json(&serde_json::to_string(&read.body).unwrap())
            .expect("empty paragraphs remain a valid snapshot");
    read.descriptor.source.body_hash = receipt.hash.clone();
    for passage in &mut read.passages {
        passage.source.body_hash = receipt.hash.clone();
    }
    read.body = receipt.snapshot;
    let error = mock_navigation_digest(&read).expect_err("empty chapter must fail clearly");
    assert_eq!(error.code, "EmptyMemorySource");
}
