//! Rebuildable app-local project index and narrowly scoped staged-file receipts.
//! Manuscripts remain in portable project folders, never in this database.
use crate::projects::{
    CoreError, CoreResult, CreationOrigin, ProjectSession, read_creation_origin,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub project_id: String,
    pub title: String,
    pub path: PathBuf,
    pub archived: bool,
    pub last_opened: String,
    pub missing: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProject {
    pub origin: CreationOrigin,
    pub kind: String,
    pub title: String,
    pub staging_path: PathBuf,
    pub final_path: PathBuf,
    pub source_path: Option<PathBuf>,
    pub source_fingerprint: Option<String>,
    pub completed: bool,
}
impl PendingProject {
    pub fn require_available(&self) -> CoreResult<()> {
        if self.completed && !self.final_path.try_exists()? {
            return Err(CoreError::new(
                "CompletedProjectMissing",
                "This operation already completed, but its project folder has moved or is unavailable. Locate the existing project; creating another copy requires a new operation.",
            ));
        }
        Ok(())
    }
}
pub struct Library {
    connection: Connection,
    _lock: File,
    pub root: PathBuf,
    namespace: String,
}
impl Library {
    pub fn begin_recovery(
        &mut self,
        operation_id: &str,
        title: &str,
        archive: &Path,
    ) -> CoreResult<PendingProject> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut file = File::open(archive)?;
        if file.metadata()?.len() > 512 * 1024 * 1024 {
            return Err(CoreError::new(
                "InvalidBackup",
                "This backup exceeds the supported size.",
            ));
        }
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        let fingerprint: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.begin(
            operation_id,
            "recover",
            title,
            Some((archive, &fingerprint)),
        )
    }
    pub fn open(root: impl AsRef<Path>) -> CoreResult<Self> {
        std::fs::create_dir_all(root.as_ref())?;
        let root = std::fs::canonicalize(root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".library.lock"))?;
        lock.try_lock().map_err(|_| {
            CoreError::new(
                "LibraryAlreadyOpen",
                "WebnovelStudio is already using this library.",
            )
        })?;
        let mut connection = Connection::open(root.join("library.sqlite3"))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(CoreError::new(
                "UnsupportedSchema",
                "This library needs a newer WebnovelStudio.",
            ));
        }
        crate::storage::configure(&connection)?;
        if version == 0 {
            let tx = connection.transaction()?;
            tx.execute_batch("CREATE TABLE identity (namespace TEXT NOT NULL) STRICT;
                CREATE TABLE entries (project_id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, path TEXT NOT NULL UNIQUE,
                    archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN(0,1)), last_opened TEXT NOT NULL) STRICT;
                CREATE TABLE operations (operation_id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL,
                    staging_path TEXT NOT NULL UNIQUE, final_path TEXT NOT NULL UNIQUE, source_path TEXT, source_fingerprint TEXT,
                    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1))) STRICT;
                PRAGMA user_version=1;")?;
            tx.execute(
                "INSERT INTO identity(namespace) VALUES(?)",
                [Uuid::new_v4().to_string()],
            )?;
            tx.commit().map_err(CoreError::uncertain)?;
        }
        let namespace = connection.query_row("SELECT namespace FROM identity", [], |r| r.get(0))?;
        std::fs::create_dir_all(root.join("Projects"))?;
        Ok(Self {
            connection,
            _lock: lock,
            root,
            namespace,
        })
    }
    pub fn list(&self) -> CoreResult<Vec<LibraryEntry>> {
        let mut statement = self.connection.prepare("SELECT project_id,title,path,archived,last_opened FROM entries ORDER BY last_opened DESC,title COLLATE NOCASE")?;
        let rows = statement.query_map([], |r| {
            let path = PathBuf::from(r.get::<_, String>(2)?);
            let missing = !path.join("project.wns.json").is_file();
            Ok(LibraryEntry {
                project_id: r.get(0)?,
                title: r.get(1)?,
                path,
                archived: r.get(3)?,
                last_opened: r.get(4)?,
                missing,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    /// Registration follows project installation/commit. Callers report a registry
    /// error as a library warning; they must not report a manuscript save failure.
    pub fn register(&mut self, project: &ProjectSession) -> CoreResult<()> {
        let metadata = project.project_metadata()?;
        let path = std::fs::canonicalize(&project.path)?;
        let previous: Option<String> = self
            .connection
            .query_row(
                "SELECT path FROM entries WHERE project_id=?",
                [&project.info.project_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            let previous = PathBuf::from(previous);
            if previous.try_exists()? && std::fs::canonicalize(previous)? != path {
                return Err(CoreError::new(
                    "DuplicateProjectIdentity",
                    "Another folder has this project identity. Open that project or create an independent copy.",
                ));
            }
        }
        self.connection.execute("INSERT INTO entries(project_id,title,path,last_opened) VALUES(?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            ON CONFLICT(project_id) DO UPDATE SET title=excluded.title,path=excluded.path,last_opened=excluded.last_opened",
            params![metadata.project.project_id, metadata.project.title, path_string(&path)?])?;
        Ok(())
    }
    pub fn archive(&mut self, project_id: &str, archived: bool) -> CoreResult<()> {
        let count = self.connection.execute(
            "UPDATE entries SET archived=? WHERE project_id=?",
            params![archived, project_id],
        )?;
        if count == 0 {
            return Err(CoreError::new(
                "ProjectNotFound",
                "This project is not in the library.",
            ));
        }
        Ok(())
    }
    /// Persists selected paths before any project file work. A retried operation
    /// must match the original request; it never chooses a second destination.
    pub fn begin(
        &mut self,
        operation_id: &str,
        kind: &str,
        title: &str,
        source: Option<(&Path, &str)>,
    ) -> CoreResult<PendingProject> {
        if operation_id.is_empty()
            || operation_id.len() > 64
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || title.trim().is_empty()
            || title.len() > 512
            || title.chars().any(char::is_control)
            || !["create", "duplicate", "recover"].contains(&kind)
            || (kind == "create") != source.is_none()
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Choose a valid project title and operation.",
            ));
        }
        let source_path = source
            .map(|(path, _)| std::fs::canonicalize(path))
            .transpose()?;
        let source_fingerprint = source.map(|(_, fingerprint)| fingerprint.to_owned());
        if let Some(previous) = self.operation(operation_id)? {
            if previous.kind != kind
                || previous.title != title
                || previous.source_path != source_path
                || previous.source_fingerprint != source_fingerprint
            {
                return Err(CoreError::new(
                    "OperationIdReuse",
                    "This project operation was already used for a different request.",
                ));
            }
            previous.require_available()?;
            return Ok(previous);
        }
        let parent = self.root.join("Projects");
        let final_path = parent.join(format!("project-{operation_id}"));
        let staging_path = parent.join(format!(".wns-stage-{operation_id}"));
        self.connection.execute("INSERT INTO operations(operation_id,kind,title,staging_path,final_path,source_path,source_fingerprint) VALUES(?,?,?,?,?,?,?)",
            params![operation_id, kind, title, path_string(&staging_path)?, path_string(&final_path)?, source_path.as_deref().map(path_string).transpose()?, source_fingerprint])?;
        self.operation(operation_id)?.ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "Could not retain the project operation.",
            )
        })
    }
    pub fn operation(&self, operation_id: &str) -> CoreResult<Option<PendingProject>> {
        Ok(self.connection.query_row("SELECT operation_id,kind,title,staging_path,final_path,source_path,source_fingerprint,completed FROM operations WHERE operation_id=?", [operation_id], |r| self.read_operation(r)).optional()?)
    }
    fn read_operation(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<PendingProject> {
        Ok(PendingProject {
            origin: CreationOrigin {
                operation_namespace: self.namespace.clone(),
                operation_id: row.get(0)?,
            },
            kind: row.get(1)?,
            title: row.get(2)?,
            staging_path: PathBuf::from(row.get::<_, String>(3)?),
            final_path: PathBuf::from(row.get::<_, String>(4)?),
            source_path: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
            source_fingerprint: row.get(6)?,
            completed: row.get(7)?,
        })
    }
    pub fn pending(&self) -> CoreResult<Vec<PendingProject>> {
        let mut statement = self.connection.prepare("SELECT operation_id,kind,title,staging_path,final_path,source_path,source_fingerprint,completed FROM operations WHERE completed=0 ORDER BY rowid")?;
        Ok(statement
            .query_map([], |r| self.read_operation(r))?
            .collect::<Result<_, _>>()?)
    }
    pub fn finish(&mut self, operation_id: &str, project: &ProjectSession) -> CoreResult<()> {
        let pending = self
            .operation(operation_id)?
            .ok_or_else(|| CoreError::new("InvalidRequest", "Unknown project operation."))?;
        if std::fs::canonicalize(&pending.final_path)? != project.path
            || read_creation_origin(&project.path)? != pending.origin
        {
            return Err(CoreError::new(
                "InvalidProject",
                "This folder does not match the pending project operation.",
            ));
        }
        // Registration is safely repeatable if a crash occurs before completion.
        self.register(project)?;
        self.connection.execute(
            "UPDATE operations SET completed=1 WHERE operation_id=?",
            [operation_id],
        )?;
        Ok(())
    }
    pub fn create(&mut self, operation_id: &str, title: &str) -> CoreResult<ProjectSession> {
        let pending = self.begin(operation_id, "create", title, None)?;
        let project = ProjectSession::create_staged(
            &pending.staging_path,
            &pending.final_path,
            title,
            &pending.origin,
        )?;
        self.finish(operation_id, &project)?;
        Ok(project)
    }
}
fn path_string(path: &Path) -> CoreResult<&str> {
    path.to_str().ok_or_else(|| {
        CoreError::new(
            "InvalidRequest",
            "The project path cannot be represented as Unicode.",
        )
    })
}
