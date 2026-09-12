use serde_json::{Value, json};
use webnovel_core::context::lookup::{
    LOOKUP_SCHEMA_VERSION, LookupAllowance, LookupEnvelope, LookupErrorCode, LookupPacketInput,
    LookupRead, LookupReadResult, MAX_LOOKUP_ENVELOPE_BYTES, MAX_LOOKUP_TOTAL_INPUT_BYTES,
    MAX_LOOKUP_TOTAL_OUTPUT_BYTES, parse_lookup_envelope, validate_lookup_read,
};

fn envelope(value: Value) -> Result<LookupEnvelope, webnovel_core::context::lookup::LookupError> {
    parse_lookup_envelope(&serde_json::to_string(&value).expect("test JSON serializes"))
}

fn error_code(value: Value) -> LookupErrorCode {
    envelope(value)
        .expect_err("test input should be rejected")
        .code
}

fn valid_reads() -> Value {
    json!([
        {
            "kind": "search",
            "id": "promise-search",
            "query": "the jade pendant",
            "mode": "literal",
            "limit": 5
        },
        {
            "kind": "read",
            "id": "chapter-014",
            "handle": "chapter:014",
            "blockIds": ["p-01", "p-02"]
        }
    ])
}

#[test]
fn accepts_bounded_search_read_and_discussion_envelopes() {
    let needs_context = envelope(json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": valid_reads()
    }))
    .expect("valid lookup request");
    assert!(matches!(needs_context, LookupEnvelope::NeedsContext { .. }));

    for mode in ["literal", "lexical", "exactAlias"] {
        let parsed = envelope(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{
                "kind": "search",
                "id": "search-1",
                "query": "Mei",
                "mode": mode,
                "limit": 1
            }]
        }))
        .expect("all declared search modes are accepted");
        assert!(matches!(parsed, LookupEnvelope::NeedsContext { .. }));
    }

    let discussion = envelope(json!({
        "kind": "discussion",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "text": "The requested passage is already in the supplied context.\nI can revise it next."
    }))
    .expect("valid final response");
    assert!(matches!(discussion, LookupEnvelope::Discussion { .. }));

    let read: LookupRead = serde_json::from_value(json!({
        "kind": "read",
        "id": "chapter-015",
        "handle": "chapter:015"
    }))
    .expect("request descriptor decodes");
    assert_eq!(read.id(), "chapter-015");
    validate_lookup_read(&read).expect("individual request validates");
}

#[test]
fn rejects_unknown_fields_wrong_schema_and_provider_commands() {
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": valid_reads(),
            "tool": "read_file"
        })),
        LookupErrorCode::InvalidEnvelope
    );
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{
                "kind": "read",
                "id": "read-1",
                "handle": "chapter:001",
                "path": "C:/secrets"
            }]
        })),
        LookupErrorCode::InvalidEnvelope
    );
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": "story-lookup.v0",
            "reads": valid_reads()
        })),
        LookupErrorCode::InvalidEnvelope
    );
    assert_eq!(
        error_code(json!({
            "kind": "command",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "command": "shell"
        })),
        LookupErrorCode::InvalidEnvelope
    );
}

#[test]
fn rejects_duplicate_keys_at_any_json_object_level() {
    let duplicate_top = format!(
        r#"{{"kind":"discussion","schemaVersion":"{LOOKUP_SCHEMA_VERSION}","schemaVersion":"{LOOKUP_SCHEMA_VERSION}","text":"hello"}}"#
    );
    assert_eq!(
        parse_lookup_envelope(&duplicate_top)
            .expect_err("duplicate top-level key")
            .code,
        LookupErrorCode::DuplicateKey
    );

    let duplicate_nested = format!(
        r#"{{"kind":"needsContext","schemaVersion":"{LOOKUP_SCHEMA_VERSION}","reads":[{{"kind":"search","id":"s","query":"Mei","query":"secret","mode":"literal","limit":1}}]}}"#
    );
    assert_eq!(
        parse_lookup_envelope(&duplicate_nested)
            .expect_err("duplicate nested key")
            .code,
        LookupErrorCode::DuplicateKey
    );
}

#[test]
fn enforces_read_counts_ids_queries_limits_handles_and_blocks() {
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": []
        })),
        LookupErrorCode::InvalidField
    );

    let nine_reads = (0..9)
        .map(|index| {
            json!({
                "kind": "search",
                "id": format!("s-{index}"),
                "query": "needle",
                "mode": "lexical",
                "limit": 1
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": nine_reads
        })),
        LookupErrorCode::InvalidField
    );

    for reads in [
        json!([
            {"kind":"search","id":"same","query":"one","mode":"literal","limit":1},
            {"kind":"read","id":"same","handle":"chapter:1"}
        ]),
        json!([{"kind":"search","id":"ümlaut","query":"one","mode":"literal","limit":1}]),
        json!([{"kind":"search","id":"s","query":"   ","mode":"literal","limit":1}]),
        json!([{"kind":"search","id":"s","query":"line\nfeed","mode":"literal","limit":1}]),
        json!([{"kind":"search","id":"s","query":"one","mode":"literal","limit":0}]),
        json!([{"kind":"search","id":"s","query":"one","mode":"literal","limit":21}]),
        json!([{"kind":"read","id":"r","handle":"   "}]),
        json!([{"kind":"read","id":"r","handle":"chapter\t1"}]),
        json!([{"kind":"read","id":"r","handle":"chapter:1","blockIds":["b","b"]}]),
        json!([{"kind":"read","id":"r","handle":"chapter-1","blockIds":[]}]),
        json!([{"kind":"read","id":"r","handle":"chapter 1"}]),
    ] {
        assert_eq!(
            error_code(json!({
                "kind": "needsContext",
                "schemaVersion": LOOKUP_SCHEMA_VERSION,
                "reads": reads
            })),
            LookupErrorCode::InvalidField
        );
    }

    // A present null is a malformed wire shape, distinct from a valid array
    // whose contents fail the typed field validation above.
    assert_eq!(
        error_code(json!({
            "kind": "needsContext", "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{"kind":"read","id":"r","handle":"chapter-1","blockIds":null}]
        })),
        LookupErrorCode::InvalidEnvelope
    );

    let too_many_blocks = (0..33)
        .map(|index| format!("b-{index}"))
        .collect::<Vec<_>>();
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{
                "kind": "read",
                "id": "r",
                "handle": "chapter:1",
                "blockIds": too_many_blocks
            }]
        })),
        LookupErrorCode::InvalidField
    );

    let long_query = "q".repeat(513);
    assert_eq!(
        error_code(json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{"kind":"search","id":"s","query":long_query,"mode":"literal","limit":1}]
        })),
        LookupErrorCode::InvalidField
    );
}

#[test]
fn discussion_text_and_raw_envelope_have_byte_caps() {
    let too_long_text = "x".repeat(65 * 1024);
    assert_eq!(
        error_code(json!({
            "kind": "discussion",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "text": too_long_text
        })),
        LookupErrorCode::InvalidEnvelope
    );
    assert_eq!(
        error_code(json!({
            "kind": "discussion",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "text": "\u{0007}"
        })),
        LookupErrorCode::InvalidField
    );
    envelope(json!({
        "kind": "discussion",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "text": "Paragraph one.\n\tParagraph two."
    }))
    .expect("normal chat whitespace is allowed");

    let oversized_raw = format!(
        r#"{{"kind":"discussion","schemaVersion":"{LOOKUP_SCHEMA_VERSION}","text":"{}"}}"#,
        "x".repeat(MAX_LOOKUP_ENVELOPE_BYTES)
    );
    assert!(oversized_raw.len() > MAX_LOOKUP_ENVELOPE_BYTES);
    assert_eq!(
        parse_lookup_envelope(&oversized_raw)
            .expect_err("raw envelope byte cap")
            .code,
        LookupErrorCode::InvalidEnvelope
    );
}

#[test]
fn allowance_requires_canonical_positive_decimal_caps() {
    assert_eq!(
        LookupAllowance::default(),
        LookupAllowance {
            max_additional_invocations: 2,
            total_input_bytes: "73728".into(),
            total_output_bytes: "196608".into()
        }
    );
    LookupAllowance::new(
        0,
        MAX_LOOKUP_TOTAL_INPUT_BYTES.to_string(),
        MAX_LOOKUP_TOTAL_OUTPUT_BYTES.to_string(),
    )
    .expect("zero additional invocations is valid");

    for (invocations, input, output) in [
        (3, "1", "1"),
        (2, "0", "1"),
        (2, "01", "1"),
        (2, "1", "0"),
        (2, "1", "01"),
        (2, "73729", "1"),
        (2, "1", "196609"),
        (2, "1.0", "1"),
        (2, "+1", "1"),
        (2, "1", "999999999999999999999999999999999999999999999999"),
    ] {
        assert!(
            LookupAllowance::new(invocations, input, output).is_err(),
            "invalid allowance should be rejected: {invocations}, {input}, {output}"
        );
    }

    let unknown = serde_json::from_value::<LookupAllowance>(json!({
        "maxAdditionalInvocations": 1,
        "totalInputBytes": "1",
        "totalOutputBytes": "1",
        "tokens": 10
    }));
    assert!(unknown.is_err(), "allowance rejects unknown fields");
}

#[test]
fn packet_exchange_dtos_are_source_bound_and_strict() {
    let input: LookupPacketInput = serde_json::from_value(json!({
        "allowance": {
            "maxAdditionalInvocations": 1,
            "totalInputBytes": "100",
            "totalOutputBytes": "200"
        },
        "completedInvocations": 1,
        "exchanges": [{
            "request": {
                "kind": "read",
                "id": "chapter-1",
                "handle": "chapter:1",
                "blockIds": ["p-1"]
            },
            "result": {
                "kind": "unavailable",
                "code": "notFound",
                "detail": "The requested source was unavailable."
            }
        }]
    }))
    .expect("valid exchange DTO");
    assert_eq!(input.completed_invocations, 1);
    assert!(matches!(
        &input.exchanges[0].result,
        LookupReadResult::Unavailable { .. }
    ));

    let encoded = serde_json::to_value(&input).expect("DTO serializes");
    assert_eq!(encoded["exchanges"][0]["request"]["kind"], "read");
    assert_eq!(encoded["exchanges"][0]["result"]["kind"], "unavailable");

    let unknown = serde_json::from_value::<LookupPacketInput>(json!({
        "allowance": {
            "maxAdditionalInvocations": 1,
            "totalInputBytes": "100",
            "totalOutputBytes": "200"
        },
        "completedInvocations": 1,
        "exchanges": [],
        "shell": "echo unsafe"
    }));
    assert!(unknown.is_err(), "packet input rejects arbitrary fields");
}
