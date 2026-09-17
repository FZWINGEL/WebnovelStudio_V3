use super::*;

pub(crate) struct OwnedProject {
    connection: Option<Connection>,
    // Dropped only when the owned connection thread exits, never when a UI changes projects.
    _lock: File,
    pub(crate) path: PathBuf,
    pub(crate) info: ProjectInfo,
    pub(crate) access: Option<ProjectAccess>,
    renderer_session: Option<String>,
    retired_sessions: HashSet<String>,
    pub(crate) needs_reopen: bool,
}
impl OwnedProject {
    pub(crate) fn db(&self) -> CoreResult<&Connection> {
        self.connection.as_ref().ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The project connection needs recovery.",
            )
        })
    }
    pub(crate) fn db_mut(&mut self) -> CoreResult<&mut Connection> {
        self.connection.as_mut().ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The project connection needs recovery.",
            )
        })
    }
    pub(crate) fn fence_uncertain<T>(&mut self, result: &CoreResult<T>) {
        if result.as_ref().is_err_and(|e| e.code == "UncertainOutcome") {
            self.access = None;
            self.needs_reopen = true;
        }
    }
    pub(crate) fn recover_connection(&mut self) -> CoreResult<()> {
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
    pub(crate) fn open_direct(path: PathBuf, title: Option<String>) -> CoreResult<Self> {
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
        Self::validate_schema_floor(&connection)?;
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
        crate::projects::discussions::recover_interrupted_discussions(&mut project)?;
        crate::projects::memory::recover_interrupted_memory(&mut project)?;
        Ok(project)
    }

    /// Schema 33 adds reviewed character-knowledge columns. Schema 34 raises
    /// the reader floor for reviewed-memory lookup packets, and schema 35
    /// adds Story Workshop storage. Schema 36 raises the floor for the
    /// optional typed relationship identity carried by immutable Workshop
    /// state and context packet JSON. Schema 37 raises the floor for typed
    /// story possibilities carried by the same existing JSON records. A
    /// database can be manually copied or
    /// have its user_version altered without running the migration, so opening
    /// it must verify the physical floor before any retained rows are read.
    pub(crate) fn validate_schema_floor(connection: &Connection) -> CoreResult<()> {
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version >= 39 {
            let roles: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('documents') WHERE name='role')",
                [],
                |r| r.get(0),
            )?;
            let immutable: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND name='documents_role_immutable')", [], |r| r.get(0))?;
            if !roles || !immutable {
                return Err(CoreError::new(
                    "UnsupportedSchema",
                    "This project is missing its document-role isolation contract.",
                ));
            }
        }
        if version >= 40 {
            for (table, column) in [
                ("project_conversations", "anchor_document_id"),
                ("conversation_items", "payload_hash"),
                ("assistant_drafts", "disposition_version"),
            ] {
                let present: bool = connection.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |r| r.get(0),
                )?;
                if !present {
                    return Err(CoreError::new(
                        "UnsupportedSchema",
                        "This project is missing its conversation persistence contract.",
                    ));
                }
            }
        }
        if version < 33 {
            return Ok(());
        }
        for (table, column) in [
            ("review_stages", "knowledge_json"),
            ("review_stages", "knowledge_hash"),
            ("ready_bundles", "knowledge_json"),
            ("ready_bundles", "knowledge_hash"),
        ] {
            let present: bool = connection.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"),
                [column],
                |row| row.get(0),
            )?;
            if !present {
                return Err(CoreError::new(
                    "UnsupportedSchema",
                    "This project claims schema 33 but is missing reviewed knowledge columns.",
                ));
            }
        }
        if version >= 34 {
            // The memory lookup capability is retained in existing immutable
            // JSON rows. Reaching this floor is itself the physical check:
            // these rows and their legacy nullable fields must still exist.
            for table in [
                "discussion_lookup_invocations",
                "discussion_lookup_results",
                "discussion_lookup_reads",
            ] {
                let present: bool = connection.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='{table}')"
                    ),
                    [],
                    |row| row.get(0),
                )?;
                if !present {
                    return Err(CoreError::new(
                        "UnsupportedSchema",
                        "This project claims schema 34 but is missing durable lookup tables.",
                    ));
                }
            }
        }
        if version >= 35 {
            for table in [
                "workshop_state",
                "workshop_snapshots",
                "workshop_adoption_previews",
                "workshop_receipts",
            ] {
                let present: bool = connection.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='{table}')"
                    ),
                    [],
                    |row| row.get(0),
                )?;
                if !present {
                    return Err(CoreError::new(
                        "UnsupportedSchema",
                        "This project claims schema 35 or newer but is missing Story Workshop tables.",
                    ));
                }
            }
        }
        if version >= 36 {
            for (table, column) in [
                ("workshop_state", "state_json"),
                ("workshop_snapshots", "state_json"),
                ("workshop_adoption_previews", "request_json"),
                ("workshop_adoption_previews", "preview_json"),
                ("context_packets", "request_json"),
                ("context_packets", "packet_json"),
                ("context_packets", "input_hash"),
            ] {
                let present: bool = connection.query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"
                    ),
                    [column],
                    |row| row.get(0),
                )?;
                if !present {
                    return Err(CoreError::new(
                        "UnsupportedSchema",
                        "This project claims schema 36 or newer but is missing typed Workshop relationship storage.",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn attach(&mut self, session: String) -> CoreResult<ProjectAccess> {
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
    pub(crate) fn attach_snapshot(&mut self, session: String) -> CoreResult<AttachedProject> {
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
    pub(crate) fn check_access(&self, access: &ProjectAccess) -> CoreResult<()> {
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
    pub(crate) fn project_metadata(&self) -> CoreResult<ProjectMetadata> {
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
    pub(crate) fn rename_project(
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
    pub(crate) fn rename_document(
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
    pub(crate) fn view_state(&self, access: ProjectAccess) -> CoreResult<Option<ViewState>> {
        self.check_access(&access)?;
        self.current_view_state()
    }
    pub(crate) fn current_view_state(&self) -> CoreResult<Option<ViewState>> {
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
    pub(crate) fn save_view_state(
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
    pub(crate) fn context_source_epoch(&self) -> CoreResult<SourceEpoch> {
        let epoch: i64 = self.db()?.query_row(
            "SELECT context_source_epoch FROM project WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        Ok(SourceEpoch::new(parse_stored_version(epoch)?))
    }
    pub(crate) fn create_document(&mut self, request: CreateDocument) -> CoreResult<DocumentRecord> {
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
        tx.execute("INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash,role) VALUES(?,?,?,(SELECT COALESCE(MAX(position),-1)+1 FROM documents WHERE role='ordinary'),0,1,?,?, 'ordinary')", params![request.document_id, request.kind, request.title, validated.canonical_json, validated.hash])?;
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
    pub(crate) fn list(&self, access: ProjectAccess) -> CoreResult<Vec<DocumentRecord>> {
        self.check_access(&access)?;
        self.document_records()
    }
    pub(crate) fn document_records(&self) -> CoreResult<Vec<DocumentRecord>> {
        let mut statement = self.db()?.prepare(
            "SELECT id FROM documents WHERE trashed=0 AND role='ordinary' ORDER BY position,id",
        )?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| read_document(self.db()?, id)).collect()
    }
    pub(crate) fn save(&mut self, request: SaveSnapshot) -> CoreResult<SaveAck> {
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
    pub(crate) fn checkpoint(&mut self, request: CheckpointRequest) -> CoreResult<Revision> {
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
    pub(crate) fn history(&self, access: ProjectAccess, document_id: &str) -> CoreResult<Vec<Revision>> {
        self.check_access(&access)?;
        read_document(self.db()?, document_id)?;
        let mut statement = self.db()?.prepare(
            "SELECT id FROM revisions WHERE document_id=? ORDER BY source_working_version DESC",
        )?;
        let ids = statement
            .query_map([document_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| read_revision(self.db()?, id)).collect()
    }
    pub(crate) fn reconcile(&mut self, request: ReconcileRequest) -> CoreResult<ReconciledDocument> {
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
    pub(crate) fn storage_info(&self) -> CoreResult<StorageInfo> {
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


#[test]
fn save_receipt_matches_shared_literal_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(contracts::W2_SAVE_RECEIPT).unwrap();
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
