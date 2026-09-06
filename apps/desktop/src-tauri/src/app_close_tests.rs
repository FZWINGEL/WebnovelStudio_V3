use super::*;
use crate::discussion_recovery::{PendingSave, SaveOutcome};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionOutputAppend, DiscussionRun, DiscussionRunStatus,
    ProviderOutcomeStatus, StartDiscussion,
};
use webnovel_core::projects::memory::{CompleteMemory, StartMemory};
use webnovel_core::projects::{CreateDocument, DocumentRecord, ProjectAccess, ProjectSession};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("wns-desktop-close-{label}-{nonce}"));
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

fn open_project(
    projects: &DesktopProjects,
    root: &TempDir,
    folder: &str,
    title: &str,
    session: &str,
) -> (ProjectSession, ProjectAccess) {
    let opened = projects
        .open(root.child(folder), Some(title.into()), session.into())
        .expect("open synthetic project");
    let project = projects
        .project(&opened.project.project_id)
        .expect("find opened project");
    (project, opened.access)
}

fn create_document(
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
        .expect("create synthetic document")
}

fn start_discussion(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation_id: &str,
) -> DiscussionRun {
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

fn start_memory(
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
        .expect("start memory")
}

fn begin_discussion(project: &ProjectSession, run: &DiscussionRun) {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: run.owner.clone(),
        })
        .expect("begin discussion");
}

fn append_prefix(project: &ProjectSession, run: &DiscussionRun) {
    project
        .append_discussion_output(DiscussionOutputAppend {
            owner: run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "prefix-output".into(),
            chunk: "retained prefix".into(),
        })
        .expect("append discussion prefix");
}

fn coordinator_for(
    projects: &DesktopProjects,
    runtime: &DesktopProviders,
    discussions: &DiscussionRecovery,
    memory: &MemoryRecovery,
) -> CloseCoordinator {
    coordinator(projects, runtime, discussions, memory)
}

#[test]
fn status_counts_jobs_across_documents_and_projects_and_finish_waits_for_work() {
    let temp = TempDir::new("status-count");
    let projects = DesktopProjects::default();
    let (first, first_access) = open_project(&projects, &temp, "first", "First", "first-session");
    let first_doc = create_document(
        &first,
        &first_access,
        "first-doc",
        "First document",
        "first body",
        "create-first",
    );
    let second_doc = create_document(
        &first,
        &first_access,
        "second-doc",
        "Second document",
        "second body",
        "create-second",
    );
    let first_run = start_discussion(&first, &first_access, &first_doc, "first-run");
    let first_memory = start_memory(&first, &first_access, &second_doc, "first-memory");
    let (second, second_access) =
        open_project(&projects, &temp, "second", "Second", "second-session");
    let other_doc = create_document(
        &second,
        &second_access,
        "other-doc",
        "Other document",
        "other body",
        "create-other",
    );
    let second_run = start_discussion(&second, &second_access, &other_doc, "second-run");

    let runtime = DesktopProviders::default();
    let discussions = DiscussionRecovery::default();
    let memory = MemoryRecovery::default();
    let close = coordinator_for(&projects, &runtime, &discussions, &memory);
    runtime.begin_close("status-count").unwrap();
    let status = close.status("status-count").unwrap();
    assert_eq!(status.starting_requests, 0);
    assert_eq!(status.active_jobs, 3);
    assert_eq!(status.active_workers, 0);
    assert_eq!(status.pending_results, 0);
    assert!(!status.ready);
    assert_eq!(first.background_work().unwrap().items.len(), 2);
    assert_eq!(second.background_work().unwrap().items.len(), 1);
    assert_eq!(
        close.finish("status-count").unwrap_err().code,
        "CloseNotReady"
    );
    assert_eq!(first_run.status, DiscussionRunStatus::Queued);
    assert_eq!(
        first_memory.status,
        webnovel_core::projects::memory::MemoryJobStatus::Queued
    );
    assert_eq!(second_run.status, DiscussionRunStatus::Queued);
    assert_eq!(
        runtime
            .admit_request()
            .err()
            .expect("close gate rejects a new request")
            .code,
        "AppClosing"
    );
    runtime.cancel_close("status-count").unwrap();
    assert!(runtime.admit_request().is_ok());
}

#[test]
fn stop_persists_successes_and_cancels_http_workers_after_partial_database_failure() {
    let temp = TempDir::new("partial-stop");
    let projects = DesktopProjects::default();
    let (project, access) = open_project(&projects, &temp, "project", "Project", "session");
    let first = create_document(&project, &access, "first", "First", "first", "create-first");
    let second = create_document(
        &project,
        &access,
        "second",
        "Second",
        "second",
        "create-second",
    );
    let good = start_discussion(&project, &access, &first, "good-run");
    let bad = start_discussion(&project, &access, &second, "bad-run");
    let memory_job = start_memory(&project, &access, &first, "memory-packet");
    let connection = Connection::open(project.path.join("project.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE discussion_runs SET packet_id=? WHERE id=?",
            params![memory_job.packet_id, bad.id],
        )
        .unwrap();
    drop(connection);

    let runtime = DesktopProviders::default();
    let good_signal = runtime.register_http(&good.owner).unwrap();
    let bad_signal = runtime.register_http(&bad.owner).unwrap();
    let discussions = DiscussionRecovery::default();
    let memory = MemoryRecovery::default();
    let close = coordinator_for(&projects, &runtime, &discussions, &memory);
    runtime.begin_close("partial-stop").unwrap();
    let error = close.stop("partial-stop").unwrap_err();
    assert!(!error.code.is_empty());
    assert!(good_signal.is_cancelled());
    assert!(bad_signal.is_cancelled());
    let remaining = project.background_work().unwrap();
    assert_eq!(remaining.items.len(), 1);
    assert_eq!(remaining.items[0].id, bad.id);
    let view = project
        .read_discussion(access.clone(), "first".into())
        .unwrap();
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Stopped);
    assert_eq!(
        project.read_memory(access, "first".into()).unwrap().jobs[0].status,
        webnovel_core::projects::memory::MemoryJobStatus::Stopped
    );
    runtime.cancel_close("partial-stop").unwrap();
}

#[test]
fn retained_discussion_and_memory_results_block_finish_and_orphan_settlement() {
    let temp = TempDir::new("retained-results");
    let projects = DesktopProjects::default();
    let (project, access) = open_project(&projects, &temp, "project", "Project", "session");
    let document = create_document(
        &project,
        &access,
        "chapter",
        "Chapter",
        "body",
        "create-chapter",
    );
    let run = start_discussion(&project, &access, &document, "retained-discussion");
    begin_discussion(&project, &run);
    append_prefix(&project, &run);
    let memory_job = start_memory(&project, &access, &document, "retained-memory");
    project.begin_memory(memory_job.owner.clone()).unwrap();

    // Keep a terminal provider payload in the native recovery map. The
    // coordinator must treat it as pending local persistence and leave the
    // durable active job alone until the payload is reconciled.
    let memory_recovery = MemoryRecovery::default();
    memory_recovery.retain_terminal(
        CompleteMemory {
            owner: memory_job.owner.clone(),
            event_id: "retained-memory-result".into(),
            raw_output: "{}".into(),
            outcome: ProviderOutcomeStatus::Failed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: None,
            error: Some("synthetic retained result".into()),
            effective_identity: None,
            delivery: None,
        },
        "chapter".into(),
    );
    // Rebuild the discussion map separately so both pending counters are
    // represented by the coordinator.
    let discussions_recovery = DiscussionRecovery::default();
    discussions_recovery.retain(PendingSave {
        run: run.clone(),
        outcome: SaveOutcome::Fail,
    });
    let runtime = DesktopProviders::default();
    let close = coordinator_for(&projects, &runtime, &discussions_recovery, &memory_recovery);
    runtime.begin_close("retained-results").unwrap();
    runtime.confirm_close_stop("retained-results").unwrap();
    let status = close.status("retained-results").unwrap();
    assert_eq!(status.active_jobs, 2);
    assert_eq!(status.pending_results, 2);
    assert!(!status.ready);
    assert_eq!(
        close.finish("retained-results").unwrap_err().code,
        "CloseNotReady"
    );
    let view = project
        .read_discussion(access.clone(), "chapter".into())
        .unwrap();
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Running);
    assert!(view.runs[0].output_text.contains("retained prefix"));
    assert_eq!(project.background_work().unwrap().items.len(), 2);
    runtime.cancel_close("retained-results").unwrap();
}

#[test]
fn stop_without_registered_workers_interrupts_orphan_and_preserves_prefix() {
    let temp = TempDir::new("orphan-stop");
    let projects = DesktopProjects::default();
    let (project, access) = open_project(&projects, &temp, "project", "Project", "session");
    let document = create_document(
        &project,
        &access,
        "chapter",
        "Chapter",
        "body",
        "create-chapter",
    );
    let run = start_discussion(&project, &access, &document, "orphan-discussion");
    begin_discussion(&project, &run);
    append_prefix(&project, &run);
    let original_head = document.head.clone();
    let runtime = DesktopProviders::default();
    let discussions = DiscussionRecovery::default();
    let memory = MemoryRecovery::default();
    let close = coordinator_for(&projects, &runtime, &discussions, &memory);
    runtime.begin_close("orphan-stop").unwrap();
    let status = close
        .stop("orphan-stop")
        .expect("orphan interruption settles");
    assert!(status.ready);
    assert_eq!(status.active_jobs, 0);
    let view = project
        .read_discussion(access.clone(), "chapter".into())
        .unwrap();
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
    assert!(view.runs[0].output_text.contains("retained prefix"));
    assert_eq!(project.background_work().unwrap().items.len(), 0);
    assert_eq!(
        project.document(access, "chapter".into()).unwrap().head,
        original_head
    );
    runtime.cancel_close("orphan-stop").unwrap();
}

#[test]
fn registered_worker_keeps_finish_blocked_until_drop_and_cancel() {
    let projects = DesktopProjects::default();
    let runtime = DesktopProviders::default();
    let discussions = DiscussionRecovery::default();
    let memory = MemoryRecovery::default();
    let close = coordinator_for(&projects, &runtime, &discussions, &memory);
    runtime.begin_close("worker-held").unwrap();
    let worker = runtime.track_local_worker().unwrap();
    assert_eq!(
        close.finish("worker-held").unwrap_err().code,
        "CloseNotReady"
    );
    drop(worker);
    assert!(close.finish("worker-held").is_ok());
    assert_eq!(
        runtime
            .admit_request()
            .err()
            .expect("close gate rejects a new request")
            .code,
        "AppClosing"
    );
    runtime.cancel_close("worker-held").unwrap();
    assert!(runtime.admit_request().is_ok());
}

#[test]
fn renderer_started_retires_abandoned_close_without_cancelling_worker() {
    let runtime = DesktopProviders::default();
    let owner = webnovel_core::projects::discussions::RunOwner {
        project_id: "project".into(),
        operation_namespace: "namespace".into(),
        run_id: "run".into(),
    };
    let signal = runtime.register_http(&owner).unwrap();
    runtime.begin_close("abandoned-close").unwrap();
    runtime.renderer_started();
    assert!(!signal.is_cancelled());
    assert_eq!(
        runtime.close_activity("abandoned-close").unwrap_err().code,
        "CloseRequestChanged"
    );
    assert!(runtime.admit_request().is_ok());
}
