use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::background_work::{
    BackgroundWork, BackgroundWorkKind, BackgroundWorkStatus,
};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionOutputAppend, DiscussionRunStatus, StartDiscussion,
};
use webnovel_core::projects::memory::StartMemory;
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("wns-background-{label}-{}", Uuid::new_v4()));
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
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "p1"},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

fn budget() -> MockContextBudget {
    MockContextBudget::new("100000", "100", "100")
}

fn document(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    title: &str,
    text: &str,
    operation_id: &str,
) -> DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: operation_id.into(),
            document_id: id.into(),
            title: title.into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .expect("create document")
}

fn discussion(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::discussions::DiscussionRun {
    project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            instruction: "What should happen next?".into(),
            intent: Default::default(),
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: budget(),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start discussion")
        .run
}

fn memory(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::memory::MemoryJob {
    project
        .start_memory(StartMemory {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            budget: budget(),
            provider_binding: None,
        })
        .expect("start memory job")
}

fn item<'a>(
    work: &'a BackgroundWork,
    kind: BackgroundWorkKind,
    id: &str,
) -> &'a webnovel_core::projects::background_work::BackgroundWorkItem {
    work.items
        .iter()
        .find(|item| item.kind == kind && item.id == id)
        .expect("background item")
}

#[test]
fn census_and_exact_stop_cover_discussion_and_memory_without_document_mutation() {
    let temp = TempDir::new("census-stop");
    let project = ProjectSession::create(temp.child("project"), "Background work").unwrap();
    let access = project.attach("background-session".into()).unwrap();
    let first = document(
        &project,
        &access,
        "chapter-one",
        "Chapter One",
        "first body",
        "create-first",
    );
    let second = document(
        &project,
        &access,
        "chapter-two",
        "Chapter Two",
        "second body",
        "create-second",
    );
    let discussion_queued = discussion(&project, &access, &first, "discussion-queued");
    let discussion_running = discussion(&project, &access, &second, "discussion-running");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: discussion_running.owner.clone(),
        })
        .expect("begin running discussion");
    let memory_queued = memory(&project, &access, &first, "memory-queued");
    let memory_running = memory(&project, &access, &second, "memory-running");
    project
        .begin_memory(memory_running.owner.clone())
        .expect("begin running memory");

    let census = project.work().census().expect("read active census");
    assert_eq!(census.errors.len(), 0);
    assert_eq!(census.items.len(), 4);
    assert_eq!(
        item(
            &census,
            BackgroundWorkKind::Discussion,
            &discussion_queued.id
        )
        .title
        .as_deref(),
        Some("Chapter One")
    );
    assert_eq!(
        item(&census, BackgroundWorkKind::Memory, &memory_running.id).document_id,
        "chapter-two"
    );
    for active in &census.items {
        assert_eq!(active.project_id, project.info.project_id);
        assert_eq!(active.operation_namespace, project.info.operation_namespace);
    }

    // A request arriving after the census is deliberately outside the stop
    // set. This is the close-admission race the native supervisor fences.
    let later = discussion(&project, &access, &first, "discussion-later");
    let stopped = project
        .work().stop(census.clone())
        .expect("stop captured jobs");
    assert!(stopped.errors.is_empty());
    assert_eq!(stopped.items.len(), 4);
    assert_eq!(
        item(
            &stopped,
            BackgroundWorkKind::Discussion,
            &discussion_queued.id
        )
        .status,
        BackgroundWorkStatus::Stopped
    );
    assert_eq!(
        item(
            &stopped,
            BackgroundWorkKind::Discussion,
            &discussion_running.id
        )
        .status,
        BackgroundWorkStatus::Stopping
    );
    assert_eq!(
        item(&stopped, BackgroundWorkKind::Memory, &memory_queued.id).status,
        BackgroundWorkStatus::Stopped
    );
    assert_eq!(
        item(&stopped, BackgroundWorkKind::Memory, &memory_running.id).status,
        BackgroundWorkStatus::Stopping
    );

    let current = project
        .work().census()
        .expect("read remaining active work");
    assert_eq!(current.items.len(), 3);
    assert_eq!(
        item(&current, BackgroundWorkKind::Discussion, &later.id).status,
        BackgroundWorkStatus::Queued
    );
    assert_eq!(
        item(
            &current,
            BackgroundWorkKind::Discussion,
            &discussion_running.id
        )
        .status,
        BackgroundWorkStatus::Stopping
    );
    assert_eq!(
        item(&current, BackgroundWorkKind::Memory, &memory_running.id).status,
        BackgroundWorkStatus::Stopping
    );

    assert_eq!(
        project
            .document(access.clone(), "chapter-one".into())
            .unwrap()
            .head,
        first.head
    );
    assert_eq!(
        project
            .document(access.clone(), "chapter-one".into())
            .unwrap()
            .body,
        first.body
    );
    assert_eq!(
        project
            .document(access.clone(), "chapter-two".into())
            .unwrap()
            .head,
        second.head
    );
    assert_eq!(
        project.document(access, "chapter-two".into()).unwrap().body,
        second.body
    );
}

#[test]
fn interrupt_exact_census_preserves_partial_output_and_leaves_new_jobs_alive() {
    let temp = TempDir::new("interrupt-census");
    let project = ProjectSession::create(temp.child("project"), "Interrupt census").unwrap();
    let access = project.attach("interrupt-session".into()).unwrap();
    let first = document(
        &project,
        &access,
        "chapter-one",
        "Chapter One",
        "first body",
        "create-first",
    );
    let second = document(
        &project,
        &access,
        "chapter-two",
        "Chapter Two",
        "second body",
        "create-second",
    );
    let discussion_run = discussion(&project, &access, &first, "discussion-running");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: discussion_run.owner.clone(),
        })
        .unwrap();
    project
        .append_discussion_output(DiscussionOutputAppend {
            owner: discussion_run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "partial-output".into(),
            chunk: "partial output before close".into(),
        })
        .unwrap();
    let memory_job = memory(&project, &access, &second, "memory-running");
    project.begin_memory(memory_job.owner.clone()).unwrap();
    let census = project.work().census().unwrap();
    assert_eq!(census.items.len(), 2);

    let mut wrong_namespace = census.clone();
    wrong_namespace.items[0].project_id = "foreign-project".into();
    assert_eq!(
        project
            .work().interrupt(wrong_namespace)
            .unwrap_err()
            .code,
        "WrongProjectSession"
    );

    let later = discussion(&project, &access, &first, "discussion-after-census");
    let interrupted = project
        .work().interrupt(census)
        .expect("interrupt exact orphan census");
    assert!(interrupted.errors.is_empty());
    assert_eq!(interrupted.items.len(), 2);
    assert!(
        interrupted
            .items
            .iter()
            .all(|item| item.status == BackgroundWorkStatus::Interrupted)
    );

    let view = project
        .read_discussion(access.clone(), "chapter-one".into())
        .unwrap();
    let interrupted_run = view
        .runs
        .iter()
        .find(|run| run.id == discussion_run.id)
        .expect("interrupted discussion");
    assert_eq!(interrupted_run.status, DiscussionRunStatus::Interrupted);
    assert!(
        interrupted_run
            .output_text
            .contains("partial output before close")
    );

    let remaining = project.work().census().unwrap();
    assert_eq!(remaining.items.len(), 1);
    assert_eq!(remaining.items[0].id, later.id);
    assert_eq!(remaining.items[0].status, BackgroundWorkStatus::Queued);
    assert_eq!(
        project
            .document(access.clone(), "chapter-one".into())
            .unwrap()
            .head,
        first.head
    );
    assert_eq!(
        project.document(access, "chapter-two".into()).unwrap().body,
        second.body
    );
}

#[test]
fn stop_keeps_partial_errors_inspectable_and_rejects_bad_census_items() {
    let temp = TempDir::new("partial-stop");
    let project = ProjectSession::create(temp.child("project"), "Partial stop").unwrap();
    let access = project.attach("partial-session".into()).unwrap();
    let first = document(&project, &access, "first", "First", "first", "create-first");
    let second = document(
        &project,
        &access,
        "second",
        "Second",
        "second",
        "create-second",
    );
    let good = discussion(&project, &access, &first, "discussion-good");
    let bad = discussion(&project, &access, &second, "discussion-bad");
    let memory_job = memory(&project, &access, &first, "memory-for-corruption");
    let census = project.work().census().unwrap();

    let duplicate = BackgroundWork {
        items: census
            .items
            .iter()
            .cloned()
            .chain(std::iter::once(census.items[0].clone()))
            .collect(),
        errors: Vec::new(),
    };
    assert_eq!(
        project.work().stop(duplicate).unwrap_err().code,
        "InvalidRequest"
    );

    let mut wrong_kind = census.clone();
    wrong_kind.items[0].kind = match wrong_kind.items[0].kind {
        BackgroundWorkKind::Discussion => BackgroundWorkKind::Memory,
        BackgroundWorkKind::Memory => BackgroundWorkKind::Discussion,
    };
    assert!(project.work().stop(wrong_kind).is_err());

    // Keep all IDs and document ownership valid, but make the second
    // discussion point at a valid memory packet. Its Stop path then fails
    // while reading the immutable discussion context after the first stop,
    // proving that partial outcomes remain inspectable.
    let connection = Connection::open(project.path.join("project.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE discussion_runs SET packet_id=? WHERE id=?",
            params![memory_job.packet_id, bad.id],
        )
        .unwrap();
    drop(connection);

    let stopped = project.work().stop(census).unwrap();
    assert_eq!(stopped.items.len(), 2);
    assert_eq!(stopped.errors.len(), 1);
    assert_eq!(stopped.errors[0].item.id, bad.id);
    assert_eq!(
        stopped
            .items
            .iter()
            .find(|item| item.id == good.id)
            .unwrap()
            .status,
        BackgroundWorkStatus::Stopped
    );
    assert_eq!(
        stopped
            .items
            .iter()
            .find(|item| item.id == memory_job.id)
            .unwrap()
            .status,
        BackgroundWorkStatus::Stopped
    );
    let remaining = project.work().census().unwrap();
    assert_eq!(remaining.items.len(), 1);
    assert_eq!(remaining.items[0].id, bad.id);
}

#[test]
fn recovery_copy_filters_historical_namespace_and_unattached_session_requires_recovery() {
    let temp = TempDir::new("recovery-copy");
    let source = ProjectSession::create(temp.child("source"), "Source").unwrap();
    let source_access = source.attach("source-session".into()).unwrap();
    let source_doc = document(
        &source,
        &source_access,
        "chapter",
        "Source Chapter",
        "source",
        "create-source",
    );
    let source_run = discussion(&source, &source_access, &source_doc, "source-discussion");
    let source_census = source.work().census().unwrap();
    assert_eq!(source_census.items.len(), 1);

    let backup = temp.child("source.wnsbackup");
    create_backup(&source, &backup).unwrap();
    let recovered = recover_backup(&backup, &temp.child("recovered"), "Recovered").unwrap();
    assert_ne!(recovered.info.project_id, source.info.project_id);
    assert_ne!(
        recovered.info.operation_namespace,
        source.info.operation_namespace
    );
    assert_eq!(
        recovered.work().census().unwrap_err().code,
        "RecoveryRequired"
    );
    let recovered_access = recovered.attach("recovered-session".into()).unwrap();
    assert!(recovered.work().census().unwrap().items.is_empty());

    // Make a retained historical row look active under the source identity;
    // the recovered project's current namespace must still exclude it.
    let connection = Connection::open(recovered.path.join("project.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE discussion_runs SET project_id=?,operation_namespace=?,status='queued' WHERE id=?",
            params![source.info.project_id, source.info.operation_namespace, source_run.id],
        )
        .unwrap();
    drop(connection);
    assert!(recovered.work().census().unwrap().items.is_empty());
    assert_eq!(
        recovered
            .document(recovered_access, "chapter".into())
            .unwrap()
            .title,
        "Source Chapter"
    );
}

#[test]
fn attach_snapshot_reads_latest_head_before_retiring_old_session() {
    let temp = TempDir::new("attach-snapshot");
    let project = ProjectSession::create(temp.child("project"), "Attach snapshot").unwrap();
    let old_access = project.attach("old-session".into()).unwrap();
    let initial = document(
        &project,
        &old_access,
        "chapter",
        "Chapter",
        "before",
        "create-chapter",
    );
    let changed = body("after");
    let saved = project
        .save(SaveSnapshot {
            access: old_access.clone(),
            operation_id: "save-after".into(),
            expected: initial.head.clone(),
            local_generation: "1".into(),
            body: changed.clone(),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let attached = project.attach_snapshot("new-session".into()).unwrap();
    assert_eq!(attached.documents[0].head, saved.head);
    assert_eq!(attached.documents[0].body, changed);
    assert_eq!(attached.access.session, "new-session");
    let error = project.document(old_access, "chapter".into()).unwrap_err();
    assert_eq!(error.code, "WriterLeaseExpired");
}
