use super::*;
use sha2::{Digest, Sha256};
use webnovel_core::context::lookup::{
    LookupExchange, LookupRead, LookupReadResult, LookupSourceProjection,
};
use webnovel_core::context::memory_lookup::execute_memory_lookup;
use webnovel_core::context::{reviewed_evidence, reviewed_knowledge, reviewed_promises};

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn memory_request() -> PacketRequest {
    let target_body = body(&[("target", "The confrontation begins.")]);
    let older_body = body(&[(
        "old",
        "Mei kept the key and promised to return. She believed the gate was locked.",
    )]);
    let target = source("target", "target-doc", &target_body);
    let older = source("older", "older-doc", &older_body);
    let mut req = request(
        frozen(
            vec![target.clone(), older.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &target_body), read(&older, &older_body)],
    );
    req.invocation_ordinal = "0".into();
    req.response_contract = Some(LOOKUP_RESPONSE_CONTRACT.into());
    req.lookup = Some(LookupPacketInput {
        allowance: LookupAllowance::default(),
        completed_invocations: 0,
        exchanges: vec![],
        source_projection: None,
        reviewed_memory: Some("reviewed-memory.v1".into()),
    });
    let quote = req.sources[1].passages[0].text.clone();
    let anchor = json!({"blockId":"old", "fromUtf16":0, "toUtf16":quote.encode_utf16().count(),
        "quote":quote, "quoteHash":sha256(&quote)});
    let mei = json!({"id":"mei", "label":"Mei"});
    let base = json!({"projectId":PROJECT,"operationNamespace":"namespace", "bundleId":"bundle",
        "recordsHash":"", "sourceHandle":"older", "source":older.source});
    let mut knowledge = base.clone();
    knowledge["records"] = json!([
        {"id":"belief","character":mei,"topic":{"id":"gate","label":"Gate"},"attitude":"believes",
         "statement":"The gate is locked.","timing":"atPassage","audience":"reader","evidence":anchor},
        {"id":"doubt","character":mei,"topic":{"id":"gate","label":"Gate"},"attitude":"suspects",
         "statement":"The gate might be locked.","timing":"unknown","audience":"authorRoom","evidence":anchor}
    ]);
    let mut set: reviewed_knowledge::ReviewedKnowledgeSet =
        serde_json::from_value(knowledge).unwrap();
    set.records_hash = reviewed_knowledge::records_hash(&set.records).unwrap();
    req.frozen.reviewed_knowledge.push(set);
    let mut possession = base.clone();
    possession["records"] = json!([{"id":"holding","object":{"id":"key","label":"Brass key"},
        "holder":mei,"timing":"atPassage","audience":"reader","evidence":anchor}]);
    let mut set: reviewed_evidence::ReviewedEvidenceSet =
        serde_json::from_value(possession).unwrap();
    set.records_hash = reviewed_evidence::records_hash(&set.records).unwrap();
    req.frozen.reviewed_evidence.push(set);
    let mut promise = base;
    promise["records"] = json!([{"id":"vow","promise":{"id":"return","label":"Return home"},
        "phase":"setup","note":"Return to her brother.","timing":"atPassage","audience":"reader","evidence":anchor}]);
    let mut set: reviewed_promises::ReviewedPromiseSet = serde_json::from_value(promise).unwrap();
    set.records_hash = reviewed_promises::records_hash(&set.records).unwrap();
    req.frozen.reviewed_promises.push(set);
    req
}

fn expand(req: &mut PacketRequest, queries: Vec<Value>) {
    let exchanges: Vec<_> = queries
        .into_iter()
        .map(|query| {
            let request: LookupRead = serde_json::from_value(query).unwrap();
            let result = execute_memory_lookup(&req.frozen, &request)
                .unwrap()
                .unwrap();
            LookupExchange { request, result }
        })
        .collect();
    let source_projection =
        Some(LookupSourceProjection::from_exchanges(&req.frozen, &exchanges).unwrap());
    req.invocation_ordinal = "1".into();
    let lookup = req.lookup.as_mut().unwrap();
    lookup.completed_invocations = 1;
    lookup.exchanges = exchanges;
    lookup.source_projection = source_projection;
}

fn knowledge_query() -> Value {
    json!({"id":"knowledge","kind":"knowledgeHistory","characterId":"mei","topicId":"gate","limit":1})
}

#[test]
fn all_reviewed_memory_operations_keep_exact_evidence_and_partial_history_metadata() {
    let mut req = memory_request();
    expand(
        &mut req,
        vec![
            json!({"id":"entities","kind":"findEntities","entityKind":"character","query":"Mei","limit":6}),
            knowledge_query(),
            json!({"id":"promises","kind":"promiseHistory","promiseId":"return","limit":6}),
            json!({"id":"objects","kind":"possessionHistory","objectId":"key","limit":6}),
        ],
    );
    let packet = compile_packet(&req).unwrap();
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    let knowledge = &envelope["lookup"]["exchanges"][1]["result"];
    assert_eq!(knowledge["totalObservations"], 2);
    assert_eq!(knowledge["nextOffset"], 1);
    assert_eq!(
        knowledge["history"]["observations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        knowledge["history"]["observations"][0]["attitude"],
        "believes"
    );
    assert_eq!(
        knowledge["history"]["observations"][0]["source"],
        serde_json::to_value(&req.sources[1].descriptor.source).unwrap()
    );
    assert_eq!(knowledge["history"]["incomplete"], true);
    assert!(
        knowledge["history"]["uncertainty"]
            .as_array()
            .unwrap()
            .contains(&json!("multipleRecordedAttitudes"))
    );
    assert_eq!(
        envelope["lookup"]["sourceProjection"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(packet.receipt.lookup, req.lookup);
}

#[test]
fn legacy_packets_keep_their_original_bytes_and_do_not_gain_memory_tools() {
    let mut legacy = memory_request();
    legacy.lookup.as_mut().unwrap().reviewed_memory = None;
    let before = compile_packet(&legacy).unwrap();
    // Recorded from the pre-extension system instruction at 1747229.
    assert_eq!(
        sha256(&before.messages[0].content),
        "94871766bfaa86bcb2a53e3509f10c9af166911262d57065fde2afd6454667c1"
    );
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert!(encoded["lookup"].get("reviewedMemory").is_none());
    let reconstructed: PacketRequest = serde_json::from_value(encoded).unwrap();
    assert_eq!(compile_packet(&reconstructed).unwrap(), before);
    assert!(!before.messages[0].content.contains("findEntities"));
    let mut enabled = legacy.clone();
    enabled.lookup.as_mut().unwrap().reviewed_memory = Some("reviewed-memory.v1".into());
    let after = compile_packet(&enabled).unwrap();
    assert!(
        after.messages[0]
            .content
            .starts_with(&before.messages[0].content)
    );
    assert!(after.messages[0].content.contains("findEntities"));
    expand(&mut legacy, vec![knowledge_query()]);
    assert!(compile_packet(&legacy).is_err());
    enabled.lookup.as_mut().unwrap().reviewed_memory = Some("future-capability".into());
    assert!(compile_packet(&enabled).is_err());
}

#[test]
fn lookup_results_are_recomputed_even_when_serialization_and_hashes_are_valid() {
    let mut req = memory_request();
    expand(&mut req, vec![knowledge_query()]);
    let valid = req.lookup.as_ref().unwrap().exchanges[0].result.clone();
    for (field, value) in [
        ("nextOffset", json!(null)),
        ("totalObservations", json!(99)),
    ] {
        let mut altered = serde_json::to_value(&valid).unwrap();
        altered[field] = value;
        req.lookup.as_mut().unwrap().exchanges[0].result = serde_json::from_value(altered).unwrap();
        assert!(compile_packet(&req).is_err(), "accepted altered {field}");
    }
    let mut altered = serde_json::to_value(&valid).unwrap();
    altered["history"]["observations"][0]["statement"] = json!("The gate is definitely open.");
    req.lookup.as_mut().unwrap().exchanges[0].result = serde_json::from_value(altered).unwrap();
    assert!(compile_packet(&req).is_err());
    req.lookup.as_mut().unwrap().exchanges[0].result = LookupReadResult::Unavailable {
        code: "NoRecords".into(),
        detail: "No reviewed records were found.".into(),
    };
    assert!(compile_packet(&req).is_err());
}

#[test]
fn an_unreturned_record_is_validated_before_the_budget_branch() {
    let mut req = memory_request();
    expand(&mut req, vec![knowledge_query()]);
    let set = &mut req.frozen.reviewed_knowledge[0];
    set.records[1].evidence.quote = "A fabricated observation outside the returned page.".into();
    set.records[1].evidence.quote_hash = sha256(&set.records[1].evidence.quote);
    set.records_hash = reviewed_knowledge::records_hash(&set.records).unwrap();
    req.budget = MockContextBudget::new("201", "100", "100");
    assert!(matches!(
        compile_packet(&req),
        Err(PacketError::SourceBinding { .. })
    ));
}

#[test]
fn required_lookup_evidence_is_never_dropped_to_fit_the_packet() {
    let mut initial = memory_request();
    let mut expanded = initial.clone();
    expand(&mut expanded, vec![knowledge_query()]);
    initial.budget = MockContextBudget::new("201", "100", "100");
    let Err(PacketError::Budget(before)) = compile_packet(&initial) else {
        panic!("A one-byte input allowance must reject the initial packet");
    };
    let input_bytes = before.required_input_tokens.parse::<usize>().unwrap();
    initial.budget = MockContextBudget::new((input_bytes + 250).to_string(), "100", "100");
    assert!(compile_packet(&initial).is_ok());
    expanded.budget = initial.budget;
    assert!(matches!(
        compile_packet(&expanded),
        Err(PacketError::Budget(_))
    ));
}
