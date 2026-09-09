use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::projects::{CreateDocument, DocumentRole, Head, ProjectSession, SaveCause, SaveSnapshot};
use webnovel_core::transfer::create_backup;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-document-roles-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary directory");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn open_with_chapter(path: &Path) -> (ProjectSession, webnovel_core::projects::ProjectAccess, webnovel_core::projects::DocumentRecord) {
    let project = ProjectSession::create(path, "Document roles").expect("create project");
    let access = project.attach("document-role-session".into()).expect("attach");
    let chapter = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter".into(),
            title: "Chapter".into(),
            kind: "chapter".into(),
            body: body("ordinary text"),
        })
        .expect("create chapter");
    (project, access, chapter)
}

#[test]
fn ordinary_role_is_default_and_omitted_from_legacy_document_json() {
    let temp = TempDir::new();
    let path = temp.child("project");
    let (project, _access, chapter) = open_with_chapter(&path);
    assert_eq!(chapter.role, DocumentRole::Ordinary);
    let encoded = serde_json::to_value(&chapter).expect("serialize document");
    assert!(encoded.get("role").is_none(), "ordinary role must preserve legacy bytes");
    drop(project);

    let db = Connection::open(path.join("project.sqlite3")).expect("open database");
    let role: String = db
        .query_row("SELECT role FROM documents WHERE id='chapter'", [], |row| row.get(0))
        .expect("read document role");
    assert_eq!(role, "ordinary");
    let role_change = db
        .execute("UPDATE documents SET role='assistantDraft' WHERE id='chapter'", []);
    assert!(role_change.is_err(), "authority roles must be immutable");
}

#[test]
fn generic_document_paths_reject_assistant_and_anchor_roles() {
    let temp = TempDir::new();
    let path = temp.child("project");
    let (project, _access, chapter) = open_with_chapter(&path);
    drop(project);
    let db = Connection::open(path.join("project.sqlite3")).expect("open database");
    db.execute(
        "INSERT INTO documents(id,kind,title,position,schema_version,body_json,body_hash,role)
         SELECT 'chapter-draft','note','Draft',position+1,1,body_json,body_hash,'assistantDraft'
         FROM documents WHERE id='chapter'",
        [],
    )
    .expect("install assistant draft");
    db.execute(
        "INSERT INTO documents(id,kind,title,position,schema_version,body_json,body_hash,role)
         SELECT 'anchor','note','Control anchor',position+2,1,body_json,body_hash,'conversationAnchor'
         FROM documents WHERE id='chapter'",
        [],
    )
    .expect("install conversation anchor");
    drop(db);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let access = reopened.attach("document-role-reopen".into()).expect("attach");
    assert_eq!(reopened.documents(access.clone()).expect("list documents").len(), 1);
    let hidden = reopened
        .document(access.clone(), "chapter-draft".into())
        .expect_err("ordinary read must reject assistant drafts");
    assert_eq!(hidden.code, "DocumentRoleMismatch");
    let renamed = reopened.rename_document(
        access.clone(),
        "chapter-draft".into(),
        "0".into(),
        "Renamed".into(),
    );
    assert_eq!(renamed.expect_err("rename must reject hidden document").code, "DocumentRoleMismatch");
    let history = reopened.history(access.clone(), "chapter-draft".into()).expect_err("history must reject hidden document");
    assert_eq!(history.code, "DocumentRoleMismatch");
    let pins = reopened
        .read_source_pins(access.clone(), "chapter-draft".into())
        .expect_err("source pins must reject hidden document");
    assert_eq!(pins.code, "DocumentNotFound");
    let save = reopened.save(SaveSnapshot {
        access,
        operation_id: "hidden-save".into(),
        expected: Head {
            document_id: "chapter-draft".into(),
            version: "0".into(),
            body_hash: chapter.head.body_hash,
        },
        local_generation: "1".into(),
        body: body("should not save"),
        cause: SaveCause::Typing,
    });
    assert_eq!(save.expect_err("generic save must reject hidden document").code, "DocumentRoleMismatch");
}

#[test]
fn backups_reject_orphan_assistant_and_control_documents() {
    let temp = TempDir::new();
    let source = temp.child("source");
    let (project, _access, _chapter) = open_with_chapter(&source);
    drop(project);
    let db = Connection::open(source.join("project.sqlite3")).expect("open database");
    db.execute(
        "INSERT INTO documents(id,kind,title,position,schema_version,body_json,body_hash,role)
         SELECT 'assistant-draft','note','Draft',position+1,1,body_json,body_hash,'assistantDraft'
         FROM documents WHERE id='chapter'",
        [],
    )
    .expect("install assistant draft");
    db.execute(
        "INSERT INTO documents(id,kind,title,position,schema_version,body_json,body_hash,role)
         SELECT 'conversation-anchor','note','Anchor',position+2,1,body_json,body_hash,'conversationAnchor'
         FROM documents WHERE id='chapter'",
        [],
    )
    .expect("install conversation anchor");
    drop(db);

    let project = ProjectSession::open(&source).expect("reopen source");
    let backup = temp.child("roles.wnsbackup");
    let error = create_backup(&project, &backup).expect_err("roles need valid conversation provenance");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!backup.exists());
}
