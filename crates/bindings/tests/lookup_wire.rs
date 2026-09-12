use serde_json::{Value, json};
use wns_context::lookup::{LookupPacketInput, LookupRead};

#[test]
fn lookup_option_fields_are_omitted_and_reject_explicit_null() {
    for (mut value, field, supplied) in [
        (
            json!({ "kind": "read", "id": "r", "handle": "h" }),
            "blockIds",
            json!(["b"]),
        ),
        (
            json!({ "kind": "knowledgeHistory", "id": "r", "characterId": "c", "limit": 5 }),
            "topicId",
            json!("topic"),
        ),
    ] {
        let read: LookupRead =
            serde_json::from_value(value.clone()).expect("omitted field accepted");
        assert_eq!(serde_json::to_value(read).unwrap(), value);
        value[field] = supplied;
        let read: LookupRead =
            serde_json::from_value(value.clone()).expect("supplied value accepted");
        assert_eq!(serde_json::to_value(read).unwrap(), value);
        value[field] = Value::Null;
        assert!(
            serde_json::from_value::<LookupRead>(value).is_err(),
            "{field} cannot be null"
        );
    }

    let mut value = json!({
        "allowance": { "maxAdditionalInvocations": 2, "totalInputBytes": "1024", "totalOutputBytes": "1024" },
        "completedInvocations": 0,
        "exchanges": [],
    });
    let packet: LookupPacketInput = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(packet).unwrap(), value);
    value["reviewedMemory"] = json!("reviewed-memory.v1");
    let packet: LookupPacketInput = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(packet).unwrap(), value);
    value["reviewedMemory"] = Value::Null;
    assert!(serde_json::from_value::<LookupPacketInput>(value).is_err());
}

#[test]
fn generated_lookup_fields_match_the_serde_omission_contract() {
    let files = wns_bindings::render_all().unwrap();
    let (_, context) = files.iter().find(|(name, _)| name == "context.ts").unwrap();
    let read = context
        .lines()
        .find(|line| line.starts_with("export type LookupRead ="))
        .unwrap();
    let packet = context
        .lines()
        .find(|line| line.starts_with("export type LookupPacketInput ="))
        .unwrap();
    assert!(read.contains("blockIds?: string[] }"), "{read}");
    assert!(read.contains("topicId?: string;"), "{read}");
    assert!(packet.contains("reviewedMemory?: string }"), "{packet}");
}
