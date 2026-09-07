use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::story_context::{FreezeStory, SearchMode, SearchStory};
use webnovel_core::projects::workshop::{
    AdoptionMode, Lens, PreferencePolarity, PreferenceScope, PreferenceStrength,
    PreviewWorkshopAdoption, SaveWorkshop, WorkshopAdoptionTarget, WorkshopBranchKind,
    WorkshopDepth, WorkshopPreference, WorkshopSession, WorkshopState,
};
use webnovel_core::projects::workshop_generation::{
    StartWorkshop, WorkshopExploration, from_session,
};
use webnovel_core::projects::{CreateDocument, Head, ProjectSession, SaveCause, SaveSnapshot};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("wns-workshop-boundary-{}", Uuid::new_v4())))
    }

    fn project(&self) -> ProjectSession {
        ProjectSession::create(&self.0, "Workshop boundary test").expect("create project")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn body(block_id: &str, text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": block_id},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

fn session(id: &str) -> WorkshopSession {
    WorkshopSession {
        id: id.into(),
        title: "Boundary exploration".into(),
        lens: Lens::Overview,
        parent_session_id: None,
        branch_kind: WorkshopBranchKind::Working,
        brief: "A bounded test exploration".into(),
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

fn state_with_session(id: &str) -> (WorkshopState, WorkshopSession) {
    let workshop_session = session(id);
    let state = WorkshopState {
        current_session_id: Some(id.into()),
        sessions: vec![workshop_session.clone()],
        ..WorkshopState::default()
    };
    (state, workshop_session)
}

fn save_state(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
    operation_id: &str,
    expected_version: &str,
    state: WorkshopState,
) -> webnovel_core::projects::workshop::WorkshopSnapshot {
    project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected_version: expected_version.into(),
            state,
        })
        .expect("save workshop state")
}

fn target(
    document_id: &str,
    expected: Option<Head>,
    title: &str,
    kind: &str,
    text: &str,
    mode: AdoptionMode,
) -> WorkshopAdoptionTarget {
    WorkshopAdoptionTarget {
        document_id: document_id.into(),
        expected,
        title: title.into(),
        kind: kind.into(),
        body: body(&format!("{document_id}-paragraph"), text),
        mode,
    }
}

fn preview_request(
    access: &webnovel_core::projects::ProjectAccess,
    session_id: &str,
    expected_version: &str,
    targets: Vec<WorkshopAdoptionTarget>,
) -> PreviewWorkshopAdoption {
    PreviewWorkshopAdoption {
        access: access.clone(),
        session_id: session_id.into(),
        expected_version: expected_version.into(),
        candidate_ids: Vec::new(),
        targets,
        rationale: "A deliberate author-room choice".into(),
        protected_text: Vec::new(),
        relationships: Vec::new(),
        impact_drafts: Vec::new(),
    }
}

#[test]
fn fixed_literals_preserve_paragraph_and_hard_break_boundaries() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-boundary-renderer".into()).unwrap();
    let literal = "First line\nSecond line\n\nThird line";
    let (mut state, _) = state_with_session("session-one");
    state.sessions[0].working_text = literal.into();
    let saved = save_state(&project, &access, "save-workshop", "0", state);
    assert_eq!(project.read_workshop(access.clone()).unwrap().state.sessions[0].working_text, literal);
    let mut request = preview_request(&access, "session-one", &saved.version, vec![
        WorkshopAdoptionTarget {
            document_id: "new-world".into(), expected: None, title: "World".into(), kind: "world".into(), mode: AdoptionMode::Add,
            body: json!({"schemaVersion":1,"body":{"type":"doc","content":[
                {"type":"paragraph","attrs":{"id":"first"},"content":[{"type":"text","text":"First line"},{"type":"hardBreak"},{"type":"text","text":"Second line"}]},
                {"type":"paragraph","attrs":{"id":"second"},"content":[{"type":"text","text":"Third line"}]}
            ]}}),
        }
    ]);
    request.protected_text = vec![literal.into()];
    let preview = project.preview_workshop_adoption(request).unwrap();
    let adopted = project.adopt_workshop(access.clone(), "adopt-lines".into(), preview.id).unwrap();
    let replacement = preview_request(&access, "session-one", &adopted.snapshot.version, vec![
        target("new-world", Some(adopted.documents[0].head.clone()), "World", "world", "First line Second line Third line", AdoptionMode::Replace)
    ]);
    assert_eq!(project.preview_workshop_adoption(replacement).unwrap_err().code, "ProtectedContentChanged");
}

#[test]
fn stale_existing_target_refuses_all_multi_target_adoption_before_writing() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-boundary-renderer".into()).unwrap();
    let character = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-character".into(),
            document_id: "existing-character".into(),
            title: "Existing character".into(),
            kind: "character".into(),
            body: body("character-paragraph", "The character before preview."),
        })
        .unwrap();
    let (state, _) = state_with_session("session-one");
    let saved = save_state(&project, &access, "save-workshop", "0", state);

    let preview = project
        .preview_workshop_adoption(preview_request(
            &access,
            "session-one",
            &saved.version,
            vec![
                target(
                    "new-world",
                    None,
                    "New world",
                    "world",
                    "The world proposed at preview time.",
                    AdoptionMode::Add,
                ),
                target(
                    "existing-character",
                    Some(character.head.clone()),
                    "Existing character",
                    "character",
                    "The character proposed at preview time.",
                    AdoptionMode::Replace,
                ),
            ],
        ))
        .unwrap();

    let changed = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-character-before-adoption".into(),
            expected: character.head.clone(),
            local_generation: "1".into(),
            body: body(
                "character-paragraph",
                "The character changed after preview.",
            ),
            cause: SaveCause::Typing,
        })
        .unwrap();

    let error = project
        .adopt_workshop(
            access.clone(),
            "adopt-stale-multi-target".into(),
            preview.id,
        )
        .expect_err("a stale existing target must refuse the whole adoption");
    assert_eq!(error.code, "VersionConflict");
    assert_eq!(error.current_head, Some(changed.head.clone()));
    assert_eq!(
        project
            .document(access.clone(), "existing-character".into())
            .unwrap()
            .head,
        changed.head
    );
    assert_eq!(
        project
            .document(access.clone(), "existing-character".into())
            .unwrap()
            .body,
        body(
            "character-paragraph",
            "The character changed after preview."
        )
    );
    assert_eq!(
        project
            .document(access.clone(), "new-world".into())
            .unwrap_err()
            .code,
        "DocumentNotFound"
    );
    assert_eq!(project.documents(access.clone()).unwrap().len(), 1);
    assert_eq!(
        project.read_workshop(access.clone()).unwrap().version,
        saved.version
    );
    assert_eq!(project.workshop_history(access).unwrap().len(), 1);
}

#[test]
fn successful_multi_target_adoption_records_exact_history_and_leaves_chapters_untouched() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-boundary-renderer".into()).unwrap();
    let chapter = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("chapter-paragraph", "Chapter text remains unchanged."),
        })
        .unwrap();
    let character = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-character".into(),
            document_id: "existing-character".into(),
            title: "Existing character".into(),
            kind: "character".into(),
            body: body("character-paragraph", "The original character."),
        })
        .unwrap();
    let chapter_before = chapter;
    let chapter_history_before = project
        .history(access.clone(), "chapter-one".into())
        .unwrap();
    let character_history_before = project
        .history(access.clone(), "existing-character".into())
        .unwrap();
    let (state, _) = state_with_session("session-one");
    let saved = save_state(&project, &access, "save-workshop", "0", state);
    let character_body = body("existing-character-paragraph", "The adopted character.");
    let preview = project
        .preview_workshop_adoption(preview_request(
            &access,
            "session-one",
            &saved.version,
            vec![
                target(
                    "new-world",
                    None,
                    "New world",
                    "world",
                    "The adopted world.",
                    AdoptionMode::Add,
                ),
                WorkshopAdoptionTarget {
                    document_id: "existing-character".into(),
                    expected: Some(character.head.clone()),
                    title: "Existing character".into(),
                    kind: "character".into(),
                    body: character_body.clone(),
                    mode: AdoptionMode::Replace,
                },
            ],
        ))
        .unwrap();
    assert_eq!(preview.before.len(), 1);
    assert_eq!(preview.before[0].head, character.head);
    let adopted = project
        .adopt_workshop(
            access.clone(),
            "adopt-successful-multi-target".into(),
            preview.id,
        )
        .unwrap();

    assert_eq!(adopted.documents.len(), 2);
    assert_eq!(adopted.documents[0].head.document_id, "new-world");
    assert_eq!(adopted.documents[1].head.document_id, "existing-character");
    assert_eq!(adopted.documents[1].body, character_body);
    assert_eq!(adopted.decision_ids.len(), 2);
    let view = project.read_workshop(access.clone()).unwrap();
    assert_eq!(view.state.decisions.len(), 2);
    assert!(view.state.decisions.iter().all(|decision| decision.status
        == webnovel_core::projects::workshop::WorkshopDecisionStatus::Chosen
        && decision.access == "authorRoom"));

    let character_history = project
        .history(access.clone(), "existing-character".into())
        .unwrap();
    assert_eq!(character_history.len(), character_history_before.len() + 2);
    assert_eq!(character_history[0].head, adopted.documents[1].head);
    assert_eq!(character_history[0].body, character_body);
    assert_eq!(character_history[1].head, character.head);
    assert_eq!(character_history[1].body, character.body);
    let world_history = project.history(access.clone(), "new-world".into()).unwrap();
    assert_eq!(world_history.len(), 1);
    assert_eq!(world_history[0].head, adopted.documents[0].head);
    assert_eq!(world_history[0].body, adopted.documents[0].body);

    let chapter_after = project
        .document(access.clone(), "chapter-one".into())
        .unwrap();
    assert_eq!(chapter_after.head, chapter_before.head);
    assert_eq!(chapter_after.body, chapter_before.body);
    let chapter_history_after = project
        .history(access.clone(), "chapter-one".into())
        .unwrap();
    assert_eq!(chapter_history_after.len(), chapter_history_before.len());
    for (after, before) in chapter_history_after.iter().zip(&chapter_history_before) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.head, before.head);
        assert_eq!(after.body, before.body);
        assert_eq!(after.reason, before.reason);
        assert_eq!(after.parent_id, before.parent_id);
    }
    assert_eq!(project.documents(access).unwrap().len(), 3);
}

#[test]
fn chosen_author_secret_stays_out_of_restricted_writing_context() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-boundary-renderer".into()).unwrap();
    let chapter = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("chapter-paragraph", "The reader-visible chapter."),
        })
        .unwrap();
    let (state, _) = state_with_session("session-one");
    let saved = save_state(&project, &access, "save-workshop", "0", state);
    let preview = project
        .preview_workshop_adoption(preview_request(
            &access,
            "session-one",
            &saved.version,
            vec![target(
                "author-secret",
                None,
                "Author secret",
                "note",
                "The hidden culprit is the mentor.",
                AdoptionMode::Add,
            )],
        ))
        .unwrap();
    let adopted = project
        .adopt_workshop(access.clone(), "adopt-author-secret".into(), preview.id)
        .unwrap();
    let decision = project
        .read_workshop(access.clone())
        .unwrap()
        .state
        .decisions
        .into_iter()
        .find(|decision| decision.document_id == "author-secret")
        .expect("chosen secret decision");
    assert_eq!(decision.access, "authorRoom");
    assert_eq!(adopted.documents[0].head.document_id, "author-secret");

    let policy = project.context_epochs(access.clone()).unwrap().policy;
    let frozen = project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "freeze-restricted-after-workshop".into(),
            expected: chapter.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::Continue,
            policy: InformationPolicy {
                version: policy,
                audience: Audience::RestrictedWriting,
                reader_frontier: Some("0".into()),
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert_eq!(frozen.snapshot.sources.len(), 1);
    assert_eq!(frozen.excluded_source_count, 1);
    assert_eq!(frozen.snapshot.sources[0].source.document_id, "chapter-one");
    let serialized = serde_json::to_string(&frozen).unwrap();
    assert!(!serialized.contains("author-secret"));
    assert!(!serialized.contains("Author secret"));
    assert!(!serialized.contains("hidden culprit"));
    let search = project
        .search_story(SearchStory {
            access,
            snapshot_id: frozen.snapshot.snapshot_id,
            query: "culprit".into(),
            mode: SearchMode::Literal,
            limit: 20,
        })
        .unwrap();
    assert!(search.hits.is_empty());
}

#[test]
fn hard_project_conflict_is_reported_and_neutral_local_preference_is_not_an_avoid() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.attach("workshop-boundary-renderer".into()).unwrap();
    let (mut state, workshop_session) = state_with_session("session-one");
    state.preferences = vec![
        WorkshopPreference {
            id: "project-preference".into(),
            label: "Pacing".into(),
            family: "story".into(),
            meaning: "Prefer a patient build".into(),
            examples: String::new(),
            timing: String::new(),
            polarity: PreferencePolarity::Want,
            strength: PreferenceStrength::Hard,
            scope: PreferenceScope::Project,
            target_id: None,
            confirmed: true,
        },
        WorkshopPreference {
            id: "local-preference".into(),
            label: "Pacing".into(),
            family: "exploration".into(),
            meaning: "Do not rush this scene".into(),
            examples: String::new(),
            timing: String::new(),
            polarity: PreferencePolarity::Avoid,
            strength: PreferenceStrength::Soft,
            scope: PreferenceScope::Exploration,
            target_id: Some("session-one".into()),
            confirmed: true,
        },
    ];
    let conflict = project
        .save_workshop(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-conflicting-preferences".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .expect_err("opposing local preference must surface a Rust conflict");
    assert_eq!(conflict.code, "PreferenceConflict");
    assert_eq!(project.read_workshop(access.clone()).unwrap().version, "0");

    state.preferences[1].polarity = PreferencePolarity::Neutral;
    let saved = save_state(
        &project,
        &access,
        "save-neutral-preferences",
        "0",
        state.clone(),
    );
    assert_eq!(saved.version, "1");

    let generated = from_session(
        StartWorkshop {
            access: access.clone(),
            operation_id: "generation-from-neutral-preferences".into(),
            exploration: WorkshopExploration {
                session_id: "session-one".into(),
                expected_version: saved.version,
                working_generation: "0".into(),
                action: "directions".into(),
                instruction: "Explore the pacing choice.".into(),
                selected_scope: "Whole working version".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: webnovel_core::context::packet::MockContextBudget::new("32000", "8000", "1000"),
            provider_binding: None,
        },
        &workshop_session,
        &state,
        Head {
            document_id: "workshop-session-one".into(),
            version: "0".into(),
            body_hash: "0".repeat(64),
        },
    )
    .expect("neutral preference remains valid workshop context");
    assert!(
        generated
            .context
            .preferences
            .iter()
            .any(|preference| preference.starts_with("neutral Pacing"))
    );
    assert!(
        !generated
            .context
            .preferences
            .iter()
            .any(|preference| preference.starts_with("avoid Pacing"))
    );
    assert!(
        generated
            .context
            .hard_constraints
            .iter()
            .any(|constraint| constraint.starts_with("hard constraint: want Pacing"))
    );
    assert!(
        !generated
            .context
            .hard_constraints
            .iter()
            .any(|constraint| constraint.contains("neutral Pacing"))
    );
}
