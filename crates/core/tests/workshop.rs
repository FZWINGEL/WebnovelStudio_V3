use serde_json::{Value, json};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
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
        relationship_id: None,
        story_possibilities: Vec::new(),
    }
}

fn what_if_session(id: &str, parent_session_id: &str) -> WorkshopSession {
    let mut child = session(id);
    child.parent_session_id = Some(parent_session_id.into());
    child.branch_kind = WorkshopBranchKind::WhatIf;
    child
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
    let saved = project.workshop().save(request.clone()).unwrap();
    assert_eq!(saved.version, "1");
    assert_eq!(project.workshop().save(request).unwrap(), saved);
    let view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(view.version, saved.version);
    assert_eq!(view.state, saved.state);
    assert_eq!(project.workshop().history(access.clone()).unwrap().len(), 1);
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let access = reopened.attach("workshop-renderer-2".into()).unwrap();
    let view = reopened.workshop().read(access).unwrap();
    assert_eq!(view.version, saved.version);
    assert_eq!(view.state, saved.state);
}

#[test]
fn workshop_branch_graph_rejects_invalid_shapes_atomically() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-branches".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("working-root".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("working-root"));
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-root".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();

    let reject = |operation_id: &str, candidate: WorkshopState, detail: &str| {
        let error = project
            .workshop().save(SaveWorkshop {
                access: access.clone(),
                operation_id: operation_id.into(),
                expected_version: saved.version.clone(),
                state: candidate,
            })
            .unwrap_err();
        assert_eq!(error.code, "InvalidRequest");
        assert!(error.detail.contains(detail), "{}", error.detail);
        let current = project.workshop().read(access.clone()).unwrap();
        assert_eq!(current.version, saved.version);
        assert_eq!(current.state, saved.state);
    };

    let mut working_with_parent = saved.state.clone();
    working_with_parent.sessions[0].parent_session_id = Some("working-root".into());
    reject(
        "branch-working-parent",
        working_with_parent,
        "working workshop session cannot have a parent",
    );

    let mut what_if_without_parent = saved.state.clone();
    let mut orphan = session("orphan-what-if");
    orphan.branch_kind = WorkshopBranchKind::WhatIf;
    what_if_without_parent.sessions.push(orphan);
    reject(
        "branch-what-if-without-parent",
        what_if_without_parent,
        "what-if workshop session must have a parent",
    );

    let mut missing_parent = saved.state.clone();
    missing_parent
        .sessions
        .push(what_if_session("missing-parent", "does-not-exist"));
    reject(
        "branch-missing-parent",
        missing_parent,
        "what-if workshop session has an unknown parent",
    );

    let mut self_parent = saved.state.clone();
    self_parent
        .sessions
        .push(what_if_session("self-parent", "self-parent"));
    reject(
        "branch-self-parent",
        self_parent,
        "what-if workshop session cannot parent itself",
    );

    let mut long_cycle = saved.state.clone();
    for index in 0..8 {
        let parent = if index == 0 {
            "working-root".to_string()
        } else {
            format!("fork-{}", index - 1)
        };
        long_cycle
            .sessions
            .push(what_if_session(&format!("fork-{index}"), &parent));
    }
    long_cycle.sessions[1].parent_session_id = Some("fork-7".into());
    reject(
        "branch-long-cycle",
        long_cycle,
        "session parents cannot contain a cycle",
    );
}

#[test]
fn nested_what_if_branches_persist_and_parent_edits_do_not_rewrite_ancestors() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-nested-branches".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("working-root".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("working-root"));
    state
        .sessions
        .push(what_if_session("fork-one", "working-root"));
    state.sessions.push(what_if_session("fork-two", "fork-one"));
    state
        .sessions
        .push(what_if_session("fork-three", "fork-two"));
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "nested-branches-save".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();

    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let reopened_access = reopened.attach("workshop-nested-reopened".into()).unwrap();
    let reopened_view = reopened.workshop().read(reopened_access.clone()).unwrap();
    assert_eq!(reopened_view.version, saved.version);
    for (id, parent) in [
        ("working-root", None),
        ("fork-one", Some("working-root")),
        ("fork-two", Some("fork-one")),
        ("fork-three", Some("fork-two")),
    ] {
        let branch = reopened_view
            .state
            .sessions
            .iter()
            .find(|session| session.id == id)
            .unwrap();
        assert_eq!(branch.parent_session_id.as_deref(), parent);
        assert_eq!(
            branch.branch_kind,
            if parent.is_some() {
                WorkshopBranchKind::WhatIf
            } else {
                WorkshopBranchKind::Working
            }
        );
    }

    let mut edited = reopened_view.state.clone();
    edited
        .sessions
        .iter_mut()
        .find(|session| session.id == "fork-three")
        .unwrap()
        .direction = "Only the deepest branch changes".into();
    let edited_snapshot = reopened
        .workshop().save(SaveWorkshop {
            access: reopened_access.clone(),
            operation_id: "nested-branches-edit".into(),
            expected_version: reopened_view.version,
            state: edited,
        })
        .unwrap();
    for id in ["working-root", "fork-one", "fork-two"] {
        let before = reopened_view
            .state
            .sessions
            .iter()
            .find(|session| session.id == id)
            .unwrap();
        let after = edited_snapshot
            .state
            .sessions
            .iter()
            .find(|session| session.id == id)
            .unwrap();
        assert_eq!(before, after);
    }
    assert_eq!(edited_snapshot.version, "2");
}

#[test]
fn existing_branch_identity_cannot_be_reparented_or_flipped_but_navigation_is_allowed() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-branch-identity".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("working-root".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("working-root"));
    state
        .sessions
        .push(what_if_session("fork-child", "working-root"));
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-identity-save".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();

    let mut reparented = saved.state.clone();
    reparented
        .sessions
        .push(what_if_session("fork-sibling", "working-root"));
    reparented
        .sessions
        .iter_mut()
        .find(|session| session.id == "fork-child")
        .unwrap()
        .parent_session_id = Some("fork-sibling".into());
    let reparent_error = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-identity-reparent".into(),
            expected_version: saved.version.clone(),
            state: reparented,
        })
        .unwrap_err();
    assert_eq!(reparent_error.code, "InvalidRequest");
    assert!(
        reparent_error
            .detail
            .contains("branch identity is immutable")
    );

    let mut flipped = saved.state.clone();
    let child = flipped
        .sessions
        .iter_mut()
        .find(|session| session.id == "fork-child")
        .unwrap();
    child.parent_session_id = None;
    child.branch_kind = WorkshopBranchKind::Working;
    let flip_error = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-identity-flip".into(),
            expected_version: saved.version.clone(),
            state: flipped,
        })
        .unwrap_err();
    assert_eq!(flip_error.code, "InvalidRequest");
    assert!(flip_error.detail.contains("branch identity is immutable"));

    let mut navigated = saved.state.clone();
    navigated.current_session_id = Some("fork-child".into());
    let navigation = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-identity-navigation".into(),
            expected_version: saved.version,
            state: navigated,
        })
        .unwrap();
    assert_eq!(navigation.version, "2");
    assert_eq!(
        navigation.state.current_session_id.as_deref(),
        Some("fork-child")
    );
    let after = project.workshop().read(access).unwrap();
    assert_eq!(
        after.state.sessions[1].parent_session_id.as_deref(),
        Some("working-root")
    );
    assert_eq!(
        after.state.sessions[1].branch_kind,
        WorkshopBranchKind::WhatIf
    );
}

#[test]
fn fixed_selected_details_are_scoped_to_the_adoption_session() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-fixed-branches".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-fixed-target".into(),
            document_id: "fixed-target".into(),
            title: "Fixed target".into(),
            kind: "world".into(),
            body: body("Parent phrase"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("working-root".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("working-root"));
    let mut child = what_if_session("fork-child", "working-root");
    child
        .selected_details
        .push(webnovel_core::projects::workshop::SelectedDetail {
            id: "child-fixed-detail".into(),
            candidate_id: None,
            text: "Parent phrase".into(),
            fixed: true,
        });
    state.sessions.push(child);
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "fixed-branch-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();

    let parent_preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "working-root".into(),
            expected_version: saved.version.clone(),
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "fixed-target".into(),
                expected: Some(document.head.clone()),
                title: "Fixed target".into(),
                kind: "world".into(),
                body: body("Changed by parent"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Change the parent story only".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();
    assert_eq!(parent_preview.targets[0].document_id, "fixed-target");

    let child_error = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "fork-child".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "fixed-target".into(),
                expected: Some(document.head),
                title: "Fixed target".into(),
                kind: "world".into(),
                body: body("Changed by child"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Try to remove the fixed detail".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(child_error.code, "ProtectedContentChanged");
    let adopted = project
        .workshop().adopt(
            access.clone(),
            "fixed-parent-adopt".into(),
            parent_preview.id,
        )
        .unwrap();
    assert_eq!(adopted.documents[0].head.document_id, "fixed-target");
    let changed = project
        .document(access.clone(), "fixed-target".into())
        .unwrap();
    assert_eq!(
        changed.body["body"]["content"][0]["content"][0]["text"],
        "Changed by parent"
    );
    let after = project.workshop().read(access).unwrap();
    let child_after = after
        .state
        .sessions
        .iter()
        .find(|session| session.id == "fork-child")
        .unwrap();
    assert_eq!(child_after.selected_details[0].text, "Parent phrase");
    assert!(child_after.selected_details[0].fixed);
}

#[test]
fn child_generation_includes_chosen_ancestor_material_but_excludes_siblings() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-branch-context".into()).unwrap();
    let parent_document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-parent-canon".into(),
            document_id: "parent-canon".into(),
            title: "Parent canon".into(),
            kind: "world".into(),
            body: body("Parent canon body"),
        })
        .unwrap();
    let sibling_document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-sibling-canon".into(),
            document_id: "sibling-canon".into(),
            title: "Sibling canon".into(),
            kind: "world".into(),
            body: body("Sibling canon body"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("working-root".into()),
        ..WorkshopState::default()
    };
    state.sessions.push(session("working-root"));
    let mut child = what_if_session("fork-child", "working-root");
    child.anchor_document_id = Some("workshop-fork-child".into());
    state.sessions.push(child);
    state
        .sessions
        .push(what_if_session("fork-sibling", "working-root"));
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "branch-context-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let parent_preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "working-root".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "parent-canon".into(),
                expected: Some(parent_document.head.clone()),
                title: "Parent canon".into(),
                kind: "world".into(),
                body: body("Parent canon adopted body"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Adopt the parent canon".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();
    project
        .workshop().adopt(
            access.clone(),
            "branch-context-parent-adopt".into(),
            parent_preview.id,
        )
        .unwrap();
    let after_parent = project.workshop().read(access.clone()).unwrap();
    let sibling_preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "fork-sibling".into(),
            expected_version: after_parent.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: "sibling-canon".into(),
                expected: Some(sibling_document.head.clone()),
                title: "Sibling canon".into(),
                kind: "world".into(),
                body: body("Sibling canon adopted body"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Adopt the sibling canon".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();
    project
        .workshop().adopt(
            access.clone(),
            "branch-context-sibling-adopt".into(),
            sibling_preview.id,
        )
        .unwrap();
    let after_sibling = project.workshop().read(access.clone()).unwrap();
    let started = project
        .workshop().start(StartWorkshop {
            access,
            operation_id: "branch-context-start".into(),
            exploration: WorkshopExploration {
                session_id: "fork-child".into(),
                expected_version: after_sibling.version,
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Compare child directions".into(),
                selected_scope: "Whole working version".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let metadata = webnovel_core::projects::workshop_generation::metadata_from_instruction(
        &started.packet.messages.last().unwrap().content,
    )
    .unwrap();
    assert!(metadata.chosen_details.iter().any(|detail| {
        detail.contains("Parent canon") && detail.contains("Parent canon adopted body")
    }));
    assert!(
        !metadata
            .chosen_details
            .iter()
            .any(|detail| detail.contains("Sibling canon"))
    );
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
        .workshop().save(SaveWorkshop {
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
    let started = project.workshop().start(request.clone()).unwrap();
    assert_eq!(started.run.target.document_id, "workshop-session-one");
    let legacy_user: Value = serde_json::from_str(
        &started
            .packet
            .messages
            .last()
            .expect("workshop request message")
            .content,
    )
    .unwrap();
    assert!(legacy_user["workshop"].get("relationship").is_none());
    let anchor = project
        .document(access.clone(), "workshop-session-one".into())
        .unwrap();
    assert_eq!(anchor.kind, "note");
    assert_eq!(anchor.head.version, "0");

    let view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(view.results.len(), 1);
    assert_eq!(view.results[0].run.id, started.run.id);
    assert!(view.results[0].output.is_none());
    assert!(!view.results[0].stale);

    let mut changed_budget = request.clone();
    changed_budget.budget = MockContextBudget::new("100000", "101", "100");
    assert_eq!(
        project.workshop().start(changed_budget).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );

    let mut changed = view.state.clone();
    changed.sessions[0].direction = "Keep the first direction".into();
    let changed = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-start-state-two".into(),
            expected_version: view.version,
            state: changed,
        })
        .unwrap();
    assert_ne!(changed.version, request.exploration.expected_version);
    let replay = project.workshop().start(request.clone()).unwrap();
    assert_eq!(replay.run.id, started.run.id);
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let reopened_access = reopened.attach("workshop-reopened".into()).unwrap();
    let reopened_view = reopened.workshop().read(reopened_access.clone()).unwrap();
    assert_eq!(reopened_view.results.len(), 1);
    assert_eq!(reopened_view.results[0].run.id, started.run.id);
    let mut replay_request = request;
    replay_request.access = reopened_access;
    let replay_after_reopen = reopened.workshop().start(replay_request).unwrap();
    assert_eq!(replay_after_reopen.run.id, started.run.id);
}

#[test]
fn relationship_exploration_freezes_typed_edge_and_pins_both_endpoints() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-relationship-generation".into()).unwrap();
    let from = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-generation-from".into(),
            document_id: "relationship-from".into(),
            title: "Mira".into(),
            kind: "character".into(),
            body: body("Mira keeps the old promise."),
        })
        .unwrap();
    let to = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-generation-to".into(),
            document_id: "relationship-to".into(),
            title: "The Lantern Court".into(),
            kind: "world".into(),
            body: body("The court remembers every oath."),
        })
        .unwrap();
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-generation-unrelated".into(),
            document_id: "unrelated-document".into(),
            title: "Unrelated note".into(),
            kind: "note".into(),
            body: body("This note is outside the relationship."),
        })
        .unwrap();
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-generation-normal-anchor".into(),
            document_id: "workshop-normal-session".into(),
            title: "Normal focus anchor".into(),
            kind: "note".into(),
            body: body("Anchor"),
        })
        .unwrap();
    let mut workshop_session = session("relationship-session");
    workshop_session.anchor_document_id = Some("workshop-relationship-session".into());
    workshop_session.relationship_id = Some("relationship-one".into());
    workshop_session.composer = "Do not let the relationship record replace the author's question."
        .into();
    let mut state = WorkshopState {
        current_session_id: Some(workshop_session.id.clone()),
        ..WorkshopState::default()
    };
    state.sessions.push(workshop_session);
    state.relationships.push(WorkshopRelationship {
        id: "relationship-one".into(),
        from_document_id: from.head.document_id.clone(),
        to_document_id: to.head.document_id.clone(),
        relationship_type: "owes a hidden debt to".into(),
        description: "Mira's oath binds the court's gatekeeper, but neither side knows the full price.".into(),
        uncertainty: "The debt may be inherited rather than chosen.".into(),
        status: WorkshopRelationshipStatus::Tentative,
        source_heads: vec![from.head.clone(), to.head.clone()],
    });
    let mut normal_session = session("normal-session");
    normal_session.anchor_document_id = Some("workshop-normal-session".into());
    normal_session.focus_document_id = Some(from.head.document_id.clone());
    state.sessions.push(normal_session);
    state.preferences = vec![
        WorkshopPreference {
            id: "relationship-from-preference".into(),
            label: "Keep Mira close to the oath".into(),
            family: "voice".into(),
            meaning: "Stay near Mira's felt experience.".into(),
            examples: String::new(),
            timing: String::new(),
            polarity: PreferencePolarity::Want,
            strength: PreferenceStrength::Soft,
            scope: PreferenceScope::Element,
            target_id: Some(from.head.document_id.clone()),
            confirmed: true,
        },
        WorkshopPreference {
            id: "relationship-to-preference".into(),
            label: "Keep the court uncertain".into(),
            family: "tone".into(),
            meaning: "Do not resolve the court's motives too quickly.".into(),
            examples: String::new(),
            timing: String::new(),
            polarity: PreferencePolarity::Avoid,
            strength: PreferenceStrength::Hard,
            scope: PreferenceScope::Element,
            target_id: Some(to.head.document_id.clone()),
            confirmed: true,
        },
        WorkshopPreference {
            id: "unrelated-preference".into(),
            label: "Unrelated material preference".into(),
            family: "scope".into(),
            meaning: "This must stay outside the relationship request.".into(),
            examples: String::new(),
            timing: String::new(),
            polarity: PreferencePolarity::Want,
            strength: PreferenceStrength::Soft,
            scope: PreferenceScope::Element,
            target_id: Some("unrelated-document".into()),
            confirmed: true,
        },
    ];
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-generation-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let request = StartWorkshop {
        access: access.clone(),
        operation_id: "relationship-generation-start".into(),
        exploration: WorkshopExploration {
            session_id: "relationship-session".into(),
            expected_version: saved.version.clone(),
            working_generation: "0".into(),
            action: "directions".into(),
            instruction: "Explore what this bond could force into the open.".into(),
            selected_scope: "Relationship between the selected endpoints".into(),
            selected_text: String::new(),
            working_selection: None,
        },
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
    };
    let started = project.workshop().start(request).unwrap();
    let metadata = webnovel_core::projects::workshop_generation::metadata_from_instruction(
        &started.packet.messages.last().unwrap().content,
    )
    .unwrap();
    let relationship = metadata.relationship.expect("typed relationship metadata");
    assert_eq!(relationship.id, "relationship-one");
    assert_eq!(relationship.relationship_type, "owes a hidden debt to");
    assert_eq!(relationship.source_heads, vec![from.head.clone(), to.head.clone()]);
    assert_eq!(relationship.description, "Mira's oath binds the court's gatekeeper, but neither side knows the full price.");
    assert_eq!(relationship.uncertainty, "The debt may be inherited rather than chosen.");
    assert!(metadata
        .preferences
        .iter()
        .any(|preference| preference.contains("Keep Mira close to the oath")));
    assert!(metadata
        .preferences
        .iter()
        .any(|preference| preference.contains("Keep the court uncertain")));
    assert!(metadata
        .hard_constraints
        .iter()
        .any(|preference| preference.contains("Keep the court uncertain")));
    assert!(!metadata
        .preferences
        .iter()
        .any(|preference| preference.contains("Unrelated material preference")));
    assert_eq!(started.packet.receipt.mandatory_source_handles.len(), 2);
    let context_message: Value = serde_json::from_str(
        &started
            .packet
            .messages
            .iter()
            .find(|message| message.role == "user" && message.content.contains("context.packet"))
            .expect("compiled context message")
            .content,
    )
    .unwrap();
    let source_document_ids = context_message["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|source| source["source"]["documentId"].as_str())
        .collect::<Vec<_>>();
    assert!(source_document_ids.contains(&from.head.document_id.as_str()));
    assert!(source_document_ids.contains(&to.head.document_id.as_str()));

    let owner = started.run.owner.clone();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .unwrap();
    project.mark_discussion_delivered(owner.clone()).unwrap();
    let output = json!({
        "schemaVersion": "story-workshop-output.v1",
        "requestKind": "directions",
        "question": "What does the debt demand?",
        "questionReason": "The exact relationship remains open.",
        "dimension": "Consequence",
        "interpretation": {
            "youSaid": "Explore the saved relationship.",
            "possibleDirection": "The debt becomes visible under pressure.",
            "stillOpen": "Who chose the debt?"
        },
        "candidates": [
            {"id":"","title":"Public claim","content":"The court demands a public repayment.","dimensionValue":"public","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["relationship"]},
            {"id":"","title":"Private bargain","content":"Mira offers a private bargain to the gatekeeper.","dimensionValue":"private","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["relationship"]},
            {"id":"","title":"Inherited debt","content":"The debt belongs to an oath neither endpoint remembers making.","dimensionValue":"inherited","implications":[],"assumptions":[],"affectedTargets":[],"preservedDetails":[],"changedDetails":["relationship"]}
        ]
    });
    let completed = project
        .finish_discussion(DiscussionFinish {
            owner,
            expected_sequence: "0".into(),
            event_id: "relationship-generation-finish".into(),
            assistant_text: serde_json::to_string(&output).unwrap(),
        })
        .unwrap();
    assert_eq!(
        completed.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Completed
    );
    let first_view = project.workshop().read(access.clone()).unwrap();
    let relationship_result = first_view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("relationship result");
    let candidate_id = relationship_result
        .output
        .as_ref()
        .unwrap()
        .candidates[0]
        .id
        .clone();
    assert!(!relationship_result.stale);

    let normal_started = project
        .workshop().start(StartWorkshop {
            access: access.clone(),
            operation_id: "normal-focus-generation-start".into(),
            exploration: WorkshopExploration {
                session_id: "normal-session".into(),
                expected_version: first_view.version.clone(),
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Explore the selected endpoint normally.".into(),
                selected_scope: "The selected endpoint".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let normal_metadata = webnovel_core::projects::workshop_generation::metadata_from_instruction(
        &normal_started.packet.messages.last().unwrap().content,
    )
    .unwrap();
    assert!(normal_metadata
        .preferences
        .iter()
        .any(|preference| preference.contains("Keep Mira close to the oath")));
    assert!(!normal_metadata
        .preferences
        .iter()
        .any(|preference| preference.contains("Keep the court uncertain")));

    let mut cleared = first_view.state.clone();
    cleared.sessions[0].relationship_id = None;
    let cleared_saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-clear-scope".into(),
            expected_version: first_view.version,
            state: cleared,
        })
        .unwrap();
    let cleared_view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(cleared_view.version, cleared_saved.version);
    let cleared_relationship = cleared_view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("cleared relationship result");
    assert!(cleared_relationship.stale);

    let mut restored = cleared_view.state.clone();
    restored.sessions[0].relationship_id = Some("relationship-one".into());
    let restored_saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-restore-scope".into(),
            expected_version: cleared_view.version,
            state: restored,
        })
        .unwrap();
    let restored_view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(restored_view.version, restored_saved.version);
    let restored_relationship = restored_view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("restored relationship result");
    assert!(!restored_relationship.stale);

    let mut unrelated = restored_view.state.clone();
    unrelated.relationships.push(WorkshopRelationship {
        id: "relationship-unrelated".into(),
        from_document_id: from.head.document_id.clone(),
        to_document_id: to.head.document_id.clone(),
        relationship_type: "protects".into(),
        description: "The gatekeeper protects the court's records.".into(),
        uncertainty: String::new(),
        status: WorkshopRelationshipStatus::Tentative,
        source_heads: vec![from.head.clone(), to.head.clone()],
    });
    let unrelated_saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-unrelated-edit".into(),
            expected_version: restored_view.version,
            state: unrelated,
        })
        .unwrap();
    let unrelated_view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(unrelated_view.version, unrelated_saved.version);
    let unrelated_relationship = unrelated_view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("unrelated relationship result");
    assert!(!unrelated_relationship.stale);

    let mut changed = unrelated_view.state.clone();
    changed.relationships[0].description =
        "The debt is now understood as a public obligation.".into();
    let changed = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-target-edit".into(),
            expected_version: unrelated_view.version,
            state: changed,
        })
        .unwrap();
    let stale_view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(stale_view.version, changed.version);
    let stale_relationship = stale_view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("stale relationship result");
    assert!(stale_relationship.stale);

    let preview_error = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "relationship-session".into(),
            expected_version: stale_view.version,
            candidate_ids: vec![candidate_id],
            targets: vec![WorkshopAdoptionTarget {
                document_id: "relationship-adoption-target".into(),
                expected: None,
                title: "Relationship note".into(),
                kind: "world".into(),
                body: body("Candidate material"),
                mode: AdoptionMode::Add,
            }],
            rationale: "Try the relationship direction explicitly.".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(preview_error.code, "InvalidWorkshopCandidate");
    assert!(project
        .document(access, "relationship-adoption-target".into())
        .is_err());
}

#[test]
fn unknown_relationship_reference_is_rejected_and_legacy_bytes_omit_optional_id() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-relationship-reopen".into()).unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("relationship-session".into()),
        ..WorkshopState::default()
    };
    let mut workshop_session = session("relationship-session");
    workshop_session.relationship_id = Some("relationship-one".into());
    state.sessions.push(workshop_session.clone());
    let error = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-reopen-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidRequest");
    assert!(error.detail.contains("unknown relationship"));

    let baseline_state = WorkshopState {
        current_session_id: Some("reopened-session".into()),
        sessions: vec![session("reopened-session")],
        ..WorkshopState::default()
    };
    project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-reopen-baseline".into(),
            expected_version: "0".into(),
            state: baseline_state,
        })
        .unwrap();

    let legacy = session("legacy-session");
    let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
    let legacy_value: Value = serde_json::from_slice(&legacy_bytes).unwrap();
    assert!(legacy_value.get("relationshipId").is_none());
    let reopened_legacy: WorkshopSession = serde_json::from_slice(&legacy_bytes).unwrap();
    assert_eq!(reopened_legacy.relationship_id, None);
    assert_eq!(serde_json::to_vec(&reopened_legacy).unwrap(), legacy_bytes);

    drop(project);
    let database_path = temp.0.join("project.sqlite3");
    let database = Connection::open(&database_path).unwrap();
    let state_json: String = database
        .query_row(
            "SELECT state_json FROM workshop_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut persisted_state: Value = serde_json::from_str(&state_json).unwrap();
    persisted_state["sessions"] = json!([{
        "id": "reopened-session",
        "title": "Reopened",
        "lens": "overview",
        "parentSessionId": null,
        "branchKind": "working",
        "brief": "",
        "direction": "",
        "stillOpen": "",
        "focusQuestion": "",
        "focusReason": "",
        "focusDocumentId": null,
        "anchorDocumentId": null,
        "depth": "sketch",
        "outsideDirection": false,
        "includedDocumentIds": [],
        "workingText": "",
        "workingTitle": "",
        "workingGeneration": "0",
        "selectedDetails": [],
        "choices": [],
        "questions": [],
        "composer": "",
        "selectedScope": "",
        "originalNotes": "",
        "activeRunId": null,
        "relationshipId": "missing-relationship"
    }]);
    let tampered_state = serde_json::to_string(&persisted_state).unwrap();
    let state_hash = Sha256::digest(tampered_state.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    database
        .execute(
            "UPDATE workshop_state SET state_json=?,state_hash=? WHERE singleton=1",
            params![tampered_state, state_hash],
        )
        .unwrap();
    drop(database);

    let reopened = ProjectSession::open(&temp.0).unwrap();
    let reopened_access = reopened.attach("workshop-relationship-reopened".into()).unwrap();
    let error = reopened.workshop().read(reopened_access).unwrap_err();
    assert_eq!(error.code, "InvalidRequest");
    assert!(error.detail.contains("unknown relationship"));
}

#[test]
fn adoption_refuses_independently_mutated_frozen_request_json() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-preview-integrity".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "workshop-preview-integrity-document".into(),
            document_id: "workshop-preview-integrity-document".into(),
            title: "Integrity target".into(),
            kind: "world".into(),
            body: body("Before adoption"),
        })
        .unwrap();
    let mut state = WorkshopState {
        current_session_id: Some("workshop-preview-integrity-session".into()),
        ..WorkshopState::default()
    };
    state
        .sessions
        .push(session("workshop-preview-integrity-session"));
    let saved = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-preview-integrity-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
            access: access.clone(),
            session_id: "workshop-preview-integrity-session".into(),
            expected_version: saved.version,
            candidate_ids: Vec::new(),
            targets: vec![WorkshopAdoptionTarget {
                document_id: document.head.document_id.clone(),
                expected: Some(document.head.clone()),
                title: document.title.clone(),
                kind: document.kind.clone(),
                body: body("After adoption"),
                mode: AdoptionMode::Replace,
            }],
            rationale: "Adopt the frozen target".into(),
            protected_text: Vec::new(),
            relationships: Vec::new(),
            impact_drafts: Vec::new(),
        })
        .unwrap();

    let database = Connection::open(temp.0.join("project.sqlite3")).unwrap();
    let request_json: String = database
        .query_row(
            "SELECT request_json FROM workshop_adoption_previews WHERE id=?",
            [&preview.id],
            |row| row.get(0),
        )
        .unwrap();
    let mut request: Value = serde_json::from_str(&request_json).unwrap();
    request["rationale"] = Value::String("Tampered after preview".into());
    let tampered_json = serde_json::to_string(&request).unwrap();
    database
        .execute_batch("DROP TRIGGER workshop_previews_no_update;")
        .unwrap();
    database
        .execute(
            "UPDATE workshop_adoption_previews SET request_json=? WHERE id=?",
            params![tampered_json, preview.id],
        )
        .unwrap();
    drop(database);

    let error = project
        .workshop().adopt(
            access.clone(),
            "workshop-preview-integrity-adopt".into(),
            preview.id,
        )
        .unwrap_err();
    assert_eq!(error.code, "InvalidProject");
    assert!(error.detail.contains("request failed its fingerprint"));
    assert_eq!(
        project
            .document(access, "workshop-preview-integrity-document".into())
            .unwrap()
            .body["body"]["content"][0]["content"][0]["text"],
        "Before adoption"
    );
}

#[test]
fn relationship_exploration_refuses_missing_archived_and_stale_targets_without_anchor() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-relationship-refusal".into()).unwrap();
    let from = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-refusal-from".into(),
            document_id: "relationship-refusal-from".into(),
            title: "From".into(),
            kind: "character".into(),
            body: body("From body"),
        })
        .unwrap();
    let to = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-refusal-to".into(),
            document_id: "relationship-refusal-to".into(),
            title: "To".into(),
            kind: "world".into(),
            body: body("To body"),
        })
        .unwrap();

    let start = |project: &ProjectSession, access: webnovel_core::projects::ProjectAccess, version: &str, operation_id: &str| {
        project.workshop().start(StartWorkshop {
            access,
            operation_id: operation_id.into(),
            exploration: WorkshopExploration {
                session_id: "relationship-refusal-session".into(),
                expected_version: version.into(),
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Explore the saved relationship.".into(),
                selected_scope: "Relationship".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
    };
    let save_state = |project: &ProjectSession,
                      access: webnovel_core::projects::ProjectAccess,
                      operation_id: &str,
                      relationship_id: &str,
                      expected_version: &str,
                      include_relationship: bool,
                      status: WorkshopRelationshipStatus,
                      from_head: Head,
                      to_head: Head| {
        let mut state = WorkshopState {
            current_session_id: Some("relationship-refusal-session".into()),
            ..WorkshopState::default()
        };
        let mut workshop_session = session("relationship-refusal-session");
        workshop_session.anchor_document_id = Some("workshop-refusal-session".into());
        workshop_session.relationship_id = Some(relationship_id.into());
        state.sessions.push(workshop_session);
        if include_relationship {
            state.relationships.push(WorkshopRelationship {
                id: relationship_id.into(),
                from_document_id: from_head.document_id.clone(),
                to_document_id: to_head.document_id.clone(),
                relationship_type: "knows".into(),
                description: "A saved edge".into(),
                uncertainty: String::new(),
                status,
                source_heads: vec![from_head, to_head],
            });
        }
        project.workshop().save(SaveWorkshop {
            access,
            operation_id: operation_id.into(),
            expected_version: expected_version.into(),
            state,
        })
    };

    let missing_error = save_state(
        &project,
        access.clone(),
        "relationship-refusal-missing-state",
        "missing-relationship",
        "0",
        false,
        WorkshopRelationshipStatus::Tentative,
        from.head.clone(),
        to.head.clone(),
    )
    .unwrap_err();
    assert_eq!(missing_error.code, "InvalidRequest");
    assert!(missing_error.detail.contains("unknown relationship"));
    assert!(project
        .document(access.clone(), "workshop-refusal-session".into())
        .is_err());

    let archived = save_state(
        &project,
        access.clone(),
        "relationship-refusal-archived-state",
        "archived-relationship",
        "0",
        true,
        WorkshopRelationshipStatus::Archived,
        from.head.clone(),
        to.head.clone(),
    )
    .unwrap();
    let archived_error = start(
        &project,
        access.clone(),
        &archived.version,
        "relationship-refusal-archived-start",
    )
    .unwrap_err();
    assert_eq!(archived_error.code, "InvalidWorkshopRelationship");
    assert!(project
        .document(access.clone(), "workshop-refusal-session".into())
        .is_err());

    let stale = save_state(
        &project,
        access.clone(),
        "relationship-refusal-stale-state",
        "stale-relationship",
        &archived.version,
        true,
        WorkshopRelationshipStatus::Tentative,
        from.head.clone(),
        to.head.clone(),
    )
    .unwrap();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "relationship-refusal-stale-edit".into(),
            expected: to.head,
            local_generation: "1".into(),
            body: body("To changed after the relationship was saved"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let stale_error = start(
        &project,
        access.clone(),
        &stale.version,
        "relationship-refusal-stale-start",
    )
    .unwrap_err();
    assert_eq!(stale_error.code, "StaleRelationship");
    assert!(project
        .document(access.clone(), "workshop-refusal-session".into())
        .is_err());
    assert!(project.workshop().read(access).unwrap().results.is_empty());
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-recovery-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let started = project
        .workshop().start(StartWorkshop {
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
    let result_view = project.workshop().read(access.clone()).unwrap();
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
        .workshop().save(SaveWorkshop {
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
    let recovered_view = recovered.workshop().read(recovered_access.clone()).unwrap();
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
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-save".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let forged = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(access.clone(), "workshop-adopt".into(), preview.id.clone())
        .unwrap();
    assert_eq!(first.documents.len(), 1);
    assert_eq!(first.decision_ids.len(), 1);
    let replay = project
        .workshop().adopt(access.clone(), "workshop-adopt".into(), preview.id)
        .unwrap();
    assert_eq!(replay.snapshot, first.snapshot);
    assert_eq!(replay.decision_ids, first.decision_ids);
    assert_eq!(replay.documents[0].head, first.documents[0].head);
    let after_first = project.workshop().read(access.clone()).unwrap();
    let second_preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(
            access.clone(),
            "workshop-adopt-two".into(),
            second_preview.id,
        )
        .unwrap();
    let final_view = project.workshop().read(access.clone()).unwrap();
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
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-repromote-previous".into(),
            expected_version: archived_snapshot.version.clone(),
            state: conflicting,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidRequest");
    assert!(error.detail.contains("more than one chosen"));
    let after_rejection = project.workshop().read(access.clone()).unwrap();
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "workshop-preference-conflict".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap_err();
    assert_eq!(conflict.code, "PreferenceConflict");

    state.preferences[1].confirmed = false;
    let saved = project
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(access.clone(), "adopt-linked-world".into(), preview.id)
        .unwrap();
    let new_world = project
        .document(access.clone(), "new-world".into())
        .unwrap();
    assert_eq!(adopted.documents.len(), 1);
    assert_eq!(new_world.head.version, "0");
    let view = project.workshop().read(access).unwrap();
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
        .workshop().save(SaveWorkshop {
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
            .workshop().preview_adoption(blank_description)
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
            .workshop().preview_adoption(forged_new_head)
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "stale-relationship-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(access.clone(), "stale-edge-adopt".into(), preview.id)
        .unwrap_err();
    assert_eq!(error.code, "VersionConflict");
    assert_eq!(project.workshop().read(access.clone()).unwrap().version, "1");
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-impact-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "impact-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let started = project
        .workshop().start(StartWorkshop {
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
    let result = project.workshop().read(access.clone()).unwrap();
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "impact-selection".into(),
            expected_version: result.version,
            state: selected,
        })
        .unwrap();
    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().adopt(access.clone(), "impact-adopt".into(), preview.id)
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
    let view = project.workshop().read(access).unwrap();
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-state".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    let error = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
        .workshop().save(SaveWorkshop {
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-unrelated-state".into(),
            expected_version: saved.version,
            state,
        })
        .unwrap();
    assert_eq!(saved_again.version, "2");

    let preview = project
        .workshop().preview_adoption(PreviewWorkshopAdoption {
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
