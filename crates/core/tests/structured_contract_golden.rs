use serde::Deserialize;
use serde_json::{Value, json};
use webnovel_core::documents::{
    ScopeGrant, ScopeValidationRequest, TypedReplacementBlock, capture_scope,
    validate_structured_replacement,
};
use webnovel_core::validate_snapshot_json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureFile {
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    name: String,
    scope: ScopeGrant,
    expected_quote: String,
    replacement_blocks: Vec<TypedReplacementBlock>,
    replacement_ids: Vec<String>,
    source_snapshot: Value,
    result_snapshot: Value,
}

#[test]
fn shared_structured_fixtures_match_rust_scope_and_snapshot_validation() {
    let path = contracts::STRUCTURED_PROPOSALS_GOLDEN;
    let fixture: FixtureFile =
        serde_json::from_str(path)
            .expect("parse structured proposal fixture");

    for case in fixture.cases {
        let captured = capture_scope(&case.source_snapshot, case.scope)
            .unwrap_or_else(|error| panic!("fixture {} scope capture failed: {error}", case.name));
        assert_eq!(
            captured.quote, case.expected_quote,
            "fixture {} quote",
            case.name
        );
        assert!(
            !captured.source_hash.is_empty(),
            "fixture {} source hash",
            case.name
        );
        assert!(
            !captured.quote_hash.is_empty(),
            "fixture {} structural quote hash",
            case.name
        );
        assert_eq!(
            case.replacement_ids.len(),
            case.replacement_blocks.len(),
            "fixture {} replacement identity count",
            case.name
        );
        let source_blocks = case.source_snapshot["body"]["content"]
            .as_array()
            .expect("source blocks");
        let result_blocks = case.result_snapshot["body"]["content"]
            .as_array()
            .expect("result blocks");
        let (first, last) = if captured.kind == webnovel_core::documents::ScopeKind::WholeDocument {
            (0, source_blocks.len() - 1)
        } else {
            let start = captured.start.as_ref().expect("structured scope start");
            let end = captured.end.as_ref().expect("structured scope end");
            (
                source_blocks
                    .iter()
                    .position(|block| block["attrs"]["id"] == start.block_id)
                    .expect("structured scope start block"),
                source_blocks
                    .iter()
                    .position(|block| block["attrs"]["id"] == end.block_id)
                    .expect("structured scope end block"),
            )
        };
        let suffix_count = source_blocks.len() - last - 1;
        let result_end = result_blocks.len() - suffix_count;
        let inserted_ids: Vec<String> = result_blocks[first..result_end]
            .iter()
            .map(|block| {
                block["attrs"]["id"]
                    .as_str()
                    .expect("replacement block ID")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            inserted_ids, case.replacement_ids,
            "fixture {} replacement IDs",
            case.name
        );

        let receipt = validate_structured_replacement(
            &ScopeValidationRequest {
                source_snapshot: case.source_snapshot.clone(),
                result_snapshot: case.result_snapshot.clone(),
                scope: captured.clone(),
            },
            &case.replacement_blocks,
        )
        .unwrap_or_else(|error| {
            panic!(
                "fixture {} structured validation failed: {error}",
                case.name
            )
        });
        assert!(receipt.accepted, "fixture {} was not accepted", case.name);
        assert_eq!(
            receipt.quote, case.expected_quote,
            "fixture {} receipt quote",
            case.name
        );

        let canonical_result = validate_snapshot_json(
            &serde_json::to_string(&case.result_snapshot).expect("encode result snapshot"),
        )
        .expect("canonical result snapshot");
        assert_eq!(
            receipt.result_hash, canonical_result.hash,
            "fixture {} result hash",
            case.name
        );
        assert_eq!(
            canonical_result.snapshot, case.result_snapshot,
            "fixture {} is not canonical",
            case.name
        );
    }
}

#[test]
fn whole_document_scope_rejects_start_or_end_endpoints() {
    let source = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type": "paragraph", "attrs": {"id": "source"}, "content": [{"type": "text", "text": "Old."}]}
        ]}
    });
    let result = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type": "paragraph", "attrs": {"id": "fresh"}, "content": [{"type": "text", "text": "New."}]}
        ]}
    });
    let captured = capture_scope(
        &source,
        ScopeGrant {
            kind: webnovel_core::documents::ScopeKind::WholeDocument,
            start: None,
            end: None,
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .expect("baseline whole-document scope");
    let block = TypedReplacementBlock::Paragraph {
        content: vec![webnovel_core::documents::TypedReplacementInline::Text {
            text: "New.".into(),
            marks: Vec::new(),
        }],
    };
    for (label, start_endpoint) in [("start", true), ("end", false)] {
        let mut invalid = captured.clone();
        if start_endpoint {
            invalid.start = Some(webnovel_core::documents::Endpoint {
                block_id: "source".into(),
                utf16_offset: 0,
            });
        } else {
            invalid.end = Some(webnovel_core::documents::Endpoint {
                block_id: "source".into(),
                utf16_offset: 4,
            });
        }
        assert!(
            capture_scope(&source, invalid.clone()).is_err(),
            "capture accepted {label} endpoint"
        );
        assert!(
            validate_structured_replacement(
                &ScopeValidationRequest {
                    source_snapshot: source.clone(),
                    result_snapshot: result.clone(),
                    scope: invalid,
                },
                std::slice::from_ref(&block),
            )
            .is_err(),
            "structured validation accepted {label} endpoint"
        );
    }
}
