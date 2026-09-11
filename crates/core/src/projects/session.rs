//! The project actor and its session façade.
//!
//! Split out of `projects.rs` (see [`super::records`] for why). One locked
//! project, one owned SQLite connection, one command channel, explicit renderer
//! leases.
//!
//! `ProjectSession` is the deepest coupling in the tree: its `Command` enum
//! carries ~28 variants and its impl exposes 32 public methods, so every project
//! operation in the codebase routes through it. Isolating it here is the
//! prerequisite for step 5 of the architecture sequence — replacing that single
//! façade with per-concern ones (`DocumentApi`, `ContextApi`, `StoryApi`, …)
//! over the same handle, without touching the actor, the channel or the
//! ordering guarantees the tests assert.

use super::*;
use super::records::*;
use super::{
    background_work, context_packets, discussions, evidence_queries, exports, guidance, history,
    memory, project_chat, proposals, reviewed_story, source_pins, story_context, workshop,
    workshop_generation,
};
use crate::documents::Endpoint;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use wns_kernel::{CoreError, CoreResult, Head};

pub(crate) use wns_kernel::Reply;
pub(crate) enum Command {
    ProjectChat(Box<project_chat::ProjectChatCommand>),
    Memory(Box<memory::MemoryCommand>),
    Packet(Box<context_packets::PacketCommand>),
    Context(Box<story_context::ContextCommand>),
    Discussion(Box<discussions::DiscussionCommand>),
    Guidance(Box<guidance::GuidanceCommand>),
    History(Box<history::HistoryCommand>),
    Proposal(Box<proposals::ProposalCommand>),
    Review(Box<reviewed_story::ReviewCommand>),
    EvidenceQuery(Box<evidence_queries::EvidenceQueryCommand>),
    Export(Box<exports::ExportCommand>),
    SourcePins(Box<source_pins::SourcePinCommand>),
    WorkshopStart(
        Box<workshop_generation::StartWorkshop>,
        Reply<discussions::DiscussionStart>,
    ),
    WorkshopRead(ProjectAccess, Reply<workshop::WorkshopView>),
    WorkshopSave(workshop::SaveWorkshop, Reply<workshop::WorkshopSnapshot>),
    WorkshopHistory(ProjectAccess, Reply<Vec<workshop::WorkshopSnapshot>>),
    WorkshopPreview(
        workshop::PreviewWorkshopAdoption,
        Reply<workshop::WorkshopAdoptionPreview>,
    ),
    WorkshopAdopt(
        ProjectAccess,
        String,
        String,
        Reply<workshop::WorkshopAdoptionAck>,
    ),
    BackgroundWork(Reply<background_work::BackgroundWork>),
    StopBackgroundWork(
        background_work::BackgroundWork,
        Reply<background_work::BackgroundWork>,
    ),
    InterruptBackgroundWork(
        background_work::BackgroundWork,
        Reply<background_work::BackgroundWork>,
    ),
    Attach(String, Reply<ProjectAccess>),
    AttachSnapshot(String, Reply<AttachedProject>),
    Create(CreateDocument, Reply<DocumentRecord>),
    List(ProjectAccess, Reply<Vec<DocumentRecord>>),
    Read(ProjectAccess, String, Reply<DocumentRecord>),
    Save(SaveSnapshot, Reply<SaveAck>),
    Checkpoint(CheckpointRequest, Reply<Revision>),
    LegacyHistory(ProjectAccess, String, Reply<Vec<Revision>>),
    Reconcile(ReconcileRequest, Reply<ReconciledDocument>),
    ProjectMetadata(Reply<ProjectMetadata>),
    RenameProject(ProjectAccess, String, String, Reply<ProjectMetadata>),
    RenameDocument(ProjectAccess, String, String, String, Reply<DocumentRecord>),
    ViewState(ProjectAccess, Reply<Option<ViewState>>),
    SaveViewState(ProjectAccess, Head, Endpoint, Endpoint, Reply<ViewState>),
    ContextSourceEpoch(Reply<String>),
    StorageInfo(Reply<StorageInfo>),
    Shutdown,
}
pub(crate) struct Handle {
    pub(crate) queue: mpsc::SyncSender<Command>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.queue.send(Command::Shutdown);
        if let Ok(thread) = self.thread.get_mut()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}
#[derive(Clone)]
pub struct ProjectSession {
    handle: Arc<Handle>,
    pub info: ProjectInfo,
    pub path: PathBuf,
}

impl ProjectSession {
    /// The destination must not exist. No existing author folder is overwritten.
    pub fn create(path: impl AsRef<Path>, title: &str) -> CoreResult<Self> {
        let destination = path.as_ref();
        let parent = destination
            .parent()
            .ok_or_else(|| CoreError::new("InvalidRequest", "Choose a project folder."))?;
        Self::create_staged(
            &parent.join(format!(".wns-create-{}", new_id())),
            destination,
            title,
            &CreationOrigin {
                operation_namespace: new_id(),
                operation_id: new_id(),
            },
        )
    }
    /// Resume only the exact recorded staging/final paths. Incomplete staging is
    /// retained for diagnosis; it is never silently replaced or listed as complete.
    pub fn create_staged(
        staging: &Path,
        destination: &Path,
        title: &str,
        origin: &CreationOrigin,
    ) -> CoreResult<Self> {
        validate_title(title)?;
        check_id(&origin.operation_namespace)?;
        check_id(&origin.operation_id)?;
        let parent = |path: &Path| -> CoreResult<PathBuf> {
            Ok(std::fs::canonicalize(path.parent().ok_or_else(|| {
                CoreError::new("InvalidRequest", "Choose a project folder.")
            })?)?)
        };
        if staging.file_name().is_none()
            || destination.file_name().is_none()
            || parent(staging)? != parent(destination)?
            || staging.file_name() == destination.file_name()
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Staging must be a separate folder beside the destination.",
            ));
        }
        if destination.try_exists()? {
            if read_creation_origin(destination).ok().as_ref() == Some(origin) {
                return Self::open(destination);
            }
            return Err(CoreError::new(
                "ProjectExists",
                "Choose a new folder for this project.",
            ));
        }
        let initialized = if staging.try_exists()? {
            if read_creation_origin(staging).ok().as_ref() != Some(origin) {
                return Err(CoreError::new(
                    "IncompleteCreation",
                    "The unfinished project folder needs inspection. It has been retained.",
                ));
            }
            OwnedProject::open_direct(staging.to_owned(), None)?
        } else {
            let project = OwnedProject::open_direct(staging.to_owned(), Some(title.to_owned()))?;
            write_creation_origin(staging, origin)?;
            project
        };
        initialized
            .db()?
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        drop(initialized);
        if destination.try_exists()? {
            return Err(CoreError::new(
                "ProjectExists",
                "The destination was created while preparing the project.",
            ));
        }
        install_project_directory(staging, destination)?;
        Self::open(destination)
    }
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::start(path.as_ref().to_owned(), None)
    }
    fn start(path: PathBuf, title: Option<String>) -> CoreResult<Self> {
        let (queue, receiver) = mpsc::sync_channel(64);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("webnovel-project".into())
            .spawn(move || match OwnedProject::open_direct(path, title) {
                Ok(mut project) => {
                    if ready_tx
                        .send(Ok((project.info.clone(), project.path.clone())))
                        .is_err()
                    {
                        return;
                    }
                    while let Ok(command) = receiver.recv() {
                        match command {
                            Command::Memory(command) => project.handle_memory(*command),
                            Command::ProjectChat(command) => project.handle_project_chat(*command),
                            Command::Packet(command) => project.handle_packet(*command),
                            Command::Context(command) => project.handle_context(*command),
                            Command::Discussion(command) => project.handle_discussion(*command),
                            Command::Guidance(command) => project.handle_guidance(*command),
                            Command::History(command) => project.handle_history(*command),
                            Command::Proposal(command) => project.handle_proposal(*command),
                            Command::Review(command) => project.handle_review(*command),
                            Command::EvidenceQuery(command) => {
                                project.handle_evidence_query(*command)
                            }
                            Command::Export(command) => project.handle_export(*command),
                            Command::SourcePins(command) => project.handle_source_pins(*command),
                            Command::WorkshopStart(request, reply) => {
                                let result = project.start_workshop(*request);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::WorkshopRead(access, reply) => {
                                let _ = reply.send(project.read_workshop(access));
                            }
                            Command::WorkshopSave(request, reply) => {
                                let result = project.save_workshop(request);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::WorkshopHistory(access, reply) => {
                                let _ = reply.send(project.workshop_history(access));
                            }
                            Command::WorkshopPreview(request, reply) => {
                                let _ = reply.send(project.preview_workshop_adoption(request));
                            }
                            Command::WorkshopAdopt(access, operation_id, preview_id, reply) => {
                                let result =
                                    project.adopt_workshop(access, operation_id, preview_id);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::BackgroundWork(reply) => {
                                let _ = reply.send(project.background_work());
                            }
                            Command::StopBackgroundWork(expected, reply) => {
                                let result = project.stop_background_work(expected);
                                let _ = reply.send(result);
                            }
                            Command::InterruptBackgroundWork(expected, reply) => {
                                let result = project.interrupt_background_work(expected);
                                let _ = reply.send(result);
                            }
                            Command::Attach(session, reply) => {
                                let _ = reply.send(project.attach(session));
                            }
                            Command::Create(request, reply) => {
                                let result = project.create_document(request);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::List(access, reply) => {
                                let _ = reply.send(project.list(access));
                            }
                            Command::Read(access, id, reply) => {
                                let _ = reply.send(
                                    project
                                        .check_access(&access)
                                        .and_then(|()| read_document(project.db()?, &id)),
                                );
                            }
                            Command::Save(request, reply) => {
                                let result = project.save(request);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::Checkpoint(request, reply) => {
                                let result = project.checkpoint(request);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::LegacyHistory(access, id, reply) => {
                                let _ = reply.send(project.history(access, &id));
                            }
                            Command::Reconcile(request, reply) => {
                                let _ = reply.send(project.reconcile(request));
                            }
                            Command::ProjectMetadata(reply) => {
                                let _ = reply.send(
                                    project
                                        .recover_connection()
                                        .and_then(|()| project.project_metadata()),
                                );
                            }
                            Command::AttachSnapshot(session, reply) => {
                                let _ = reply.send(project.attach_snapshot(session));
                            }
                            Command::RenameProject(access, expected, title, reply) => {
                                let result = project.rename_project(access, &expected, &title);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::RenameDocument(access, id, expected, title, reply) => {
                                let result =
                                    project.rename_document(access, &id, &expected, &title);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::ViewState(access, reply) => {
                                let _ = reply.send(project.view_state(access));
                            }
                            Command::SaveViewState(access, head, anchor, focus, reply) => {
                                let result = project.save_view_state(access, head, anchor, focus);
                                project.fence_uncertain(&result);
                                let _ = reply.send(result);
                            }
                            Command::ContextSourceEpoch(reply) => {
                                let _ = reply.send(project.context_source_epoch());
                            }
                            Command::StorageInfo(reply) => {
                                let _ = reply.send(project.storage_info());
                            }
                            Command::Shutdown => break,
                        }
                    }
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })?;
        let started = ready_rx.recv().map_err(|_| CoreError::disconnected())?;
        match started {
            Ok((info, path)) => Ok(Self {
                handle: Arc::new(Handle {
                    queue,
                    thread: Mutex::new(Some(thread)),
                }),
                info,
                path,
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
    pub(crate) fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle
            .queue
            .send(command(sender))
            .map_err(|_| CoreError::disconnected())?;
        receiver.recv().map_err(|_| CoreError::disconnected())?
    }
    /// Host-owned attachment: every renderer creation begins with a fresh lease.
    /// Narrow interface to the document concern: eleven methods instead of 28,
    /// and the group that actually widens this type's surface. See
    /// [`DocumentApi`].
    ///
    /// Distinct from the actor's own methods of the same names, which run on
    /// the actor thread against the live connection.
    pub fn documents(&self) -> DocumentApi {
        DocumentApi::new(Arc::clone(&self.handle))
    }
    /// Read project metadata without a renderer lease so an unknown metadata
    /// commit can be reconciled even when the project has no documents.
    /// Narrow interface to project-level lifecycle and inspection: metadata,
    /// the two renames, and the storage report. See [`ProjectApi`].
    ///
    /// Distinct from the actor's own methods of the same names, which run on the
    /// actor thread against the live connection.
    pub fn project(&self) -> ProjectApi {
        ProjectApi::new(Arc::clone(&self.handle))
    }
    /// Narrow interface to the context-freshness concern. See [`ContextApi`].
    ///
    /// Distinct from the actor's own `context_source_epoch`, which runs on the
    /// actor thread against the live connection. This one asks the actor over
    /// the channel.
    pub fn context(&self) -> ContextApi {
        ContextApi::new(Arc::clone(&self.handle))
    }
    /// Narrow interface to the Workshop concern: six methods instead of 28.
    ///
    /// The returned handle shares this session's actor and command channel, so
    /// there is no second connection and no ordering change. See [`WorkshopApi`].
    pub fn workshop(&self) -> WorkshopApi {
        WorkshopApi::new(Arc::clone(&self.handle))
    }
    /// Inspect active discussion and memory work owned by this project's
    /// current operation namespace. The actor's current renderer access is
    /// used internally; callers cannot supply or rotate a lease for this
    /// inspection.
    /// Narrow interface to the background-work concern: three methods that read
    /// the active-work census, persist a stop intent, or interrupt. See
    /// [`WorkApi`].
    pub fn work(&self) -> WorkApi {
        WorkApi::new(Arc::clone(&self.handle))
    }
}

