//! One locked project, one owned SQLite connection, and explicit renderer leases.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentRecord {
    pub head: Head,
    pub title: String,
    pub kind: String,
    pub body: Value,
    pub last_checkpoint_id: Option<String>,
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
    Attach(String, Reply<ProjectAccess>),
    Create(CreateDocument, Reply<DocumentRecord>),
    List(ProjectAccess, Reply<Vec<DocumentRecord>>),
    Read(ProjectAccess, String, Reply<DocumentRecord>),
    Save(SaveSnapshot, Reply<SaveAck>),
    Checkpoint(CheckpointRequest, Reply<Revision>),
    History(ProjectAccess, String, Reply<Vec<Revision>>),
    Reconcile(ReconcileRequest, Reply<ReconciledDocument>),
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
        validate_title(title)?;
        Self::start(path.as_ref().to_owned(), Some(title.to_owned()))
    }
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::start(path.as_ref().to_owned(), None)
    }
    fn start(path: PathBuf, title: Option<String>) -> CoreResult<Self> {
        let (queue, receiver) = mpsc::sync_channel(64);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("webnovel-project".into())
            .spawn(move || match OwnedProject::open(path, title) {
                Ok(mut project) => {
                    if ready_tx
                        .send(Ok((project.info.clone(), project.path.clone())))
                        .is_err()
                    {
                        return;
                    }
                    while let Ok(command) = receiver.recv() {
                        match command {
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
                            Command::History(access, id, reply) => {
                                let _ = reply.send(project.history(access, &id));
                            }
                            Command::Reconcile(request, reply) => {
                                let _ = reply.send(project.reconcile(request));
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
        self.request(|r| Command::History(access, id, r))
    }
    pub fn reconcile(&self, request: ReconcileRequest) -> CoreResult<ReconciledDocument> {
        self.request(|r| Command::Reconcile(request, r))
    }
    pub fn storage_info(&self) -> CoreResult<StorageInfo> {
        self.request(Command::StorageInfo)
    }
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
    fn open(path: PathBuf, title: Option<String>) -> CoreResult<Self> {
        let Some(title) = title else {
            return Self::open_direct(path, None);
        };
        if path.try_exists()? {
            return Err(CoreError::new(
                "ProjectExists",
                "Choose a new folder for this project.",
            ));
        }
        let parent = std::fs::canonicalize(
            path.parent()
                .ok_or_else(|| CoreError::new("InvalidRequest", "Choose a project folder."))?,
        )?;
        let name = path
            .file_name()
            .ok_or_else(|| CoreError::new("InvalidRequest", "Choose a project folder."))?;
        let destination = parent.join(name);
        let staging = parent.join(format!(".wns-create-{}", new_id()));
        // A failed initialization remains unregistered staging, never a completed
        // destination. Do not delete recovery evidence on an uncertain write.
        let initialized = Self::open_direct(staging.clone(), Some(title))?;
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
        std::fs::rename(&staging, &destination)?;
        Self::open_direct(destination, None)
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
        if version > 1 || (title.is_none() && version == 0) {
            return Err(CoreError::new(
                "UnsupportedSchema",
                "This project format is not supported.",
            ));
        }
        storage::configure(&connection)?;
        storage::migrate(&mut connection)?;
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
        Ok(Self {
            connection: Some(connection),
            _lock: lock,
            path,
            info,
            access: None,
            renderer_session: None,
            retired_sessions: HashSet::new(),
            needs_reopen: false,
        })
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
            },
        )?;
        let record = read_document(&tx, &request.document_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(record)
    }
    fn list(&self, access: ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.check_access(&access)?;
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
fn logical_hash<T: Serialize>(request: &T) -> CoreResult<String> {
    let mut value = serde_json::to_value(request)?;
    if let Some(access) = value.get_mut("access").and_then(Value::as_object_mut) {
        access.remove("session");
        access.remove("writerLease");
    }
    Ok(sha256_hex(serde_json::to_string(&value)?.as_bytes()))
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
fn read_document(connection: &Connection, id: &str) -> CoreResult<DocumentRecord> {
    let row: Option<(String, String, i64, String, String, Option<String>)> = connection.query_row("SELECT title,kind,working_version,body_hash,body_json,last_checkpoint_id FROM documents WHERE id=? AND trashed=0", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let (title, kind, version, hash, body, checkpoint) = row.ok_or_else(|| {
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
        if operation_id == "crash-save"
            && let Some(root) = std::env::var_os("WNS_UNIT_CRASH_ROOT")
        {
            let mut marker = File::create_new(PathBuf::from(root).join("committed")).unwrap();
            marker
                .write_all(b"COMMIT returned successfully; no SaveAck has been sent")
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
        let mut request: SaveSnapshot =
            serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
        request.access = access;
        project.save(request).unwrap();
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
