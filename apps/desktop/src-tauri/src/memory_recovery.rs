//! Local settlement recovery for memory workers.
//!
//! A memory provider call is never replayed from this module.  The map keeps
//! only the immutable terminal payload (or a pending installation) until the
//! actor accepts the local write.  Reopening a project is handled by core,
//! which marks active jobs interrupted and therefore cannot cause a replay.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use webnovel_core::projects::memory::{
    CompleteMemory, MemoryDispatch, MemoryJob, MemoryJobStatus, MemoryOwner,
};
use webnovel_core::projects::{CoreError, CoreResult, ProjectSession};

type MemoryKey = (String, String, String);

#[derive(Clone, Default)]
pub struct MemoryRecovery(Arc<Mutex<HashMap<MemoryKey, PendingMemorySave>>>);

#[derive(Clone)]
struct PendingMemorySave {
    owner: MemoryOwner,
    document_id: String,
    kind: PendingMemoryKind,
}

#[derive(Clone)]
enum PendingMemoryKind {
    DispatchClaim,
    Terminal(Box<CompleteMemory>),
    Installation,
}

fn key(owner: &MemoryOwner) -> MemoryKey {
    (
        owner.project_id.clone(),
        owner.operation_namespace.clone(),
        owner.job_id.clone(),
    )
}

impl MemoryRecovery {
    pub fn pending_count(&self) -> usize {
        self.lock().len()
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<MemoryKey, PendingMemorySave>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn retain_terminal(&self, completion: CompleteMemory, document_id: String) {
        let owner = completion.owner.clone();
        self.lock().insert(
            key(&owner),
            PendingMemorySave {
                owner,
                document_id,
                kind: PendingMemoryKind::Terminal(Box::new(completion)),
            },
        );
    }

    pub fn retain_install(&self, owner: MemoryOwner, document_id: String) {
        self.lock().insert(
            key(&owner),
            PendingMemorySave {
                owner,
                document_id,
                kind: PendingMemoryKind::Installation,
            },
        );
    }

    fn retain_claim(&self, job: &MemoryJob) {
        self.lock()
            .entry(key(&job.owner))
            .or_insert_with(|| PendingMemorySave {
                owner: job.owner.clone(),
                document_id: job.target.document_id.clone(),
                kind: PendingMemoryKind::DispatchClaim,
            });
    }

    pub fn claim_pending(&self, owner: &MemoryOwner) -> bool {
        self.lock()
            .get(&key(owner))
            .is_some_and(|pending| matches!(pending.kind, PendingMemoryKind::DispatchClaim))
    }

    /// A claim error never authorizes a worker. Retain an uncertain claim so
    /// a later local check can stop displaying it as running without replay.
    pub fn claim(&self, project: &ProjectSession, job: &MemoryJob) -> CoreResult<MemoryDispatch> {
        match project.begin_memory(job.owner.clone()) {
            Ok(dispatch) => Ok(dispatch),
            Err(error) => {
                if error.code == "UncertainOutcome"
                    || (["MemoryBasisChanged", "ContextPolicyChanged"]
                        .contains(&error.code.as_str())
                        && project.interrupt_memory_claim(job.owner.clone()).is_err())
                {
                    self.retain_claim(job);
                }
                Err(error)
            }
        }
    }

    pub fn worker_not_registered(&self, project: &ProjectSession, job: &MemoryJob) {
        if project.interrupt_memory_claim(job.owner.clone()).is_err() {
            self.retain_claim(job);
        }
    }

    pub fn pending_job_ids(
        &self,
        project_id: &str,
        operation_namespace: &str,
        document_id: &str,
    ) -> Vec<String> {
        let mut ids = self
            .lock()
            .values()
            .filter(|pending| {
                pending.owner.project_id == project_id
                    && pending.owner.operation_namespace == operation_namespace
                    && pending.document_id == document_id
            })
            .map(|pending| pending.owner.job_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    fn clear(&self, owner: &MemoryOwner) {
        self.lock().remove(&key(owner));
    }

    /// Settle one provider result and, after that transaction succeeds, create
    /// the generated navigation view in a separate transaction.  Any failed
    /// local step remains retryable with the exact same payload.
    pub fn save_or_retain(
        &self,
        project: &ProjectSession,
        completion: CompleteMemory,
        document_id: String,
    ) -> CoreResult<MemoryJob> {
        let owner = completion.owner.clone();
        match project.complete_memory(completion.clone()) {
            Ok(completed) => {
                if completed.job.status == MemoryJobStatus::Completed {
                    match project.install_memory(owner.clone()) {
                        Ok(_) => {
                            self.clear(&owner);
                            return project.read_memory_job(owner);
                        }
                        Err(error) => {
                            if error.code == "ContextPolicyChanged" {
                                self.clear(&owner);
                                return project.read_memory_job(owner);
                            }
                            self.retain_install(owner, document_id);
                            return Err(error);
                        }
                    }
                }
                self.clear(&owner);
                Ok(completed.job)
            }
            Err(error) => {
                self.retain_terminal(completion, document_id);
                Err(error)
            }
        }
    }

    /// Retry only the local terminal write and/or installation.  This method
    /// intentionally has no provider connection and cannot dispatch work.
    pub fn retry(&self, project: &ProjectSession, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        let current = project.read_memory_job(owner.clone())?;
        let pending = self.lock().get(&key(&owner)).cloned();

        if let Some(pending) = pending {
            if matches!(pending.kind, PendingMemoryKind::DispatchClaim) {
                let interrupted = project.interrupt_memory_claim(owner.clone())?;
                self.clear(&owner);
                return Ok(interrupted);
            }
            if let PendingMemoryKind::Terminal(completion) = pending.kind {
                let completion = *completion;
                if current.result.is_none()
                    && (matches!(
                        current.status,
                        MemoryJobStatus::Running | MemoryJobStatus::Stopping
                    ) || (current.status == MemoryJobStatus::Interrupted
                        && current.stop_reason.as_deref()
                            == Some("recovered_unknown_external_outcome")))
                {
                    let settled = match project.complete_memory(completion.clone()) {
                        Ok(value) => value.job,
                        Err(error) => {
                            self.retain_terminal(completion, pending.document_id);
                            return Err(error);
                        }
                    };
                    if settled.status != MemoryJobStatus::Completed {
                        self.clear(&owner);
                        return Ok(settled);
                    }
                } else if current.status == MemoryJobStatus::Interrupted {
                    self.clear(&owner);
                    return Ok(current);
                }
            }

            let latest = project.read_memory_job(owner.clone())?;
            if latest.status == MemoryJobStatus::Completed {
                match project.install_memory(owner.clone()) {
                    Ok(_) => {
                        self.clear(&owner);
                        return project.read_memory_job(owner);
                    }
                    Err(error) => {
                        if error.code == "ContextPolicyChanged" {
                            self.clear(&owner);
                            return project.read_memory_job(owner);
                        }
                        self.retain_install(owner, pending.document_id);
                        return Err(error);
                    }
                }
            }
            self.clear(&owner);
            return Ok(latest);
        }

        if current.status == MemoryJobStatus::Completed {
            match project.install_memory(owner.clone()) {
                Ok(_) => project.read_memory_job(owner),
                Err(error) => Err(error),
            }
        } else if current.status == MemoryJobStatus::Interrupted {
            Ok(current)
        } else {
            Err(CoreError::new(
                "MemorySaveNotPending",
                "No local memory result is waiting to be saved.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use webnovel_core::context::memory::mock_navigation_digest;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::projects::discussions::{ProviderCleanup, ProviderOutcomeStatus};
    use webnovel_core::projects::memory::{MemoryDispatch, StartMemory};
    use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};

    fn fixture(label: &str) -> (ProjectSession, ProjectAccess, MemoryDispatch) {
        let path = std::env::temp_dir().join(format!(
            "wns-desktop-memory-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Memory recovery test").expect("create");
        let access = project
            .documents()
            .attach("memory-session".into())
            .expect("attach");
        let document = project
            .documents()
            .create(CreateDocument {
                access: access.clone(),
                operation_id: format!("create-{label}"),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: json!({
                    "schemaVersion": 1,
                    "body": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "attrs": {"id": "p1"},
                            "content": [{"type": "text", "text": "Mei walks toward the gate."}]
                        }]
                    }
                }),
            })
            .expect("document");
        let queued = project
            .start_memory(StartMemory {
                access: access.clone(),
                operation_id: format!("memory-{label}"),
                expected: document.head,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: None,
            })
            .expect("start");
        let dispatch = project.begin_memory(queued.owner).expect("begin");
        (project, access, dispatch)
    }

    fn completion(dispatch: &MemoryDispatch, event_suffix: &str) -> CompleteMemory {
        let candidate = mock_navigation_digest(&dispatch.source).expect("mock candidate");
        CompleteMemory {
            app_server: None,
            owner: dispatch.job.owner.clone(),
            event_id: format!("{}-{event_suffix}", dispatch.job.id),
            raw_output: serde_json::to_string(&candidate).expect("candidate JSON"),
            outcome: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: Some(ProviderCleanup::Settled),
            error: None,
            effective_identity: None,
            delivery: None,
        }
    }

    fn trigger(project: &ProjectSession, sql: &str) {
        Connection::open(project.path.join("project.sqlite3"))
            .expect("open test database")
            .execute_batch(sql)
            .expect("trigger operation");
    }

    fn clean(project: ProjectSession) {
        let path = project.path.clone();
        let temp_root = std::fs::canonicalize(std::env::temp_dir()).expect("temp root");
        let canonical = std::fs::canonicalize(&path).expect("project path");
        assert_eq!(canonical.parent(), Some(temp_root.as_path()));
        assert!(
            canonical
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("wns-desktop-memory-"))
        );
        drop(project);
        std::fs::remove_dir_all(path).expect("remove test project");
    }

    #[test]
    fn result_write_fault_retains_exact_terminal_and_retries_after_lease_rotation() {
        let (project, old_access, dispatch) = fixture("result-fault");
        let recovery = MemoryRecovery::default();
        let owner = dispatch.job.owner.clone();
        let document_id = dispatch.job.target.document_id.clone();
        let completion = completion(&dispatch, "terminal");
        let expected_raw = completion.raw_output.clone();

        trigger(
            &project,
            "CREATE TRIGGER memory_result_fault BEFORE INSERT ON memory_results BEGIN SELECT RAISE(ABORT,'test memory result failure'); END;",
        );
        let error = recovery
            .save_or_retain(&project, completion, document_id.clone())
            .expect_err("the injected result fault must be visible");
        assert_ne!(error.code, "Completed");
        assert_eq!(
            recovery.pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id),
            vec![owner.job_id.clone()]
        );

        trigger(&project, "DROP TRIGGER memory_result_fault;");
        let new_access = project
            .documents()
            .attach("memory-session-after-navigation".into())
            .expect("new lease");
        assert_eq!(
            project
                .read_memory(old_access, document_id.clone())
                .expect_err("old navigation lease must expire")
                .code,
            "WriterLeaseExpired"
        );
        assert_eq!(
            project
                .read_memory(new_access.clone(), document_id.clone())
                .expect("read after navigation")
                .jobs
                .len(),
            1
        );

        let retried = recovery
            .retry(&project, owner.clone())
            .expect("local retry");
        assert_eq!(retried.id, owner.job_id);
        assert_eq!(
            retried
                .result
                .as_ref()
                .expect("result")
                .raw_output
                .as_deref(),
            Some(expected_raw.as_str())
        );
        let read = project
            .read_memory(new_access, document_id.clone())
            .expect("read settled memory");
        assert_eq!(read.jobs.len(), 1, "retry must not create a second job");
        assert_eq!(read.views.len(), 1, "successful retry installs one view");
        assert!(
            recovery
                .pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id)
                .is_empty()
        );
        clean(project);
    }

    #[test]
    fn install_fault_retains_candidate_and_retry_is_idempotent() {
        let (project, access, dispatch) = fixture("install-fault");
        let recovery = MemoryRecovery::default();
        let owner = dispatch.job.owner.clone();
        let document_id = dispatch.job.target.document_id.clone();
        let completion = completion(&dispatch, "install");

        trigger(
            &project,
            "CREATE TRIGGER memory_view_fault BEFORE INSERT ON memory_views BEGIN SELECT RAISE(ABORT,'test memory view failure'); END;",
        );
        let error = recovery
            .save_or_retain(&project, completion, document_id.clone())
            .expect_err("the injected view fault must be visible");
        assert_ne!(error.code, "MemoryInstallBlocked");
        let completed = project
            .read_memory(access.clone(), document_id.clone())
            .expect("completed candidate");
        assert_eq!(completed.jobs[0].status, MemoryJobStatus::Completed);
        assert!(
            completed.jobs[0]
                .result
                .as_ref()
                .and_then(|result| result.candidate.as_ref())
                .is_some()
        );
        assert!(completed.views.is_empty());
        assert_eq!(
            recovery.pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id),
            vec![owner.job_id.clone()]
        );

        trigger(&project, "DROP TRIGGER memory_view_fault;");
        recovery
            .retry(&project, owner.clone())
            .expect("install retry");
        let after = project
            .read_memory(access.clone(), document_id.clone())
            .expect("read installed candidate");
        assert_eq!(after.views.len(), 1);
        assert!(
            recovery
                .pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id)
                .is_empty()
        );
        recovery
            .retry(&project, owner)
            .expect("idempotent install retry");
        assert_eq!(
            project
                .read_memory(access, document_id)
                .expect("read stable view")
                .views
                .len(),
            1
        );
        clean(project);
    }

    #[test]
    fn stop_before_pending_terminal_retry_never_installs_a_view() {
        let (project, access, dispatch) = fixture("stop-before-retry");
        let recovery = MemoryRecovery::default();
        let owner = dispatch.job.owner.clone();
        let document_id = dispatch.job.target.document_id.clone();
        let completion = completion(&dispatch, "stopped");
        let stopped = project
            .stop_memory(access.clone(), owner.job_id.clone())
            .expect("stop");
        assert_eq!(stopped.status, MemoryJobStatus::Stopping);
        recovery.retain_terminal(completion, document_id.clone());

        let settled = recovery
            .retry(&project, owner.clone())
            .expect("settle stopped job");
        assert_eq!(settled.status, MemoryJobStatus::Stopped);
        assert!(settled.view.is_none());
        let read = project
            .read_memory(access, document_id.clone())
            .expect("read stopped job");
        assert_eq!(read.jobs[0].status, MemoryJobStatus::Stopped);
        assert!(read.views.is_empty());
        assert!(
            recovery
                .pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id)
                .is_empty()
        );
        clean(project);
    }

    #[test]
    fn a_lost_durable_claim_is_reconciled_without_a_worker_or_replay() {
        let (project, _access, dispatch) = fixture("lost-claim");
        let recovery = MemoryRecovery::default();
        // The actor committed begin, but its caller could not confirm the
        // acknowledgment and therefore never started an external worker.
        recovery.retain_claim(&dispatch.job);
        assert!(recovery.claim_pending(&dispatch.job.owner));
        let access = project
            .documents()
            .attach("claim-reconciliation-renderer".into())
            .expect("reattach");
        let checked = recovery
            .retry(&project, dispatch.job.owner.clone())
            .expect("reconcile claim");
        assert_eq!(checked.status, MemoryJobStatus::Interrupted);
        assert_eq!(
            checked.stop_reason.as_deref(),
            Some("dispatch_outcome_unknown")
        );
        assert!(checked.result.is_none());
        assert!(checked.view.is_none());
        assert!(!recovery.claim_pending(&dispatch.job.owner));
        let read = project
            .read_memory(access, dispatch.job.target.document_id)
            .expect("read");
        assert_eq!(read.jobs.len(), 1);
        assert!(read.views.is_empty());
        assert_eq!(
            project.begin_memory(dispatch.job.owner).unwrap_err().code,
            "MemoryJobSealed"
        );
        clean(project);
    }

    #[test]
    fn retained_terminal_survives_registry_close_as_interrupted_history() {
        let (project, _access, dispatch) = fixture("archive-pending-terminal");
        let recovery = MemoryRecovery::default();
        let terminal = completion(&dispatch, "terminal");
        let expected_raw = terminal.raw_output.clone();
        trigger(
            &project,
            "CREATE TRIGGER memory_result_fault BEFORE INSERT ON memory_results BEGIN SELECT RAISE(ABORT,'test memory result failure'); END;",
        );
        recovery
            .save_or_retain(&project, terminal, dispatch.job.target.document_id.clone())
            .expect_err("retain failed write");
        trigger(&project, "DROP TRIGGER memory_result_fault;");
        let path = project.path.clone();
        drop(project);
        // Archive removes the registry session. The same process can retain
        // an unsaved result while a new actor performs startup recovery.
        let reopened = ProjectSession::open(&path).expect("reopen archived project");
        let access = reopened
            .documents()
            .attach("after-archive".into())
            .expect("attach");
        let settled = recovery
            .retry(&reopened, dispatch.job.owner.clone())
            .expect("retain historical terminal without restarting");
        assert_eq!(settled.status, MemoryJobStatus::Interrupted);
        assert_eq!(
            settled.result.unwrap().raw_output.as_deref(),
            Some(expected_raw.as_str())
        );
        assert!(settled.view.is_none());
        assert!(
            recovery
                .pending_job_ids(
                    &access.project_id,
                    &access.operation_namespace,
                    &dispatch.job.target.document_id,
                )
                .is_empty()
        );
        assert!(
            reopened
                .read_memory(access, dispatch.job.target.document_id)
                .unwrap()
                .views
                .is_empty()
        );
        assert_eq!(
            reopened.begin_memory(dispatch.job.owner).unwrap_err().code,
            "MemoryJobSealed"
        );
        clean(reopened);
    }

    #[test]
    fn policy_revoked_install_clears_pending_without_exposing_a_view() {
        let (project, access, dispatch) = fixture("policy-revoked-install");
        let recovery = MemoryRecovery::default();
        let owner = dispatch.job.owner.clone();
        let document_id = dispatch.job.target.document_id.clone();
        let completion = completion(&dispatch, "policy");
        project
            .complete_memory(completion)
            .expect("terminal result");
        recovery.retain_install(owner.clone(), document_id.clone());
        project
            .revoke_story_context(access.clone(), "0".into())
            .expect("revoke policy");

        let after = recovery
            .retry(&project, owner.clone())
            .expect("policy-revoked retry settles");
        assert_eq!(after.status, MemoryJobStatus::Completed);
        assert!(after.view.is_none());
        assert!(
            recovery
                .pending_job_ids(&owner.project_id, &owner.operation_namespace, &document_id)
                .is_empty()
        );
        assert!(
            project
                .read_memory(access, document_id)
                .expect("read policy-revoked job")
                .views
                .is_empty()
        );
        clean(project);
    }
}
