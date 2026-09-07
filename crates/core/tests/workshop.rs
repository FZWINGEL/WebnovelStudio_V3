use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::workshop::{
    AdoptionMode, Lens, PreferencePolarity, PreferenceScope, PreferenceStrength,
    PreviewWorkshopAdoption, SaveWorkshop, WorkshopAdoptionTarget, WorkshopBranchKind,
    WorkshopDecisionStatus, WorkshopDepth, WorkshopImpactDraft, WorkshopImpactKind,
    WorkshopImpactStatus, WorkshopPreference, WorkshopRelationship, WorkshopRelationshipDraft,
    WorkshopRelationshipStatus, WorkshopSession, WorkshopState,
};
use webnovel_core::projects::workshop_generation::{StartWorkshop, WorkshopExploration};
use webnovel_core::projects::{CreateDocument, Head, ProjectSession, SaveCause, SaveSnapshot};
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
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
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
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
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
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
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
    assert_eq!(project.documents(access.clone()).unwrap().len(), 1);

    let mut archived = final_view.state.clone();
    let previous_id = first.decision_ids[0].clone();
    archived
        .decisions
        .iter_mut()
        .find(|decision| decision.id == previous_id)
        .expect("previous decision")
        .status = WorkshopDecisionStatus::Archived;
    let archived_snapshot = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-archive-previous".into(),
            expected_version: final_view.version.clone(),
            state: archived,
        })
        .unwrap();
    assert_eq!(
        archived_snapshot
            .state
            .decisions
            .iter()
            .filter(|decision| decision.status == WorkshopDecisionStatus::Chosen)
            .count(),
        1
    );

    let mut conflicting = archived_snapshot.state.clone();
    conflicting
        .decisions
        .iter_mut()
        .find(|decision| decision.id == previous_id)
        .expect("archived previous decision")
        .status = WorkshopDecisionStatus::Chosen;
    let error = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-repromote-previous".into(),
            expected_version: archived_snapshot.version.clone(),
            state: conflicting,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidRequest");
    assert!(error.detail.contains("more than one chosen"));
    let after_rejection = project.read_workshop(access.clone()).unwrap();
    assert_eq!(after_rejection.version, archived_snapshot.version);
    assert_eq!(after_rejection.state, archived_snapshot.state);
}

#[test]
fn hard_preference_conflicts_normalize_confirmed_labels_and_ignore_neutral_polarity() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-preference-conflicts".into())
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("session-one"));
    let project_preference = WorkshopPreference {
        id: "project-preference".into(),
        label: "  Élan  ".into(),
        family: "structure".into(),
        meaning: "Keep the story moving".into(),
        examples: String::new(),
        timing: String::new(),
        polarity: PreferencePolarity::Want,
        strength: PreferenceStrength::Hard,
        scope: PreferenceScope::Project,
        target_id: None,
        confirmed: true,
    };
    let local_preference = WorkshopPreference {
        id: "local-preference".into(),
        label: "élan".into(),
        family: "structure".into(),
        meaning: "Avoid slowing this exploration".into(),
        examples: String::new(),
        timing: String::new(),
        polarity: PreferencePolarity::Avoid,
        strength: PreferenceStrength::Soft,
        scope: PreferenceScope::Exploration,
        target_id: Some("session-one".into()),
        confirmed: true,
    };
    state.preferences = vec![project_preference, local_preference];
    let conflict = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-preference-conflict".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap_err();
    assert_eq!(conflict.code, "PreferenceConflict");

    state.preferences[1].confirmed = false;
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-preference-unconfirmed".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap();
    assert_eq!(saved.version, "1");

    state.preferences[1].confirmed = true;
    state.preferences[1].polarity = PreferencePolarity::Neutral;
    state.preferences.push(WorkshopPreference {
        id: "second-project-preference".into(),
        label: " ÉLAN ".into(),
        family: "structure".into(),
        meaning: "Slow the story down".into(),
        examples: String::new(),
        timing: String::new(),
        polarity: PreferencePolarity::Avoid,
        strength: PreferenceStrength::Soft,
        scope: PreferenceScope::Project,
        target_id: None,
        confirmed: true,
    });
    let second_project_conflict = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-second-project-conflict".into(),
            expected_version: saved.version.clone(),
            state: state.clone(),
        })
        .unwrap_err();
    assert_eq!(second_project_conflict.code, "PreferenceConflict");

    state.preferences[1].polarity = PreferencePolarity::Neutral;
    state.preferences[2].polarity = PreferencePolarity::Neutral;
    let neutral = project
        .save_workshop(SaveWorkshop {
            access,
            operation_id: "workshop-preference-neutral".into(),
            expected_version: saved.version,
            state,
        })
        .unwrap();
    assert_eq!(neutral.version, "2");
}

#[test]
fn adoption_creates_new_linked_endpoints_with_exact_committed_heads() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-relationships".into()).unwrap();
    let existing = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-existing-character".into(),
            document_id: "existing-character".into(),
            title: "Existing character".into(),
            kind: "character".into(),
            body: body("The existing character."),
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
            operation_id: "relationship-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "new-world".into(),
                expected: None,
                title: "New world".into(),
                kind: "world".into(),
                body: body("The new world."),
                mode: AdoptionMode::Add,
            }],
            rationale: "Connect the new world to the existing character".into(),
            protected_text: Vec::new(),
            relationships: vec![WorkshopRelationshipDraft {
                id: "relationship-new-world".into(),
                from_document_id: existing.head.document_id.clone(),
                to_document_id: "new-world".into(),
                relationship_type: "depends-on".into(),
                description: "The character depends on the new world's repair tradition.".into(),
                uncertainty: "The cost remains uncertain.".into(),
                from_expected: Some(existing.head.clone()),
                to_expected: None,
            }],
            impact_drafts: Vec::new(),
        })
        .unwrap();
    assert_eq!(preview.relationships.len(), 1);
    assert_eq!(
        preview.relationships[0].status,
        WorkshopRelationshipStatus::Chosen
    );
    assert_eq!(preview.relationships[0].source_heads[0], existing.head);
    assert_eq!(preview.relationships[0].source_heads[1].version, "0");
    assert_eq!(preview.endpoint_sources.len(), 1);
    assert_eq!(preview.endpoint_sources[0].head, existing.head);
    let adopted = project
        .adopt_workshop(access.clone(), "adopt-linked-world".into(), preview.id)
        .unwrap();
    let new_world = project
        .document(access.clone(), "new-world".into())
        .unwrap();
    assert_eq!(adopted.documents.len(), 1);
    assert_eq!(new_world.head.version, "0");
    let view = project.read_workshop(access).unwrap();
    let relationship = view
        .state
        .relationships
        .iter()
        .find(|relationship| relationship.id == "relationship-new-world")
        .unwrap();
    assert_eq!(relationship.status, WorkshopRelationshipStatus::Chosen);
    assert_eq!(relationship.source_heads[0], existing.head);
    assert_eq!(relationship.source_heads[1], new_world.head);
}

#[test]
fn relationship_drafts_require_descriptions_and_null_heads_for_new_endpoints() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-relationship-validation".into())
        .unwrap();
    let existing = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-validation-character".into(),
            document_id: "validation-character".into(),
            title: "Validation character".into(),
            kind: "character".into(),
            body: body("Existing"),
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
            operation_id: "relationship-validation-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let request = PreviewWorkshopAdoption {
        access: access.clone(),
        session_id: "session-one".into(),
        expected_version: saved.version,
        candidate_ids: Vec::new(),
        targets: vec![WorkshopAdoptionTarget {
            document_id: "validation-world".into(),
            expected: None,
            title: "Validation world".into(),
            kind: "world".into(),
            body: body("New"),
            mode: AdoptionMode::Add,
        }],
        rationale: "Validate relationship request boundaries".into(),
        protected_text: Vec::new(),
        relationships: vec![WorkshopRelationshipDraft {
            id: "validation-edge".into(),
            from_document_id: existing.head.document_id.clone(),
            to_document_id: "validation-world".into(),
            relationship_type: "knows".into(),
            description: "Valid description".into(),
            uncertainty: String::new(),
            from_expected: Some(existing.head.clone()),
            to_expected: None,
        }],
        impact_drafts: Vec::new(),
    };
    let mut blank_description = request.clone();
    blank_description.relationships[0].description = "  ".into();
    assert_eq!(
        project
            .preview_workshop_adoption(blank_description)
            .unwrap_err()
            .code,
        "InvalidRequest"
    );
    let mut forged_new_head = request;
    forged_new_head.relationships[0].to_expected = Some(Head {
        document_id: "validation-world".into(),
        version: "0".into(),
        body_hash: existing.head.body_hash,
    });
    assert_eq!(
        project
            .preview_workshop_adoption(forged_new_head)
            .unwrap_err()
            .code,
        "InvalidRequest"
    );
}

#[test]
fn stale_relationship_endpoint_rolls_back_all_adoption_targets() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-stale-relationship".into())
        .unwrap();
    let from = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-stale-from".into(),
            document_id: "stale-from".into(),
            title: "From".into(),
            kind: "character".into(),
            body: body("From"),
        })
        .unwrap();
    let to = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-stale-to".into(),
            document_id: "stale-to".into(),
            title: "To".into(),
            kind: "world".into(),
            body: body("To"),
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
            operation_id: "stale-relationship-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "should-not-appear".into(),
                expected: None,
                title: "Should not appear".into(),
                kind: "world".into(),
                body: body("Atomic"),
                mode: AdoptionMode::Add,
            }],
            rationale: "This must remain atomic".into(),
            protected_text: Vec::new(),
            relationships: vec![WorkshopRelationshipDraft {
                id: "stale-edge".into(),
                from_document_id: from.head.document_id.clone(),
                to_document_id: to.head.document_id.clone(),
                relationship_type: "knows".into(),
                description: "The edge is stale.".into(),
                uncertainty: String::new(),
                from_expected: Some(from.head.clone()),
                to_expected: Some(to.head.clone()),
            }],
            impact_drafts: Vec::new(),
        })
        .unwrap();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-stale-to".into(),
            expected: to.head,
            local_generation: "1".into(),
            body: body("To changed"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let error = project
        .adopt_workshop(access.clone(), "stale-edge-adopt".into(), preview.id)
        .unwrap_err();
    assert_eq!(error.code, "VersionConflict");
    assert_eq!(project.read_workshop(access.clone()).unwrap().version, "1");
    assert_eq!(
        project
            .document(access, "should-not-appear".into())
            .unwrap_err()
            .code,
        "DocumentNotFound"
    );
}

#[test]
fn relationship_impacts_follow_the_changed_endpoint_decision() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project
        .attach("workshop-relationship-impact".into())
        .unwrap();
    let from = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-impact-from".into(),
            document_id: "impact-from".into(),
            title: "Impact from".into(),
            kind: "character".into(),
            body: body("From"),
        })
        .unwrap();
    let to = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-impact-to".into(),
            document_id: "impact-to".into(),
            title: "Impact to".into(),
            kind: "world".into(),
            body: body("To"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("session-one"));
    state.relationships.push(WorkshopRelationship {
        id: "impact-relationship".into(),
        from_document_id: from.head.document_id.clone(),
        to_document_id: to.head.document_id.clone(),
        relationship_type: "depends-on".into(),
        description: "From depends on to".into(),
        uncertainty: String::new(),
        status: WorkshopRelationshipStatus::Chosen,
        source_heads: vec![from.head.clone(), to.head.clone()],
    });
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-impact-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: to.head.document_id.clone(),
                expected: Some(to.head.clone()),
                title: to.title.clone(),
                kind: to.kind.clone(),
                body: body("To revised"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Revise the destination while reviewing the relationship".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();
    let adopted = project
        .adopt_workshop(
            access.clone(),
            "relationship-impact-adopt".into(),
            preview.id,
        )
        .unwrap();
    let decision = adopted
        .snapshot
        .state
        .decisions
        .iter()
        .find(|decision| decision.document_id == "impact-to")
        .unwrap();
    let impact = adopted
        .snapshot
        .state
        .impacts
        .iter()
        .find(|impact| impact.relationship_id.as_deref() == Some("impact-relationship"))
        .unwrap();
    assert_eq!(impact.document_id, "impact-to");
    assert_eq!(impact.decision_id, decision.id);
    assert_eq!(impact.candidate_id, None);
    assert!(impact.reason.contains("impact-relationship"));
    assert!(impact.reason.contains("impact-to"));
}

#[test]
fn candidate_impacts_are_reviewable_without_rewriting_affected_documents() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-impacts".into()).unwrap();
    let affected = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-impact-world".into(),
            document_id: "impact-world".into(),
            title: "Impact world".into(),
            kind: "world".into(),
            body: body("Original"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("session-one".into()),
        ..WorkshopState::default()
    };
    let mut workshop_session = session("session-one");
    workshop_session.focus_document_id = Some(affected.head.document_id.clone());
    workshop_session.anchor_document_id = Some("workshop-impact-session".into());
    state.sessions.push(workshop_session);
    let saved = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "impact-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let started = project
        .start_workshop(StartWorkshop {
            access: access.clone(),
            operation_id: "impact-start".into(),
            exploration: WorkshopExploration {
                session_id: "session-one".into(),
                expected_version: saved.version,
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Find three directions".into(),
                selected_scope: "Whole working version".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let output = json!({
        "schemaVersion":"story-workshop-output.v1",
        "requestKind":"directions",
        "question":"Which direction fits?",
        "questionReason":"Compare the directions.",
        "dimension":"Approach",
        "interpretation":{"youSaid":"A seed","possibleDirection":"A path","stillOpen":"Its cost"},
        "candidates":[
            {"id":"","title":"One","content":"Direction one","dimensionValue":"one","implications":[],"assumptions":[],"affectedTargets":[{"documentId":"impact-world","reason":"May alter the foundational rule."},{"documentId":"new-impact-world","reason":"Creates a direct contradiction if adopted."},{"documentId":"workshop-impact-session","reason":"Internal anchor should never become story material."}],"preservedDetails":[],"changedDetails":["direction"]},
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
    project
        .finish_discussion(DiscussionFinish {
            owner,
            expected_sequence: "0".into(),
            event_id: "impact-finish".into(),
            assistant_text: serde_json::to_string(&output).unwrap(),
        })
        .unwrap();
    let result = project.read_workshop(access.clone()).unwrap();
    let packet_id = result.results[0].run.packet_id.clone();
    let candidate_id = result.results[0].output.as_ref().unwrap().candidates[0]
        .id
        .clone();
    let mut selected = result.state;
    selected.sessions[0]
        .choices
        .push(webnovel_core::projects::workshop::CandidateChoice {
            candidate_id: candidate_id.clone(),
            status: webnovel_core::projects::workshop::CandidateChoiceStatus::Saved,
            rationale: "Use this direction".into(),
            include_in_context: false,
        });
    let selected = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "impact-selection".into(),
            expected_version: result.version,
            state: selected,
        })
        .unwrap();
    let preview = project
        .preview_workshop_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "session-one".into(),
            expected_version: selected.version,
            candidate_ids: vec![candidate_id.clone()],
            targets: vec![WorkshopAdoptionTarget {
                document_id: "new-impact-world".into(),
                expected: None,
                title: "New impact world".into(),
                kind: "world".into(),
                body: body("A new connected place"),
                mode: AdoptionMode::Add,
            }],
            rationale: "Adopt the direction while reviewing its consequence".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: vec![
                WorkshopImpactDraft {
                    document_id: "new-impact-world".into(),
                    kind: WorkshopImpactKind::Contradiction,
                    reason: "The adopted direction contradicts the new world's stated rule.".into(),
                },
                WorkshopImpactDraft {
                    document_id: "workshop-impact-session".into(),
                    kind: WorkshopImpactKind::Contradiction,
                    reason: "A stale renderer annotation for the hidden anchor.".into(),
                },
            ],
        })
        .unwrap();
    assert_eq!(preview.impacts.len(), 2);
    let default_impact = preview
        .impacts
        .iter()
        .find(|impact| impact.document_id == "impact-world")
        .unwrap();
    assert_eq!(default_impact.kind, WorkshopImpactKind::PossibleTension);
    assert_eq!(default_impact.status, WorkshopImpactStatus::NeedsReview);
    let explicit_impact = preview
        .impacts
        .iter()
        .find(|impact| impact.document_id == "new-impact-world")
        .unwrap();
    assert_eq!(explicit_impact.kind, WorkshopImpactKind::Contradiction);
    assert_eq!(explicit_impact.status, WorkshopImpactStatus::NeedsReview);
    let adopted = project
        .adopt_workshop(access.clone(), "impact-adopt".into(), preview.id)
        .unwrap();
    assert_eq!(adopted.documents.len(), 1);
    let unchanged = project
        .document(access.clone(), "impact-world".into())
        .unwrap();
    assert_eq!(
        unchanged.body["body"]["content"][0]["content"][0]["text"],
        "Original"
    );
    let anchor = project
        .document(access.clone(), "workshop-impact-session".into())
        .unwrap();
    assert_eq!(anchor.kind, "note");
    assert_eq!(anchor.head.version, "0");
    let view = project.read_workshop(access).unwrap();
    assert!(view.state.impacts.iter().any(|impact| {
        impact.document_id == "impact-world"
            && impact.kind == WorkshopImpactKind::PossibleTension
            && impact.status == WorkshopImpactStatus::NeedsReview
            && impact.candidate_id.as_deref() == Some(candidate_id.as_str())
            && impact.relationship_id.is_none()
    }));
    assert!(view.state.impacts.iter().any(|impact| {
        impact.document_id == "new-impact-world"
            && impact.kind == WorkshopImpactKind::Contradiction
            && impact.status == WorkshopImpactStatus::NeedsReview
            && impact.candidate_id.as_deref() == Some(candidate_id.as_str())
            && impact.relationship_id.is_none()
    }));
    assert_eq!(view.results[0].run.packet_id, packet_id);
    assert!(
        view.results[0].output.as_ref().unwrap().candidates[0]
            .affected_targets
            .iter()
            .any(|target| target.document_id == "workshop-impact-session")
    );
    assert!(
        !view
            .state
            .impacts
            .iter()
            .any(|impact| impact.document_id == "workshop-impact-session")
    );
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
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
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
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();
    assert_eq!(preview.targets[0].document_id, "new-world");
}
