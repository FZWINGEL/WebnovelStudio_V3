//! Normal close coordinates the existing project actors and owned workers.
//! The renderer owns its editor barrier; this module never saves or replaces text.
use crate::discussion_recovery::DiscussionRecovery;
use crate::memory_recovery::MemoryRecovery;
use crate::project_commands::{DesktopProjects, execute};
use crate::provider_runtime::DesktopProviders;
use serde::Serialize;
use tauri::State;
use webnovel_core::projects::background_work::BackgroundWork;
use webnovel_core::projects::{CoreError, CoreResult, ProjectSession};

#[cfg(test)]
#[path = "app_close_tests.rs"]
mod tests;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCloseStatus {
    starting_requests: u32,
    active_jobs: usize,
    active_workers: usize,
    pending_results: usize,
    ready: bool,
}

#[derive(Clone, Default)]
struct CloseCoordinator {
    projects: DesktopProjects,
    runtime: DesktopProviders,
    discussions: DiscussionRecovery,
    memory: MemoryRecovery,
}

impl CloseCoordinator {
    fn census(&self) -> CoreResult<Vec<(ProjectSession, BackgroundWork)>> {
        self.projects
            .all_open()?
            .into_iter()
            .map(|project| {
                let work = project.work().census()?;
                Ok((project, work))
            })
            .collect()
    }

    fn status(&self, close_id: &str) -> CoreResult<AppCloseStatus> {
        self.read_status(close_id, true)
    }

    fn read_status(&self, close_id: &str, settle_orphans: bool) -> CoreResult<AppCloseStatus> {
        let before = self.runtime.close_activity(close_id)?;
        let captured = self.census()?;
        let after = self.runtime.close_activity(close_id)?;
        // Read retained output after worker activity. Registrations outlive
        // result retention, so zero workers cannot hide a later retained result.
        let pending_results = self.discussions.pending_count() + self.memory.pending_count();
        let starting_requests = before.starting_requests.max(after.starting_requests);
        let active_jobs = captured
            .iter()
            .map(|(_, work)| work.items.len())
            .sum::<usize>();
        if settle_orphans
            && after.stopping
            && starting_requests == 0
            && after.active_workers == 0
            && pending_results == 0
            && active_jobs > 0
        {
            // No producer or retained result remains. Seal only the captured
            // jobs; a cancelled close cannot target a subsequently created job.
            for (project, work) in captured {
                self.runtime.close_activity(close_id)?;
                let result = project.work().interrupt(work)?;
                if let Some(error) = result.errors.into_iter().next() {
                    return Err(error.error);
                }
            }
            return self.read_status(close_id, false);
        }
        self.runtime.close_activity(close_id)?;
        Ok(AppCloseStatus {
            starting_requests,
            active_jobs,
            active_workers: after.active_workers,
            pending_results,
            ready: starting_requests == 0
                && active_jobs == 0
                && after.active_workers == 0
                && pending_results == 0,
        })
    }

    fn stop(&self, close_id: &str) -> CoreResult<AppCloseStatus> {
        let cancellations = self.runtime.capture_close_cancellations(close_id)?;
        let stopped = (|| {
            self.runtime.confirm_close_stop(close_id)?;
            let captured = self.census()?;
            let mut first_error = None;
            for (project, work) in captured {
                self.runtime.close_activity(close_id)?;
                match project.work().stop(work) {
                    Ok(result) => {
                        if first_error.is_none() {
                            first_error = result.errors.into_iter().next().map(|error| error.error);
                        }
                    }
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
            first_error.map_or(Ok(()), Err)
        })();
        // Disk failure cannot require paid work to continue. These handles
        // refer only to workers captured at the author's Stop confirmation.
        cancellations.cancel();
        stopped?;
        self.status(close_id)
    }

    fn finish(&self, close_id: &str) -> CoreResult<()> {
        if !self.status(close_id)?.ready {
            return Err(CoreError::new(
                "CloseNotReady",
                "Some replies or story memory are still running or need saving. Stay open until they are safely finished.",
            ));
        }
        // Keep admission closed until the window is destroyed. If destruction
        // fails, the renderer must cancel this same close request.
        self.runtime.shutdown_app_server(close_id)?;
        Ok(())
    }
}

#[tauri::command]
pub fn begin_app_close(close_id: String, runtime: State<'_, DesktopProviders>) -> CoreResult<()> {
    runtime.begin_close(&close_id)
}

#[tauri::command]
pub fn cancel_app_close(close_id: String, runtime: State<'_, DesktopProviders>) -> CoreResult<()> {
    runtime.cancel_close(&close_id)
}

fn coordinator(
    projects: &DesktopProjects,
    runtime: &DesktopProviders,
    discussions: &DiscussionRecovery,
    memory: &MemoryRecovery,
) -> CloseCoordinator {
    CloseCoordinator {
        projects: projects.clone(),
        runtime: runtime.clone(),
        discussions: discussions.clone(),
        memory: memory.clone(),
    }
}

#[tauri::command]
pub async fn app_close_status(
    close_id: String,
    projects: State<'_, DesktopProjects>,
    runtime: State<'_, DesktopProviders>,
    discussions: State<'_, DiscussionRecovery>,
    memory: State<'_, MemoryRecovery>,
) -> CoreResult<AppCloseStatus> {
    let close = coordinator(&projects, &runtime, &discussions, &memory);
    execute(move || close.status(&close_id)).await
}

#[tauri::command]
pub async fn stop_app_jobs(
    close_id: String,
    projects: State<'_, DesktopProjects>,
    runtime: State<'_, DesktopProviders>,
    discussions: State<'_, DiscussionRecovery>,
    memory: State<'_, MemoryRecovery>,
) -> CoreResult<AppCloseStatus> {
    let close = coordinator(&projects, &runtime, &discussions, &memory);
    execute(move || close.stop(&close_id)).await
}

#[tauri::command]
pub async fn finish_app_close(
    close_id: String,
    projects: State<'_, DesktopProjects>,
    runtime: State<'_, DesktopProviders>,
    discussions: State<'_, DiscussionRecovery>,
    memory: State<'_, MemoryRecovery>,
) -> CoreResult<()> {
    let close = coordinator(&projects, &runtime, &discussions, &memory);
    execute(move || close.finish(&close_id)).await
}
