use serde_json::{Value, json};
use webnovel_core::context::lookup::{
    LOOKUP_SCHEMA_VERSION, LookupAllowance, LookupEnvelope, LookupErrorCode, LookupPacketInput,
    LookupRead, LookupReadResult, MemoryEntityKind, REVIEWED_MEMORY_CAPABILITY,
    parse_lookup_envelope,
};

fn envelope(value: Value) -> Result<LookupEnvelope, webnovel_core::context::lookup::LookupError> {
    parse_lookup_envelope(&serde_json::to_string(&value).expect("test JSON serializes"))
}

fn packet_input(reviewed_memory: Option<&str>) -> LookupPacketInput {
    LookupPacketInput {
        allowance: LookupAllowance::default(),
        completed_invocations: 0,
        exchanges: Vec::new(),
        source_projection: None,
        reviewed_memory: reviewed_memory.map(str::to_owned),
    }
}

#[test]
fn memory_reads_use_camel_case_wire_fields_and_page_bounds() {
    let parsed = envelope(json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": [{
            "kind": "findEntities",
            "id": "entities",
            "entityKind": "character",
            "query": "Mei",
            "offset": 3,
            "limit": 2
        }]
    }))
    .expect("memory read parses");
    let LookupEnvelope::NeedsContext { reads, .. } = parsed else {
        panic!("expected needsContext")
    };
    assert!(matches!(
        reads[0],
        LookupRead::FindEntities { offset: 3, .. }
    ));

    for (field, value) in [
        ("offset", json!(100_001)),
        ("limit", json!(0)),
        ("limit", json!(21)),
    ] {
        let mut read = json!({
            "kind": "needsContext",
            "schemaVersion": LOOKUP_SCHEMA_VERSION,
            "reads": [{
                "kind": "findEntities",
                "id": "entities",
                "entityKind": "character",
                "query": "Mei",
                "offset": 0,
                "limit": 2
            }]
        });
        read["reads"][0][field] = value;
        let error = envelope(read).expect_err("invalid page bound");
        assert_eq!(error.code, LookupErrorCode::InvalidField);
    }
}

#[test]
fn explicit_null_topic_id_is_rejected_and_missing_topic_is_allowed() {
    let error = envelope(json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": [{
            "kind": "knowledgeHistory",
            "id": "history",
            "characterId": "character-1",
            "topicId": null,
            "limit": 1
        }]
    }))
    .expect_err("null topicId is ambiguous");
    assert_eq!(error.code, LookupErrorCode::InvalidEnvelope);

    envelope(json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": [{
            "kind": "knowledgeHistory",
            "id": "history",
            "characterId": "character-1",
            "limit": 1
        }]
    }))
    .expect("missing topicId means all topics");
}

#[test]
fn memory_result_serializes_page_metadata_in_camel_case() {
    let result = LookupReadResult::FindEntities {
        entity_kind: MemoryEntityKind::Character,
        query: "Mei".into(),
        entries: Vec::new(),
        offset: 0,
        total_matches: 4,
        next_offset: Some(2),
        incomplete: true,
    };
    let value = serde_json::to_value(result).expect("result serializes");
    assert_eq!(value["kind"], "findEntities");
    assert_eq!(value["entityKind"], "character");
    assert_eq!(value["totalMatches"], 4);
    assert_eq!(value["nextOffset"], 2);
    assert!(value.get("total_matches").is_none());
}

#[test]
fn reviewed_memory_capability_is_required_and_exact() {
    let read = LookupRead::FindEntities {
        id: "entities".into(),
        entity_kind: MemoryEntityKind::Character,
        query: "Mei".into(),
        offset: 0,
        limit: 1,
    };
    let legacy = packet_input(None);
    legacy
        .validate_capability()
        .expect("legacy capability absent is valid");
    assert_eq!(
        legacy
            .authorize_read(&read)
            .expect_err("memory read is gated")
            .code,
        LookupErrorCode::InvalidCapability
    );
    let wrong = packet_input(Some("reviewed-memory.v0"));
    assert_eq!(
        wrong
            .validate_capability()
            .expect_err("wrong capability")
            .code,
        LookupErrorCode::InvalidCapability
    );
    let enabled = packet_input(Some(REVIEWED_MEMORY_CAPABILITY));
    enabled
        .authorize_read(&read)
        .expect("exact capability authorizes memory read");
    let legacy_json = serde_json::to_value(legacy).expect("legacy serializes");
    assert!(legacy_json.get("reviewedMemory").is_none());
}
