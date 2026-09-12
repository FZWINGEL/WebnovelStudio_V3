//! Explicit review and atomic author decisions -- the session half.
//!
//! The actor-side logic moved to `wns-conversation`. What stays is the command
//! plumbing, because `Command::Proposal` is a variant of the actor's own enum.

use super::*;

impl ProjectSession {
    pub fn proposals(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<Vec<Proposal>> {
        self.request(|reply| {
            Command::Proposal(Box::new(ProposalCommand::List(access, document_id, reply)))
        })
    }
    pub fn prepare_proposal(&self, request: PrepareProposal) -> CoreResult<PreparedProposal> {
        self.request(|reply| Command::Proposal(Box::new(ProposalCommand::Prepare(request, reply))))
    }
    pub fn prepare_continuation(
        &self,
        request: PrepareContinuation,
    ) -> CoreResult<PreparedProposal> {
        self.request(|reply| {
            Command::Proposal(Box::new(ProposalCommand::PrepareContinuation(
                request, reply,
            )))
        })
    }
    pub fn prepare_structured(&self, request: PrepareStructured) -> CoreResult<PreparedProposal> {
        self.request(|reply| {
            Command::Proposal(Box::new(ProposalCommand::PrepareStructured(request, reply)))
        })
    }
    pub fn apply_proposal(&self, request: ApplyProposal) -> CoreResult<ApplyAck> {
        self.request(|reply| Command::Proposal(Box::new(ProposalCommand::Apply(request, reply))))
    }
    pub fn reject_proposal(&self, request: RejectProposal) -> CoreResult<ProposalDecision> {
        self.request(|reply| Command::Proposal(Box::new(ProposalCommand::Reject(request, reply))))
    }
}

pub use wns_conversation::proposals::*;
