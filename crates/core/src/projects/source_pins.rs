//! Durable AuthorRoom source preferences — the session half.
//!
//! The module's vocabulary and its actor-side logic moved to `wns-story` behind
//! [`wns_story::source_pins::SourcePinHost`]. What stays here is the command
//! plumbing, because `Command::SourcePins` is a variant of the actor's own enum
//! and an inherent `impl` must live in the crate that owns the type.
//!
//! That split — command plumbing with the actor, domain logic in the crate, a
//! trait between them — is the shape step 7 takes for every module.

use super::*;
use rusqlite::Connection;

pub use wns_story::source_pins::{
    AUTHOR_ROOM_AUDIENCE, SaveSourcePins, SourcePinCommand, SourcePinHost, SourcePinScope, SourcePinSet,
    SourcePinsView, persistent_for_discussion,
};

impl ProjectSession {
    pub fn read_source_pins(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<SourcePinsView> {
        self.request(|reply| {
            Command::SourcePins(Box::new(SourcePinCommand::Read(access, document_id, reply)))
        })
    }

    pub fn save_source_pins(&self, request: SaveSourcePins) -> CoreResult<SourcePinSet> {
        self.request(|reply| Command::SourcePins(Box::new(SourcePinCommand::Save(request, reply))))
    }
}

/// The four things the moved module needs from the actor, and nothing else.
impl SourcePinHost for OwnedProject {
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
}

impl OwnedProject {
    pub(super) fn handle_source_pins(&mut self, command: SourcePinCommand) {
        wns_story::source_pins::handle_source_pins(self, command);
    }
}
