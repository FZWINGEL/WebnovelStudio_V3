//! Session façade for the discussion concern.
//!
//! The logic moved to `wns-conversation` (L5); what is left here is the half
//! that cannot travel — an `impl` block for `ProjectSession`, which core owns.
//! The trait/type impl rule is the same one that kept the `WorkshopHost` impl
//! here: an inherent `impl` must live in the crate that owns the type.
//!
//! Every item the moved module used to expose is re-exported below, so the
//! ~150 references at `webnovel_core::projects::discussions::…` — the desktop
//! app, the examples, and fourteen integration-test files — resolve unchanged.

use super::*;

impl ProjectSession {
    pub fn discussion_retry(
        &self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionRetry> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Retry(access, run_id, reply)))
        })
    }

    pub fn start_discussion(&self, request: StartDiscussion) -> CoreResult<DiscussionStart> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Start(request, reply)))
        })
    }

    pub fn begin_discussion_run(&self, request: DiscussionBegin) -> CoreResult<DiscussionDispatch> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Begin(request, reply)))
        })
    }

    pub fn mark_discussion_delivered(&self, owner: RunOwner) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::MarkDelivered(owner, reply)))
        })
    }

    pub fn append_discussion_output(
        &self,
        request: DiscussionOutputAppend,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Append(request, reply)))
        })
    }

    pub fn finish_discussion(&self, request: DiscussionFinish) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Finish(request, reply)))
        })
    }

    pub fn fail_discussion_run(&self, request: DiscussionFail) -> CoreResult<DiscussionRun> {
        self.request(|reply| Command::Discussion(Box::new(DiscussionCommand::Fail(request, reply))))
    }

    pub fn stop_discussion(
        &self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionStop> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Stop(access, run_id, reply)))
        })
    }

    pub fn settle_discussion_stop(
        &self,
        request: DiscussionStopSettled,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleStop(request, reply)))
        })
    }

    /// Atomically settle a bounded provider result and its discussion run.
    /// The report must carry the exact binding and sequence captured before
    /// dispatch; retries with the same event are idempotent.
    pub fn settle_provider_discussion(
        &self,
        request: ProviderTerminalReport,
    ) -> CoreResult<ProviderDiscussionSettlement> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleProvider(request, reply)))
        })
    }

    pub fn claim_lookup_invocation(
        &self,
        owner: RunOwner,
        ordinal: String,
    ) -> CoreResult<crate::projects::discussion_lookup::LookupDispatch> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ClaimLookup(
                owner, ordinal, reply,
            )))
        })
    }

    pub fn settle_lookup_invocation(
        &self,
        request: crate::projects::discussion_lookup::LookupInvocationReport,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleLookup(request, reply)))
        })
    }

    pub fn advance_lookup(
        &self,
        request: crate::projects::discussion_lookup::LookupAdvanceRequest,
    ) -> CoreResult<crate::projects::discussion_lookup::LookupAdvance> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::AdvanceLookup(request, reply)))
        })
    }

    pub fn halt_lookup(
        &self,
        request: crate::projects::discussion_lookup::LookupHaltRequest,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::HaltLookup(request, reply)))
        })
    }

    /// Read an owned run for a worker or recovery actor. This deliberately
    /// does not require a renderer session or writer lease.
    pub fn read_discussion_run(&self, owner: RunOwner) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ReadRun(owner, reply)))
        })
    }

    pub fn read_discussion(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<DiscussionView> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Read(
                access,
                document_id,
                reply,
            )))
        })
    }

    pub fn save_discussion_draft(
        &self,
        request: SaveDiscussionDraft,
    ) -> CoreResult<DiscussionDraft> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SaveDraft(request, reply)))
        })
    }
}

// Core-side session half for the app-server claim/ack pair.
mod app_server;

pub use wns_conversation::discussions::*;
