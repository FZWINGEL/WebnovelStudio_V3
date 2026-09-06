use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::lookup::LookupAllowance;
use webnovel_core::context::packet::{MockContextBudget, serialized_input};
use webnovel_core::projects::discussion_lookup::{
    LookupAdvance, LookupAdvanceRequest, LookupInvocationReport,
};
use webnovel_core::projects::discussions::{
    DiscussionBegin, FeedbackIntent, ProviderCleanup, ProviderOutcomeStatus, RunOwner,
    StartDiscussion,
};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{BackupManifest, create_backup, recover_backup};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-lookup-integrity-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary directory");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph",
            "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn setup(
    path: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(path, "Lookup backup integrity").expect("create project");
    let access = project
        .attach("lookup-integrity-session".into())
        .expect("attach");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("A jade pendant rests on the table."),
        })
        .expect("create chapter");
    (project, access, document)
}

fn start(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::discussions::DiscussionStart {
    project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            instruction: "Find an old story detail.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .expect("start lookup discussion")
}

fn begin(project: &ProjectSession, owner: &RunOwner) {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin lookup discussion");
}

fn report(
    owner: &RunOwner,
    packet: &webnovel_core::context::packet::CompiledPacket,
    ordinal: &str,
    event_id: &str,
    assistant_text: String,
) -> LookupInvocationReport {
    LookupInvocationReport {
        owner: owner.clone(),
        ordinal: ordinal.into(),
        event_id: event_id.into(),
        assistant_text,
        binding: None,
        status: ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: serialized_input(&packet.messages, &packet.options)
            .expect("serialize packet")
            .len()
            .to_string(),
        usage: None,
        cleanup: ProviderCleanup::Settled,
        error: None,
    }
}

fn final_response() -> String {
    serde_json::to_string(&json!({
        "kind": "discussion",
        "schemaVersion": "story-lookup.v1",
        "text": "The pendant remains on the table."
    }))
    .expect("serialize final response")
}

fn needs_context_missing_source() -> String {
    serde_json::to_string(&json!({
        "kind": "needsContext",
        "schemaVersion": "story-lookup.v1",
        "reads": [{
            "kind": "read",
            "id": "missing-source",
            "handle": "source-that-does-not-exist"
        }]
    }))
    .expect("serialize needs-context response")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn archive_entries(path: &Path) -> (BackupManifest, Vec<u8>) {
    let file = File::open(path).expect("open backup");
    let mut archive = ZipArchive::new(file).expect("read backup");
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .expect("manifest entry")
        .read_to_end(&mut manifest_bytes)
        .expect("read manifest");
    let manifest = serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .expect("database entry")
        .read_to_end(&mut database)
        .expect("read database");
    (manifest, database)
}

fn write_archive(path: &Path, manifest: &BackupManifest, database: &[u8]) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create tampered archive");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("manifest.json", options)
        .expect("manifest entry");
    zip.write_all(&serde_json::to_vec(manifest).expect("serialize manifest"))
        .expect("write manifest");
    zip.start_file("project.sqlite3", options)
        .expect("database entry");
    zip.write_all(database).expect("write database");
    zip.finish().expect("finish archive");
}

fn mutate_database(
    temp: &TempDir,
    name: &str,
    backup: &Path,
    mutate: impl FnOnce(&Connection),
) -> PathBuf {
    let (mut manifest, database) = archive_entries(backup);
    let database_path = temp.child(&format!("{name}.sqlite3"));
    fs::write(&database_path, database).expect("write extracted database");
    let connection = Connection::open(&database_path).expect("open extracted database");
    mutate(&connection);
    drop(connection);
    let database = fs::read(&database_path).expect("read mutated database");
    manifest.database_sha256 = sha256(&database);
    let tampered = temp.child(&format!("{name}.wnsbackup"));
    write_archive(&tampered, &manifest, &database);
    tampered
}

#[test]
fn backup_rejects_forged_completed_lookup_history() {
    let temp = TempDir::new("terminal");
    let (project, access, document) = setup(&temp.child("project"));
    let started = start(&project, &access, &document, "terminal-integrity");
    let owner = started.run.owner.clone();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin");
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    project
        .settle_lookup_invocation(report(
            &owner,
            &claimed.packet,
            "0",
            "terminal-integrity-result",
            final_response(),
        ))
        .expect("settle final response");
    assert_eq!(
        dispatch.packet.receipt.packet_id,
        claimed.packet.receipt.packet_id
    );
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create valid backup");
    let run_id = owner.run_id.clone();
    let tampered = mutate_database(&temp, "terminal-tampered", &backup, |db| {
        db.execute(
            "UPDATE discussion_runs SET output_text='forged final history' WHERE id=?",
            [&run_id],
        )
        .expect("tamper run output");
    });
    let error = recover_backup(&tampered, &temp.child("recovered-terminal"), "Recovered")
        .err()
        .expect("forged completed history must be rejected");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn backup_rejects_truncated_unavailable_lookup_receipt() {
    let temp = TempDir::new("unavailable");
    let (project, access, document) = setup(&temp.child("project"));
    let started = start(&project, &access, &document, "unavailable-integrity");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    project
        .settle_lookup_invocation(report(
            &owner,
            &claimed.packet,
            "0",
            "unavailable-integrity-result",
            needs_context_missing_source(),
        ))
        .expect("settle needs-context response");
    let advance = project
        .advance_lookup(LookupAdvanceRequest {
            owner: owner.clone(),
            completed_ordinal: "0".into(),
        })
        .expect("execute missing-source lookup");
    assert!(matches!(advance, LookupAdvance::Prepared { .. }));
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create valid unavailable backup");
    let tampered = mutate_database(&temp, "unavailable-tampered", &backup, |db| {
        db.execute("DROP TRIGGER discussion_lookup_reads_no_update", [])
            .expect("drop immutable test trigger");
        db.execute(
            "UPDATE discussion_lookup_reads SET truncated=1 WHERE run_id=?",
            [&owner.run_id],
        )
        .expect("tamper unavailable truncation");
    });
    let error = recover_backup(&tampered, &temp.child("recovered-unavailable"), "Recovered")
        .err()
        .expect("unavailable reads cannot claim truncation");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn backup_rejects_child_packet_from_another_lookup_session() {
    let temp = TempDir::new("provenance");
    let (project, access, document) = setup(&temp.child("project"));
    let started = start(&project, &access, &document, "provenance-integrity");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    project
        .settle_lookup_invocation(report(
            &owner,
            &claimed.packet,
            "0",
            "provenance-integrity-result",
            needs_context_missing_source(),
        ))
        .expect("settle needs-context response");
    assert!(matches!(
        project
            .advance_lookup(LookupAdvanceRequest {
                owner: owner.clone(),
                completed_ordinal: "0".into(),
            })
            .expect("prepare child"),
        LookupAdvance::Prepared { .. }
    ));
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create valid provenance backup");
    let child_packet_id = owner.run_id.clone();
    let tampered = mutate_database(&temp, "provenance-tampered", &backup, |db| {
        let child_packet: String = db
            .query_row(
                "SELECT packet_id FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=1",
                [&child_packet_id],
                |row| row.get(0),
            )
            .expect("child packet");
        db.execute("DROP TRIGGER immutable_context_packet_update", [])
            .expect("drop immutable packet test trigger");
        let packet_json: String = db
            .query_row(
                "SELECT packet_json FROM context_packets WHERE id=?",
                [&child_packet],
                |row| row.get(0),
            )
            .expect("packet JSON");
        let mut packet: Value = serde_json::from_str(&packet_json).expect("parse packet JSON");
        packet["receipt"]["sessionId"] = json!("forged-lookup-session");
        let packet_json = serde_json::to_string(&packet).expect("serialize packet JSON");
        db.execute(
            "UPDATE context_packets SET session_id=?,packet_json=?,packet_hash=? WHERE id=?",
            rusqlite::params![
                "forged-lookup-session",
                packet_json,
                sha256(packet_json.as_bytes()),
                child_packet,
            ],
        )
        .expect("tamper child packet session");
    });
    let error = recover_backup(&tampered, &temp.child("recovered-provenance"), "Recovered")
        .err()
        .expect("child packet from another session must be rejected");
    assert_eq!(error.code, "InvalidBackup");
}
