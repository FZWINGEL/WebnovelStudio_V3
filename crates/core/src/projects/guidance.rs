//! Durable author guidance — the session half.
//!
//! The authoring half moved to `wns-conversation`; the frozen half was already
//! at L3 in `wns-context`. What stays is the command plumbing, because
//! `Command::Guidance` is a variant of the actor's own enum and an inherent
//! `impl` must live in the crate that owns the type.

use super::*;

impl ProjectSession {
    pub fn save_guidance(&self, request: SaveGuidance) -> CoreResult<GuidanceVersion> {
        self.request(|reply| Command::Guidance(Box::new(GuidanceCommand::Save(request, reply))))
    }

    /// Return active guidance applicable to the selected document. Request
    /// guidance already consumed by a successful request is excluded: it is
    /// historical context, not a promise to use it again.
    pub fn guidance(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<Vec<GuidanceVersion>> {
        self.request(|reply| {
            Command::Guidance(Box::new(GuidanceCommand::List(access, document_id, reply)))
        })
    }
}

use wns_context::guidance::GuidanceVersion;
pub use wns_conversation::guidance::validate_guidance_storage;
pub use wns_conversation::guidance::*;
