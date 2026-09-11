use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::lookup::{LOOKUP_SCHEMA_VERSION, LookupAllowance, LookupReadResult};
use webnovel_core::context::packet::{
    CompiledPacket, MockContextBudget, packet_input_hash, serialized_input,
};
use webnovel_core::projects::discussion_lookup::{LookupAdvance, LookupInvocationReport};
use webnovel_core::projects::discussions::{
    DiscussionBegin, FeedbackIntent, ProviderCleanup, ProviderOutcomeStatus, RunOwner,
    StartDiscussion,
};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, KnowledgeAttitude, KnowledgeRecord, PossessionTiming,
    StoryEntityRef,
};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("wns-memory-lookup-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create temporary root");
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize(value));
            }
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

fn logical_hash(value: &Value) -> String {
    let mut value = value.clone();
    if let Some(access) = value.get_mut("access").and_then(Value::as_object_mut) {
        access.remove("session");
        access.remove("writerLease");
    }
    hash(&serde_json::to_string(&canonicalize(value)).unwrap())
}

fn downgrade_lookup_packet(temp: &TempProject, packet_id: &str) {
    let database = temp.project().join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    let request_json: String = connection
        .query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [packet_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut request: Value = serde_json::from_str(&request_json).unwrap();
    request["lookup"]
        .as_object_mut()
        .unwrap()
        .remove("reviewedMemory");
    let request_json = serde_json::to_string(&request).unwrap();
    assert!(!request_json.contains("reviewedMemory"));
    assert!(!request_json.contains("reviewed-memory.v1"));
    let packet_json: String = connection
        .query_row(
            "SELECT packet_json FROM context_packets WHERE id=?",
            [packet_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut packet: Value = serde_json::from_str(&packet_json).unwrap();
    packet["receipt"]["lookup"]
        .as_object_mut()
        .unwrap()
        .remove("reviewedMemory");
    for message in packet["messages"].as_array_mut().unwrap() {
        let Some(content) = message["content"].as_str() else {
            continue;
        };
        if let Some(index) = content.find("\n\nThis packet also authorizes reviewed-memory.v1") {
            message["content"] = Value::String(content[..index].to_owned());
            continue;
        }
        let stripped = content
            .replace(",\"reviewedMemory\":\"reviewed-memory.v1\"", "")
            .replace("\"reviewedMemory\":\"reviewed-memory.v1\",", "");
        message["content"] = Value::String(stripped);
    }
    let mut compiled: CompiledPacket = serde_json::from_value(packet).unwrap();
    compiled.receipt.input_tokens = serialized_input(&compiled.messages, &compiled.options)
        .unwrap()
        .len()
        .to_string();
    let input_hash = packet_input_hash(&compiled.messages, &compiled.options).unwrap();
    compiled.receipt.input_hash = input_hash.clone();
    let packet_json = serde_json::to_string(&compiled).unwrap();
    assert!(!packet_json.contains("reviewedMemory"));
    assert!(!packet_json.contains("reviewed-memory.v1"));
    connection
        .execute_batch("DROP TRIGGER immutable_context_packet_update;")
        .unwrap();
    connection
        .execute(
            "UPDATE context_packets SET request_json=?, payload_hash=?, packet_json=?, packet_hash=?, input_hash=? WHERE id=?",
            (
                &request_json,
                logical_hash(&request),
                &packet_json,
                hash(&packet_json),
                &input_hash,
                packet_id,
            ),
        )
        .unwrap();
}

fn setup(
    label: &str,
) -> (
    TempProject,
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let temp = TempProject::new(label);
    let project = ProjectSession::create(temp.project(), "Memory lookup storage").unwrap();
    let access = project.documents().attach("memory-lookup-session".into()).unwrap();
    let text = "Mei knows the key opens the eastern gate.";
    let document = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap();
    let record = KnowledgeRecord {
        id: "knowledge-one".into(),
        character: StoryEntityRef {
            id: "mei".into(),
            label: "Mei".into(),
        },
        topic: StoryEntityRef {
            id: "key".into(),
            label: "The key".into(),
        },
        attitude: KnowledgeAttitude::Knows,
        statement: "The key opens the eastern gate.".into(),
        timing: PossessionTiming::AtPassage,
        audience: EvidenceAudience::Reader,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash(text),
        },
    };
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-knowledge".into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            knowledge: Some(vec![record]),
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-knowledge".into(),
            stage_id: stage.id,
        })
        .unwrap();
    (temp, project, access, document)
}

fn start(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
) -> webnovel_core::projects::discussions::DiscussionStart {
    project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "lookup-start".into(),
            expected: document.head.clone(),
            instruction: "Find what Mei knows about the key.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "4096", "1024"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .unwrap()
}

fn memory_response() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "kind": "needsContext",
        "reads": [{
            "id": "knowledge-one",
            "kind": "knowledgeHistory",
            "characterId": "mei",
            "topicId": "key",
            "limit": 1
        }]
    }))
    .unwrap()
}

fn search_response() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "kind": "needsContext",
        "reads": [{
            "id": "search-one",
            "kind": "search",
            "query": "eastern gate",
            "mode": "literal",
            "limit": 1
        }]
    }))
    .unwrap()
}

fn final_response() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "kind": "discussion",
        "text": "The frozen chapter records the eastern gate detail."
    }))
    .unwrap()
}

fn settle_response(
    project: &ProjectSession,
    owner: &RunOwner,
    packet: &webnovel_core::context::packet::CompiledPacket,
    ordinal: &str,
    assistant_text: String,
    event_id: &str,
) {
    let bytes = serialized_input(&packet.messages, &packet.options)
        .unwrap()
        .len();
    project
        .settle_lookup_invocation(LookupInvocationReport {
            owner: owner.clone(),
            ordinal: ordinal.into(),
            event_id: event_id.into(),
            assistant_text,
            binding: None,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: bytes.to_string(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
        })
        .unwrap();
}

fn settle_initial(
    project: &ProjectSession,
    owner: &RunOwner,
    packet: &webnovel_core::context::packet::CompiledPacket,
) {
    settle_response(
        project,
        owner,
        packet,
        "0",
        memory_response(),
        "memory-needs-context",
    );
}

#[test]
fn durable_memory_lookup_executes_from_the_frozen_reviewed_set() {
    let (_temp, project, access, document) = setup("happy");
    let started = start(&project, &access, &document);
    assert_eq!(
        started
            .packet
            .receipt
            .lookup
            .as_ref()
            .unwrap()
            .reviewed_memory
            .as_deref(),
        Some("reviewed-memory.v1")
    );
    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    let initial = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .unwrap();
    settle_initial(&project, &owner, &initial.packet);
    let child = match project
        .advance_lookup(
            webnovel_core::projects::discussion_lookup::LookupAdvanceRequest {
                owner,
                completed_ordinal: "0".into(),
            },
        )
        .unwrap()
    {
        LookupAdvance::Prepared { dispatch } => dispatch,
        LookupAdvance::Finished { .. } => panic!("memory read should prepare a child packet"),
    };
    let lookup = child.packet.receipt.lookup.unwrap();
    assert_eq!(
        lookup.reviewed_memory.as_deref(),
        Some("reviewed-memory.v1")
    );
    assert!(matches!(
        &lookup.exchanges[0].result,
        LookupReadResult::KnowledgeHistory { .. }
    ));
}

#[test]
fn rehashed_memory_receipt_is_rejected_by_durable_backup_validation() {
    let (temp, project, access, document) = setup("tampered");
    let started = start(&project, &access, &document);
    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    let initial = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .unwrap();
    settle_initial(&project, &owner, &initial.packet);
    project
        .advance_lookup(
            webnovel_core::projects::discussion_lookup::LookupAdvanceRequest {
                owner,
                completed_ordinal: "0".into(),
            },
        )
        .unwrap();

    let database = temp.project().join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER discussion_lookup_reads_no_update;")
        .unwrap();
    let original: String = connection
        .query_row(
            "SELECT result_json FROM discussion_lookup_reads LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut altered: Value = serde_json::from_str(&original).unwrap();
    altered["history"]["observations"][0]["statement"] = json!("The key is lost.");
    let altered_json = serde_json::to_string(&altered).unwrap();
    connection
        .execute(
            "UPDATE discussion_lookup_reads SET result_json=?, result_hash=?",
            (&altered_json, hash(&altered_json)),
        )
        .unwrap();
    drop(connection);
    let error = create_backup(&project, &temp.root.join("tampered.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn a_legacy_lookup_packet_keeps_search_reads_and_child_replay_working() {
    let (temp, project, access, document) = setup("legacy-search");
    let started = start(&project, &access, &document);
    let packet_id = started.packet.receipt.packet_id.clone();
    downgrade_lookup_packet(&temp, &packet_id);

    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    let initial = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .unwrap();
    settle_response(
        &project,
        &owner,
        &initial.packet,
        "0",
        search_response(),
        "legacy-search-needs-context",
    );
    let child = match project
        .advance_lookup(
            webnovel_core::projects::discussion_lookup::LookupAdvanceRequest {
                owner: owner.clone(),
                completed_ordinal: "0".into(),
            },
        )
        .unwrap()
    {
        LookupAdvance::Prepared { dispatch } => dispatch,
        LookupAdvance::Finished { .. } => panic!("legacy search should prepare a child packet"),
    };
    let lookup = child.packet.receipt.lookup.unwrap();
    assert!(lookup.reviewed_memory.is_none());
    assert!(matches!(
        &lookup.exchanges[0].result,
        LookupReadResult::Search { .. }
    ));

    let claimed_child = project
        .claim_lookup_invocation(owner.clone(), "1".into())
        .unwrap();
    settle_response(
        &project,
        &owner,
        &claimed_child.packet,
        "1",
        final_response(),
        "legacy-search-final",
    );
    let archive = temp.root.join("legacy-search.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let recovered = recover_backup(
        &archive,
        &temp.root.join("legacy-search-copy"),
        "Legacy search",
    )
    .unwrap();
    let recovered_access = recovered
        .documents().attach("legacy-search-copy-session".into())
        .unwrap();
    let recovered_view = recovered
        .read_discussion(recovered_access, document.head.document_id)
        .unwrap();
    assert_eq!(
        recovered_view.runs[0].status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Completed
    );
    assert!(
        recovered_view.runs[0].lookup.as_ref().unwrap().invocations[0]
            .response
            .is_some()
    );
}

#[test]
fn a_legacy_lookup_packet_rejects_memory_reads_and_seals_the_raw_response() {
    let (temp, project, access, document) = setup("legacy-capability");
    let started = start(&project, &access, &document);
    let packet_id = started.packet.receipt.packet_id.clone();
    downgrade_lookup_packet(&temp, &packet_id);

    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    let initial = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .unwrap();
    settle_initial(&project, &owner, &initial.packet);
    settle_initial(&project, &owner, &initial.packet);
    let view = project
        .read_discussion(access, document.head.document_id)
        .unwrap();
    assert_eq!(
        view.runs[0].status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Failed
    );
    assert_eq!(view.runs[0].lookup.as_ref().unwrap().invocations.len(), 1);
    assert!(
        view.runs[0].lookup.as_ref().unwrap().invocations[0]
            .response
            .is_none()
    );
}
