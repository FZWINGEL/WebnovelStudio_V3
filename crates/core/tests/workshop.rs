use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::workshop::{
    AdoptionMode, Lens, PreviewWorkshopAdoption, SaveWorkshop, WorkshopAdoptionTarget,
    WorkshopBranchKind, WorkshopDepth, WorkshopRelationship, WorkshopRelationshipStatus,
    WorkshopSession, WorkshopState,
};
use webnovel_core::projects::workshop_generation::{StartWorkshop, WorkshopExploration};
use webnovel_core::projects::{CreateDocument, ProjectSession, SaveCause, SaveSnapshot};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempProject(PathBuf);
impl TempProject {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("wns-workshop-{}", Uuid::new_v4())))
    }
    fn project(&self) -> ProjectSession {
        ProjectSession::create(&self.0, "Workshop test").expect("create project")
    }
}
impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":text}]}]}})
}

fn session(id: &str) -> WorkshopSession {
    WorkshopSession {
        id: id.into(),
        title: "Explore".into(),
        lens: Lens::Overview,
        parent_session_id: None,
        branch_kind: WorkshopBranchKind::Working,
        brief: "A seed".into(),
        direction: String::new(),
        still_open: String::new(),
        focus_question: String::new(),
        focus_reason: String::new(),
        focus_document_id: None,
        anchor_document_id: None,
        depth: WorkshopDepth::Sketch,
        outside_direction: false,
        included_document_ids: Vec::new(),
        working_text: String::new(),
        working_title: String::new(),
        working_generation: "0".into(),
        selected_details: Vec::new(),
        choices: Vec::new(),
        questions: Vec::new(),
        composer: String::new(),
        selected_scope: String::new(),
        original_notes: String::new(),
        active_run_id: None,
    }
}

#[test]
fn workshop_state_reopens_and_save_is_cas_idempotent() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("session-one"));
    let request = SaveWorkshop {
        access: access.clone(),
        operation_id: "workshop-save-one".into(),
        expected_version: "0".into(),
        state: state.clone(),
    };
    let saved = project.save_workshop(request.clone()).unwrap();
    assert_eq!(saved.version, "1");
    assert_eq!(project.save_workshop(request).unwrap(), saved);
    let view = project.read_workshop(access.clone()).unwrap();
    assert_eq!(view.version, saved.version);
    assert_eq!(view.state, saved.state);
    assert_eq!(project.workshop_history(access.clone()).unwrap().len(), 1);
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let access = reopened.attach("workshop-renderer-2".into()).unwrap();
    let view = reopened.read_workshop(access).unwrap();
    assert_eq!(view.version, saved.version);
    assert_eq!(view.state, saved.state);
}

#[test]
fn start_workshop_creates_blank_anchor_and_replays_before_cas() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    let mut workshop_session = session("session-one");
    workshop_session.anchor_document_id = Some("workshop-session-one".into());
    state.sessions.push(workshop_session);
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-start-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let request = StartWorkshop {
        access: access.clone(),
        operation_id: "workshop-start".into(),
        exploration: WorkshopExploration {
            session_id: "session-one".into(),
            expected_version: saved.version,
            working_generation: "0".into(),
            action: "directions".into(),
            instruction: "Find three possible directions".into(),
            selected_scope: "Whole working version".into(),
            selected_text: String::new(),
            working_selection: None,
        },
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
    };
    let started = project.start_workshop(request.clone()).unwrap();
    assert_eq!(started.run.target.document_id, "workshop-session-one");
    let anchor = project
        .document(access.clone(), "workshop-session-one".into())
        .unwrap();
    assert_eq!(anchor.kind, "note");
    assert_eq!(anchor.head.version, "0");

    let view = project.read_workshop(access.clone()).unwrap();
    assert_eq!(view.results.len(), 1);
    assert_eq!(view.results[0].run.id, started.run.id);
    assert!(view.results[0].output.is_none());
    assert!(!view.results[0].stale);

    let mut changed_budget = request.clone();
    changed_budget.budget = MockContextBudget::new("100000", "101", "100");
    assert_eq!(
        project.start_workshop(changed_budget).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );

    let mut changed = view.state.clone();
    changed.sessions[0].direction = "Keep the first direction".into();
    let changed = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-start-state-two".into(),
            expected_version: view.version,
            state: changed,
        })
        .unwrap();
    assert_ne!(changed.version, request.exploration.expected_version);
    let replay = project.start_workshop(request.clone()).unwrap();
    assert_eq!(replay.run.id, started.run.id);
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let reopened_access = reopened.attach("workshop-reopened".into()).unwrap();
    let reopened_view = reopened.read_workshop(reopened_access.clone()).unwrap();
    assert_eq!(reopened_view.results.len(), 1);
    assert_eq!(reopened_view.results[0].run.id, started.run.id);
    let mut replay_request = request;
    replay_request.access = reopened_access;
    let replay_after_reopen = reopened.start_workshop(replay_request).unwrap();
    assert_eq!(replay_after_reopen.run.id, started.run.id);
}

#[test]
fn recovered_workshop_keeps_historical_results_and_selected_candidates_reviewable() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    let mut workshop_session = session("session-one");
    workshop_session.anchor_document_id = Some("workshop-session-one".into());
    state.sessions.push(workshop_session);
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-recovery-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let started = project
        .start_workshop(StartWorkshop {
            access: access.clone(),
            operation_id: "workshop-recovery-start".into(),
            exploration: WorkshopExploration {
                session_id: "session-one".into(),
                expected_version: saved.version,
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Find three possible directions".into(),
                selected_scope: "Whole working version".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let output = json!({
        "schemaVersion": "story-workshop-output.v1",
        "requestKind": "directions",
        "question": "Which direction fits?",
        "questionReason": "Compare three approaches.",
        "dimension": "Approach",
        "interpretation": {"youSaid":"A seed", "possibleDirection":"A path", "stillOpen":"Its cost"},
        "candidates": [
            {"id":"","title":"One","content":"Direction one","dimensionValue":"one","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["direction"]},
            {"id":"","title":"Two","content":"Direction two","dimensionValue":"two","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["direction"]},
            {"id":"","title":"Three","content":"Direction three","dimensionValue":"three","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["direction"]}
        ]
    });
    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    project.mark_discussion_delivered(owner.clone()).unwrap();
    let completed = project
        .finish_discussion(DiscussionFinish {
            owner,
            expected_sequence: "0".into(),
            event_id: "workshop-recovery-finish".into(),
            assistant_text: serde_json::to_string(&output).unwrap(),
        })
        .unwrap();
    assert_eq!(
        completed.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Completed
    );
    let result_view = project.read_workshop(access.clone()).unwrap();
    let candidate_id = result_view.results[0].output.as_ref().unwrap().candidates[0]
        .id
        .clone();
    let mut selected = result_view.state.clone();
    selected.sessions[0]
        .choices
        .push(webnovel_core::projects::workshop::CandidateChoice {
            candidate_id: candidate_id.clone(),
            status: webnovel_core::projects::workshop::CandidateChoiceStatus::Saved,
            rationale: "Fits the seed".into(),
            include_in_context: true,
        });
    let selected = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-recovery-selection".into(),
            expected_version: result_view.version,
            state: selected,
        })
        .unwrap();
    let backup = temp.0.parent().unwrap().join(format!(
        "wns-workshop-recovery-{}.wnsbackup",
        Uuid::new_v4()
    ));
    create_backup(&project, &backup).unwrap();
    drop(project);
    let recovered_path = temp
        .0
        .parent()
        .unwrap()
        .join(format!("wns-workshop-recovered-{}", Uuid::new_v4()));
    let recovered = recover_backup(&backup, &recovered_path, "Recovered workshop").unwrap();
    let recovered_access = recovered.attach("workshop-recovered".into()).unwrap();
    let recovered_view = recovered.read_workshop(recovered_access.clone()).unwrap();
    assert_eq!(recovered_view.results.len(), 1);
    assert!(recovered_view.results[0].stale);
    assert_eq!(recovered_view.results[0].run.id, completed.id);
    assert_eq!(
        recovered_view.state.sessions[0].choices[0].candidate_id,
        candidate_id
    );
    let mut edited = recovered_view.state.clone();
    edited.sessions[0].direction = "Review the historical option".into();
    let edited = recovered
        .save_workshop(SaveWorkshop {
            access: recovered_access,
            operation_id: "workshop-recovery-edit".into(),
            expected_version: selected.version,
            state: edited,
        })
        .unwrap();
    assert_eq!(edited.version, "3");
    let _ = std::fs::remove_file(backup);
    let _ = std::fs::remove_dir_all(recovered_path);
}

#[test]
fn adoption_is_atomic_nonchapter_and_replayable() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let world = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-world".into(),
            document_id: "world-one".into(),
            title: "The world".into(),
            kind: "world".into(),
            body: body("Keep this"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    let mut workshop_session = session("session-one");
    workshop_session.focus_document_id = Some("world-one".into());
    state.sessions.push(workshop_session);
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-save".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let forged = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: saved.version.clone(),
            candidate_ids: vec!["forged-candidate".into()],
            targets: vec![WorkshopAdoptionTarget {
                document_id: "world-one".into(),
                expected: Some(world.head.clone()),
                title: "The world".into(),
                kind: "world".into(),
                body: body("Keep this, expanded"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Make the setting concrete".into(),
            protected_text: vec!["Keep this".into(), "expanded".into()],
        })
        .unwrap_err();
    assert_eq!(forged.code, "InvalidWorkshopCandidate");
    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "world-one".into(),
                expected: Some(world.head),
                title: "The world".into(),
                kind: "world".into(),
                body: body("Keep this, expanded"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Make the setting concrete".into(),
            protected_text: vec!["Keep this".into()],
        })
        .unwrap();
    let first = project
        .adopt_workshop(access.clone(), "workshop-adopt".into(), preview.id.clone())
        .unwrap();
    assert_eq!(first.documents.len(), 1);
    assert_eq!(first.decision_ids.len(), 1);
    let replay = project
        .adopt_workshop(access.clone(), "workshop-adopt".into(), preview.id)
        .unwrap();
    assert_eq!(replay.snapshot, first.snapshot);
    assert_eq!(replay.decision_ids, first.decision_ids);
    assert_eq!(replay.documents[0].head, first.documents[0].head);
    let after_first = project.read_workshop(access.clone()).unwrap();
    let second_preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: after_first.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "world-one".into(),
                expected: Some(first.documents[0].head.clone()),
                title: "The world".into(),
                kind: "world".into(),
                body: body("Keep this, expanded again"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Refine the setting".into(),
            protected_text: vec!["Keep this".into()],
        })
        .unwrap();
    let second = project
        .adopt_workshop(
            access.clone(),
            "workshop-adopt-two".into(),
            second_preview.id,
        )
        .unwrap();
    let final_view = project.read_workshop(access.clone()).unwrap();
    let decisions = &final_view.state.decisions;
    assert_eq!(
        decisions
            .iter()
            .filter(
                |d| d.status == webnovel_core::projects::workshop::WorkshopDecisionStatus::Chosen
            )
            .count(),
        1
    );
    assert_eq!(
        decisions
            .iter()
            .filter(|d| d.status
                == webnovel_core::projects::workshop::WorkshopDecisionStatus::Superseded)
            .count(),
        1
    );
    assert_eq!(
        project
            .history(access.clone(), "world-one".into())
            .unwrap()
            .len(),
        3
    );
    assert_eq!(second.documents[0].head.version, "2");
    assert_eq!(project.documents(access).unwrap().len(), 1);
}

#[test]
fn chapter_targets_are_rejected_before_preview() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let chapter = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter".into(),
            kind: "chapter".into(),
            body: body("Do not touch"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("session-one"));
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let error = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access,
            session_id: "session-one".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "chapter-one".into(),
                expected: Some(chapter.head),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body("bad"),
                mode: AdoptionMode::Replace,
            }],
            rationale: String::new(),
            protected_text: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidDocument");
}

#[test]
fn stale_unrelated_relationship_remains_reviewable_without_blocking_other_work() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-renderer".into()).unwrap();
    let first = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-related-one".into(),
            document_id: "related-one".into(),
            title: "Related one".into(),
            kind: "world".into(),
            body: body("One"),
        })
        .unwrap();
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-related-two".into(),
            document_id: "related-two".into(),
            title: "Related two".into(),
            kind: "world".into(),
            body: body("Two"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("session-one"));
    state.relationships.push(WorkshopRelationship {
        id: "relationship-one".into(),
        from_document_id: first.head.document_id.clone(),
        to_document_id: second.head.document_id.clone(),
        relationship_type: "depends-on".into(),
        description: "One depends on two".into(),
        uncertainty: String::new(),
        status: WorkshopRelationshipStatus::Tentative,
        source_heads: vec![first.head.clone(), second.head.clone()],
    });
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-related-state".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap();
    let updated = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-related-one".into(),
            expected: first.head,
            local_generation: "1".into(),
            body: body("One changed"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert_ne!(updated.head.version, "0");

    state.sessions[0].direction = "Keep developing another area".into();
    let saved_again = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-unrelated-state".into(),
            expected_version: saved.version,
            state,
        })
        .unwrap();
    assert_eq!(saved_again.version, "2");

    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access,
            session_id: "session-one".into(),
            expected_version: saved_again.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "new-world".into(),
                expected: None,
                title: "New world".into(),
                kind: "world".into(),
                body: body("A separate world"),
                mode: AdoptionMode::Add,
            }],
            rationale: "Explore independently".into(),
            protected_text: Vec::new(),
        })
        .unwrap();
    assert_eq!(preview.targets[0].document_id, "new-world");
}
