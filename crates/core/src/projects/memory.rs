//! Durable, source-bound chapter navigation memory — the session half.
//!
//! The actor-side logic moved to `wns-story` with `story_context` and
//! `context_packets`; the three are mutually dependent and travel together.

use super::*;

pub mod app_server;

impl ProjectSession {
    pub fn start_memory(&self, request: StartMemory) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Start(request, reply))))
    }

    pub fn begin_memory(&self, owner: MemoryOwner) -> CoreResult<MemoryDispatch> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Begin(owner, reply))))
    }

    pub fn stop_memory(&self, access: ProjectAccess, job_id: String) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Stop(access, job_id, reply))))
    }

    pub fn complete_memory(&self, request: CompleteMemory) -> CoreResult<MemoryCompletion> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Complete(request, reply))))
    }

    pub fn install_memory(&self, owner: MemoryOwner) -> CoreResult<MemoryView> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Install(owner, reply))))
    }

    pub fn read_memory(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<MemoryRead> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::Read(access, document_id, reply)))
        })
    }

    pub fn list_memory(&self, access: ProjectAccess) -> CoreResult<MemoryList> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::List(access, reply))))
    }

    pub fn read_memory_job(&self, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::ReadJob(owner, reply))))
    }

    pub fn read_memory_source(
        &self,
        access: ProjectAccess,
        view_id: String,
    ) -> CoreResult<SourceRead> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::ReadViewSource(
                access, view_id, reply,
            )))
        })
    }

    pub fn interrupt_memory_claim(&self, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::InterruptClaim(owner, reply))))
    }
}

use wns_context::frozen::SourceRead;
pub use wns_story::memory::*;
