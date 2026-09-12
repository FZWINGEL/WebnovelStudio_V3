//! What a project-chat operation needs from the project that owns it.
//!
//! [`StoryHost`] carries the durable-project half every story-concern module
//! reaches for: the connection, the access check, the uncertain-outcome fence.
//! Chat adds three session-lifecycle operations on top.
//!
//! Declared here rather than widened into `StoryHost` because all three are
//! session concerns that no L4 module calls, and a trait is sized by its
//! lowest consumer like anything else. `webnovel-core` implements it for
//! `OwnedProject` — an inherent or trait `impl` must live in the crate that
//! owns the type, which is the same rule that keeps `ProjectSession`'s
//! discussion and chat façades in core.

use wns_kernel::{CoreResult, ProjectAccess};
use wns_story::host::StoryHost;

pub trait ProjectChatHost: StoryHost {
    /// Take a renderer lease for an already-open project.
    fn attach(&mut self, session: String) -> CoreResult<ProjectAccess>;
    /// Reopen the connection after it was dropped, before a writer needs it.
    fn recover_connection(&mut self) -> CoreResult<()>;
}
