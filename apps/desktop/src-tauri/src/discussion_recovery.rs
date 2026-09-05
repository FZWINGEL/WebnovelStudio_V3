//! Pending local writes survive renderer navigation, but never replay generation.
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::{CoreError, CoreResult, ProjectAccess, ProjectSession};

type OwnerKey = (String, String, String);

#[derive(Clone, Default)]
pub struct DiscussionRecovery(Arc<Mutex<HashMap<OwnerKey, PendingSave>>>);

#[derive(Clone)]
pub(super) enum SaveOutcome {
    Provider(Box<ProviderTerminalReport>),
    Complete(DiscussionFinish),
    Fail,
    Stop,
}

#[derive(Clone)]
pub(super) struct PendingSave {
    pub run: DiscussionRun,
    pub outcome: SaveOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerIssue {
    run_id: String,
    detail: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDiscussionView {
    #[serde(flatten)]
    view: DiscussionView,
    worker_issues: Vec<WorkerIssue>,
}

fn key(owner: &RunOwner) -> OwnerKey {
    (
        owner.project_id.clone(),
        owner.operation_namespace.clone(),
        owner.run_id.clone(),
    )
}

fn active(run: &DiscussionRun) -> bool {
    matches!(
        run.status,
        DiscussionRunStatus::Queued | DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    )
}

impl PendingSave {
    pub(super) fn attempt(
        &self,
        project: &ProjectSession,
        current: &DiscussionRun,
    ) -> CoreResult<()> {
        if !active(current) {
            return Ok(());
        }
        if let SaveOutcome::Provider(report) = &self.outcome {
            if !report.assistant_text.starts_with(&current.output_text) {
                return Err(CoreError::new(
                    "ProviderOutputMismatch",
                    "The saved response does not match the retained provider result.",
                ));
            }
            let mut report = report.as_ref().clone();
            report.expected_sequence = current.sequence.clone();
            project.settle_provider_discussion(report)?;
            return Ok(());
        }
        if current.status == DiscussionRunStatus::Stopping {
            project.settle_discussion_stop(DiscussionStopSettled {
                owner: current.owner.clone(),
                expected_sequence: current.sequence.clone(),
                event_id: format!("{}-stop-settled", current.id),
                assistant_text: current.output_text.clone(),
                cleanup: DiscussionStopCleanup::Settled,
            })?;
        } else if let SaveOutcome::Complete(request) = &self.outcome {
            project.finish_discussion(request.clone())?;
        } else {
            project.fail_discussion_run(DiscussionFail {
                owner: current.owner.clone(), expected_sequence: current.sequence.clone(),
                event_id: format!("{}-storage-failed-{}", current.id, current.sequence),
                reason: "The local test response could not finish. Its saved partial output is retained.".into(),
            })?;
        }
        Ok(())
    }
}

impl DiscussionRecovery {
    // Poison recovery is safe: entries are complete owned values and map writes
    // do not execute author code. Losing the map would hide unfinished saves.
    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<OwnerKey, PendingSave>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn retain(&self, pending: PendingSave) {
        self.pending().insert(key(&pending.run.owner), pending);
    }

    pub(super) fn claim(
        &self,
        project: &ProjectSession,
        run: &DiscussionRun,
    ) -> Option<DiscussionDispatch> {
        // Serialize claim outcomes with local retries. A failed claim may have
        // committed; no duplicate start may dispatch while it needs checking.
        let mut pending = self.pending();
        let owner_key = key(&run.owner);
        if pending.contains_key(&owner_key) {
            return None;
        }
        match project.begin_discussion_run(DiscussionBegin {
            owner: run.owner.clone(),
        }) {
            Ok(dispatch) => Some(dispatch),
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "RunAlreadyStarted" | "RunSealed" | "ContextChanged"
                ) =>
            {
                None
            }
            Err(_) => {
                pending.insert(
                    owner_key,
                    PendingSave {
                        run: run.clone(),
                        outcome: SaveOutcome::Fail,
                    },
                );
                None
            }
        }
    }

    pub(super) fn view(&self, view: DiscussionView) -> DesktopDiscussionView {
        let mut pending = self.pending();
        let mut worker_issues = Vec::new();
        for run in &view.runs {
            let owner_key = key(&run.owner);
            if !active(run) {
                pending.remove(&owner_key);
            } else if pending.contains_key(&owner_key) {
                worker_issues.push(WorkerIssue {
                    run_id: run.id.clone(),
                    detail: "The response could not be started or fully saved. Retry saving it locally; this will not send another model request.",
                });
            }
        }
        DesktopDiscussionView {
            view,
            worker_issues,
        }
    }

    pub(super) fn retry(
        &self,
        project: &ProjectSession,
        access: ProjectAccess,
        document_id: String,
        run_id: String,
    ) -> CoreResult<DesktopDiscussionView> {
        // Caller first reconciles the existing document session. Validate its
        // fresh lease and exact document before consulting any pending output.
        let view = project.read_discussion(access.clone(), document_id.clone())?;
        let current = view
            .runs
            .iter()
            .find(|run| {
                run.id == run_id
                    && run.owner.project_id == access.project_id
                    && run.owner.operation_namespace == access.operation_namespace
            })
            .ok_or_else(|| {
                CoreError::new(
                    "RunNotFound",
                    "This response does not belong to the open discussion.",
                )
            })?;
        let owner_key = key(&current.owner);
        {
            // Serialize only local settlement attempts, never provider work.
            let mut pending = self.pending();
            if let Some(saved) = pending.get(&owner_key) {
                saved.attempt(project, current)?;
                pending.remove(&owner_key);
            } else if active(current) {
                return Err(CoreError::new(
                    "ResponseStillRunning",
                    "This response is still running. Stop it before retrying a saved result.",
                ));
            }
        }
        Ok(self.view(project.read_discussion(access, document_id)?))
    }
}
