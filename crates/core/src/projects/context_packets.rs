//! Local packet preparation — the session half and the host impl.
//!
//! The actor-side logic moved to `wns-story` with `story_context` and `memory`:
//! the three are mutually dependent through
//! `memory → context_packets → story_context → memory`, so none could leave
//! before the others. What stays is the command plumbing, because
//! `Command::Packet` is a variant of the actor's own enum and an inherent
//! `impl` must live in the crate that owns the type, plus the host impl,
//! because `OwnedProject` is declared in this crate.

use super::*;

impl ProjectSession {
    pub fn prepare_context(&self, request: PrepareContext) -> CoreResult<PreparationResult> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Prepare(Box::new(request), reply)))
        })
    }
    pub fn prepared_context(
        &self,
        access: ProjectAccess,
        packet_id: String,
    ) -> CoreResult<CompiledPacket> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Read(access, packet_id, reply)))
        })
    }
    pub fn prepared_context_is_current(
        &self,
        access: ProjectAccess,
        packet_id: String,
    ) -> CoreResult<bool> {
        self.request(|reply| {
            Command::Packet(Box::new(PacketCommand::Current(access, packet_id, reply)))
        })
    }
}

impl wns_story::host::StoryHost for OwnedProject {
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()> {
        OwnedProject::check_access(self, access)
    }
    fn db(&self) -> CoreResult<&Connection> {
        OwnedProject::db(self)
    }
    fn db_mut(&mut self) -> CoreResult<&mut Connection> {
        OwnedProject::db_mut(self)
    }
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>) {
        OwnedProject::fence_uncertain(self, result)
    }
    fn info(&self) -> &wns_kernel::ProjectInfo {
        &self.info
    }
    /// The crash-injection hook compiles the real body only into core's own
    /// test binary, so production behaviour is unchanged.
    fn hold_context_after_commit_before_ack(&self, operation_id: &str) {
        #[cfg(test)]
        super::hold_context_after_commit_before_ack(operation_id);
        #[cfg(not(test))]
        let _ = operation_id;
    }
    fn context_source_epoch(&self) -> CoreResult<String> {
        OwnedProject::context_source_epoch(self)
    }
    fn current_access(&self) -> CoreResult<ProjectAccess> {
        let missing = || {
            CoreError::new(
                "RecoveryRequired",
                "Project activity requires an attached current project session.",
            )
        };
        if self.needs_reopen {
            return Err(missing());
        }
        self.access.clone().ok_or_else(missing)
    }
}

pub use wns_story::context_packets::*;
use wns_context::packet::CompiledPacket;
