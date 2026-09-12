//! What the story-context cluster needs from the actor.
//!
//! Core implements this seam for its owned project. Story operations validate
//! the current access and use the actor's connection; they do not create a
//! second writer. The trait also supplies the source epoch, project identity,
//! filesystem root and current renderer access needed by its consumers.
//!
//! Raw connection access is still part of this boundary. Domain functions own
//! their transaction scopes and call `fence_uncertain` after uncertain outcomes.
//! This is a composition seam, not a compile-time restriction on SQL ownership.

use rusqlite::Connection;
use wns_kernel::{CoreResult, ProjectAccess, ProjectInfo, SourceEpoch};

pub trait StoryHost {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()>;
    fn db(&self) -> CoreResult<&Connection>;
    fn db_mut(&mut self) -> CoreResult<&mut Connection>;
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>);
    fn context_source_epoch(&self) -> CoreResult<SourceEpoch>;
    /// Crash-injection point for `kill_after_commit_before_ack_recovers_once`,
    /// declared unconditionally for the same reason `HistoryHost` declares it:
    /// the real hook compiles only into core's own test binary.
    fn hold_context_after_commit_before_ack(&self, operation_id: &str);
    /// The actor reads two fields off this (`project_id`, `operation_namespace`)
    /// to validate a runtime owner. Everything else it reaches for is a method.
    fn info(&self) -> &ProjectInfo;
    /// The project folder. Export installation writes beside it, and a backup
    /// reads `project.sqlite3` from it.
    fn path(&self) -> &std::path::Path;
    /// The access of the renderer session currently attached, or the recovery
    /// error that says there is none.
    ///
    /// Distinct from [`Self::check_access`], which validates an access the
    /// caller already holds. Three concerns need this one — the chat activity
    /// projection, the background-work census, and the assistant-draft
    /// lifecycle — and each used to reach the actor's own `access` and
    /// `needs_reopen` fields, which no other crate can see. It sits here
    /// rather than on a concern's own host trait because the lowest consumer
    /// is shared.
    fn current_access(&self) -> CoreResult<ProjectAccess>;
}
