use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use webnovel_core::context::BasisKind;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::documents::{Endpoint, ScopeKind};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFinish, DiscussionRunStatus, DiscussionScopeInput, FeedbackIntent,
    SaveDiscussionDraft, StartDiscussion,
};
use webnovel_core::projects::proposals::{
    ApplyProposal, PrepareContinuation, PrepareProposal, ProposalContent, ProposalKind,
};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::{
    CreateDocument, Head, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

struct Fixture {
    root: PathBuf,
    project: Option<ProjectSession>,
    access: ProjectAccess,
    document: webnovel_core::projects::DocumentRecord,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("wns-continuation-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("project"), "Continuation tests").unwrap();
        let access = project.attach("renderer".into()).unwrap();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body("selected"),
            })
            .unwrap();
        Self {
            root,
            project: Some(project),
            access,
            document,
        }
    }

    fn project(&self) -> &ProjectSession {
        self.project.as_ref().unwrap()
    }

    fn continuation(
        &self,
        operation: &str,
    ) -> webnovel_core::projects::discussions::DiscussionStart {
        self.project()
            .start_discussion(StartDiscussion {
                access: self.access.clone(),
                operation_id: operation.into(),
                expected: self.document.head.clone(),
                instruction: "Continue this chapter with the next beat.".into(),
                intent: FeedbackIntent::Continue,
                basis: Some(BasisKind::Working),
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
            })
            .unwrap()
    }

    fn passage(&self, operation: &str) -> webnovel_core::projects::discussions::DiscussionStart {
        self.project()
            .start_discussion(StartDiscussion {
                access: self.access.clone(),
                operation_id: operation.into(),
                expected: self.document.head.clone(),
                instruction: "Clarify the selected passage.".into(),
                intent: FeedbackIntent::ProposeEdits,
                basis: None,
                scope: Some(DiscussionScopeInput {
                    kind: ScopeKind::Passage,
                    start: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 7,
                    }),
                    end: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 15,
                    }),
                    quote: "selected".into(),
                    source_body_hash: self.document.head.body_hash.clone(),
                }),
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
            })
            .unwrap()
    }

    fn finish(
        &self,
        start: &webnovel_core::projects::discussions::DiscussionStart,
        paragraphs: &[&str],
    ) {
        self.project()
            .begin_discussion_run(DiscussionBegin {
                owner: start.run.owner.clone(),
            })
            .unwrap();
        self.project()
            .mark_discussion_delivered(start.run.owner.clone())
            .unwrap();
        let output = serde_json::to_string(&json!({
            "schemaVersion": "continuation-output.v1",
            "suggestions": [{
                "title": "Next beat",
                "paragraphs": paragraphs,
                "explanation": "Keeps the scene moving."
            }]
        }))
        .unwrap();
        self.finish_raw(start, &output);
    }

    fn finish_raw(
        &self,
        start: &webnovel_core::projects::discussions::DiscussionStart,
        output: &str,
    ) {
        self.project()
            .finish_discussion(DiscussionFinish {
                owner: start.run.owner.clone(),
                expected_sequence: "0".into(),
                event_id: format!("finish-{}", start.run.id),
                assistant_text: output.into(),
            })
            .unwrap();
    }

    fn finish_passage(&self, start: &webnovel_core::projects::discussions::DiscussionStart) {
        self.project()
            .begin_discussion_run(DiscussionBegin {
                owner: start.run.owner.clone(),
            })
            .unwrap();
        self.project()
            .mark_discussion_delivered(start.run.owner.clone())
            .unwrap();
        self.finish_raw(
            start,
            r#"{"suggestions":[{"title":"clearer","replacementText":"clearer","explanation":"A test alternative."}]}"#,
        );
    }

    fn current(&self) -> webnovel_core::projects::DocumentRecord {
        self.project()
            .document(self.access.clone(), "chapter".into())
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.project.take());
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn body(text: &str) -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[
        {"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":format!("prefix {text} suffix")}]},
        {"type":"paragraph","attrs":{"id":"end"},"content":[{"type":"text","marks":[{"type":"italic"}],"text":"Protected ending."}]}
    ]}})
}

fn append_body(paragraphs: &[&str]) -> Value {
    let mut result = body("selected");
    let content = result["body"]["content"].as_array_mut().unwrap();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        content.push(json!({
            "type": "paragraph",
            "attrs": {"id": format!("generated-{index}")},
            "content": [{"type": "text", "text": paragraph}]
        }));
    }
    result
}

fn apply_request(
    access: &ProjectAccess,
    document: &Head,
    proposal_id: &str,
    prepared_id: &str,
    hash: &str,
) -> ApplyProposal {
    ApplyProposal {
        access: access.clone(),
        operation_id: "apply-continuation".into(),
        proposal_id: proposal_id.into(),
        prepared_id: prepared_id.into(),
        expected: document.clone(),
        result_hash: hash.into(),
        local_generation: "1".into(),
    }
}

fn make_schema18_archive(source: &Path, target: &Path, temp_root: &Path) {
    let file = fs::File::open(source).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut manifest_bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut archive.by_name("manifest.json").unwrap(),
        &mut manifest_bytes,
    )
    .unwrap();
    let mut database_bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut archive.by_name("project.sqlite3").unwrap(),
        &mut database_bytes,
    )
    .unwrap();
    let legacy_path = temp_root.join("schema18.sqlite3");
    fs::write(&legacy_path, database_bytes).unwrap();
    let database = Connection::open(&legacy_path).unwrap();
    database
        .execute_batch(
            "ALTER TABLE export_records DROP COLUMN review_bundle_id;
             ALTER TABLE proposals DROP COLUMN kind;
             ALTER TABLE proposal_versions DROP COLUMN payload_json;
             PRAGMA user_version=18;",
        )
        .unwrap();
    drop(database);
    let database_bytes = fs::read(&legacy_path).unwrap();
    let _ = fs::remove_file(&legacy_path);
    let digest = Sha256::digest(&database_bytes);
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    manifest["databaseSchemaVersion"] = json!(18);
    manifest["databaseSha256"] = json!(hash);
    let file = fs::File::create(target).unwrap();
    let mut output = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    output.start_file("manifest.json", options).unwrap();
    output
        .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
        .unwrap();
    output.start_file("project.sqlite3", options).unwrap();
    output.write_all(&database_bytes).unwrap();
    output.finish().unwrap();
}

fn reviewed_chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    operation: &str,
) -> webnovel_core::projects::DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: operation.into(),
            document_id: id.into(),
            title: id.into(),
            kind: "chapter".into(),
            body: body(id),
        })
        .unwrap()
}

fn mark_ready(project: &ProjectSession, access: &ProjectAccess, chapter: &Head, prefix: &str) {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: format!("stage-{prefix}"),
            expected: chapter.clone(),
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: format!("ready-{prefix}"),
            stage_id: stage.id,
        })
        .unwrap();
}

#[test]
fn continuation_candidate_and_prepared_paragraphs_survive_restart_and_backup() {
    let mut fixture = Fixture::new();
    let start = fixture.continuation("continue");
    fixture.finish(&start, &["The door opened.", "A cold wind entered."]);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(proposal.kind, ProposalKind::Continuation);
    match &proposal.candidate {
        ProposalContent::Continuation(candidate) => assert_eq!(
            candidate.paragraphs,
            vec!["The door opened.", "A cold wind entered."]
        ),
        ProposalContent::Passage(_) => panic!("continuation decoded as passage"),
    }
    let prepared = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "prepare-continuation".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["The door opened.".into(), "A cold wind entered.".into()],
            body: append_body(&["The door opened.", "A cold wind entered."]),
        })
        .unwrap();
    assert_eq!(prepared.replacement_text, "");
    assert_eq!(
        prepared.paragraphs.as_ref().unwrap(),
        &vec![
            "The door opened.".to_owned(),
            "A cold wind entered.".to_owned()
        ]
    );
    let archive = fixture.root.join("continuation.wnsbackup");
    create_backup(fixture.project(), &archive).unwrap();
    drop(fixture.project.take());
    fixture.project = Some(ProjectSession::open(fixture.root.join("project")).unwrap());
    fixture.access = fixture.project().attach("restarted".into()).unwrap();
    let reopened = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap();
    assert_eq!(reopened[0].prepared.as_ref().unwrap().id, prepared.id);
    assert_eq!(reopened[0].kind, ProposalKind::Continuation);

    let recovered_path = fixture.root.join("recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered continuation").unwrap();
    let recovered_access = recovered.attach("recovered".into()).unwrap();
    let copied = recovered
        .proposals(recovered_access.clone(), "chapter".into())
        .unwrap();
    assert!(copied[0].historical_copy);
    assert_eq!(
        copied[0].prepared.as_ref().unwrap().paragraphs,
        prepared.paragraphs
    );
    let mut copied_apply = apply_request(
        &recovered_access,
        &fixture.document.head,
        &copied[0].id,
        &prepared.id,
        &prepared.body_hash,
    );
    copied_apply.operation_id = "copied-apply".into();
    assert_eq!(
        recovered.apply_proposal(copied_apply).unwrap_err().code,
        "ContextProjectMismatch"
    );
}

#[test]
fn continuation_apply_is_atomic_and_idempotent() {
    let fixture = Fixture::new();
    let start = fixture.continuation("continue");
    fixture.finish(&start, &["The door opened."]);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    let prepared = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "prepare-continuation".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["The door opened.".into()],
            body: append_body(&["The door opened."]),
        })
        .unwrap();
    let applied = fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap();
    assert_eq!(applied.document.body, append_body(&["The door opened."]));
    let replay = fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap();
    assert!(replay.already_applied);

    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    assert!(proposal.decision.is_some());
    assert_eq!(fixture.current().head.version, "1");
}

#[test]
fn continuation_becomes_stale_after_typing_without_erasing_the_prepared_history() {
    let fixture = Fixture::new();
    let start = fixture.continuation("continue");
    fixture.finish(&start, &["The door opened."]);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    let prepared = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "prepare-continuation".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["The door opened.".into()],
            body: append_body(&["The door opened."]),
        })
        .unwrap();
    let changed = fixture
        .project()
        .save(SaveSnapshot {
            access: fixture.access.clone(),
            operation_id: "manual-edit".into(),
            expected: fixture.document.head.clone(),
            local_generation: "1".into(),
            body: body("manual edit"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let error = fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "SuggestionStale" | "VersionConflict"
    ));
    assert_eq!(fixture.current().head, changed.head);
    let retained = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap();
    assert!(!retained[0].current);
    assert!(retained[0].decision.is_none());
    create_backup(
        fixture.project(),
        &fixture.root.join("stale-continuation.wnsbackup"),
    )
    .unwrap();
}

#[test]
fn reviewed_continuation_requires_a_real_reviewed_prefix_and_never_falls_back_to_working() {
    let root = std::env::temp_dir().join(format!("wns-reviewed-continuation-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let project = ProjectSession::create(root.join("project"), "Reviewed continuation").unwrap();
    let access = project.attach("reviewed".into()).unwrap();
    let earlier = reviewed_chapter(&project, &access, "earlier", "create-earlier");
    let target = reviewed_chapter(&project, &access, "target", "create-target");
    mark_ready(&project, &access, &earlier.head, "earlier");
    let start = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "reviewed-continuation".into(),
            expected: target.head.clone(),
            instruction: "Continue from the reviewed story.".into(),
            intent: FeedbackIntent::Continue,
            basis: Some(BasisKind::Reviewed),
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
        })
        .unwrap();
    assert_eq!(start.run.basis, Some(BasisKind::Reviewed));
    drop(project);

    let missing_root = root.join("missing");
    let missing = ProjectSession::create(&missing_root, "Missing reviewed prefix").unwrap();
    let missing_access = missing.attach("missing".into()).unwrap();
    let missing_target = reviewed_chapter(&missing, &missing_access, "target", "create-target");
    let error = missing
        .start_discussion(StartDiscussion {
            access: missing_access,
            operation_id: "missing-reviewed".into(),
            expected: missing_target.head,
            instruction: "This must not silently use Working draft.".into(),
            intent: FeedbackIntent::Continue,
            basis: Some(BasisKind::Reviewed),
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
        })
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "ReviewBasisUnavailable" | "BasisUnavailable"
    ));
    drop(missing);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn continuation_draft_retry_and_stop_preserve_basis_without_retaining_a_candidate() {
    let fixture = Fixture::new();
    let start = fixture.continuation("continue-stop");
    let draft = fixture
        .project()
        .save_discussion_draft(SaveDiscussionDraft {
            access: fixture.access.clone(),
            operation_id: "continuation-draft".into(),
            document_id: "chapter".into(),
            expected_version: "0".into(),
            text: "Continue from this ending.".into(),
            intent: FeedbackIntent::Continue,
            basis: Some(BasisKind::Working),
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
        })
        .unwrap();
    assert_eq!(draft.intent, FeedbackIntent::Continue);
    assert_eq!(draft.basis, Some(BasisKind::Working));
    assert!(draft.scope.is_none());
    let view = fixture
        .project()
        .read_discussion(fixture.access.clone(), "chapter".into())
        .unwrap();
    assert_eq!(view.draft.unwrap().basis, Some(BasisKind::Working));

    let stopped = fixture
        .project()
        .stop_discussion(fixture.access.clone(), start.run.id.clone())
        .unwrap();
    assert_eq!(stopped.run.status, DiscussionRunStatus::Stopped);
    assert!(
        fixture
            .project()
            .proposals(fixture.access.clone(), "chapter".into())
            .unwrap()
            .is_empty()
    );
    let retry = fixture
        .project()
        .discussion_retry(fixture.access.clone(), start.run.id)
        .unwrap();
    assert_eq!(retry.intent, FeedbackIntent::Continue);
    assert_eq!(retry.basis, Some(BasisKind::Working));
    assert!(retry.scope.is_none());
}

#[test]
fn malformed_append_output_and_scope_input_never_create_a_candidate() {
    let fixture = Fixture::new();
    let start = fixture.continuation("bad-output");
    fixture
        .project()
        .begin_discussion_run(DiscussionBegin {
            owner: start.run.owner.clone(),
        })
        .unwrap();
    fixture
        .project()
        .mark_discussion_delivered(start.run.owner.clone())
        .unwrap();
    fixture.finish_raw(
        &start,
        r#"{"schemaVersion":"continuation-output.v1","suggestions":[{"title":"bad","paragraphs":["line\nnext"],"explanation":"bad"}]}"#,
    );
    assert!(
        fixture
            .project()
            .proposals(fixture.access.clone(), "chapter".into())
            .unwrap()
            .is_empty()
    );

    let wrong_scope = StartDiscussion {
        access: fixture.access.clone(),
        operation_id: "wrong-scope".into(),
        expected: fixture.document.head.clone(),
        instruction: "Wrong scope must fail closed.".into(),
        intent: FeedbackIntent::Continue,
        basis: Some(BasisKind::Working),
        scope: Some(webnovel_core::projects::discussions::DiscussionScopeInput {
            kind: webnovel_core::documents::ScopeKind::Passage,
            start: None,
            end: None,
            quote: String::new(),
            source_body_hash: fixture.document.head.body_hash.clone(),
        }),
        pinned_document_ids: Vec::new(),
        safe_brief: None,
        budget: MockContextBudget::new("100000", "1000", "100"),
        provider_binding: None,
        previous_run_id: None,
    };
    let error = fixture.project().start_discussion(wrong_scope).unwrap_err();
    assert_eq!(error.code, "InvalidContinuationBasis");
}

#[test]
fn unrelated_late_source_creation_invalidates_continuation_apply() {
    let fixture = Fixture::new();
    let start = fixture.continuation("late-unrelated");
    fixture.finish(&start, &["The door opened."]);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    let prepared = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "prepare-late-unrelated".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["The door opened.".into()],
            body: append_body(&["The door opened."]),
        })
        .unwrap();
    fixture
        .project()
        .create_document(CreateDocument {
            access: fixture.access.clone(),
            operation_id: "create-unrelated-note".into(),
            document_id: "unrelated-note".into(),
            title: "Unrelated note".into(),
            kind: "note".into(),
            body: body("new evidence"),
        })
        .unwrap();
    let error = fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "SuggestionStale" | "VersionConflict"
    ));
    assert!(
        fixture
            .project()
            .proposals(fixture.access.clone(), "chapter".into())
            .unwrap()
            .iter()
            .all(|proposal| proposal.decision.is_none() && !proposal.current)
    );
}

#[test]
fn policy_revocation_blocks_continuation_apply_but_keeps_the_retained_candidate_readable() {
    let fixture = Fixture::new();
    let start = fixture.continuation("policy-revocation");
    fixture.finish(&start, &["The door opened."]);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    let prepared = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "prepare-policy".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["The door opened.".into()],
            body: append_body(&["The door opened."]),
        })
        .unwrap();
    let policy = fixture
        .project()
        .context_epochs(fixture.access.clone())
        .unwrap()
        .policy;
    fixture
        .project()
        .revoke_story_context(fixture.access.clone(), policy)
        .unwrap();
    let error = fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "SuggestionStale" | "ContextPolicyChanged" | "VersionConflict"
    ));
    let retained = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap();
    assert!(!retained[0].current);
    assert!(retained[0].decision.is_none());
}

#[test]
fn schema18_archive_with_legacy_passage_chain_migrates_before_reader_validation() {
    let mut fixture = Fixture::new();
    let start = fixture.passage("legacy-passage");
    fixture.finish_passage(&start);
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    let prepared = fixture
        .project()
        .prepare_proposal(PrepareProposal {
            access: fixture.access.clone(),
            operation_id: "legacy-prepare".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            replacement_text: "clearer".into(),
            body: body("clearer"),
        })
        .unwrap();
    fixture
        .project()
        .apply_proposal(apply_request(
            &fixture.access,
            &fixture.document.head,
            &proposal.id,
            &prepared.id,
            &prepared.body_hash,
        ))
        .unwrap();
    let current_archive = fixture.root.join("current.wnsbackup");
    create_backup(fixture.project(), &current_archive).unwrap();
    drop(fixture.project.take());

    let schema18_archive = fixture.root.join("schema18.wnsbackup");
    make_schema18_archive(&current_archive, &schema18_archive, &fixture.root);
    let recovered_path = fixture.root.join("schema18-recovered");
    let recovered =
        recover_backup(&schema18_archive, &recovered_path, "Recovered schema18").unwrap();
    let access = recovered.attach("schema18-reader".into()).unwrap();
    let proposals = recovered.proposals(access, "chapter".into()).unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].kind, ProposalKind::Passage);
    assert!(proposals[0].historical_copy);
    assert_eq!(
        proposals[0].prepared.as_ref().unwrap().replacement_text,
        "clearer"
    );
    assert_eq!(proposals[0].decision.as_ref().unwrap().kind, "apply");
}
