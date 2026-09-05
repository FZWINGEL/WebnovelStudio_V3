use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::{
    library::Library,
    projects::{ProjectSession, read_creation_origin},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-library-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("wns-library-test-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn installation_before_registry_ack_is_reconciled_without_a_second_copy() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let pending = library
        .begin("create-1", "create", "Harbour", None)
        .unwrap();
    let project = ProjectSession::create_staged(
        &pending.staging_path,
        &pending.final_path,
        &pending.title,
        &pending.origin,
    )
    .unwrap();
    let id = project.info.project_id.clone();
    assert!(library.list().unwrap().is_empty());
    assert_eq!(read_creation_origin(&project.path).unwrap(), pending.origin);
    drop(project);
    drop(library);
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let project = library.create("create-1", "Harbour").unwrap();
    assert_eq!(project.info.project_id, id);
    assert_eq!(library.list().unwrap().len(), 1);
    assert!(library.pending().unwrap().is_empty());
    drop(project);
    assert_eq!(
        library
            .create("create-1", "Harbour")
            .unwrap()
            .info
            .project_id,
        id
    );
    assert!(
        library
            .begin("create-1", "create", "Changed", None)
            .is_err()
    );
}

#[test]
fn partial_staging_is_retained_and_never_registered_as_a_project() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let pending = library
        .begin("partial", "create", "Incomplete", None)
        .unwrap();
    std::fs::create_dir(&pending.staging_path).unwrap();
    std::fs::write(pending.staging_path.join("evidence.txt"), "partial").unwrap();
    assert!(library.create("partial", "Incomplete").is_err());
    assert!(!pending.final_path.exists());
    assert!(pending.staging_path.join("evidence.txt").exists());
    assert!(library.list().unwrap().is_empty());
    assert_eq!(library.pending().unwrap().len(), 1);
}

#[test]
fn moving_a_closed_folder_can_be_located_but_a_second_copy_is_refused() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let project = library.create("one", "Movable").unwrap();
    let old = project.path.clone();
    let id = project.info.project_id.clone();
    drop(project);
    let moved = fixture.0.join("Moved");
    std::fs::rename(&old, &moved).unwrap();
    assert!(library.list().unwrap()[0].missing);
    let project = ProjectSession::open(&moved).unwrap();
    library.register(&project).unwrap();
    drop(project);
    let copy = fixture.0.join("Copied");
    std::fs::create_dir(&copy).unwrap();
    for name in ["project.wns.json", "project.sqlite3"] {
        std::fs::copy(moved.join(name), copy.join(name)).unwrap();
    }
    let copied = ProjectSession::open(copy).unwrap();
    assert_eq!(
        library.register(&copied).unwrap_err().code,
        "DuplicateProjectIdentity"
    );
    library.archive(&id, true).unwrap();
    assert!(library.list().unwrap()[0].archived);
    library.archive(&id, false).unwrap();
    assert!(!library.list().unwrap()[0].archived);
    assert_eq!(
        library.list().unwrap()[0].path,
        std::fs::canonicalize(moved).unwrap()
    );
}

#[test]
fn the_library_is_rebuildable_from_project_folders_and_has_one_owner() {
    let fixture = Fixture::new();
    let mut first = Library::open(fixture.0.join("app")).unwrap();
    assert!(Library::open(fixture.0.join("app")).is_err());
    let project = first.create("one", "Saved outside the registry").unwrap();
    let mut rebuilt = Library::open(fixture.0.join("replacement-app-index")).unwrap();
    rebuilt.register(&project).unwrap();
    assert_eq!(
        rebuilt.list().unwrap()[0].project_id,
        project.info.project_id
    );
}

#[test]
fn a_completed_creation_cannot_run_again_after_its_folder_moves() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let project = library.create("once", "One copy").unwrap();
    let original = project.path.clone();
    drop(project);
    let moved = fixture.0.join("Moved completed project");
    std::fs::rename(&original, &moved).unwrap();
    let error = match library.create("once", "One copy") {
        Ok(_) => panic!("completed operation unexpectedly ran twice"),
        Err(error) => error,
    };
    assert_eq!(error.code, "CompletedProjectMissing");
    assert!(!original.exists());
    assert!(moved.join("project.sqlite3").exists());
    assert_eq!(library.list().unwrap().len(), 1);
}
