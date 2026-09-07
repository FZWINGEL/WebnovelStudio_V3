use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::workshop::{
    AdoptionMode, Lens, PreviewWorkshopAdoption, SaveWorkshop, WorkshopAdoptionTarget,
    WorkshopBranchKind, WorkshopDecisionStatus, WorkshopDepth, WorkshopSession, WorkshopState,
};
use webnovel_core::projects::workshop_generation::{
    metadata_from_instruction, StartWorkshop, WorkshopExploration,
};
use webnovel_core::projects::{CreateDocument, ProjectSession};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("wns-workshop-protection-{}", Uuid::new_v4())))
    }

    fn project(&self) -> ProjectSession {
        ProjectSession::create(&self.0, "Workshop protection test").expect("create project")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "paragraph"},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

fn session(id: &str) -> WorkshopSession {
    WorkshopSession {
        id: id.into(),
        title: "Protection test".into(),
        lens: Lens::World,
        parent_session_id: None,
        branch_kind: WorkshopBranchKind::Working,
        brief: "A protected world detail".into(),
        direction: String::new(),
        still_open: String::new(),
        focus_question: String::new(),
        focus_reason: String::new(),
        focus_document_id: Some("fixed-world".into()),
        anchor_document_id: None,
        depth: WorkshopDepth::Develop,
        outside_direction: false,
        included_document_ids: vec!["fixed-world".into()],
        working_text: String::new(),
        working_title: String::new(),
        working_generation: "0".into(),
        selected_details: Vec::new(),
        choices: Vec::new(),
        questions: Vec::new(),
        composer: String::new(),
        selected_scope: "Element: Fixed world".into(),
        original_notes: String::new(),
        active_run_id: None,
        relationship_id: None,
    }
}

fn base_state() -> WorkshopState {
    WorkshopState {
        current_session_id: Some("protection-session".into()),
        sessions: vec![session("protection-session")],
        ..WorkshopState::default()
    }
}

fn target(head: webnovel_core::projects::Head, text: &str) -> WorkshopAdoptionTarget {
    WorkshopAdoptionTarget {
        document_id: "fixed-world".into(),
        expected: Some(head),
        title: "Fixed world".into(),
        kind: "world".into(),
        body: body(text),
        mode: AdoptionMode::Replace,
    }
}

fn request(
    access: &webnovel_core::projects::ProjectAccess,
    version: String,
    head: webnovel_core::projects::Head,
    text: &str,
) -> PreviewWorkshopAdoption {
    request_for_session(access, "protection-session", version, head, text)
}

fn request_for_session(
    access: &webnovel_core::projects::ProjectAccess,
    session_id: &str,
    version: String,
    head: webnovel_core::projects::Head,
    text: &str,
) -> PreviewWorkshopAdoption {
    PreviewWorkshopAdoption {
        access: access.clone(),
        session_id: session_id.into(),
        expected_version: version,
        candidate_ids: Vec::new(),
        targets: vec![target(head, text)],
        rationale: "Review this possible source change.".into(),
        protected_text: Vec::new(),
        relationships: Vec::new(),
        impact_drafts: Vec::new(),
    }
}

fn protected_request(
    access: &webnovel_core::projects::ProjectAccess,
    version: String,
    head: webnovel_core::projects::Head,
    text: &str,
    protected_text: &[&str],
) -> PreviewWorkshopAdoption {
    let mut request = request(access, version, head, text);
    request.protected_text = protected_text.iter().map(|text| (*text).into()).collect();
    request
}

fn start_request(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
    operation_id: &str,
) -> StartWorkshop {
    let saved = project.read_workshop(access.clone()).unwrap();
    let session = saved
        .state
        .sessions
        .iter()
        .find(|session| Some(session.id.as_str()) == saved.state.current_session_id.as_deref())
        .unwrap();
    StartWorkshop {
        access: access.clone(),
        operation_id: operation_id.into(),
        exploration: WorkshopExploration {
            session_id: session.id.clone(),
            expected_version: saved.version,
            working_generation: session.working_generation.clone(),
            action: "directions".into(),
            instruction: "Compare the relevant protected material.".into(),
            selected_scope: "Whole working version".into(),
            selected_text: String::new(),
            working_selection: None,
        },
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
    }
}

#[test]
fn archiving_keeps_fixed_protection_until_explicit_unfix() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-protection-archive".into())
        .unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-fixed-world".into(),
            document_id: "fixed-world".into(),
            title: "Fixed world".into(),
            kind: "world".into(),
            body: body("The old rule remains."),
        })
        .unwrap();
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-fixed-world".into(),
            expected_version: "0".into(),
            state: base_state(),
        })
        .unwrap();

    let fixed_preview = project
        .preview_workshop_adoption(protected_request(
            &access,
            saved.version,
            document.head.clone(),
            "The old rule remains.",
            &["The old rule remains."],
        ))
        .unwrap();
    let fixed_ack = project
        .adopt_workshop(access.clone(), "adopt-fixed-world".into(), fixed_preview.id)
        .unwrap();
    let fixed_snapshot = project.read_workshop(access.clone()).unwrap();
    assert_eq!(fixed_snapshot.state.decisions.len(), 1);
    assert!(fixed_snapshot.state.decisions[0].fixed);

    let mut archived = fixed_snapshot.state.clone();
    archived.decisions[0].status = WorkshopDecisionStatus::Archived;
    let archived_snapshot = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "archive-fixed-world".into(),
            expected_version: fixed_snapshot.version,
            state: archived,
        })
        .unwrap();
    assert_eq!(archived_snapshot.state.decisions[0].status, WorkshopDecisionStatus::Archived);
    assert!(archived_snapshot.state.decisions[0].fixed);

    let error = project
        .preview_workshop_adoption(request(
            &access,
            archived_snapshot.version.clone(),
            fixed_ack.documents[0].head.clone(),
            "The replacement rule.",
        ))
        .unwrap_err();
    assert_eq!(error.code, "ProtectedContentChanged");

    let after_refusal = project.read_workshop(access.clone()).unwrap();
    assert_eq!(after_refusal.version, archived_snapshot.version);
    assert_eq!(after_refusal.state, archived_snapshot.state);

    let mut unfixed = archived_snapshot.state.clone();
    unfixed.decisions[0].fixed = false;
    let unfixed_snapshot = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "unfix-fixed-world".into(),
            expected_version: archived_snapshot.version,
            state: unfixed,
        })
        .unwrap();
    assert_eq!(
        unfixed_snapshot.state.decisions[0].status,
        WorkshopDecisionStatus::Archived
    );
    assert!(!unfixed_snapshot.state.decisions[0].fixed);
    let preview = project
        .preview_workshop_adoption(request(
            &access,
            unfixed_snapshot.version,
            fixed_ack.documents[0].head.clone(),
            "The replacement rule.",
        ))
        .unwrap();
    assert_eq!(preview.targets[0].document_id, "fixed-world");
}

#[test]
fn adoption_supersession_cannot_release_a_fixed_previous_revision() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-protection-supersede".into())
        .unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-fixed-world".into(),
            document_id: "fixed-world".into(),
            title: "Fixed world".into(),
            kind: "world".into(),
            body: body("The old rule remains."),
        })
        .unwrap();
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-fixed-world".into(),
            expected_version: "0".into(),
            state: base_state(),
        })
        .unwrap();

    let fixed_preview = project
        .preview_workshop_adoption(protected_request(
            &access,
            saved.version,
            document.head.clone(),
            "The old rule remains.",
            &["The old rule remains."],
        ))
        .unwrap();
    let fixed_ack = project
        .adopt_workshop(access.clone(), "adopt-fixed-world".into(), fixed_preview.id)
        .unwrap();
    let fixed_view = project.read_workshop(access.clone()).unwrap();
    let previous = fixed_view
        .state
        .decisions
        .iter()
        .find(|decision| decision.status == WorkshopDecisionStatus::Chosen)
        .unwrap();
    assert!(previous.fixed);
    let unchanged_preview = project
        .preview_workshop_adoption(request(
            &access,
            fixed_view.version,
            fixed_ack.documents[0].head.clone(),
            "The old rule remains.",
        ))
        .unwrap();
    let ack = project
        .adopt_workshop(access.clone(), "adopt-unchanged-fixed-again".into(), unchanged_preview.id)
        .unwrap();
    let view = project.read_workshop(access.clone()).unwrap();
    let previous = view
        .state
        .decisions
        .iter()
        .find(|decision| decision.status == WorkshopDecisionStatus::Superseded)
        .unwrap();
    assert!(previous.fixed);
    assert_eq!(ack.documents[0].head.document_id, "fixed-world");

    let current_head = ack.documents[0].head.clone();
    let error = project
        .preview_workshop_adoption(request(
            &access,
            view.version,
            current_head,
            "The replacement rule.",
        ))
        .unwrap_err();
    assert_eq!(error.code, "ProtectedContentChanged");
}

#[test]
fn actor_omits_unrelated_fixed_source_from_request_packet() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-protection-packet".into())
        .unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-unrelated-world".into(),
            document_id: "unrelated-world".into(),
            title: "Unrelated world".into(),
            kind: "world".into(),
            body: body("UNRELATED FIXED BODY"),
        })
        .unwrap();
    let mut current = session("current-session");
    current.focus_document_id = None;
    current.included_document_ids.clear();
    current.anchor_document_id = Some("workshop-current-session".into());
    let mut older = session("older-session");
    older.focus_document_id = None;
    older.included_document_ids.clear();
    let initial = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-packet-sessions".into(),
            expected_version: "0".into(),
            state: WorkshopState {
                current_session_id: Some(older.id.clone()),
                sessions: vec![current, older],
                ..WorkshopState::default()
            },
        })
        .unwrap();

    let mut unrelated_request = request_for_session(
            &access,
            "older-session",
            initial.version,
            document.head.clone(),
            "UNRELATED FIXED BODY",
        );
    unrelated_request.targets[0].document_id = "unrelated-world".into();
    unrelated_request.targets[0].title = "Unrelated world".into();
    let preview = project
        .preview_workshop_adoption(unrelated_request)
        .unwrap();
    project
        .adopt_workshop(access.clone(), "adopt-unrelated-world".into(), preview.id)
        .unwrap();
    let adopted = project.read_workshop(access.clone()).unwrap();
    let mut protected = adopted.state.clone();
    protected.current_session_id = Some("current-session".into());
    let decision = protected
        .decisions
        .iter_mut()
        .find(|decision| decision.document_id == "unrelated-world")
        .unwrap();
    decision.fixed = true;
    assert!(decision.protected_text.is_empty());
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "protect-unrelated-world".into(),
            expected_version: adopted.version,
            state: protected,
        })
        .unwrap();
    assert_eq!(saved.state.current_session_id.as_deref(), Some("current-session"));

    let started = project
        .start_workshop(start_request(
            &project,
            &access,
            "start-current-packet",
        ))
        .unwrap();
    let metadata = metadata_from_instruction(
        &started
            .packet
            .messages
            .last()
            .expect("workshop request message")
            .content,
    )
    .unwrap();
    assert!(!metadata
        .fixed_details
        .iter()
        .any(|text| text.contains("UNRELATED FIXED BODY")));
    assert!(!metadata
        .fixed_source_refs
        .iter()
        .any(|reference| reference.starts_with("unrelated-world@")));
}
