//! Rebuildable app-local project index and narrowly scoped staged-file receipts.
//! Manuscripts remain in portable project folders, never in this database.
use crate::projects::import::{
    V2ImportRequest, V2ImportResult, decode_import_operation, encode_import_operation,
    read_import_result, recover_import_staging, request_fingerprint, stage_v2_import,
};
use crate::projects::{
    CoreError, CoreResult, CreationOrigin, ProjectSession, read_creation_origin,
};
use crate::providers::{
    catalog::ProviderState,
    endpoints::{
        ENDPOINT_PROFILES_KEY, ENDPOINT_PROFILES_SCHEMA_VERSION, EndpointProfile,
        EndpointProfileDraft, EndpointProfilesSettings, StoredEndpointProfiles,
        parse_revision as parse_endpoint_revision, validate_discovered_model_ids,
    },
    preferences::{
        MODEL_SETTINGS_KEY, MODEL_SETTINGS_SCHEMA_VERSION, ModelKey, ModelSelection, ModelSettings,
        StoredModelSettings, parse_revision, provider_state_with_endpoints as build_provider_state,
    },
};
use crate::v2_import::preview_v2_import;
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
        if version > 3 {
            return Err(CoreError::new(
                "UnsupportedSchema",
                "This library needs a newer WebnovelStudio.",
            ));
        }
        crate::storage::configure(&connection)?;
        let mut version = version;
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
            version = 1;
        }
        if version < 2 {
            let tx = connection.transaction()?;
            tx.execute_batch(
                "CREATE TABLE app_preferences (
                    key TEXT PRIMARY KEY NOT NULL,
                    schema_version INTEGER NOT NULL CHECK(schema_version=1),
                    revision INTEGER NOT NULL CHECK(revision >= 0),
                    value_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                ) STRICT;
                PRAGMA user_version=2;",
            )?;
            tx.commit().map_err(CoreError::uncertain)?;
        }
        if version < 3 {
            let tx = connection.transaction()?;
            tx.execute_batch("PRAGMA user_version=3;")?;
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
    /// Read the app-global model preference and the offline catalog.  This
    /// method never probes a provider or reads credentials.
    pub fn provider_state(&self) -> CoreResult<ProviderState> {
        let settings = self.read_model_settings()?;
        let endpoints = self.read_endpoint_profiles()?;
        build_provider_state(settings, &endpoints)
    }
    /// Persist an explicit active model and favorites with a compare-and-swap
    /// revision.  The returned state is read from the committed values, so a
    /// caller that loses this acknowledgment can safely call provider_state.
    pub fn save_model_settings(
        &mut self,
        expected_revision: &str,
        active: ModelSelection,
        favorites: Vec<ModelKey>,
    ) -> CoreResult<ProviderState> {
        let expected = parse_revision(expected_revision)?;
        let current = self.read_model_settings()?;
        let current_revision = parse_revision(&current.revision)?;
        if current_revision != expected {
            return Err(CoreError::new(
                "PreferenceConflict",
                "The model settings changed. Read them again before saving.",
            ));
        }
        let next_revision = expected.checked_add(1).ok_or_else(|| {
            CoreError::new(
                "PreferenceRevisionLimit",
                "The model preference revision limit was reached.",
            )
        })?;
        let settings = ModelSettings {
            revision: next_revision.to_string(),
            active,
            favorites,
        };
        let endpoints = self.read_endpoint_profiles()?;
        settings.validate_with_endpoints(&endpoints)?;
        let value_json = serde_json::to_string(&settings.stored()).map_err(|error| {
            CoreError::new(
                "InvalidModelSettings",
                &format!("Could not serialize model settings: {error}"),
            )
        })?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO app_preferences(key,schema_version,revision,value_json,updated_at)
             VALUES(?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(key) DO UPDATE SET schema_version=excluded.schema_version,
               revision=excluded.revision,value_json=excluded.value_json,
               updated_at=excluded.updated_at",
            params![
                MODEL_SETTINGS_KEY,
                i64::from(MODEL_SETTINGS_SCHEMA_VERSION),
                next_revision,
                value_json,
            ],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        build_provider_state(settings, &endpoints)
    }

    /// Read all nonsecret OpenAI-compatible endpoint profiles.  Omitted
    /// profiles are never inferred to be deleted; callers can disable a
    /// profile while preserving selected and favorite model references.
    pub fn endpoint_profiles(&self) -> CoreResult<EndpointProfilesSettings> {
        self.read_endpoint_profiles()
    }

    /// Create or update endpoint profiles under the app-preference CAS
    /// revision.  Existing profiles omitted by `drafts` are retained so a
    /// stale picker cannot delete a profile or strand a model selection.
    pub fn save_endpoint_profiles(
        &mut self,
        expected_revision: &str,
        drafts: Vec<EndpointProfileDraft>,
    ) -> CoreResult<ProviderState> {
        let expected = parse_revision(expected_revision)?;
        let current = self.read_endpoint_profiles()?;
        if parse_revision(&current.revision)? != expected {
            return Err(CoreError::new(
                "PreferenceConflict",
                "The endpoint settings changed. Read them again before saving.",
            ));
        }

        let mut profiles = current.profiles.clone();
        let mut touched = std::collections::HashSet::new();
        for draft in &drafts {
            let normalized = draft.validate()?;
            let id = match &draft.id {
                Some(id) => id.clone(),
                None => format!(
                    "{}{}",
                    crate::providers::endpoints::ENDPOINT_PROVIDER_PREFIX,
                    Uuid::new_v4()
                ),
            };
            if !touched.insert(id.clone()) {
                return Err(CoreError::new(
                    "DuplicateEndpointProfile",
                    "An endpoint profile may appear only once in one save.",
                ));
            }
            if let Some(index) = profiles.iter().position(|profile| profile.id == id) {
                let existing = &profiles[index];
                if existing.config_equals(draft, &normalized) {
                    continue;
                }
                let next_config_revision = parse_endpoint_revision(&existing.config_revision)?
                    .checked_add(1)
                    .ok_or_else(|| {
                        CoreError::new(
                            "EndpointRevisionLimit",
                            "The endpoint configuration revision limit was reached.",
                        )
                    })?;
                let cached_model_ids = if existing.base_url == normalized
                    && existing.credential_ref == draft.credential_ref
                {
                    existing.cached_model_ids.clone()
                } else {
                    Vec::new()
                };
                profiles[index] = EndpointProfile {
                    id,
                    label: draft.label.clone(),
                    base_url: normalized,
                    enabled: draft.enabled,
                    json_mode: draft.json_mode,
                    config_revision: next_config_revision.to_string(),
                    credential_ref: draft.credential_ref.clone(),
                    manual_model_ids: draft.manual_model_ids.clone(),
                    // A route or credential change makes the old discovery
                    // evidence unsafe to present as current. Manual model
                    // IDs remain available, while remembered selections are
                    // reintroduced by the catalog as unavailable tombstones.
                    cached_model_ids,
                };
            } else if draft.id.is_some() {
                return Err(CoreError::new(
                    "UnknownEndpointProfile",
                    "The endpoint profile does not exist; create it without an ID.",
                ));
            } else {
                profiles.push(EndpointProfile::new(id, draft)?);
            }
        }

        let next_revision = expected.checked_add(1).ok_or_else(|| {
            CoreError::new(
                "PreferenceRevisionLimit",
                "The endpoint preference revision limit was reached.",
            )
        })?;
        let settings = EndpointProfilesSettings {
            revision: next_revision.to_string(),
            profiles,
        };
        settings.validate()?;
        self.write_endpoint_profiles(expected, &settings)?;
        self.provider_state()
    }

    /// Replace only the discovery cache for one profile.  The config revision
    /// fence prevents a late network response from overwriting a changed URL,
    /// credential binding, enable flag, or manual model list.
    pub fn refresh_endpoint_models(
        &mut self,
        profile_id: &str,
        expected_config_revision: &str,
        model_ids: Vec<String>,
    ) -> CoreResult<ProviderState> {
        let expected_config = parse_endpoint_revision(expected_config_revision)?;
        validate_discovered_model_ids(&model_ids)?;
        let current = self.read_endpoint_profiles()?;
        let mut profiles = current.profiles.clone();
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| {
                CoreError::new(
                    "UnknownEndpointProfile",
                    "The endpoint profile does not exist.",
                )
            })?;
        if parse_endpoint_revision(&profile.config_revision)? != expected_config {
            return Err(CoreError::new(
                "EndpointConfigConflict",
                "The endpoint configuration changed while discovery was running.",
            ));
        }
        profile.cached_model_ids = model_ids;
        let current_revision = parse_revision(&current.revision)?;
        let next_revision = current_revision.checked_add(1).ok_or_else(|| {
            CoreError::new(
                "PreferenceRevisionLimit",
                "The endpoint preference revision limit was reached.",
            )
        })?;
        let settings = EndpointProfilesSettings {
            revision: next_revision.to_string(),
            profiles,
        };
        settings.validate()?;
        self.write_endpoint_profiles(current_revision, &settings)?;
        self.provider_state()
    }

    fn write_endpoint_profiles(
        &mut self,
        expected_revision: i64,
        settings: &EndpointProfilesSettings,
    ) -> CoreResult<()> {
        settings.validate()?;
        let value_json = serde_json::to_string(&settings.stored()).map_err(|error| {
            CoreError::new(
                "InvalidEndpointProfiles",
                &format!("Could not serialize endpoint profiles: {error}"),
            )
        })?;
        let tx = self.connection.transaction()?;
        let stored_revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM app_preferences WHERE key=?",
                [ENDPOINT_PROFILES_KEY],
                |row| row.get(0),
            )
            .optional()?;
        if stored_revision.unwrap_or(0) != expected_revision {
            return Err(CoreError::new(
                "PreferenceConflict",
                "The endpoint settings changed. Read them again before saving.",
            ));
        }
        tx.execute(
            "INSERT INTO app_preferences(key,schema_version,revision,value_json,updated_at)
             VALUES(?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(key) DO UPDATE SET schema_version=excluded.schema_version,
               revision=excluded.revision,value_json=excluded.value_json,
               updated_at=excluded.updated_at",
            params![
                ENDPOINT_PROFILES_KEY,
                i64::from(ENDPOINT_PROFILES_SCHEMA_VERSION),
                parse_revision(&settings.revision)?,
                value_json,
            ],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(())
    }

    fn read_endpoint_profiles(&self) -> CoreResult<EndpointProfilesSettings> {
        let row: Option<(i64, i64, String)> = self
            .connection
            .query_row(
                "SELECT schema_version,revision,value_json FROM app_preferences WHERE key=?",
                [ENDPOINT_PROFILES_KEY],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((schema_version, revision, value_json)) = row else {
            return Ok(EndpointProfilesSettings::default());
        };
        if schema_version != i64::from(ENDPOINT_PROFILES_SCHEMA_VERSION) || revision < 0 {
            return Err(CoreError::new(
                "InvalidEndpointProfiles",
                "The stored endpoint profiles use an unsupported schema or revision.",
            ));
        }
        let stored: StoredEndpointProfiles =
            serde_json::from_str(&value_json).map_err(|error| {
                CoreError::new(
                    "InvalidEndpointProfiles",
                    &format!("The stored endpoint profiles are invalid: {error}"),
                )
            })?;
        let settings = EndpointProfilesSettings::from_stored(revision.to_string(), stored);
        settings.validate()?;
        Ok(settings)
    }

    fn read_model_settings(&self) -> CoreResult<ModelSettings> {
        let row: Option<(i64, i64, String)> = self
            .connection
            .query_row(
                "SELECT schema_version,revision,value_json FROM app_preferences WHERE key=?",
                [MODEL_SETTINGS_KEY],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((schema_version, revision, value_json)) = row else {
            return Ok(ModelSettings::default());
        };
        if schema_version != i64::from(MODEL_SETTINGS_SCHEMA_VERSION) || revision < 0 {
            return Err(CoreError::new(
                "InvalidModelSettings",
                "The stored model settings use an unsupported schema or revision.",
            ));
        }
        let stored: StoredModelSettings = serde_json::from_str(&value_json).map_err(|error| {
            CoreError::new(
                "InvalidModelSettings",
                &format!("The stored model settings are invalid: {error}"),
            )
        })?;
        let settings = ModelSettings::from_stored(revision.to_string(), stored);
        let endpoints = self.read_endpoint_profiles()?;
        settings.validate_with_endpoints(&endpoints)?;
        Ok(settings)
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
        self.register_metadata(&path, &metadata)
    }
    fn register_metadata(
        &mut self,
        path: &Path,
        metadata: &crate::projects::ProjectMetadata,
    ) -> CoreResult<()> {
        let previous: Option<String> = self
            .connection
            .query_row(
                "SELECT path FROM entries WHERE project_id=?",
                [&metadata.project.project_id],
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
            params![metadata.project.project_id, metadata.project.title, path_string(path)?])?;
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
            || !["create", "duplicate", "recover", "import"].contains(&kind)
            || (kind == "create") != source.is_none()
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Choose a valid project title and operation.",
            ));
        }
        let source_path = source
            .map(|(path, _)| match std::fs::canonicalize(path) {
                Ok(path) => Ok(path),
                // Completed imports can be reconciled from their retained
                // receipt even after the reviewed source was deleted.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_owned()),
                Err(error) => Err(error),
            })
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

    /// Import a reviewed V2 source into a fresh staged project. The source is
    /// previewed before the library operation is recorded and re-read by the
    /// staging layer, so the reviewed source fingerprint and body choices are
    /// part of the durable operation identity.
    pub fn import_v2(&mut self, request: V2ImportRequest) -> CoreResult<V2ImportResult> {
        // A completed import is its own immutable receipt. Reconcile it from
        // the retained project before touching the original V2 source again;
        // the source may have moved or changed after the first acknowledgment
        // was lost. Incomplete operations still revalidate the source below.
        let expected_request_sha256 =
            request_fingerprint(&request, &request.expected_source_sha256)?;
        let existing = self.operation(&request.operation_id)?;
        let pending = if let Some(previous) = existing {
            if previous.kind != "import" || previous.title != request.title {
                return Err(CoreError::new(
                    "OperationIdReuse",
                    "This project operation was already used for a different request.",
                ));
            }
            let request_source_path = operation_source_path(&request.source_path)?;
            if previous.source_path.as_ref() != Some(&request_source_path) {
                return Err(CoreError::new(
                    "OperationIdReuse",
                    "This project operation was already used for a different source.",
                ));
            }
            let legacy_incomplete = match previous.source_fingerprint.as_deref() {
                Some(stored) if stored == expected_request_sha256 => !previous.completed,
                Some(stored) => {
                    let decoded = decode_import_operation(
                        stored,
                        &request.operation_id,
                        request_source_path.clone(),
                        request.title.clone(),
                    )?;
                    if decoded.source_project_id != request.source_project_id
                        || decoded.expected_source_sha256 != request.expected_source_sha256
                        || request_fingerprint(&request, &request.expected_source_sha256)?
                            != request_fingerprint(&decoded, &decoded.expected_source_sha256)?
                    {
                        return Err(CoreError::new(
                            "OperationIdReuse",
                            "This project operation was already used for a different request.",
                        ));
                    }
                    false
                }
                None => !previous.completed,
            };
            previous.require_available()?;
            // Keep this marker in the operation identity check while allowing
            // the receipt/staging reconciliation below to recover a committed
            // artifact without the original source or body choices.
            if legacy_incomplete && !previous.final_path.exists() && !previous.staging_path.exists()
            {
                return Err(CoreError::new(
                    "ImportRecoveryUnavailable",
                    "This incomplete import has no retained request choices.",
                ));
            }
            previous
        } else {
            let preview = preview_v2_import(&request.source_path, &request.source_project_id)?;
            if preview.source.source_sha256 != request.expected_source_sha256 {
                return Err(CoreError::new(
                    "SourceVersionChanged",
                    "The V2 source fingerprint no longer matches the reviewed import.",
                ));
            }
            let operation_fingerprint =
                encode_import_operation(&request, &expected_request_sha256)?;
            self.begin(
                &request.operation_id,
                "import",
                &request.title,
                Some((&request.source_path, &operation_fingerprint)),
            )?
        };
        pending.require_available()?;
        if pending.completed {
            return read_import_result(
                &pending.final_path,
                &request.operation_id,
                &request.expected_source_sha256,
                &expected_request_sha256,
            );
        }

        // A valid final folder or sealed staging folder is enough to recover a
        // lost library acknowledgment.  These paths intentionally do not
        // reread the source database and do not open a project writer.
        if pending.final_path.exists()
            && crate::projects::read_creation_origin(&pending.final_path)
                .ok()
                .as_ref()
                == Some(&pending.origin)
        {
            let result = read_import_result(
                &pending.final_path,
                &request.operation_id,
                &request.expected_source_sha256,
                &expected_request_sha256,
            )?;
            self.finish_import_receipt(&request.operation_id, &pending.final_path, &result)?;
            return Ok(result);
        }
        if pending.staging_path.exists()
            && crate::projects::read_creation_origin(&pending.staging_path)
                .ok()
                .as_ref()
                == Some(&pending.origin)
        {
            let result = recover_import_staging(
                &pending.staging_path,
                &pending.final_path,
                &request.title,
                &pending.origin,
                &request.operation_id,
                &request.expected_source_sha256,
                &expected_request_sha256,
            )?;
            self.finish_import_receipt(&request.operation_id, &pending.final_path, &result)?;
            return Ok(result);
        }
        if pending.source_fingerprint.as_deref() == Some(expected_request_sha256.as_str())
            || pending.source_fingerprint.is_none()
        {
            let incomplete = !pending.completed;
            if incomplete {
                return Err(CoreError::new(
                    "ImportRecoveryUnavailable",
                    "This incomplete import has no retained request choices.",
                ));
            }
        }
        let preview = preview_v2_import(&request.source_path, &request.source_project_id)?;
        if preview.source.source_sha256 != request.expected_source_sha256 {
            return Err(CoreError::new(
                "SourceVersionChanged",
                "The V2 source fingerprint no longer matches the reviewed import.",
            ));
        }
        let request_sha256 = request_fingerprint(&request, &preview.source.source_sha256)?;
        if request_sha256 != expected_request_sha256 {
            return Err(CoreError::new(
                "ImportRequestChanged",
                "The import choices no longer match the reviewed operation.",
            ));
        }
        let _staged = stage_v2_import(
            &request,
            &request_sha256,
            &pending.staging_path,
            &pending.final_path,
            &pending.origin,
        )?;
        let result = read_import_result(
            &pending.final_path,
            &request.operation_id,
            &request.expected_source_sha256,
            &expected_request_sha256,
        )?;
        self.finish_import_receipt(&request.operation_id, &pending.final_path, &result)?;
        Ok(result)
    }

    /// Reconstruct and retry an incomplete import from its durable operation
    /// record.  The source picker and body-choice UI are deliberately absent.
    pub fn resume_v2_import(&mut self, operation_id: &str) -> CoreResult<V2ImportResult> {
        let pending = self
            .operation(operation_id)?
            .ok_or_else(|| CoreError::new("InvalidRequest", "Unknown import operation."))?;
        if pending.kind != "import" {
            return Err(CoreError::new(
                "OperationIdReuse",
                "This operation is not a V2 import.",
            ));
        }
        pending.require_available()?;
        let source_path = pending.source_path.clone().ok_or_else(|| {
            CoreError::new(
                "ImportRecoveryUnavailable",
                "The incomplete import has no retained source path.",
            )
        })?;
        let fingerprint = pending.source_fingerprint.clone().ok_or_else(|| {
            CoreError::new(
                "ImportRecoveryUnavailable",
                "The incomplete import has no retained request choices.",
            )
        })?;
        let request =
            decode_import_operation(&fingerprint, operation_id, source_path, pending.title)?;
        self.import_v2(request)
    }

    fn finish_import_receipt(
        &mut self,
        operation_id: &str,
        path: &Path,
        result: &V2ImportResult,
    ) -> CoreResult<()> {
        let pending = self
            .operation(operation_id)?
            .ok_or_else(|| CoreError::new("InvalidRequest", "Unknown project operation."))?;
        if pending.kind != "import"
            || std::fs::canonicalize(path)? != std::fs::canonicalize(&pending.final_path)?
            || crate::projects::read_creation_origin(path)? != pending.origin
            || result.operation_id != operation_id
        {
            return Err(CoreError::new(
                "InvalidProject",
                "This folder does not match the pending import operation.",
            ));
        }
        let metadata = result.project.clone();
        self.register_metadata(
            path,
            &crate::projects::ProjectMetadata {
                project: metadata,
                metadata_version: "0".into(),
            },
        )?;
        self.connection.execute(
            "UPDATE operations SET completed=1 WHERE operation_id=?",
            [operation_id],
        )?;
        Ok(())
    }
}
fn operation_source_path(path: &Path) -> CoreResult<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name())
                && let Ok(parent) = std::fs::canonicalize(parent)
            {
                return Ok(parent.join(file_name));
            }
            Ok(path.to_owned())
        }
        Err(error) => Err(error.into()),
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
