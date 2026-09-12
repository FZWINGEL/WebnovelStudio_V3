use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use webnovel_core::documents::{
    Endpoint, ScopeKind, ScopeValidationRequest, capture_append_scope, validate_append,
    validate_scope,
};

fn snapshot(blocks: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": blocks}
    })
}

fn paragraph(id: &str, text: &str) -> Value {
    if text.is_empty() {
        json!({"type":"paragraph","attrs":{"id":id}})
    } else {
        json!({
            "type":"paragraph",
            "attrs":{"id":id},
            "content":[{"type":"text","text":text}]
        })
    }
}

fn scene_break(id: &str) -> Value {
    json!({"type":"sceneBreak","attrs":{"id":id}})
}

fn append_paragraphs(mut source: Value, values: &[(&str, &str)]) -> Value {
    let blocks = source["body"]["content"].as_array_mut().unwrap();
    blocks.extend(values.iter().map(|(id, text)| paragraph(id, text)));
    source
}

fn request(source: Value, result: Value) -> ScopeValidationRequest {
    let scope = capture_append_scope(&source).expect("capture append scope");
    ScopeValidationRequest {
        source_snapshot: source,
        result_snapshot: result,
        scope,
    }
}

#[test]
fn captures_last_block_quote_hash_and_complete_utf16_end() {
    let source = snapshot(vec![paragraph("p1", "Before"), paragraph("last", "é 👩‍🚀")]);
    let scope = capture_append_scope(&source).unwrap();
    assert_eq!(scope.kind, ScopeKind::Append);
    assert!(scope.start.is_none());
    assert_eq!(
        scope.end,
        Some(Endpoint {
            block_id: "last".into(),
            utf16_offset: "é 👩‍🚀".encode_utf16().count() as u32,
        })
    );
    assert_eq!(scope.quote, "é 👩‍🚀");
    assert!(!scope.quote_hash.is_empty());
    assert!(validate_scope(&request(source.clone(), source)).is_ok());
}

#[test]
fn validates_exact_append_and_preserves_every_existing_token() {
    let source = snapshot(vec![
        json!({
            "type":"heading",
            "attrs":{"id":"h","level":2},
            "content":[{"type":"text","text":"Heading"}]
        }),
        paragraph("ending", "The unchanged ending."),
    ]);
    let result = append_paragraphs(
        source.clone(),
        &[
            ("new-1", "First generated paragraph."),
            ("new-2", "Second."),
        ],
    );
    let receipt = validate_append(
        &request(source, result),
        &["First generated paragraph.".into(), "Second.".into()],
    )
    .unwrap();
    assert_eq!(receipt.scope, ScopeKind::Append);
    assert_eq!(receipt.quote, "The unchanged ending.");
    assert!(receipt.replacement_token_count > 0);
}

#[test]
fn rejects_prefix_edits_reused_ids_formatting_and_wrong_text() {
    let source = snapshot(vec![
        paragraph("existing", "Keep exact."),
        paragraph("end", "End."),
    ]);
    let scope = capture_append_scope(&source).unwrap();

    let mut changed = append_paragraphs(source.clone(), &[("new", "Added.")]);
    changed["body"]["content"][0]["content"][0]["text"] = json!("Changed.");
    assert!(
        validate_append(
            &ScopeValidationRequest {
                source_snapshot: source.clone(),
                result_snapshot: changed,
                scope: scope.clone(),
            },
            &["Added.".into()]
        )
        .unwrap_err()
        .contains("existing source prefix")
    );

    let reused = append_paragraphs(source.clone(), &[("end", "Added.")]);
    assert!(
        validate_append(
            &ScopeValidationRequest {
                source_snapshot: source.clone(),
                result_snapshot: reused,
                scope: scope.clone(),
            },
            &["Added.".into()]
        )
        .unwrap_err()
        .contains("existing block ID")
    );

    let mut marked = append_paragraphs(source.clone(), &[("new", "Added.")]);
    marked["body"]["content"][2]["content"][0]["marks"] = json!([{"type":"bold"}]);
    assert!(
        validate_append(
            &ScopeValidationRequest {
                source_snapshot: source.clone(),
                result_snapshot: marked,
                scope: scope.clone(),
            },
            &["Added.".into()]
        )
        .unwrap_err()
        .contains("formatting")
    );

    let result = append_paragraphs(source.clone(), &[("new", "Different.")]);
    assert!(
        validate_append(
            &ScopeValidationRequest {
                source_snapshot: source,
                result_snapshot: result,
                scope,
            },
            &["Added.".into()]
        )
        .unwrap_err()
        .contains("does not match")
    );
}

#[test]
fn empty_placeholder_reuses_only_first_id_and_scene_break_anchors_at_zero() {
    let empty = snapshot(vec![paragraph("placeholder", "")]);
    let result = snapshot(vec![
        paragraph("placeholder", "First."),
        paragraph("new", "Second."),
    ]);
    validate_append(
        &request(empty, result),
        &["First.".into(), "Second.".into()],
    )
    .unwrap();

    let duplicate = snapshot(vec![
        paragraph("placeholder", "First."),
        paragraph("placeholder", "Second."),
    ]);
    let empty_source = snapshot(vec![paragraph("placeholder", "")]);
    let error = validate_append(
        &request(empty_source, duplicate),
        &["First.".into(), "Second.".into()],
    )
    .unwrap_err();
    assert!(error.contains("generated block ID"), "{error}");

    let scene = snapshot(vec![paragraph("p", "Ending."), scene_break("scene")]);
    let scope = capture_append_scope(&scene).unwrap();
    assert_eq!(scope.end.as_ref().unwrap().utf16_offset, 0);
    let result = append_paragraphs(scene.clone(), &[("new", "After scene.")]);
    validate_append(
        &ScopeValidationRequest {
            source_snapshot: scene,
            result_snapshot: result,
            scope,
        },
        &["After scene.".into()],
    )
    .unwrap();
}

#[test]
fn append_requires_nonempty_exact_candidate_and_rejects_extra_result_blocks() {
    let source = snapshot(vec![paragraph("p", "Ending.")]);
    let scope = capture_append_scope(&source).unwrap();
    let same = ScopeValidationRequest {
        source_snapshot: source.clone(),
        result_snapshot: source.clone(),
        scope: scope.clone(),
    };
    assert!(validate_scope(&same).is_ok());
    assert!(validate_append(&same, &[]).is_err());

    let result = append_paragraphs(source.clone(), &[("new", "One."), ("extra", "Two.")]);
    assert!(
        validate_append(
            &ScopeValidationRequest {
                source_snapshot: source,
                result_snapshot: result,
                scope,
            },
            &["One.".into()]
        )
        .unwrap_err()
        .contains("paragraph count")
    );
}

#[test]
fn shared_continuation_fixture_matches_rust_append_validator() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/continuation.json");
    let fixture: Value =
        serde_json::from_str(&fs::read_to_string(path).expect("read continuation fixture"))
            .expect("parse continuation fixture");
    for case in fixture["cases"].as_array().expect("fixture cases") {
        assert_eq!(case["valid"], true, "fixture includes an invalid base case");
        let source = case["sourceSnapshot"].clone();
        let result = case["resultSnapshot"].clone();
        let paragraphs = case["paragraphs"]
            .as_array()
            .expect("fixture paragraphs")
            .iter()
            .map(|paragraph| paragraph.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let request = request(source, result);
        validate_append(&request, &paragraphs)
            .unwrap_or_else(|error| panic!("fixture {} failed: {error}", case["name"]));
    }
}
