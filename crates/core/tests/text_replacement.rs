use serde_json::{Value, json};
use webnovel_core::documents::{
    Endpoint, ScopeGrant, ScopeKind, ScopeValidationRequest, capture_scope, validate_scope,
    validate_text_replacement,
};

fn body(middle: Vec<Value>) -> Value {
    let mut content = vec![json!({"type":"text","text":"prefix "})];
    content.extend(middle);
    content.push(json!({"type":"text","text":" suffix"}));
    json!({"schemaVersion":1,"body":{"type":"doc","content":[
        {"type":"paragraph","attrs":{"id":"p1"},"content":content},
        {"type":"paragraph","attrs":{"id":"p2"},"content":[{"type":"text","text":"A protected neighbour."}]}
    ]}})
}

fn text(value: &str, mark: Option<&str>) -> Value {
    let mut node = json!({"type":"text","text":value});
    if let Some(mark) = mark {
        node["marks"] = json!([{"type":mark}]);
    }
    node
}

fn request(source: Value, result: Value) -> ScopeValidationRequest {
    let scope = capture_scope(
        &source,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 7,
            }),
            end: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 15,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap();
    ScopeValidationRequest {
        source_snapshot: source,
        result_snapshot: result,
        scope,
    }
}

#[test]
fn text_fragment_matches_the_existing_real_prosemirror_golden_preparations() {
    // These exact result documents are also produced by prepareReplacement in
    // the frontend's scope.test.ts, including the left-ID paragraph merge.
    let golden: Value = serde_json::from_str(contracts::W1_SCOPE_GOLDEN)
    .unwrap();
    for (name, replacement) in [
        ("valid-inline-with-marks-and-link", "changed"),
        ("repeated-quote-targets-second-occurrence", "changed"),
        ("valid-cross-paragraph-merge", "left right"),
    ] {
        let case = golden["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let mut request: ScopeValidationRequest =
            serde_json::from_value(case["request"].clone()).unwrap();
        // The original merge fixture grants whole blocks; the existing JS test
        // selects their text endpoints and prepares this same literal result.
        request.scope.kind = ScopeKind::Passage;
        request.scope = capture_scope(&request.source_snapshot, request.scope).unwrap();
        let result = validate_text_replacement(&request, replacement);
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn prepared_result_must_match_the_actual_candidate_text_and_uniform_marks() {
    let source = body(vec![text("selected", Some("bold"))]);
    let good = request(source.clone(), body(vec![text("changed", Some("bold"))]));
    assert!(validate_text_replacement(&good, "changed").is_ok());
    assert!(validate_scope(&good).is_ok());
    assert!(
        validate_text_replacement(&good, "different candidate")
            .unwrap_err()
            .contains("exact replacement")
    );
    let changed_marks = request(source, body(vec![text("changed", Some("italic"))]));
    assert!(validate_scope(&changed_marks).is_ok());
    assert!(
        validate_text_replacement(&changed_marks, "changed")
            .unwrap_err()
            .contains("inherited formatting")
    );
    let mut neighbour = good;
    neighbour.result_snapshot["body"]["content"][1]["content"][0]["text"] =
        json!("Unrequested change.");
    assert!(
        validate_text_replacement(&neighbour, "changed")
            .unwrap_err()
            .contains("after the granted scope")
    );
}

#[test]
fn mixed_marks_use_plain_text_and_unicode_and_deletion_keep_the_protected_gaps() {
    let source = body(vec![
        text("sel", Some("bold")),
        text("ected", Some("italic")),
    ]);
    let unicode = "Éowyn 👩‍🚀";
    let plain = request(source.clone(), body(vec![text(unicode, None)]));
    assert!(validate_text_replacement(&plain, unicode).is_ok());
    let wrong = request(source.clone(), body(vec![text(unicode, Some("bold"))]));
    assert!(validate_text_replacement(&wrong, unicode).is_err());
    let deletion = request(source, body(vec![]));
    assert!(validate_text_replacement(&deletion, "").is_ok());
}

#[test]
fn text_fragment_cannot_grant_structure_or_accept_newlines_and_oversized_utf16() {
    let good = request(
        body(vec![text("selected", None)]),
        body(vec![text("changed", None)]),
    );
    for value in [
        "new\nparagraph".to_owned(),
        "new\rline".to_owned(),
        "🗝".repeat(50_001),
    ] {
        assert!(
            validate_text_replacement(&good, &value)
                .unwrap_err()
                .contains("one line")
        );
    }
    let mut whole = good;
    whole.scope.kind = ScopeKind::WholeDocument;
    assert!(
        validate_text_replacement(&whole, "changed")
            .unwrap_err()
            .contains("passage scope")
    );
}
