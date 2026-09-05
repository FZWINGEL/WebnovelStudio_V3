//! One locked project, one owned SQLite connection, and explicit renderer leases.
use crate::documents::Endpoint;
use crate::{sha256_hex, storage, validate_snapshot_json};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use uuid::Uuid;

pub mod context_packets;
mod conversation_context;
pub mod discussions;
pub mod exports;
pub mod guidance;
pub mod history;
pub mod import;
pub mod proposals;
pub mod reviewed_story;
pub mod source_pins;
pub mod story_context;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreError {
    pub code: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_head: Option<Head>,
}
impl CoreError {
    pub fn new(code: &str, detail: &str) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
            current_head: None,
        }
    }
    pub(crate) fn uncertain(error: rusqlite::Error) -> Self {
        Self::new(
            "UncertainOutcome",
            &format!("The commit outcome must be reconciled: {error}"),
        )
    }
    fn disconnected() -> Self {
        Self::new(
            "UncertainOutcome",
            "The project connection stopped. Keep your text and reopen the project to reconcile.",
        )
    }
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for CoreError {}
impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new("PersistenceUnavailable", &error.to_string())
    }
}
impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        Self::new("PersistenceUnavailable", &error.to_string())
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("InvalidDocument", &error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAccess {
    pub project_id: String,
    pub session: String,
    pub writer_lease: String,
    pub operation_namespace: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Head {
    pub document_id: String,
    pub version: String,
    pub body_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectInfo {
    pub project_id: String,
    pub operation_namespace: String,
    pub title: String,
    pub format_version: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMetadata {
    pub project: ProjectInfo,
    pub metadata_version: String,
}
/// Identity of the library operation that installed this independent folder.
/// Kept beside the database so registry recovery does not require a schema upgrade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreationOrigin {
    pub operation_namespace: String,
    pub operation_id: String,
}
pub fn write_creation_origin(path: &Path, origin: &CreationOrigin) -> CoreResult<()> {
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    let mut file = File::create_new(path.join("creation.json"))?;
    file.write_all(&serde_json::to_vec(origin)?)?;
    file.sync_all()?;
    Ok(())
}
pub fn read_creation_origin(path: &Path) -> CoreResult<CreationOrigin> {
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(path.join("creation.json"))?
        .take(4097)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(CoreError::new(
            "InvalidProject",
            "Invalid project creation record.",
        ));
    }
    let origin: CreationOrigin = serde_json::from_slice(&bytes)?;
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    Ok(origin)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewState {
    pub document_id: String,
    pub head: Head,
    pub anchor: Endpoint,
    pub focus: Endpoint,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentRecord {
    pub head: Head,
    pub title: String,
    pub kind: String,
    pub metadata_version: String,
    pub body: Value,
    pub last_checkpoint_id: Option<String>,
}
#[derive(Debug)]
pub struct AttachedProject {
    pub metadata: ProjectMetadata,
    pub access: ProjectAccess,
    pub documents: Vec<DocumentRecord>,
    pub view_state: Option<ViewState>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDocument {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub document_id: String,
    pub title: String,
    pub kind: String,
    pub body: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSnapshot {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub local_generation: String,
    pub body: Value,
    pub cause: SaveCause,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SaveCause {
    Typing,
    Undo,
    Redo,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAck {
    pub project_id: String,
    pub document_id: String,
    pub session: String,
    pub operation_namespace: String,
    pub operation_id: String,
    pub head: Head,
    pub saved_generation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconcileRequest {
    pub project_id: String,
    pub operation_namespace: String,
    pub session: String,
    pub document_id: String,
    pub pending_operation_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationReceipt {
    pub operation_id: String,
    pub operation_kind: String,
    pub payload_hash: String,
    pub result: StoredResult,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredResult {
    pub head: Head,
    pub saved_generation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<proposals::AppliedDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored: Option<history::RestoredDecision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconciledDocument {
    pub access: ProjectAccess,
    pub document: DocumentRecord,
    pub receipts: Vec<OperationReceipt>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointRequest {
    pub access: ProjectAccess,
    pub expected: Head,
    pub reason: CheckpointReason,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckpointReason {
    Manual,
    Switch,
    Close,
    Source,
    Export,
    Interval,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub head: Head,
    pub body: Value,
    pub reason: String,
    pub parent_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: i64,
    pub sqlite_version: String,
    pub sqlite_source_id: String,
    pub compile_options: Vec<String>,
}

type Reply<T> = mpsc::SyncSender<CoreResult<T>>;
enum Command {
    Packet(Box<context_packets::PacketCommand>),
    Context(Box<story_context::ContextCommand>),
    Discussion(Box<discussions::DiscussionCommand>),
    Guidance(Box<guidance::GuidanceCommand>),
    History(Box<history::HistoryCommand>),
    Proposal(Box<proposals::ProposalCommand>),
    Review(Box<reviewed_story::ReviewCommand>),
    Export(Box<exports::ExportCommand>),
    SourcePins(Box<source_pins::SourcePinCommand>),
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
struct Handle {
    queue: mpsc::SyncSender<Command>,
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
                            Command::Packet(command) => project.handle_packet(*command),
                            Command::Context(command) => project.handle_context(*command),
                            Command::Discussion(command) => project.handle_discussion(*command),
                            Command::Guidance(command) => project.handle_guidance(*command),
                            Command::History(command) => project.handle_history(*command),
                            Command::Proposal(command) => project.handle_proposal(*command),
                            Command::Review(command) => project.handle_review(*command),
                            Command::Export(command) => project.handle_export(*command),
                            Command::SourcePins(command) => project.handle_source_pins(*command),
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
    fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> CoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle
            .queue
            .send(command(sender))
            .map_err(|_| CoreError::disconnected())?;
        receiver.recv().map_err(|_| CoreError::disconnected())?
    }
    /// Host-owned attachment: every renderer creation begins with a fresh lease.
    pub fn attach(&self, session: String) -> CoreResult<ProjectAccess> {
        self.request(|r| Command::Attach(session, r))
    }
    /// Read destination state before replacing the current lease. Failed opens
    /// must not retire an editor whose manuscript remains on screen.
    pub fn attach_snapshot(&self, session: String) -> CoreResult<AttachedProject> {
        self.request(|r| Command::AttachSnapshot(session, r))
    }
    pub fn create_document(&self, request: CreateDocument) -> CoreResult<DocumentRecord> {
        self.request(|r| Command::Create(request, r))
    }
    pub fn documents(&self, access: ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.request(|r| Command::List(access, r))
    }
    pub fn document(&self, access: ProjectAccess, id: String) -> CoreResult<DocumentRecord> {
        self.request(|r| Command::Read(access, id, r))
    }
    pub fn save(&self, request: SaveSnapshot) -> CoreResult<SaveAck> {
        self.request(|r| Command::Save(request, r))
    }
    pub fn checkpoint(&self, request: CheckpointRequest) -> CoreResult<Revision> {
        self.request(|r| Command::Checkpoint(request, r))
    }
    pub fn history(&self, access: ProjectAccess, id: String) -> CoreResult<Vec<Revision>> {
        self.request(|r| Command::LegacyHistory(access, id, r))
    }
    pub fn reconcile(&self, request: ReconcileRequest) -> CoreResult<ReconciledDocument> {
        self.request(|r| Command::Reconcile(request, r))
    }
    /// Read project metadata without a renderer lease so an unknown metadata
    /// commit can be reconciled even when the project has no documents.
    pub fn project_metadata(&self) -> CoreResult<ProjectMetadata> {
        self.request(Command::ProjectMetadata)
    }
    pub fn rename_project(
        &self,
        access: ProjectAccess,
        expected_metadata_version: String,
        title: String,
    ) -> CoreResult<ProjectMetadata> {
        self.request(|r| Command::RenameProject(access, expected_metadata_version, title, r))
    }
    pub fn rename_document(
        &self,
        access: ProjectAccess,
        document_id: String,
        expected_metadata_version: String,
        title: String,
    ) -> CoreResult<DocumentRecord> {
        self.request(|r| {
            Command::RenameDocument(access, document_id, expected_metadata_version, title, r)
        })
    }
    pub fn view_state(&self, access: ProjectAccess) -> CoreResult<Option<ViewState>> {
        self.request(|r| Command::ViewState(access, r))
    }
    pub fn save_view_state(
        &self,
        access: ProjectAccess,
        head: Head,
        anchor: Endpoint,
        focus: Endpoint,
    ) -> CoreResult<ViewState> {
        self.request(|r| Command::SaveViewState(access, head, anchor, focus, r))
    }
    pub fn context_source_epoch(&self) -> CoreResult<String> {
        self.request(Command::ContextSourceEpoch)
    }
    pub fn storage_info(&self) -> CoreResult<StorageInfo> {
        self.request(Command::StorageInfo)
    }
}

fn install_project_directory(staging: &Path, destination: &Path) -> CoreResult<()> {
    // The existence check alone cannot prevent replacing a directory that
    // appears between the check and rename. Use the OS no-replace operation.
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        renameat_with(CWD, staging, CWD, destination, RenameFlags::NOREPLACE)
            .map_err(|e| CoreError::from(std::io::Error::from(e)))?;
        File::open(
            destination
                .parent()
                .ok_or_else(|| CoreError::new("InvalidRequest", "Missing destination parent."))?,
        )?
        .sync_all()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let wide = |path: &Path| -> CoreResult<Vec<u16>> {
            let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
            if units.contains(&0) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A project path cannot contain NUL.",
                ));
            }
            units.push(0);
            Ok(units)
        };
        let from = wide(staging)?;
        let to = wide(destination)?;
        // SAFETY: both buffers are owned, NUL-terminated UTF-16 and remain live
        // for this synchronous call. REPLACE_EXISTING is deliberately absent.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (staging, destination);
        Err(CoreError::new(
            "UnsupportedPlatform",
            "Safe project installation has not been qualified on this platform.",
        ))
    }
}

#[test]
fn installation_refuses_an_empty_directory_that_appeared_late() {
    let root = std::env::temp_dir().join(format!("wns-install-test-{}", new_id()));
    std::fs::create_dir(&root).unwrap();
    let staging = root.join("staging");
    let destination = root.join("existing");
    std::fs::create_dir(&staging).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(staging.join("prose.txt"), "retained").unwrap();
    assert!(install_project_directory(&staging, &destination).is_err());
    assert!(staging.join("prose.txt").exists());
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
    assert!(
        root.starts_with(std::env::temp_dir())
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wns-install-test-")
    );
    std::fs::remove_dir_all(root).unwrap();
}

struct OwnedProject {
    connection: Option<Connection>,
    // Dropped only when the owned connection thread exits, never when a UI changes projects.
    _lock: File,
    path: PathBuf,
    info: ProjectInfo,
    access: Option<ProjectAccess>,
    renderer_session: Option<String>,
    retired_sessions: HashSet<String>,
    needs_reopen: bool,
}
impl OwnedProject {
    fn db(&self) -> CoreResult<&Connection> {
        self.connection.as_ref().ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The project connection needs recovery.",
            )
        })
    }
    fn db_mut(&mut self) -> CoreResult<&mut Connection> {
        self.connection.as_mut().ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The project connection needs recovery.",
            )
        })
    }
    fn fence_uncertain<T>(&mut self, result: &CoreResult<T>) {
        if result.as_ref().is_err_and(|e| e.code == "UncertainOutcome") {
            self.access = None;
            self.needs_reopen = true;
        }
    }
    fn recover_connection(&mut self) -> CoreResult<()> {
        if !self.needs_reopen {
            return Ok(());
        }
        if let Some(connection) = self.connection.take()
            && let Err((connection, error)) = connection.close()
        {
            self.connection = Some(connection);
            return Err(CoreError::new(
                "PersistenceUnavailable",
                &format!("The project connection could not be closed safely: {error}"),
            ));
        }
        let connection = Connection::open_with_flags(
            self.path.join("project.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        storage::configure(&connection)?;
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        let identity: (String, String) = connection.query_row(
            "SELECT id,operation_namespace FROM project WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if integrity != "ok"
            || identity
                != (
                    self.info.project_id.clone(),
                    self.info.operation_namespace.clone(),
                )
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The recovered database failed its integrity or identity check.",
            ));
        }
        self.connection = Some(connection);
        self.needs_reopen = false;
        Ok(())
    }
    fn open_direct(path: PathBuf, title: Option<String>) -> CoreResult<Self> {
        if title.is_some() {
            std::fs::create_dir(&path)?;
        }
        let path = std::fs::canonicalize(path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(".writer.lock"))?;
        lock.try_lock().map_err(|_| {
            CoreError::new(
                "ProjectAlreadyOpen",
                "This project is already open for writing.",
            )
        })?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | if title.is_some() {
                OpenFlags::SQLITE_OPEN_CREATE
            } else {
                OpenFlags::empty()
            };
        let mut connection = Connection::open_with_flags(path.join("project.sqlite3"), flags)?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > storage::LATEST_SCHEMA_VERSION || (title.is_none() && version == 0) {
            return Err(CoreError::new(
                "UnsupportedSchema",
                "This project format is not supported.",
            ));
        }
        storage::migrate(&mut connection, &path)?;
        storage::configure(&connection)?;
        let info = if let Some(title) = title {
            let info = ProjectInfo {
                project_id: new_id(),
                operation_namespace: new_id(),
                title,
                format_version: 1,
            };
            connection.execute("INSERT INTO project(singleton,id,operation_namespace,title,format_version) VALUES(1,?,?,?,1)", params![info.project_id, info.operation_namespace, info.title])?;
            #[cfg(test)]
            if info.title == "unit-fail-project-marker" {
                std::fs::create_dir(path.join("project.wns.json"))?;
            }
            let mut marker = File::create_new(path.join("project.wns.json"))?;
            marker.write_all(serde_json::to_string_pretty(&info)?.as_bytes())?;
            marker.sync_all()?;
            info
        } else {
            let marker = std::fs::read(path.join("project.wns.json"))?;
            if marker.len() > 16_384 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The project marker is too large.",
                ));
            }
            let marker: ProjectInfo = serde_json::from_slice(&marker)?;
            let actual = connection.query_row(
                "SELECT id,operation_namespace,title,format_version FROM project WHERE singleton=1",
                [],
                |r| {
                    Ok(ProjectInfo {
                        project_id: r.get(0)?,
                        operation_namespace: r.get(1)?,
                        title: r.get(2)?,
                        format_version: r.get(3)?,
                    })
                },
            )?;
            if marker.project_id != actual.project_id
                || marker.operation_namespace != actual.operation_namespace
                || marker.format_version != actual.format_version
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The folder marker and database identify different projects.",
                ));
            }
            actual
        };
        let mut project = Self {
            connection: Some(connection),
            _lock: lock,
            path,
            info,
            access: None,
            renderer_session: None,
            retired_sessions: HashSet::new(),
            needs_reopen: false,
        };
        project.recover_interrupted_discussions()?;
        Ok(project)
    }
    fn attach(&mut self, session: String) -> CoreResult<ProjectAccess> {
        check_id(&session)?;
        if self.retired_sessions.contains(&session) {
            return Err(CoreError::new(
                "WriterLeaseExpired",
                "This renderer was replaced and cannot reacquire a writer lease.",
            ));
        }
        if self.needs_reopen {
            return Err(CoreError::new(
                "UncertainOutcome",
                "Reconcile the uncertain operation before attaching a writer.",
            ));
        }
        if self.renderer_session.as_deref() != Some(session.as_str())
            && let Some(old) = self.renderer_session.replace(session.clone())
        {
            self.retired_sessions.insert(old);
        }
        let access = ProjectAccess {
            project_id: self.info.project_id.clone(),
            operation_namespace: self.info.operation_namespace.clone(),
            session,
            writer_lease: new_id(),
        };
        self.access = Some(access.clone());
        Ok(access)
    }
    fn attach_snapshot(&mut self, session: String) -> CoreResult<AttachedProject> {
        check_id(&session)?;
        self.recover_connection()?;
        let metadata = self.project_metadata()?;
        let documents = self.document_records()?;
        let view_state = self.current_view_state()?;
        let access = self.attach(session)?;
        Ok(AttachedProject {
            metadata,
            access,
            documents,
            view_state,
        })
    }
    fn check_access(&self, access: &ProjectAccess) -> CoreResult<()> {
        if access.project_id != self.info.project_id
            || access.operation_namespace != self.info.operation_namespace
        {
            return Err(CoreError::new(
                "WrongProjectSession",
                "This command belongs to another project.",
            ));
        }
        let current = self.access.as_ref().ok_or_else(|| {
            CoreError::new(
                "WrongProjectSession",
                "Attach a document session before editing.",
            )
        })?;
        if current.session != access.session || current.writer_lease != access.writer_lease {
            return Err(CoreError::new(
                "WriterLeaseExpired",
                "This editing session is no longer current. Reconcile before writing.",
            ));
        }
        Ok(())
    }
    fn project_metadata(&self) -> CoreResult<ProjectMetadata> {
        let row: (String, String, String, u32, i64) = self.db()?.query_row(
            "SELECT id,operation_namespace,title,format_version,metadata_version FROM project WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;
        Ok(ProjectMetadata {
            project: ProjectInfo {
                project_id: row.0,
                operation_namespace: row.1,
                title: row.2,
                format_version: row.3,
            },
            metadata_version: parse_stored_version(row.4)?,
        })
    }
    fn rename_project(
        &mut self,
        access: ProjectAccess,
        expected_metadata_version: &str,
        title: &str,
    ) -> CoreResult<ProjectMetadata> {
        self.check_access(&access)?;
        let expected = parse_version(expected_metadata_version)?;
        validate_title(title)?;
        let current = self.project_metadata()?;
        let current_version = parse_version(&current.metadata_version)?;
        if current_version != expected {
            return Err(CoreError::new(
                "MetadataConflict",
                format!(
                    "The project metadata changed; expected version {expected_metadata_version}, current version {}.",
                    current.metadata_version
                )
                .as_str(),
            ));
        }
        if current.project.title == title {
            return Ok(current);
        }
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE project SET title=?,metadata_version=metadata_version+1,context_source_epoch=context_source_epoch+1 WHERE singleton=1 AND metadata_version=?",
            params![title, expected],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "MetadataConflict",
                "The project metadata changed before the rename committed.",
            ));
        }
        tx.commit().map_err(CoreError::uncertain)?;
        let metadata = self.project_metadata().map_err(|error| {
            CoreError::new(
                "UncertainOutcome",
                format!("The project title committed but metadata needs reconciliation: {error}")
                    .as_str(),
            )
        })?;
        if let Err(error) = write_project_marker(&self.path, &metadata.project) {
            return Err(CoreError::new(
                "UncertainOutcome",
                format!("The project title committed but its marker needs reconciliation: {error}")
                    .as_str(),
            ));
        }
        self.info.title = title.to_owned();
        Ok(metadata)
    }
    fn rename_document(
        &mut self,
        access: ProjectAccess,
        document_id: &str,
        expected_metadata_version: &str,
        title: &str,
    ) -> CoreResult<DocumentRecord> {
        self.check_access(&access)?;
        check_id(document_id)?;
        let expected = parse_version(expected_metadata_version)?;
        validate_title(title)?;
        let current = read_document(self.db()?, document_id)?;
        let current_version = parse_version(&current.metadata_version)?;
        if current_version != expected {
            return Err(CoreError::new(
                "MetadataConflict",
                format!(
                    "The document metadata changed; expected version {expected_metadata_version}, current version {}.",
                    current.metadata_version
                )
                .as_str(),
            ));
        }
        if current.title == title {
            return Ok(current);
        }
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE documents SET title=?,metadata_version=metadata_version+1 WHERE id=? AND metadata_version=?",
            params![title, document_id, expected],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "MetadataConflict",
                "The document metadata changed before the rename committed.",
            ));
        }
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        read_document(self.db()?, document_id).map_err(|error| {
            CoreError::new(
                "UncertainOutcome",
                format!("The document title committed but metadata needs reconciliation: {error}")
                    .as_str(),
            )
        })
    }
    fn view_state(&self, access: ProjectAccess) -> CoreResult<Option<ViewState>> {
        self.check_access(&access)?;
        self.current_view_state()
    }
    fn current_view_state(&self) -> CoreResult<Option<ViewState>> {
        let state = read_view_state(self.db()?)?;
        if let Some(state) = &state {
            let current = match read_document(self.db()?, &state.document_id) {
                Ok(current) => current,
                Err(error) if error.code == "DocumentNotFound" => return Ok(None),
                Err(error) => return Err(error),
            };
            if current.head == state.head {
                validate_endpoint(&current.body, &state.anchor)?;
                validate_endpoint(&current.body, &state.focus)?;
            }
        }
        Ok(state)
    }
    fn save_view_state(
        &mut self,
        access: ProjectAccess,
        head: Head,
        anchor: Endpoint,
        focus: Endpoint,
    ) -> CoreResult<ViewState> {
        self.check_access(&access)?;
        let current = read_document(self.db()?, &head.document_id)?;
        require_head(&current.head, &head)?;
        validate_endpoint(&current.body, &anchor)?;
        validate_endpoint(&current.body, &focus)?;
        let state = ViewState {
            document_id: head.document_id.clone(),
            head: head.clone(),
            anchor: anchor.clone(),
            focus: focus.clone(),
        };
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO view_state(singleton,document_id,head_version,head_body_hash,anchor_block_id,anchor_utf16_offset,focus_block_id,focus_utf16_offset) VALUES(1,?,?,?,?,?,?,?) ON CONFLICT(singleton) DO UPDATE SET document_id=excluded.document_id,head_version=excluded.head_version,head_body_hash=excluded.head_body_hash,anchor_block_id=excluded.anchor_block_id,anchor_utf16_offset=excluded.anchor_utf16_offset,focus_block_id=excluded.focus_block_id,focus_utf16_offset=excluded.focus_utf16_offset",
            params![
                state.document_id,
                parse_version(&head.version)?,
                head.body_hash,
                anchor.block_id,
                i64::from(anchor.utf16_offset),
                focus.block_id,
                i64::from(focus.utf16_offset),
            ],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(state)
    }
    fn context_source_epoch(&self) -> CoreResult<String> {
        let epoch: i64 = self.db()?.query_row(
            "SELECT context_source_epoch FROM project WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        parse_stored_version(epoch)
    }
    fn create_document(&mut self, request: CreateDocument) -> CoreResult<DocumentRecord> {
        self.check_access(&request.access)?;
        check_id(&request.document_id)?;
        check_id(&request.operation_id)?;
        validate_title(&request.title)?;
        if ![
            "chapter",
            "note",
            "character",
            "world",
            "theme",
            "hook",
            "scene",
        ]
        .contains(&request.kind.as_str())
        {
            return Err(CoreError::new(
                "InvalidDocument",
                "Choose a supported document kind.",
            ));
        }
        let validated = validate_snapshot_json(&serde_json::to_string(&request.body)?)
            .map_err(|e| CoreError::new("InvalidDocument", &e))?;
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = existing_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "createDocument",
            &payload,
        )? {
            let record = read_document(&tx, &result.head.document_id)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(record);
        }
        tx.execute("INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,?,?,(SELECT COUNT(*) FROM documents),0,1,?,?)", params![request.document_id, request.kind, request.title, validated.canonical_json, validated.hash])?;
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
        let head = Head {
            document_id: request.document_id.clone(),
            version: "0".into(),
            body_hash: validated.hash,
        };
        insert_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "createDocument",
            &payload,
            &StoredResult {
                head,
                saved_generation: "0".into(),
                applied: None,
                restored: None,
            },
        )?;
        let record = read_document(&tx, &request.document_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(record)
    }
    fn list(&self, access: ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.check_access(&access)?;
        self.document_records()
    }
    fn document_records(&self) -> CoreResult<Vec<DocumentRecord>> {
        let mut statement = self
            .db()?
            .prepare("SELECT id FROM documents WHERE trashed=0 ORDER BY position,id")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| read_document(self.db()?, id)).collect()
    }
    fn save(&mut self, request: SaveSnapshot) -> CoreResult<SaveAck> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        check_id(&request.expected.document_id)?;
        parse_version(&request.expected.version)?;
        parse_version(&request.local_generation)?;
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = if let Some(result) = existing_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "save",
            &payload,
        )? {
            result
        } else {
            let before = read_document(&tx, &request.expected.document_id)?;
            require_head(&before.head, &request.expected)?;
            let validated = validate_snapshot_json(&serde_json::to_string(&request.body)?)
                .map_err(|e| CoreError::new("InvalidDocument", &e))?;
            let mut head = before.head.clone();
            if validated.hash != head.body_hash {
                if request.cause != SaveCause::Typing {
                    checkpoint_at(&tx, &before, "beforeUndoRedo")?;
                }
                let next = parse_version(&head.version)?
                    .checked_add(1)
                    .ok_or_else(|| {
                        CoreError::new("VersionLimit", "The document version limit was reached.")
                    })?;
                let changed = tx.execute("UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1 WHERE id=? AND working_version=? AND body_hash=?", params![next, validated.canonical_json, validated.hash, head.document_id, parse_version(&head.version)?, head.body_hash])?;
                if changed != 1 {
                    return Err(CoreError::new(
                        "VersionConflict",
                        "The document changed before saving.",
                    ));
                }
                head.version = next.to_string();
                head.body_hash = validated.hash;
                tx.execute(
                    "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
                    [],
                )?;
                if request.cause != SaveCause::Typing {
                    checkpoint_at(
                        &tx,
                        &read_document(&tx, &head.document_id)?,
                        "afterUndoRedo",
                    )?;
                }
            }
            let result = StoredResult {
                head,
                saved_generation: request.local_generation.clone(),
                applied: None,
                restored: None,
            };
            insert_receipt(
                &tx,
                &request.access.operation_namespace,
                &request.operation_id,
                "save",
                &payload,
                &result,
            )?;
            result
        };
        tx.commit().map_err(CoreError::uncertain)?;
        #[cfg(test)]
        tests::hold_after_commit_before_ack(&request.operation_id);
        Ok(SaveAck {
            project_id: request.access.project_id,
            document_id: result.head.document_id.clone(),
            operation_namespace: request.access.operation_namespace,
            session: request.access.session,
            operation_id: request.operation_id,
            head: result.head,
            saved_generation: result.saved_generation,
        })
    }
    fn checkpoint(&mut self, request: CheckpointRequest) -> CoreResult<Revision> {
        self.check_access(&request.access)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = read_document(&tx, &request.expected.document_id)?;
        require_head(&record.head, &request.expected)?;
        let reason = serde_json::to_value(request.reason)?;
        let revision = checkpoint_at(&tx, &record, reason.as_str().unwrap_or("manual"))?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(revision)
    }
    fn history(&self, access: ProjectAccess, document_id: &str) -> CoreResult<Vec<Revision>> {
        self.check_access(&access)?;
        let mut statement = self.db()?.prepare(
            "SELECT id FROM revisions WHERE document_id=? ORDER BY source_working_version DESC",
        )?;
        let ids = statement
            .query_map([document_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| read_revision(self.db()?, id)).collect()
    }
    fn reconcile(&mut self, request: ReconcileRequest) -> CoreResult<ReconciledDocument> {
        if request.project_id != self.info.project_id
            || request.operation_namespace != self.info.operation_namespace
        {
            return Err(CoreError::new(
                "WrongProjectSession",
                "Reconciliation belongs to another project.",
            ));
        }
        check_id(&request.session)?;
        check_id(&request.document_id)?;
        if request.pending_operation_ids.len() > 64 {
            return Err(CoreError::new(
                "InvalidRequest",
                "Too many pending operations.",
            ));
        }
        for id in &request.pending_operation_ids {
            check_id(id)?;
        }
        // Fence first. Any command still queued with an old lease now fails on execution.
        self.recover_connection()?;
        let access = self.attach(request.session)?;
        let document = read_document(self.db()?, &request.document_id)?;
        let mut receipts = Vec::new();
        for id in request.pending_operation_ids {
            let row: Option<(String, String, String)> = self.db()?.query_row("SELECT operation_kind,payload_hash,result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=? AND document_id=?", params![self.info.operation_namespace, id, request.document_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if let Some((operation_kind, payload_hash, result)) = row {
                receipts.push(OperationReceipt {
                    operation_id: id,
                    operation_kind,
                    payload_hash,
                    result: serde_json::from_str(&result)?,
                });
            }
        }
        Ok(ReconciledDocument {
            access,
            document,
            receipts,
        })
    }
    fn storage_info(&self) -> CoreResult<StorageInfo> {
        let mut options = self.db()?.prepare("PRAGMA compile_options")?;
        let compile_options = options
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StorageInfo {
            journal_mode: self
                .db()?
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))?,
            synchronous: self
                .db()?
                .query_row("PRAGMA synchronous", [], |r| r.get(0))?,
            foreign_keys: self
                .db()?
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))?,
            sqlite_version: self
                .db()?
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))?,
            sqlite_source_id: self
                .db()?
                .query_row("SELECT sqlite_source_id()", [], |r| r.get(0))?,
            compile_options,
        })
    }
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}
fn check_id(id: &str) -> CoreResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Identifiers must contain 1–64 ASCII letters, digits, dashes or underscores.",
        ));
    }
    Ok(())
}
fn validate_title(title: &str) -> CoreResult<()> {
    if title.trim().is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
        return Err(CoreError::new(
            "InvalidRequest",
            "Enter a title of at most 512 bytes without control characters.",
        ));
    }
    Ok(())
}
fn write_project_marker(path: &Path, info: &ProjectInfo) -> CoreResult<()> {
    let marker = path.join("project.wns.json");
    let temporary = path.join(format!(".project.wns.json-{}", new_id()));
    let result = (|| -> CoreResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(info)?)?;
        file.sync_all()?;
        drop(file);

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            };
            let wide = |value: &Path| -> CoreResult<Vec<u16>> {
                let mut units: Vec<u16> = value.as_os_str().encode_wide().collect();
                if units.contains(&0) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A project path cannot contain NUL.",
                    ));
                }
                units.push(0);
                Ok(units)
            };
            let from = wide(&temporary)?;
            let to = wide(&marker)?;
            // SAFETY: both buffers are owned, NUL-terminated UTF-16 and stay
            // live for this synchronous call.
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(windows))]
        std::fs::rename(&temporary, &marker)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
pub fn blank_document() -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":new_id()}}]}})
}
fn parse_version(value: &str) -> CoreResult<i64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Versions must be canonical nonnegative decimal strings.",
        ));
    }
    value
        .parse()
        .map_err(|_| CoreError::new("InvalidRequest", "Version exceeds the supported range."))
}
fn parse_stored_version(value: i64) -> CoreResult<String> {
    if value < 0 {
        return Err(CoreError::new(
            "InvalidProject",
            "The project contains a negative metadata version.",
        ));
    }
    Ok(value.to_string())
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub(crate) fn read_view_state(connection: &Connection) -> CoreResult<Option<ViewState>> {
    let row: Option<(String, i64, String, String, i64, String, i64)> = connection
        .query_row(
            "SELECT document_id,head_version,head_body_hash,anchor_block_id,anchor_utf16_offset,focus_block_id,focus_utf16_offset FROM view_state WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((document_id, version, hash, anchor_block, anchor_offset, focus_block, focus_offset)) =
        row
    else {
        return Ok(None);
    };
    check_id(&document_id)?;
    let version = parse_stored_version(version)?;
    if !valid_hash(&hash) || anchor_offset < 0 || focus_offset < 0 {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved view state contains an invalid head or endpoint.",
        ));
    }
    let anchor_offset = u32::try_from(anchor_offset).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved view anchor exceeds the supported UTF-16 range.",
        )
    })?;
    let focus_offset = u32::try_from(focus_offset).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved view focus exceeds the supported UTF-16 range.",
        )
    })?;
    check_id(&anchor_block)?;
    check_id(&focus_block)?;
    Ok(Some(ViewState {
        document_id: document_id.clone(),
        head: Head {
            document_id,
            version,
            body_hash: hash,
        },
        anchor: Endpoint {
            block_id: anchor_block,
            utf16_offset: anchor_offset,
        },
        focus: Endpoint {
            block_id: focus_block,
            utf16_offset: focus_offset,
        },
    }))
}
pub(crate) fn validate_stored_view_state(connection: &Connection) -> CoreResult<()> {
    let Some(state) = read_view_state(connection)? else {
        return Ok(());
    };
    let current = match read_document(connection, &state.document_id) {
        Ok(current) => current,
        Err(error) if error.code == "DocumentNotFound" => return Ok(()),
        Err(error) => return Err(error),
    };
    if current.head == state.head {
        validate_endpoint(&current.body, &state.anchor)?;
        validate_endpoint(&current.body, &state.focus)?;
    }
    Ok(())
}
fn validate_endpoint(body: &Value, endpoint: &Endpoint) -> CoreResult<()> {
    check_id(&endpoint.block_id)?;
    let blocks = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::new("InvalidDocument", "The document has no block content."))?;
    let block = blocks.iter().find(|block| {
        block
            .get("attrs")
            .and_then(Value::as_object)
            .and_then(|attrs| attrs.get("id"))
            .and_then(Value::as_str)
            == Some(endpoint.block_id.as_str())
    });
    let block = block.ok_or_else(|| {
        CoreError::new(
            "InvalidRequest",
            "The saved view endpoint refers to an unknown block.",
        )
    })?;
    let block_type = block
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if block_type == "sceneBreak" && endpoint.utf16_offset == 0 {
        return Ok(());
    }
    if !matches!(block_type, "paragraph" | "heading") {
        return Err(CoreError::new(
            "InvalidRequest",
            "View endpoints must be inside a paragraph or heading.",
        ));
    }
    let content = block
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut offset = 0_u32;
    let mut boundaries = HashSet::from([0_u32]);
    for inline in content {
        match inline.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = inline
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| CoreError::new("InvalidDocument", "A text node has no text."))?;
                for (byte_offset, _) in
                    unicode_segmentation::UnicodeSegmentation::grapheme_indices(text, true)
                {
                    let units = u32::try_from(text[..byte_offset].encode_utf16().count())
                        .map_err(|_| CoreError::new("InvalidDocument", "Text is too long."))?;
                    boundaries.insert(offset + units);
                }
                offset = offset
                    .checked_add(
                        u32::try_from(text.encode_utf16().count())
                            .map_err(|_| CoreError::new("InvalidDocument", "Text is too long."))?,
                    )
                    .ok_or_else(|| CoreError::new("InvalidDocument", "Text is too long."))?;
                boundaries.insert(offset);
            }
            Some("hardBreak") => {
                offset = offset
                    .checked_add(1)
                    .ok_or_else(|| CoreError::new("InvalidDocument", "Text is too long."))?;
                boundaries.insert(offset);
            }
            _ => {
                return Err(CoreError::new(
                    "InvalidDocument",
                    "The document contains an unsupported inline node.",
                ));
            }
        }
    }
    if !boundaries.contains(&endpoint.utf16_offset) {
        return Err(CoreError::new(
            "InvalidRequest",
            "The saved view endpoint is not at a grapheme boundary.",
        ));
    }
    Ok(())
}
fn logical_hash<T: Serialize>(request: &T) -> CoreResult<String> {
    let mut value = serde_json::to_value(request)?;
    if let Some(access) = value.get_mut("access").and_then(Value::as_object_mut) {
        access.remove("session");
        access.remove("writerLease");
    }
    Ok(sha256_hex(
        serde_json::to_string(&crate::canonicalize_value(value))?.as_bytes(),
    ))
}
#[test]
fn save_receipt_matches_shared_literal_fixture() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/w2_save_receipt.json"
    ))
    .unwrap();
    let request: SaveSnapshot = serde_json::from_value(fixture["request"].clone()).unwrap();
    assert_eq!(
        logical_hash(&request).unwrap(),
        fixture["payloadHash"].as_str().unwrap()
    );
    let mut logical = serde_json::to_value(request).unwrap();
    logical["access"].as_object_mut().unwrap().remove("session");
    logical["access"]
        .as_object_mut()
        .unwrap()
        .remove("writerLease");
    assert_eq!(
        serde_json::to_string(&crate::canonicalize_value(logical)).unwrap(),
        fixture["logicalJson"].as_str().unwrap()
    );
}
fn require_head(current: &Head, expected: &Head) -> CoreResult<()> {
    parse_version(&expected.version)?;
    if current != expected {
        let mut error = CoreError::new(
            "VersionConflict",
            "This document has a newer saved version. Keep your text and reconcile.",
        );
        error.current_head = Some(current.clone());
        return Err(error);
    }
    Ok(())
}
type DocumentRow = (String, String, i64, i64, String, String, Option<String>);
fn read_document(connection: &Connection, id: &str) -> CoreResult<DocumentRecord> {
    let row: Option<DocumentRow> = connection.query_row("SELECT title,kind,working_version,metadata_version,body_hash,body_json,last_checkpoint_id FROM documents WHERE id=? AND trashed=0", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
    let (title, kind, version, metadata_version, hash, body, checkpoint) =
        row.ok_or_else(|| {
            CoreError::new(
                "DocumentNotFound",
                "This document is not available in this project.",
            )
        })?;
    let valid = validate_snapshot_json(&body).map_err(|e| CoreError::new("InvalidDocument", &e))?;
    if valid.hash != hash || valid.canonical_json != body {
        return Err(CoreError::new(
            "InvalidDocument",
            "The saved document failed its fingerprint check.",
        ));
    }
    Ok(DocumentRecord {
        head: Head {
            document_id: id.into(),
            version: version.to_string(),
            body_hash: hash,
        },
        title,
        kind,
        metadata_version: parse_stored_version(metadata_version)?,
        body: valid.snapshot,
        last_checkpoint_id: checkpoint,
    })
}
fn read_revision(connection: &Connection, id: &str) -> CoreResult<Revision> {
    let (doc,version,body,hash,reason,parent): (String,i64,String,String,String,Option<String>) = connection.query_row("SELECT document_id,source_working_version,body_json,body_hash,reason,parent_id FROM revisions WHERE id=?", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let valid = validate_snapshot_json(&body).map_err(|e| CoreError::new("InvalidDocument", &e))?;
    if valid.hash != hash || valid.canonical_json != body {
        return Err(CoreError::new(
            "InvalidDocument",
            "The revision failed its fingerprint check.",
        ));
    }
    Ok(Revision {
        id: id.into(),
        head: Head {
            document_id: doc,
            version: version.to_string(),
            body_hash: hash,
        },
        body: valid.snapshot,
        reason,
        parent_id: parent,
    })
}
fn checkpoint_at(
    connection: &Connection,
    document: &DocumentRecord,
    reason: &str,
) -> CoreResult<Revision> {
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM revisions WHERE document_id=? AND source_working_version=?",
            params![
                document.head.document_id,
                parse_version(&document.head.version)?
            ],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return read_revision(connection, &id);
    }
    let id = new_id();
    connection.execute("INSERT INTO revisions(id,document_id,source_working_version,schema_version,body_json,body_hash,parent_id,reason) VALUES(?,?,?,1,?,?,?,?)", params![id, document.head.document_id, parse_version(&document.head.version)?, serde_json::to_string(&document.body)?, document.head.body_hash, document.last_checkpoint_id, reason])?;
    connection.execute(
        "UPDATE documents SET last_checkpoint_id=? WHERE id=?",
        params![id, document.head.document_id],
    )?;
    read_revision(connection, &id)
}
fn existing_receipt(
    connection: &Connection,
    namespace: &str,
    id: &str,
    kind: &str,
    payload: &str,
) -> CoreResult<Option<StoredResult>> {
    let proposal_receipt: Option<(String, String)> = connection
        .query_row(
            "SELECT kind,payload_hash FROM proposal_receipts \
             WHERE operation_namespace=? AND operation_id=?",
            params![namespace, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if proposal_receipt.is_some() {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for a proposal command.",
        ));
    }
    let found: Option<(String, String, String)> = connection.query_row("SELECT operation_kind,payload_hash,result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?", params![namespace, id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    found
        .map(|(stored_kind, stored_payload, result)| {
            if stored_kind != kind || stored_payload != payload {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This operation ID was already used for a different request.",
                ));
            }
            Ok(serde_json::from_str(&result)?)
        })
        .transpose()
}
fn insert_receipt(
    connection: &Connection,
    namespace: &str,
    id: &str,
    kind: &str,
    payload: &str,
    result: &StoredResult,
) -> CoreResult<()> {
    connection.execute("INSERT INTO command_receipts(operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json) VALUES(?,?,?,?,?,?)", params![namespace, id, result.head.document_id, payload, kind, serde_json::to_string(result)?])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // Compiled only into the Rust unit-test binary, never the desktop/core library.
    pub(super) fn hold_after_commit_before_ack(operation_id: &str) {
        if matches!(operation_id, "crash-save" | "crash-apply" | "crash-restore")
            && let Some(root) = std::env::var_os("WNS_UNIT_CRASH_ROOT")
        {
            let mut marker = File::create_new(PathBuf::from(root).join("committed")).unwrap();
            marker
                .write_all(
                    b"COMMIT returned successfully; no document acknowledgment has been sent",
                )
                .unwrap();
            marker.sync_all().unwrap();
            loop {
                std::thread::park();
            }
        }
    }

    #[test]
    #[ignore = "Subprocess target for kill_after_commit_before_ack_recovers_once"]
    fn crash_child() {
        let root = PathBuf::from(std::env::var_os("WNS_UNIT_CRASH_ROOT").unwrap());
        let project = ProjectSession::open(root.join("project")).unwrap();
        let access = project.attach("crash-child".into()).unwrap();
        let json = std::fs::read(root.join("request.json")).unwrap();
        let value: Value = serde_json::from_slice(&json).unwrap();
        if value.get("preparedId").is_some() {
            let mut request: proposals::ApplyProposal = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.apply_proposal(request).unwrap();
        } else if value.get("revisionId").is_some() {
            let mut request: history::RestoreRevision = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.restore_revision(request).unwrap();
        } else {
            let mut request: SaveSnapshot = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.save(request).unwrap();
        }
        panic!("The deterministic after-commit barrier did not hold");
    }

    #[test]
    fn kill_after_commit_before_ack_recovers_once() {
        let root = std::env::temp_dir().join(format!("wns-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project =
            ProjectSession::create(root.join("project"), "Crash recovery fixture").unwrap();
        let access = project.attach("parent".into()).unwrap();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "A chapter".into(),
                kind: "chapter".into(),
                body: blank_document(),
            })
            .unwrap();
        let mut changed = document.body.clone();
        changed["body"]["content"][0]["content"] =
            json!([{"type":"text","text":"This prose survived the lost acknowledgment. 👩‍🚀"}]);
        let mut request = SaveSnapshot {
            access,
            operation_id: "crash-save".into(),
            expected: document.head,
            local_generation: "9".into(),
            body: changed,
            cause: SaveCause::Typing,
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-save".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.head.version, "1");
        assert_eq!(snapshot.document.body, request.body);
        assert_eq!(snapshot.receipts.len(), 1);
        request.access = snapshot.access.clone();
        let replay = recovered.save(request).unwrap();
        assert_eq!(replay.head, snapshot.document.head);
        assert_eq!(replay.saved_generation, "9");
        assert_eq!(replay.session, "new-renderer");
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killed_apply_after_commit_recovers_decision_and_checkpoint_once() {
        use crate::context::packet::MockContextBudget;
        use crate::documents::{Endpoint, ScopeKind};
        use discussions::*;
        use proposals::*;
        let root = std::env::temp_dir().join(format!("wns-apply-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("project"), "Apply crash fixture").unwrap();
        let access = project.attach("parent".into()).unwrap();
        let body = |text: &str| json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":text}]}]}});
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body("original"),
            })
            .unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                access: access.clone(),
                operation_id: "feedback".into(),
                expected: document.head.clone(),
                instruction: "Revise this passage.".into(),
                intent: FeedbackIntent::ProposeEdits,
                scope: Some(DiscussionScopeInput {
                    kind: ScopeKind::Passage,
                    start: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 0,
                    }),
                    end: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 8,
                    }),
                    quote: "original".into(),
                    source_body_hash: document.head.body_hash.clone(),
                }),
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
            })
            .unwrap();
        project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone(),
            })
            .unwrap();
        project
            .mark_discussion_delivered(started.run.owner.clone())
            .unwrap();
        project
            .finish_discussion(DiscussionFinish {
                owner: started.run.owner,
                expected_sequence: "0".into(),
                event_id: "terminal".into(),
                assistant_text: serde_json::to_string(&ProposalOutput {
                    suggestions: vec![ProposalCandidate {
                        title: "Alternative".into(),
                        replacement_text: "survived".into(),
                        explanation: "Crash test.".into(),
                    }],
                })
                .unwrap(),
            })
            .unwrap();
        let proposal = project
            .proposals(access.clone(), "chapter".into())
            .unwrap()
            .remove(0);
        let prepared = project
            .prepare_proposal(PrepareProposal {
                access: access.clone(),
                operation_id: "prepare".into(),
                proposal_id: proposal.id.clone(),
                expected_prepared_version: "0".into(),
                replacement_text: "survived".into(),
                body: body("survived"),
            })
            .unwrap();
        let mut request = ApplyProposal {
            access,
            operation_id: "crash-apply".into(),
            proposal_id: proposal.id,
            prepared_id: prepared.id,
            expected: document.head,
            result_hash: prepared.body_hash,
            local_generation: "1".into(),
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before Apply COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the Apply commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-apply".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.body, body("survived"));
        assert_eq!(snapshot.document.head.version, "1");
        assert_eq!(snapshot.receipts.len(), 1);
        assert!(snapshot.receipts[0].result.applied.is_some());
        request.access = snapshot.access.clone();
        let repeated = recovered.apply_proposal(request).unwrap();
        assert!(repeated.already_applied);
        assert_eq!(repeated.document.head, snapshot.document.head);
        assert_eq!(
            recovered
                .history(snapshot.access.clone(), "chapter".into())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            recovered
                .proposals(snapshot.access, "chapter".into())
                .unwrap()[0]
                .decision
                .as_ref()
                .unwrap()
                .kind,
            "apply"
        );
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killed_restore_after_commit_recovers_historical_result_once() {
        let root = std::env::temp_dir().join(format!("wns-restore-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project =
            ProjectSession::create(root.join("project"), "Restore crash fixture").unwrap();
        let access = project.attach("parent".into()).unwrap();
        let initial_body = blank_document();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: initial_body.clone(),
            })
            .unwrap();
        let source = project
            .checkpoint(CheckpointRequest {
                access: access.clone(),
                expected: document.head.clone(),
                reason: CheckpointReason::Manual,
            })
            .unwrap();
        let current = project
            .save(SaveSnapshot {
                access: access.clone(),
                operation_id: "restore-current".into(),
                expected: document.head,
                local_generation: "11".into(),
                body: json!({
                    "schemaVersion": 1,
                    "body": {"type": "doc", "content": [{
                        "type": "paragraph",
                        "attrs": {"id": "p"},
                        "content": [{"type": "text", "text": "later edit"}]
                    }]}
                }),
                cause: SaveCause::Typing,
            })
            .unwrap();
        let mut request = history::RestoreRevision {
            access,
            operation_id: "crash-restore".into(),
            expected: current.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "12".into(),
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before Restore COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the Restore commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-restore".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.body, initial_body);
        assert_eq!(snapshot.document.head.version, "2");
        assert_eq!(snapshot.receipts.len(), 1);
        assert_eq!(snapshot.receipts[0].operation_kind, "restore");
        assert!(snapshot.receipts[0].result.restored.is_some());
        request.access = snapshot.access.clone();
        let replay = recovered.restore_revision(request).unwrap();
        assert!(replay.already_applied);
        assert_eq!(replay.result, snapshot.receipts[0].result);
        assert_eq!(replay.document.head, snapshot.document.head);
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_marker_write_leaves_only_unregistered_staging() {
        let root = std::env::temp_dir().join(format!("wns-staging-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let destination = root.join("project");
        assert!(ProjectSession::create(&destination, "unit-fail-project-marker").is_err());
        assert!(!destination.exists());
        let entries = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].starts_with(".wns-create-"));
        let project = ProjectSession::create(&destination, "Successful retry").unwrap();
        assert_eq!(project.info.title, "Successful retry");
        drop(project);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
