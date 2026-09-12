//! Actor-owned inspection and stop coordination for local background work.
//!
//! This module deliberately does not dispatch providers or own cancellation
//! tokens. It snapshots the current operation namespace and persists the stop
//! intent through the existing discussion and memory lifecycle methods. Native
//! callers remain responsible for cancelling external workers and waiting for
//! their local cleanup.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use wns_kernel::{CoreError, CoreResult, ProjectAccess};
use wns_story::host::StoryHost;
use wns_story::memory;
use crate::discussions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundWorkKind {
    Discussion,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundWorkStatus {
    Queued,
    Running,
    Stopping,
    Completed,
    Stopped,
    Failed,
    Interrupted,
}

impl BackgroundWorkStatus {
    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "stopping" => Ok(Self::Stopping),
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved background job has an unknown status.",
            )),
        }
    }
}

impl From<discussions::DiscussionRunStatus> for BackgroundWorkStatus {
    fn from(status: discussions::DiscussionRunStatus) -> Self {
        match status {
            discussions::DiscussionRunStatus::Queued => Self::Queued,
            discussions::DiscussionRunStatus::Running => Self::Running,
            discussions::DiscussionRunStatus::Stopping => Self::Stopping,
            discussions::DiscussionRunStatus::Completed => Self::Completed,
            discussions::DiscussionRunStatus::Stopped => Self::Stopped,
            discussions::DiscussionRunStatus::Failed => Self::Failed,
            discussions::DiscussionRunStatus::Interrupted => Self::Interrupted,
        }
    }
}

impl From<memory::MemoryJobStatus> for BackgroundWorkStatus {
    fn from(status: memory::MemoryJobStatus) -> Self {
        match status {
            memory::MemoryJobStatus::Queued => Self::Queued,
            memory::MemoryJobStatus::Running => Self::Running,
            memory::MemoryJobStatus::Stopping => Self::Stopping,
            memory::MemoryJobStatus::Completed => Self::Completed,
            memory::MemoryJobStatus::Stopped => Self::Stopped,
            memory::MemoryJobStatus::Failed => Self::Failed,
            memory::MemoryJobStatus::Interrupted => Self::Interrupted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackgroundWorkItem {
    pub kind: BackgroundWorkKind,
    pub id: String,
    pub document_id: String,
    pub status: BackgroundWorkStatus,
    pub project_id: String,
    pub operation_namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackgroundWorkError {
    pub item: BackgroundWorkItem,
    pub error: CoreError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackgroundWork {
    /// The active set for `ProjectSession::background_work`, or successful
    /// stop requests for `ProjectSession::stop_background_work`.
    ///
    /// A queued job is `stopped`; a running or already stopping job is
    /// `stopping` after a successful stop request.
    pub items: Vec<BackgroundWorkItem>,
    /// Per-job failures are retained so native cleanup can continue and
    /// surface recovery guidance after a partial stop.
    pub errors: Vec<BackgroundWorkError>,
}

fn recovery_required() -> CoreError {
    CoreError::new(
        "RecoveryRequired",
        "Background work requires a current project session. Reconcile or reopen the project before inspecting or stopping jobs.",
    )
}

/// The census and the stop paths both refuse to act without a current session.
/// The actor owns the `access` and `needs_reopen` fields; the host answers,
/// and this module keeps its own more specific message.
pub fn current_background_access(host: &impl StoryHost) -> CoreResult<ProjectAccess> {
    host.current_access().map_err(|_| recovery_required())
}

pub fn active_background_records(
host: &impl StoryHost,
    access: &ProjectAccess,
) -> CoreResult<Vec<BackgroundWorkItem>> {
    let db = host.db().map_err(|_| recovery_required())?;
    let mut statement = db.prepare(
        "SELECT kind,id,document_id,status FROM (
             SELECT 'discussion' AS kind,id,target_document_id AS document_id,status,created_at
               FROM discussion_runs
              WHERE project_id=? AND operation_namespace=?
             UNION ALL
             SELECT 'memory' AS kind,id,target_document_id AS document_id,status,created_at
               FROM memory_jobs
              WHERE project_id=? AND operation_namespace=?
         )
         WHERE status IN ('queued','running','stopping')
         ORDER BY created_at,id",
    )?;
    let rows = statement.query_map(
        params![
            access.project_id,
            access.operation_namespace,
            access.project_id,
            access.operation_namespace
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(kind, id, document_id, status)| {
            let kind = match kind.as_str() {
                "discussion" => BackgroundWorkKind::Discussion,
                "memory" => BackgroundWorkKind::Memory,
                _ => {
                    return Err(CoreError::new(
                        "InvalidProject",
                        "The saved background job has an unknown kind.",
                    ));
                }
            };
            let status = BackgroundWorkStatus::parse(&status)?;
            let title = db
                .query_row(
                    "SELECT title FROM documents WHERE id=?",
                    [&document_id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(BackgroundWorkItem {
                kind,
                id,
                document_id,
                status,
                project_id: access.project_id.clone(),
                operation_namespace: access.operation_namespace.clone(),
                title,
            })
        })
        .collect()
}

pub fn validate_expected_background_work(
host: &impl StoryHost,
    access: &ProjectAccess,
    expected: &BackgroundWork,
) -> CoreResult<()> {
    if !expected.errors.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A background-work stop census cannot contain prior errors.",
        ));
    }
    let db = host.db().map_err(|_| recovery_required())?;
    let mut ids = HashSet::with_capacity(expected.items.len());
    for item in &expected.items {
        if item.project_id != access.project_id
            || item.operation_namespace != access.operation_namespace
        {
            return Err(CoreError::new(
                "WrongProjectSession",
                "The background-work census belongs to another project namespace.",
            ));
        }
        if !matches!(
            item.status,
            BackgroundWorkStatus::Queued
                | BackgroundWorkStatus::Running
                | BackgroundWorkStatus::Stopping
        ) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A background-work stop census must contain only active jobs.",
            ));
        }
        if !ids.insert(item.id.clone()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A background-work stop census contains a duplicate job ID.",
            ));
        }
        let table = match item.kind {
            BackgroundWorkKind::Discussion => "discussion_runs",
            BackgroundWorkKind::Memory => "memory_jobs",
        };
        let query = format!(
            "SELECT project_id,operation_namespace,target_document_id,status FROM {table} WHERE id=?"
        );
        let row: Option<(String, String, String, String)> = db
            .query_row(&query, [&item.id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .optional()?;
        let Some((project_id, operation_namespace, document_id, status)) = row else {
            return Err(CoreError::new(
                "BackgroundWorkNotFound",
                "The captured background job is no longer available.",
            ));
        };
        if project_id != access.project_id
            || operation_namespace != access.operation_namespace
            || document_id != item.document_id
        {
            return Err(CoreError::new(
                "BackgroundWorkMismatch",
                "The captured background job does not match the current project namespace or document.",
            ));
        }
        // A job may have become terminal after the census. Existing Stop
        // methods are idempotent for terminal rows, so the persisted
        // status is intentionally not required to remain active here.
        let _ = status;
    }
    Ok(())
}

pub fn background_work(host: &impl StoryHost) -> CoreResult<BackgroundWork> {
    let access = current_background_access(host, )?;
    Ok(BackgroundWork {
        items: active_background_records(host, &access)?,
        errors: Vec::new(),
    })
}

pub fn stop_background_work(
host: &mut impl StoryHost,
    expected: BackgroundWork,
) -> CoreResult<BackgroundWork> {
    let access = current_background_access(host, )?;
    validate_expected_background_work(host, &access, &expected)?;
    let mut items = Vec::with_capacity(expected.items.len());
    let mut errors = Vec::new();

    for record in &expected.items {
        let outcome = match record.kind {
            BackgroundWorkKind::Discussion => crate::discussions::stop_discussion(
                host,
                access.clone(),
                record.id.clone(),
            )
                .map(|stop| BackgroundWorkStatus::from(stop.run.status)),
            BackgroundWorkKind::Memory => memory::stop_memory(
                host,
                access.clone(),
                record.id.clone(),
            )
                .map(|job| BackgroundWorkStatus::from(job.status)),
        };
        match outcome {
            Ok(status) => {
                let mut item = record.clone();
                item.status = status;
                items.push(item);
            }
            Err(error) => {
                let fence: CoreResult<()> = Err(error.clone());
                host.fence_uncertain(&fence);
                errors.push(BackgroundWorkError {
                    item: record.clone(),
                    error,
                });
            }
        }
    }

    Ok(BackgroundWork { items, errors })
}

pub fn interrupt_background_work(
host: &mut impl StoryHost,
    expected: BackgroundWork,
) -> CoreResult<BackgroundWork> {
    let access = current_background_access(host, )?;
    validate_expected_background_work(host, &access, &expected)?;
    let mut items = Vec::with_capacity(expected.items.len());
    let mut errors = Vec::new();

    for record in &expected.items {
        let outcome = match record.kind {
            BackgroundWorkKind::Discussion => crate::discussions::interrupt_discussion(
                host,
                access.clone(),
                record.id.clone(),
            )
                .map(|run| BackgroundWorkStatus::from(run.status)),
            BackgroundWorkKind::Memory => memory::interrupt_memory_claim(
                host,
                memory::MemoryOwner {
                    project_id: access.project_id.clone(),
                    operation_namespace: access.operation_namespace.clone(),
                    job_id: record.id.clone(),
                },
            )
            .map(|job| BackgroundWorkStatus::from(job.status)),
        };
        match outcome {
            Ok(status) => {
                let mut item = record.clone();
                item.status = status;
                items.push(item);
            }
            Err(error) => {
                let fence: CoreResult<()> = Err(error.clone());
                host.fence_uncertain(&fence);
                errors.push(BackgroundWorkError {
                    item: record.clone(),
                    error,
                });
            }
        }
    }

    Ok(BackgroundWork { items, errors })
}
