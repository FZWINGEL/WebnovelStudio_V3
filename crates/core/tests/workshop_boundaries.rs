use serde_json::{Value, json};
use rusqlite::Connection;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFinish, DiscussionStart, FeedbackIntent, StartDiscussion,
};
use webnovel_core::projects::story_context::{FreezeStory, SearchMode, SearchStory};
use webnovel_core::projects::workshop::{
    AdoptionMode, CandidateChoice, CandidateChoiceStatus, Lens, PreferencePolarity,
    PreferenceScope, PreferenceStrength, PreviewWorkshopAdoption, SaveWorkshop, SelectedDetail,
    StoryPossibility, StoryPossibilityKind, StoryPossibilityStatus, WorkshopAdoptionTarget,
    WorkshopBranchKind, WorkshopDepth, WorkshopPreference, WorkshopSession, WorkshopState,
};
use webnovel_core::projects::workshop_generation::{
    StartWorkshop, WorkshopExploration, from_session, metadata_from_instruction,
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
        relationship_id: None,
        story_possibilities: Vec::new(),
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
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected_version: expected_version.into(),
            state,
        })
        .expect("save workshop state")
}

fn context_request(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
    operation: &str,
    action: &str,
    window: &str,
) -> StartWorkshop {
    let saved = project.workshop().read(access.clone()).unwrap();
    let session = saved.state.sessions.iter().find(|session| {
        Some(&session.id) == saved.state.current_session_id.as_ref()
    }).expect("current workshop session");
    StartWorkshop {
        access: access.clone(),
        operation_id: operation.into(),
        exploration: WorkshopExploration {
            session_id: session.id.clone(),
            expected_version: saved.version,
            working_generation: session.working_generation.clone(),
            action: action.into(),
            instruction: "Explore the supplied author request".into(),
            selected_scope: "Whole working version".into(),
            selected_text: String::new(),
            working_selection: None,
        },
        budget: MockContextBudget::new(window, "100", "100"),
        provider_binding: None,
    }
}

fn complete_context_fixture(project: &ProjectSession, started: &DiscussionStart, text: String) {
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
            event_id: format!("finish-{}", started.run.id),
            assistant_text: text,
        })
        .unwrap();
}

fn context_candidates(kind: &str, contents: &[&str]) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "story-workshop-output.v1",
        "requestKind": kind,
        "question": "Which direction fits?",
        "questionReason": "Compare what each choice makes possible.",
        "dimension": "Approach",
        "interpretation": {"youSaid":"A seed", "possibleDirection":"A path", "stillOpen":"Its cost"},
        "candidates": contents.iter().enumerate().map(|(index, content)| json!({
            "id":"", "title":format!("Direction {index}"), "content":content,
            "dimensionValue":format!("Approach {index}"), "implications":[],
            "assumptions":[], "affectedTargets":[], "preservedDetails":[], "changedDetails":["direction"]
        })).collect::<Vec<_>>()
    })).unwrap()
}

#[test]
fn corrected_brief_reaches_new_requests_without_rewriting_prose_or_historical_packets() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("interpretation".into()).unwrap();
    let (mut state, _) = state_with_session("interpretation-session");
    state.sessions[0].anchor_document_id = Some("workshop-interpretation".into());
    state.sessions[0].brief = "First author brief".into();
    state.sessions[0].working_text = "Before. Editable passage. After.".into();
    state.sessions[0].original_notes = "Original notes stay as evidence.".into();
    save_state(&project, &access, "interpretation-state", "0", state);
    let mut first_request = context_request(&project, &access, "interpretation-first", "concrete", "100000");
    first_request.exploration.selected_text = "Editable passage.".into();
    first_request.exploration.selected_scope = "Selected passage".into();
    first_request.exploration.working_selection = Some(
        webnovel_core::projects::workshop_generation::WorkshopWorkingSelection {
            from: 8,
            to: 25,
            text: "Editable passage.".into(),
        },
    );
    let first = project.workshop().start(first_request.clone()).unwrap();
    let first_packet = serde_json::to_value(&first.packet).unwrap();
    let first_instruction = first.packet.messages.last().unwrap().content.clone();
    let first_envelope: Value = serde_json::from_str(&first_instruction).unwrap();
    assert_eq!(first_envelope["authorBrief"], "First author brief");
    assert_eq!(first_envelope["currentElement"], "Before. Editable passage. After.");
    complete_context_fixture(&project, &first, context_candidates("refinement", &["A revised passage."]));

    let mut view = project.workshop().read(access.clone()).unwrap();
    view.state.sessions[0].brief = "Corrected author attraction, independent of the prose.".into();
    view.state.sessions[0].direction = "Shared skills rather than a chosen savior.".into();
    view.state.sessions[0].still_open = "Who pays the cost remains open.".into();
    save_state(&project, &access, "interpretation-correction", &view.version, view.state);
    let replay = project.workshop().start(first_request.clone()).unwrap();
    assert_eq!(serde_json::to_value(&replay.packet).unwrap(), first_packet);

    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let access = reopened.documents().attach("interpretation-reopened".into()).unwrap();
    let view = reopened.workshop().read(access.clone()).unwrap();
    assert_eq!(view.state.sessions[0].working_text, "Before. Editable passage. After.");
    assert_eq!(view.state.sessions[0].working_generation, "0");
    assert_eq!(view.state.sessions[0].original_notes, "Original notes stay as evidence.");
    assert!(view.results[0].output.is_some());
    first_request.access = access.clone();
    assert_eq!(serde_json::to_value(&reopened.workshop().start(first_request).unwrap().packet).unwrap(), first_packet);

    let corrected = reopened.workshop().start(context_request(&reopened, &access, "interpretation-corrected", "concrete", "100000")).unwrap();
    let corrected_envelope: Value = serde_json::from_str(&corrected.packet.messages.last().unwrap().content).unwrap();
    assert_eq!(corrected_envelope["authorBrief"], "Corrected author attraction, independent of the prose.");
    assert_eq!(corrected_envelope["direction"], "Shared skills rather than a chosen savior.");
    assert_eq!(corrected_envelope["stillOpen"], "Who pays the cost remains open.");
    assert_eq!(corrected_envelope["currentElement"], first_envelope["currentElement"]);
    complete_context_fixture(&reopened, &corrected, context_candidates("refinement", &["Another possibility."]));

    let mut view = reopened.workshop().read(access.clone()).unwrap();
    view.state.sessions[0].brief.clear();
    save_state(&reopened, &access, "interpretation-clear", &view.version, view.state);
    let cleared = reopened.workshop().start(context_request(&reopened, &access, "interpretation-cleared", "concrete", "100000")).unwrap();
    let cleared_envelope: Value = serde_json::from_str(&cleared.packet.messages.last().unwrap().content).unwrap();
    assert_eq!(cleared_envelope["authorBrief"], "");
    assert_eq!(cleared_envelope["currentElement"], first_envelope["currentElement"]);
    assert_eq!(metadata_from_instruction(&first_instruction).unwrap().exploration.working_selection.unwrap().text, "Editable passage.");
}

#[test]
fn notes_organization_preserves_originals_and_parent_work_through_review_adoption_and_reopen() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("notes-organization".into()).unwrap();
    let original = "The archive accepts memories.\nMaybe it returns them?\nA line: ‘I remember the rain.’\nKeep the ending undecided.";
    let source = project.documents().create(CreateDocument {
        access: access.clone(), operation_id: "original-note".into(),
        document_id: "original-note".into(), title: "Raw archive notes".into(),
        kind: "note".into(), body: body("original", original),
    }).unwrap();
    let manuscript = project.documents().create(CreateDocument {
        access: access.clone(), operation_id: "notes-unrelated-chapter".into(),
        document_id: "chapter-other".into(), title: "An unrelated chapter".into(),
        kind: "chapter".into(), body: body("chapter", "UNRELATED_MANUSCRIPT_PROSE"),
    }).unwrap();
    let (mut state, _) = state_with_session("parent-work");
    state.sessions[0].working_text = "PARENT_WORKING_PROSE_AND_SELECTION".into();
    state.sessions[0].original_notes = original.into();
    state.sessions[0].direction = "PARENT_DIRECTION_SHOULD_NOT_BIAS_ORGANIZATION".into();
    state.sessions[0].focus_document_id = Some(source.head.document_id.clone());
    state.sessions[0].included_document_ids = vec![source.head.document_id.clone()];
    state.sessions[0].selected_details = vec![SelectedDetail {
        id: "parent-fixed-detail".into(), candidate_id: None,
        text: "PARENT_WORKING_PROSE_AND_SELECTION".into(), fixed: true,
    }];
    let parent = state.sessions[0].clone();
    let saved = save_state(&project, &access, "save-parent-notes", "0", state);
    let mut state = saved.state;
    let mut organization = session("notes-organization");
    organization.title = "Organize archive notes".into();
    organization.lens = Lens::Notebook;
    organization.anchor_document_id = Some("workshop-notes-organization".into());
    let organization_brief = "Organize the preserved notes without deciding their uncertainties.";
    organization.brief = organization_brief.into();
    organization.original_notes = original.into();
    organization.selected_scope = "Original notes organization".into();
    state.current_session_id = Some(organization.id.clone());
    state.sessions.push(organization);
    save_state(&project, &access, "save-organization", &saved.version, state);
    let mut request = context_request(&project, &access, "organize-notes", "directions", "100000");
    request.exploration.selected_scope = "Original notes organization".into();
    request.exploration.instruction = "Propose three organizations of originalNotes; retain ambiguity and create no story facts or settings.".into();
    let started = project.workshop().start(request.clone()).unwrap();
    let final_message = started.packet.messages.last().unwrap().content.clone();
    let metadata = metadata_from_instruction(&final_message).unwrap();
    assert_eq!(metadata.current_element, organization_brief);
    assert_eq!(metadata.original_notes, original);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&final_message).unwrap()["authorBrief"], organization_brief);
    assert!(metadata.direction.is_empty());
    assert!(metadata.exploration.working_selection.is_none());
    assert!(metadata.exploration.selected_text.is_empty());
    assert!(metadata.selected_details.is_empty());
    assert!(metadata.relationship.is_none());
    let sent_text = started.packet.messages.iter().map(|message| message.content.as_str()).collect::<String>();
    for excluded in ["UNRELATED_MANUSCRIPT_PROSE", "PARENT_WORKING_PROSE_AND_SELECTION", "PARENT_DIRECTION_SHOULD_NOT_BIAS_ORGANIZATION"] {
        assert!(!sent_text.contains(excluded));
    }
    let organized = "Archive\nThe archive accepts memories.\n\nUnresolved\nMaybe it returns them?\nKeep the ending undecided.\n\nDialogue fragment\nA line: ‘I remember the rain.’".to_owned();
    complete_context_fixture(&project, &started, context_candidates("directions", &[&organized, original, "Questions first: Maybe it returns them? The other notes remain available for review."]));
    let view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(view.state.sessions[0], parent);
    assert_eq!(view.state.sessions[1].original_notes, original);
    assert!(view.state.sessions[1].working_text.is_empty());
    assert!(view.state.decisions.is_empty());
    let candidates = &view.results[0].output.as_ref().unwrap().candidates;
    assert_eq!(candidates.len(), 3);
    let chosen = candidates[0].clone();
    let mut edited_state = view.state;
    edited_state.sessions[1].working_text = format!("{}\n\nAuthor addition: preserve this uncertainty.", chosen.content);
    edited_state.sessions[1].working_generation = "1".into();
    let reviewed = edited_state.sessions[1].working_text.clone();
    let saved = save_state(&project, &access, "edit-organization-proposal", &view.version, edited_state);
    let preview = project.workshop().preview_adoption(PreviewWorkshopAdoption {
        access: access.clone(), session_id: "notes-organization".into(), expected_version: saved.version,
        candidate_ids: vec![chosen.id], targets: vec![WorkshopAdoptionTarget {
            document_id: "organized-notes".into(), expected: None, title: "Organized notes".into(),
            kind: "note".into(), mode: AdoptionMode::Add, body: body("organized", &reviewed),
        }], rationale: "Group the notes; keep uncertain questions open.".into(), protected_text: vec![],
        relationships: vec![], impact_drafts: vec![],
    }).unwrap();
    let ack = project.workshop().adopt(access.clone(), "adopt-notes-organization".into(), preview.id).unwrap();
    assert_eq!(ack.decision_ids.len(), 1);
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let access = reopened.documents().attach("notes-reopened".into()).unwrap();
    let view = reopened.workshop().read(access.clone()).unwrap();
    assert_eq!(view.state.sessions[0], parent);
    assert_eq!(view.state.sessions[1].original_notes, original);
    assert_eq!(view.state.sessions[1].working_text, reviewed);
    for before in [source, manuscript] {
        let after = reopened.documents().read(access.clone(), before.head.document_id.clone()).unwrap();
        // Freezing a story snapshot may create an immutable checkpoint, but
        // organization/adoption must not change any author-owned source field.
        assert_eq!(after.head, before.head);
        assert_eq!(after.body, before.body);
        assert_eq!(after.title, before.title);
        assert_eq!(after.kind, before.kind);
        assert_eq!(after.metadata_version, before.metadata_version);
    }
    assert_eq!(reopened.documents().read(access.clone(), "organized-notes".into()).unwrap().body, body("organized", &reviewed));
    request.access = access;
    assert_eq!(reopened.workshop().start(request).unwrap().packet.messages.last().unwrap().content, final_message);
}

#[test]
fn one_candidate_moment_is_retained_as_invalid_raw_output() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("moment-cardinality".into()).unwrap();
    let anchor = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "moment-anchor".into(),
            document_id: "workshop-moment".into(),
            title: "Moment fixture".into(),
            kind: "note".into(),
            body: json!({"schemaVersion": 1, "body": {"type": "doc", "content": [{
                "type": "paragraph", "attrs": {"id": "anchor"}, "content": []
            }]}}),
        })
        .unwrap();
    let (mut state, _) = state_with_session("moment-session");
    state.sessions[0].anchor_document_id = Some(anchor.head.document_id.clone());
    save_state(&project, &access, "moment-state", "0", state);

    let started = project
        .workshop().start(context_request(
            &project,
            &access,
            "moment-one",
            "moment",
            "100000",
        ))
        .unwrap();
    complete_context_fixture(
        &project,
        &started,
        context_candidates("refinement", &["ONE_MOMENT_TREATMENT"]),
    );

    let view = project.workshop().read(access).unwrap();
    let result = view
        .results
        .iter()
        .find(|result| result.run.id == started.run.id)
        .expect("moment result");
    assert!(result.output.is_none());
    assert!(
        result
            .validation_error
            .as_deref()
            .is_some_and(|detail| detail.contains("two or three"))
    );
    assert!(result.run.output_text.contains("ONE_MOMENT_TREATMENT"));
}

#[test]
fn exploration_packet_excludes_rejected_archived_noncanon_and_chat_but_keeps_opted_in_alternative()
{
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("context-boundaries".into()).unwrap();
    project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "unrelated-note".into(),
            document_id: "unrelated-note".into(),
            title: "Unrelated note".into(),
            kind: "note".into(),
            body: body("note", "UNRELATED_NOTE_PROSE"),
        })
        .unwrap();
    let anchor = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "context-anchor".into(),
            document_id: "workshop-context".into(),
            title: "Context fixture".into(),
            kind: "note".into(),
            body: json!({"schemaVersion": 1, "body": {"type": "doc", "content": [{
                "type": "paragraph", "attrs": {"id": "anchor"}, "content": []
            }]}}),
        })
        .unwrap();
    let (mut state, _) = state_with_session("context-session");
    state.sessions[0].anchor_document_id = Some(anchor.head.document_id.clone());
    save_state(&project, &access, "context-state", "0", state);
    let chat = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "ordinary-chat".into(),
            expected: anchor.head,
            instruction: "UNRELATED_CHAT_INSTRUCTION".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: vec![],
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .unwrap();
    complete_context_fixture(&project, &chat, "UNRELATED_CHAT_ANSWER".into());
    let directions = project
        .workshop().start(context_request(
            &project,
            &access,
            "directions",
            "directions",
            "100000",
        ))
        .unwrap();
    complete_context_fixture(
        &project,
        &directions,
        context_candidates(
            "directions",
            &[
                "REJECTED_CANDIDATE_PROSE",
                "INCLUDED_ALTERNATIVE_PROSE",
                "ARCHIVED_CANDIDATE_PROSE",
            ],
        ),
    );
    let moment = project
        .workshop().start(context_request(
            &project, &access, "moment", "moment", "100000",
        ))
        .unwrap();
    complete_context_fixture(
        &project,
        &moment,
        context_candidates(
            "refinement",
            &[
                "NONCANON_VIGNETTE_PROSE",
                "NONCANON_VIGNETTE_ALTERNATIVE_PROSE",
            ],
        ),
    );
    let view = project.workshop().read(access.clone()).unwrap();
    let candidates = &view
        .results
        .iter()
        .find(|result| result.run.id == directions.run.id)
        .unwrap()
        .output
        .as_ref()
        .unwrap()
        .candidates;
    let mut state = view.state.clone();
    state.sessions[0].choices = vec![
        CandidateChoice {
            candidate_id: candidates[0].id.clone(),
            status: CandidateChoiceStatus::Rejected,
            rationale: "Avoid solving this problem through inherited privilege".into(),
            include_in_context: true,
        },
        CandidateChoice {
            candidate_id: candidates[1].id.clone(),
            status: CandidateChoiceStatus::Saved,
            rationale: "Keep for comparison".into(),
            include_in_context: true,
        },
        CandidateChoice {
            candidate_id: candidates[2].id.clone(),
            status: CandidateChoiceStatus::Archived,
            rationale: String::new(),
            include_in_context: true,
        },
    ];
    save_state(&project, &access, "choose-context", &view.version, state);
    let next = project
        .workshop().start(context_request(
            &project,
            &access,
            "filtered-context",
            "directions",
            "100000",
        ))
        .unwrap();
    let serialized = serde_json::to_string(&next.packet).unwrap();
    for excluded in [
        "UNRELATED_NOTE_PROSE",
        "UNRELATED_CHAT_INSTRUCTION",
        "UNRELATED_CHAT_ANSWER",
        "REJECTED_CANDIDATE_PROSE",
        "ARCHIVED_CANDIDATE_PROSE",
        "NONCANON_VIGNETTE_PROSE",
    ] {
        assert!(
            !serialized.contains(excluded),
            "unexpected context: {excluded}"
        );
    }
    let metadata =
        metadata_from_instruction(&next.packet.messages.last().unwrap().content).unwrap();
    assert_eq!(
        metadata.included_alternatives,
        ["INCLUDED_ALTERNATIVE_PROSE"]
    );
    assert!(
        metadata.chosen_details.is_empty(),
        "including an alternative must not adopt it"
    );
    assert_eq!(metadata.rejected_rationales.len(), 1);
    assert!(
        metadata.rejected_rationales[0]
            .contains("Avoid solving this problem through inherited privilege")
    );
    assert!(serialized.contains("excluded from workshop context by default"));
    let retained = project.workshop().read(access.clone()).unwrap();
    assert!(
        retained
            .results
            .iter()
            .any(|result| result.run.output_text.contains("NONCANON_VIGNETTE_PROSE"))
    );
    let packet_id = next.run.packet_id;
    let exact_packet = next.packet;
    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let access = reopened.documents().attach("context-reopen".into()).unwrap();
    assert_eq!(
        reopened.prepared_context(access, packet_id).unwrap(),
        exact_packet
    );
}

#[test]
fn outside_direction_keeps_hard_exclusions_and_refuses_budget_without_truncating_author_context() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("outside-context".into()).unwrap();
    let (mut state, _) = state_with_session("outside-session");
    let session = &mut state.sessions[0];
    session.anchor_document_id = Some("workshop-outside".into());
    session.outside_direction = true;
    session.direction = "Start in a coastal city".into();
    session.working_text = "The sister survives. The city can change.".into();
    session.selected_details = vec![SelectedDetail {
        id: "fixed-sister".into(),
        candidate_id: None,
        text: "The sister survives.".into(),
        fixed: true,
    }];
    session.original_notes = "Preserve every author note. ".repeat(80);
    state.preferences = vec![WorkshopPreference {
        id: "no-inherited-gift".into(),
        label: "Inherited power".into(),
        family: "progression".into(),
        meaning: "Abilities cannot be granted by ancestry".into(),
        examples: "No bloodline unlock".into(),
        timing: "Throughout this project".into(),
        polarity: PreferencePolarity::Avoid,
        strength: PreferenceStrength::Hard,
        scope: PreferenceScope::Project,
        target_id: None,
        confirmed: true,
    }];
    let saved = save_state(&project, &access, "save-outside", "0", state);
    let error = project
        .workshop().start(context_request(
            &project,
            &access,
            "small-context",
            "directions",
            "201",
        ))
        .unwrap_err();
    assert_eq!(error.code, "ContextPreparationFailed");
    assert!(
        error
            .detail
            .contains("do not fit the reserved input budget")
    );
    let after = project.workshop().read(access.clone()).unwrap();
    assert!(
        after.results.is_empty(),
        "budget refusal must not create a provider run"
    );
    assert_eq!(
        after.state, saved.state,
        "budget refusal must preserve manual work and preferences"
    );
    let started = project
        .workshop().start(context_request(
            &project,
            &access,
            "ample-context",
            "directions",
            "100000",
        ))
        .unwrap();
    let metadata =
        metadata_from_instruction(&started.packet.messages.last().unwrap().content).unwrap();
    assert!(metadata.outside_direction);
    assert_eq!(
        metadata.original_notes,
        saved.state.sessions[0].original_notes
    );
    assert!(
        metadata
            .fixed_details
            .iter()
            .any(|text| text == "The sister survives.")
    );
    assert_eq!(metadata.hard_constraints.len(), 1);
    assert!(metadata.hard_constraints[0].contains("avoid Inherited power"));
    assert!(metadata.hard_constraints[0].contains("strength=hard"));
    assert!(metadata.hard_constraints[0].contains("Throughout this project"));
    assert_eq!(project.workshop().read(access).unwrap().state, saved.state);
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
    let access = project.documents().attach("workshop-boundary-renderer".into()).unwrap();
    let literal = "First line\nSecond line\n\nThird line";
    let (mut state, _) = state_with_session("session-one");
    state.sessions[0].working_text = literal.into();
    let saved = save_state(&project, &access, "save-workshop", "0", state);
    assert_eq!(
        project
            .workshop().read(access.clone())
            .unwrap()
            .state
            .sessions[0]
            .working_text,
        literal
    );
    let mut request = preview_request(
        &access,
        "session-one",
        &saved.version,
        vec![WorkshopAdoptionTarget {
            document_id: "new-world".into(),
            expected: None,
            title: "World".into(),
            kind: "world".into(),
            mode: AdoptionMode::Add,
            body: json!({"schemaVersion":1,"body":{"type":"doc","content":[
                {"type":"paragraph","attrs":{"id":"first"},"content":[{"type":"text","text":"First line"},{"type":"hardBreak"},{"type":"text","text":"Second line"}]},
                {"type":"paragraph","attrs":{"id":"second"},"content":[{"type":"text","text":"Third line"}]}
            ]}}),
        }],
    );
    request.protected_text = vec![literal.into()];
    let preview = project.workshop().preview_adoption(request).unwrap();
    let adopted = project
        .workshop().adopt(access.clone(), "adopt-lines".into(), preview.id)
        .unwrap();
    let replacement = preview_request(
        &access,
        "session-one",
        &adopted.snapshot.version,
        vec![target(
            "new-world",
            Some(adopted.documents[0].head.clone()),
            "World",
            "world",
            "First line Second line Third line",
            AdoptionMode::Replace,
        )],
    );
    assert_eq!(
        project
            .workshop().preview_adoption(replacement)
            .unwrap_err()
            .code,
        "ProtectedContentChanged"
    );
}

#[test]
fn stale_existing_target_refuses_all_multi_target_adoption_before_writing() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("workshop-boundary-renderer".into()).unwrap();
    let character = project
        .documents().create(CreateDocument {
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
        .workshop().preview_adoption(preview_request(
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
        .documents().save(SaveSnapshot {
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
        .workshop().adopt(
            access.clone(),
            "adopt-stale-multi-target".into(),
            preview.id,
        )
        .expect_err("a stale existing target must refuse the whole adoption");
    assert_eq!(error.code, "VersionConflict");
    assert_eq!(error.current_head, Some(changed.head.clone()));
    assert_eq!(
        project
            .documents().read(access.clone(), "existing-character".into())
            .unwrap()
            .head,
        changed.head
    );
    assert_eq!(
        project
            .documents().read(access.clone(), "existing-character".into())
            .unwrap()
            .body,
        body(
            "character-paragraph",
            "The character changed after preview."
        )
    );
    assert_eq!(
        project
            .documents().read(access.clone(), "new-world".into())
            .unwrap_err()
            .code,
        "DocumentNotFound"
    );
    assert_eq!(project.documents().list(access.clone()).unwrap().len(), 1);
    assert_eq!(
        project.workshop().read(access.clone()).unwrap().version,
        saved.version
    );
    assert_eq!(project.workshop().history(access).unwrap().len(), 1);
}

#[test]
fn successful_multi_target_adoption_records_exact_history_and_leaves_chapters_untouched() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("workshop-boundary-renderer".into()).unwrap();
    let chapter = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("chapter-paragraph", "Chapter text remains unchanged."),
        })
        .unwrap();
    let character = project
        .documents().create(CreateDocument {
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
        .documents().history(access.clone(), "chapter-one".into())
        .unwrap();
    let character_history_before = project
        .documents().history(access.clone(), "existing-character".into())
        .unwrap();
    let (state, _) = state_with_session("session-one");
    let saved = save_state(&project, &access, "save-workshop", "0", state);
    let character_body = body("existing-character-paragraph", "The adopted character.");
    let preview = project
        .workshop().preview_adoption(preview_request(
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
        .workshop().adopt(
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
    let view = project.workshop().read(access.clone()).unwrap();
    assert_eq!(view.state.decisions.len(), 2);
    assert!(view.state.decisions.iter().all(|decision| decision.status
        == webnovel_core::projects::workshop::WorkshopDecisionStatus::Chosen
        && decision.access == "authorRoom"));

    let character_history = project
        .documents().history(access.clone(), "existing-character".into())
        .unwrap();
    assert_eq!(character_history.len(), character_history_before.len() + 2);
    assert_eq!(character_history[0].head, adopted.documents[1].head);
    assert_eq!(character_history[0].body, character_body);
    assert_eq!(character_history[1].head, character.head);
    assert_eq!(character_history[1].body, character.body);
    let world_history = project.documents().history(access.clone(), "new-world".into()).unwrap();
    assert_eq!(world_history.len(), 1);
    assert_eq!(world_history[0].head, adopted.documents[0].head);
    assert_eq!(world_history[0].body, adopted.documents[0].body);

    let chapter_after = project
        .documents().read(access.clone(), "chapter-one".into())
        .unwrap();
    assert_eq!(chapter_after.head, chapter_before.head);
    assert_eq!(chapter_after.body, chapter_before.body);
    let chapter_history_after = project
        .documents().history(access.clone(), "chapter-one".into())
        .unwrap();
    assert_eq!(chapter_history_after.len(), chapter_history_before.len());
    for (after, before) in chapter_history_after.iter().zip(&chapter_history_before) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.head, before.head);
        assert_eq!(after.body, before.body);
        assert_eq!(after.reason, before.reason);
        assert_eq!(after.parent_id, before.parent_id);
    }
    assert_eq!(project.documents().list(access).unwrap().len(), 3);
}

#[test]
fn chosen_author_secret_stays_out_of_restricted_writing_context() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("workshop-boundary-renderer".into()).unwrap();
    let chapter = project
        .documents().create(CreateDocument {
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
        .workshop().preview_adoption(preview_request(
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
        .workshop().adopt(access.clone(), "adopt-author-secret".into(), preview.id)
        .unwrap();
    let decision = project
        .workshop().read(access.clone())
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
    let access = project.documents().attach("workshop-boundary-renderer".into()).unwrap();
    let (mut state, workshop_session) = state_with_session("session-one");
    state.preferences = vec![
        WorkshopPreference {
            id: "project-preference".into(),
            label: "Pacing".into(),
            family: "story".into(),
            meaning: "Prefer a patient build".into(),
            examples: String::new(),
            timing: "Across the first arc".into(),
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
            timing: "In this scene".into(),
            polarity: PreferencePolarity::Avoid,
            strength: PreferenceStrength::Soft,
            scope: PreferenceScope::Exploration,
            target_id: Some("session-one".into()),
            confirmed: true,
        },
    ];
    let conflict = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "save-conflicting-preferences".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .expect_err("opposing local preference must surface a Rust conflict");
    assert_eq!(conflict.code, "PreferenceConflict");
    assert_eq!(project.workshop().read(access.clone()).unwrap().version, "0");

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
    assert!(generated
        .context
        .preferences
        .iter()
        .any(|preference| preference.contains("family=exploration")
            && preference.contains("timing=In this scene")));
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
    assert!(generated
        .context
        .hard_constraints
        .iter()
        .any(|constraint| constraint.contains("family=story")
            && constraint.contains("timing=Across the first arc")));
    assert!(
        !generated
            .context
            .hard_constraints
            .iter()
            .any(|constraint| constraint.contains("neutral Pacing"))
    );
}

#[test]
fn story_possibilities_save_reopen_and_project_only_open_nonempty_context() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("story-possibilities-renderer".into()).unwrap();
    let (mut state, mut workshop_session) = state_with_session("story-possibilities-session");
    workshop_session.anchor_document_id = Some("workshop-story-possibilities".into());
    workshop_session.working_text = "A sealed door waits beneath the station.".into();
    workshop_session.story_possibilities = vec![
        StoryPossibility {
            id: "question-one".into(),
            kind: StoryPossibilityKind::UnresolvedQuestion,
            text: "Who sealed the door, and what are they protecting?".into(),
            status: StoryPossibilityStatus::Open,
        },
        StoryPossibility {
            id: "payoff-one".into(),
            kind: StoryPossibilityKind::IntendedPayoff,
            text: "The door may answer the station's oldest promise.".into(),
            status: StoryPossibilityStatus::Archived,
        },
        StoryPossibility {
            id: "arc-one".into(),
            kind: StoryPossibilityKind::PossibleArc,
            text: String::new(),
            status: StoryPossibilityStatus::Open,
        },
    ];
    state.sessions = vec![workshop_session.clone()];
    let request = SaveWorkshop {
        access: access.clone(),
        operation_id: "story-possibilities-save".into(),
        expected_version: "0".into(),
        state: state.clone(),
    };
    let saved = project.workshop().save(request.clone()).unwrap();
    assert_eq!(project.workshop().save(request).unwrap(), saved);
    let stored = project.workshop().read(access.clone()).unwrap();
    assert_eq!(stored.state.sessions[0].story_possibilities.len(), 3);
    assert_eq!(stored.state.sessions[0].story_possibilities[2].text, "");

    let chapter = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "story-possibilities-chapter".into(),
            document_id: "story-possibilities-chapter".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("chapter-block", "Chapter remains untouched."),
        })
        .unwrap();
    let started = project
        .workshop().start(StartWorkshop {
            access: access.clone(),
            operation_id: "story-possibilities-start".into(),
            exploration: WorkshopExploration {
                session_id: workshop_session.id.clone(),
                expected_version: saved.version.clone(),
                working_generation: workshop_session.working_generation.clone(),
                action: "directions".into(),
                instruction: "Explore the open possibilities.".into(),
                selected_scope: "Whole working version".into(),
                selected_text: String::new(),
                working_selection: None,
            },
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let metadata = metadata_from_instruction(
        &started.packet.messages.last().unwrap().content,
    )
    .unwrap();
    assert_eq!(metadata.story_possibilities.len(), 1);
    assert_eq!(metadata.story_possibilities[0].id, "question-one");
    assert!(started.packet.messages[0]
        .content
        .contains("tentative author questions and future author intentions"));
    assert_eq!(
        project
            .documents().read(access.clone(), chapter.head.document_id.clone())
            .unwrap()
            .head,
        chapter.head
    );

    drop(project);
    let reopened = ProjectSession::open(&temp.0).unwrap();
    let reopened_access = reopened.documents().attach("story-possibilities-reopened".into()).unwrap();
    let reopened_state = reopened.workshop().read(reopened_access).unwrap().state;
    assert_eq!(reopened_state.sessions[0].story_possibilities, workshop_session.story_possibilities);
}

#[test]
fn story_possibilities_reject_duplicate_ids_and_oversized_text_but_allow_clear_rows() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("story-possibilities-validation".into()).unwrap();
    let (mut state, _) = state_with_session("story-possibilities-validation-session");
    let base = StoryPossibility {
        id: "duplicate".into(),
        kind: StoryPossibilityKind::PossibleArc,
        text: "A possible direction".into(),
        status: StoryPossibilityStatus::Open,
    };
    state.sessions[0].story_possibilities = vec![base.clone(), base];
    let duplicate = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "story-possibilities-duplicate".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap_err();
    assert!(duplicate.detail.contains("unique"));

    state.sessions[0].story_possibilities = vec![StoryPossibility {
        id: "oversized".into(),
        kind: StoryPossibilityKind::IntendedPayoff,
        text: "x".repeat(4001),
        status: StoryPossibilityStatus::Open,
    }];
    let oversized = project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "story-possibilities-oversized".into(),
            expected_version: "0".into(),
            state: state.clone(),
        })
        .unwrap_err();
    assert!(oversized.detail.contains("4000"));

    state.sessions[0].story_possibilities = vec![StoryPossibility {
        id: "clearable".into(),
        kind: StoryPossibilityKind::UnresolvedQuestion,
        text: String::new(),
        status: StoryPossibilityStatus::Open,
    }];
    let saved = project
        .workshop().save(SaveWorkshop {
            access,
            operation_id: "story-possibilities-clearable".into(),
            expected_version: "0".into(),
            state,
        })
        .unwrap();
    assert_eq!(saved.state.sessions[0].story_possibilities[0].text, "");
}

#[test]
fn workshop_history_rejects_snapshot_when_save_receipt_result_drifts() {
    let temp = TempProject::new();
    let project = temp.project();
    let access = project.documents().attach("workshop-history-receipt".into()).unwrap();
    let (state, _) = state_with_session("history-receipt-session");
    let saved = save_state(&project, &access, "history-receipt-save", "0", state);

    let history = project.workshop().history(access.clone()).unwrap();
    assert_eq!(history, vec![saved]);
    drop(project);

    let database = temp.0.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open workshop database");
    connection
        .execute_batch("DROP TRIGGER workshop_receipts_no_update;")
        .expect("drop immutable trigger for tamper fixture");
    let mut result: Value = connection
        .query_row(
            "SELECT result_json FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
            [&access.operation_namespace, "history-receipt-save"],
            |row| row.get::<_, String>(0),
        )
        .map(|json| serde_json::from_str(&json).expect("parse save receipt"))
        .expect("read save receipt");
    result["version"] = Value::String("999".into());
    connection
        .execute(
            "UPDATE workshop_receipts SET result_json=? WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![
                serde_json::to_string(&result).expect("serialize tampered receipt"),
                &access.operation_namespace,
                "history-receipt-save"
            ],
        )
        .expect("tamper receipt result");
    drop(connection);

    let reopened = ProjectSession::open(&temp.0).expect("reopen tampered project");
    let reopened_access = reopened
        .documents().attach("workshop-history-receipt-reopen".into())
        .unwrap();
    let error = reopened
        .workshop().history(reopened_access)
        .expect_err("history must reject a snapshot with a mismatched receipt result");
    assert_eq!(error.code, "InvalidProject");
    assert!(
        error.detail.contains("does not match its save receipt result"),
        "{}",
        error.detail
    );
}
