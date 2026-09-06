use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use uuid::Uuid;
use webnovel_core::context::{
    BasisKind,
    packet::{MockContextBudget, ProviderBinding, serialized_input},
};
use webnovel_core::documents::{
    Endpoint, ScopeGrant, ScopeKind, TypedReplacementBlock, TypedReplacementHeadingAttrs,
    TypedReplacementInline, TypedReplacementMark, capture_scope, validate_structured_replacement,
};
use webnovel_core::projects::ProjectAccess;
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFinish, DiscussionScopeInput, FeedbackIntent, ProviderCleanup,
    ProviderOutcomeStatus, ProviderTerminalReport, StartDiscussion,
};
use webnovel_core::projects::proposals::{
    ApplyProposal, PrepareContinuation, PrepareProposal, PrepareStructured, ProposalCandidate,
    ProposalContent, ProposalKind, ProposalOutput, RejectProposal, StructuredProposalCandidate,
    StructuredProposalOutput,
};
use webnovel_core::projects::{CreateDocument, ProjectSession};

#[path = "support/schema.rs"]
mod legacy_schema;

struct Fixture {
    root: PathBuf,
    project: Option<ProjectSession>,
    access: ProjectAccess,
    document: webnovel_core::projects::DocumentRecord,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("wns-structured-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("project"), "Structured proposals").unwrap();
        let access = project.attach("renderer".into()).unwrap();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: source_body(),
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

    fn start(
        &self,
        scope: &ScopeGrant,
        operation_id: &str,
    ) -> webnovel_core::projects::discussions::DiscussionStart {
        self.project()
            .start_discussion(StartDiscussion {
                access: self.access.clone(),
                operation_id: operation_id.into(),
                expected: self.document.head.clone(),
                instruction: "Revise the selected chapter blocks.".into(),
                intent: FeedbackIntent::ProposeEdits,
                basis: None,
                scope: Some(DiscussionScopeInput {
                    kind: scope.kind,
                    start: scope.start.clone(),
                    end: scope.end.clone(),
                    quote: scope.quote.clone(),
                    source_body_hash: scope.source_hash.clone(),
                }),
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
                lookup: None,
            })
            .unwrap()
    }

    fn start_continuation(
        &self,
        operation_id: &str,
    ) -> webnovel_core::projects::discussions::DiscussionStart {
        self.project()
            .start_discussion(StartDiscussion {
                access: self.access.clone(),
                operation_id: operation_id.into(),
                expected: self.document.head.clone(),
                instruction: "Continue the chapter from its ending.".into(),
                intent: FeedbackIntent::Continue,
                basis: Some(BasisKind::Working),
                scope: None,
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
                lookup: None,
            })
            .unwrap()
    }

    fn finish(&self, start: &webnovel_core::projects::discussions::DiscussionStart, text: String) {
        self.project()
            .begin_discussion_run(DiscussionBegin {
                owner: start.run.owner.clone(),
            })
            .unwrap();
        self.project()
            .mark_discussion_delivered(start.run.owner.clone())
            .unwrap();
        self.project()
            .finish_discussion(DiscussionFinish {
                owner: start.run.owner.clone(),
                expected_sequence: "0".into(),
                event_id: "finish".into(),
                assistant_text: text,
            })
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.project.take());
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn source_body() -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type":"paragraph","attrs":{"id":"before"},"content":[{"type":"text","text":"Before."}]},
            {"type":"heading","attrs":{"id":"old-heading","level":2},"content":[{"type":"text","text":"Old heading"}]},
            {"type":"paragraph","attrs":{"id":"old-body"},"content":[{"type":"text","marks":[{"type":"bold"}],"text":"Old body."}]},
            {"type":"paragraph","attrs":{"id":"after"},"content":[{"type":"text","text":"After."}]}
        ]}
    })
}

fn block_scope(body: &Value) -> ScopeGrant {
    capture_scope(
        body,
        ScopeGrant {
            kind: ScopeKind::Blocks,
            start: Some(Endpoint {
                block_id: "old-heading".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "old-body".into(),
                utf16_offset: 9,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap()
}

fn passage_scope(body: &Value) -> ScopeGrant {
    capture_scope(
        body,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "before".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "before".into(),
                utf16_offset: 7,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap()
}

fn whole_scope(body: &Value) -> ScopeGrant {
    capture_scope(
        body,
        ScopeGrant {
            kind: ScopeKind::WholeDocument,
            start: None,
            end: None,
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap()
}

fn structured_output(blocks: Vec<TypedReplacementBlock>) -> String {
    serde_json::to_string(&StructuredProposalOutput {
        schema_version: "structured-proposal-output.v1".into(),
        suggestions: vec![StructuredProposalCandidate {
            title: "Reframed blocks".into(),
            blocks,
            explanation: "Preserves the surrounding chapter structure.".into(),
        }],
    })
    .unwrap()
}

fn legacy_output() -> String {
    serde_json::to_string(&ProposalOutput {
        suggestions: vec![ProposalCandidate {
            title: "Legacy wording".into(),
            replacement_text: "Changed legacy.".into(),
            explanation: "Keeps the old passage contract.".into(),
        }],
    })
    .unwrap()
}

fn append_body(paragraphs: &[&str]) -> Value {
    let mut result = source_body();
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

fn legacy_schema21_proposal_tables(connection: &Connection) {
    connection
        .execute_batch(
            r#"
PRAGMA foreign_keys=OFF;
DROP TRIGGER IF EXISTS proposals_immutable_update;
DROP TRIGGER IF EXISTS proposals_immutable_delete;
DROP TRIGGER IF EXISTS proposal_versions_immutable_update;
DROP TRIGGER IF EXISTS proposal_versions_immutable_delete;
DROP TRIGGER IF EXISTS proposal_decisions_immutable_update;
DROP TRIGGER IF EXISTS proposal_decisions_immutable_delete;
ALTER TABLE proposals RENAME TO proposals_before_schema22;
ALTER TABLE proposal_versions RENAME TO proposal_versions_before_schema22;
ALTER TABLE proposal_decisions RENAME TO proposal_decisions_before_schema22;
CREATE TABLE proposals (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES discussion_runs(id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 2),
    candidate_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    kind TEXT NOT NULL DEFAULT 'passage'
        CHECK (kind IN ('passage','continuation')),
    UNIQUE(run_id, ordinal)
) STRICT;
CREATE TABLE proposal_versions (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    version INTEGER NOT NULL CHECK (version > 0),
    replacement_text TEXT NOT NULL,
    body_json TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    payload_json TEXT,
    UNIQUE(proposal_id, version)
) STRICT;
CREATE TABLE proposal_decisions (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL UNIQUE REFERENCES proposals(id),
    kind TEXT NOT NULL CHECK (kind IN ('apply','reject')),
    prepared_id TEXT REFERENCES proposal_versions(id),
    before_revision_id TEXT REFERENCES revisions(id),
    after_revision_id TEXT REFERENCES revisions(id),
    operation_namespace TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK ((kind='apply' AND prepared_id IS NOT NULL AND before_revision_id IS NOT NULL AND after_revision_id IS NOT NULL)
        OR (kind='reject' AND prepared_id IS NULL AND before_revision_id IS NULL AND after_revision_id IS NULL)),
    UNIQUE(operation_namespace, operation_id)
) STRICT;
INSERT INTO proposals(id,run_id,ordinal,candidate_json,created_at,kind)
SELECT id,run_id,ordinal,candidate_json,created_at,kind
FROM proposals_before_schema22;
INSERT INTO proposal_versions(id,proposal_id,version,replacement_text,body_json,body_hash,created_at,payload_json)
SELECT id,proposal_id,version,replacement_text,body_json,body_hash,created_at,payload_json
FROM proposal_versions_before_schema22;
INSERT INTO proposal_decisions(id,proposal_id,kind,prepared_id,before_revision_id,after_revision_id,operation_namespace,operation_id,created_at)
SELECT id,proposal_id,kind,prepared_id,before_revision_id,after_revision_id,operation_namespace,operation_id,created_at
FROM proposal_decisions_before_schema22;
DROP TABLE proposal_decisions_before_schema22;
DROP TABLE proposal_versions_before_schema22;
DROP TABLE proposals_before_schema22;
CREATE TRIGGER proposals_immutable_update BEFORE UPDATE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposals_immutable_delete BEFORE DELETE ON proposals BEGIN SELECT RAISE(ABORT,'Immutable proposal'); END;
CREATE TRIGGER proposal_versions_immutable_update BEFORE UPDATE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_versions_immutable_delete BEFORE DELETE ON proposal_versions BEGIN SELECT RAISE(ABORT,'Immutable prepared proposal'); END;
CREATE TRIGGER proposal_decisions_immutable_update BEFORE UPDATE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
CREATE TRIGGER proposal_decisions_immutable_delete BEFORE DELETE ON proposal_decisions BEGIN SELECT RAISE(ABORT,'Immutable author decision'); END;
ALTER TABLE review_stages DROP COLUMN promises_json;
ALTER TABLE review_stages DROP COLUMN promises_hash;
ALTER TABLE ready_bundles DROP COLUMN promises_json;
ALTER TABLE ready_bundles DROP COLUMN promises_hash;
PRAGMA user_version=21;
PRAGMA foreign_keys=ON;
"#,
        )
        .unwrap();
}

#[test]
fn block_range_structured_candidate_prepares_and_applies_with_protected_neighbors() {
    let fixture = Fixture::new();
    let scope = block_scope(&fixture.document.body);
    let start = fixture.start(&scope, "structured-start");
    fixture.finish(
        &start,
        structured_output(vec![
            TypedReplacementBlock::Heading {
                attrs: TypedReplacementHeadingAttrs { level: 1 },
                content: vec![TypedReplacementInline::Text {
                    text: "New heading".into(),
                    marks: vec![TypedReplacementMark::Italic],
                }],
            },
            TypedReplacementBlock::Paragraph {
                content: vec![
                    TypedReplacementInline::Text {
                        text: "New ".into(),
                        marks: vec![],
                    },
                    TypedReplacementInline::Text {
                        text: "body.".into(),
                        marks: vec![TypedReplacementMark::Bold],
                    },
                ],
            },
        ]),
    );

    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(proposal.kind, ProposalKind::Structured);
    assert!(matches!(proposal.candidate, ProposalContent::Structured(_)));
    let body = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type":"paragraph","attrs":{"id":"before"},"content":[{"type":"text","text":"Before."}]},
            {"type":"heading","attrs":{"id":"new-heading","level":1},"content":[{"type":"text","marks":[{"type":"italic"}],"text":"New heading"}]},
            {"type":"paragraph","attrs":{"id":"new-body"},"content":[{"type":"text","text":"New "},{"type":"text","marks":[{"type":"bold"}],"text":"body."}]},
            {"type":"paragraph","attrs":{"id":"after"},"content":[{"type":"text","text":"After."}]}
        ]}
    });
    let prepared = fixture
        .project()
        .prepare_structured(PrepareStructured {
            access: fixture.access.clone(),
            operation_id: "structured-prepare".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            blocks: match proposal.candidate.clone() {
                ProposalContent::Structured(candidate) => candidate.blocks,
                _ => unreachable!(),
            },
            body,
        })
        .unwrap();
    assert!(prepared.blocks.is_some());
    assert!(prepared.paragraphs.is_none());
    let applied = fixture
        .project()
        .apply_proposal(ApplyProposal {
            access: fixture.access.clone(),
            operation_id: "structured-apply".into(),
            proposal_id: proposal.id,
            prepared_id: prepared.id,
            expected: fixture.document.head.clone(),
            result_hash: prepared.body_hash,
            local_generation: "1".into(),
        })
        .unwrap();
    let content = applied.document.body["body"]["content"].as_array().unwrap();
    assert_eq!(content[0]["attrs"]["id"], "before");
    assert_eq!(content[3]["attrs"]["id"], "after");
    assert_eq!(content[1]["attrs"]["id"], "new-heading");
    assert_eq!(content[2]["attrs"]["id"], "new-body");
}

#[test]
fn structured_validator_rejects_source_id_reuse_and_tampered_unselected_block() {
    let source = source_body();
    let scope = block_scope(&source);
    let blocks = vec![TypedReplacementBlock::Paragraph {
        content: vec![TypedReplacementInline::Text {
            text: "Replacement".into(),
            marks: vec![],
        }],
    }];
    let reused = json!({
        "schemaVersion":1,"body":{"type":"doc","content":[
            {"type":"paragraph","attrs":{"id":"before"},"content":[{"type":"text","text":"Changed."}]},
            {"type":"paragraph","attrs":{"id":"old-heading"},"content":[{"type":"text","text":"Replacement"}]},
            {"type":"paragraph","attrs":{"id":"after"},"content":[{"type":"text","text":"After."}]}
        ]}
    });
    let error = validate_structured_replacement(
        &webnovel_core::documents::ScopeValidationRequest {
            source_snapshot: source.clone(),
            result_snapshot: reused,
            scope: scope.clone(),
        },
        &blocks,
    )
    .unwrap_err();
    assert!(
        error.contains("source ID") || error.contains("unselected"),
        "{error}"
    );
}

#[test]
fn whole_document_scope_uses_versioned_contract_and_preserves_formatting() {
    let fixture = Fixture::new();
    let scope = whole_scope(&fixture.document.body);
    let request = StartDiscussion {
        access: fixture.access.clone(),
        operation_id: "structured-whole-start".into(),
        expected: fixture.document.head.clone(),
        instruction: "Rewrite this chapter structure while retaining its meaning.".into(),
        intent: FeedbackIntent::ProposeEdits,
        basis: None,
        scope: Some(DiscussionScopeInput {
            kind: scope.kind,
            start: None,
            end: None,
            quote: scope.quote.clone(),
            source_body_hash: scope.source_hash.clone(),
        }),
        pinned_document_ids: Vec::new(),
        safe_brief: None,
        budget: MockContextBudget::new("100000", "1000", "100"),
        provider_binding: Some(ProviderBinding::codex_luna()),
        previous_run_id: None,
        lookup: None,
    };
    let started = fixture.project().start_discussion(request.clone()).unwrap();
    assert!(
        started.packet.messages[0]
            .content
            .contains("structured-proposal-output.v1")
    );
    assert!(started.packet.messages[0].content.contains("sceneBreak"));

    let local = fixture.start(&scope, "structured-whole-local");
    fixture.finish(
        &local,
        structured_output(vec![
            TypedReplacementBlock::Paragraph {
                content: vec![TypedReplacementInline::Text {
                    text: "New opening.".into(),
                    marks: vec![TypedReplacementMark::Bold],
                }],
            },
            TypedReplacementBlock::SceneBreak,
            TypedReplacementBlock::Heading {
                attrs: TypedReplacementHeadingAttrs { level: 3 },
                content: vec![TypedReplacementInline::Text {
                    text: "New ending".into(),
                    marks: vec![],
                }],
            },
        ]),
    );
    let proposal = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(proposal.kind, ProposalKind::Structured);
    assert!(proposal.current);
    let blocks = match proposal.candidate.clone() {
        ProposalContent::Structured(candidate) => candidate.blocks,
        _ => unreachable!(),
    };
    let body = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type":"paragraph","attrs":{"id":"whole-opening"},"content":[{"type":"text","marks":[{"type":"bold"}],"text":"New opening."}]},
            {"type":"sceneBreak","attrs":{"id":"whole-break"}},
            {"type":"heading","attrs":{"id":"whole-ending","level":3},"content":[{"type":"text","text":"New ending"}]}
        ]}
    });
    let prepared = fixture
        .project()
        .prepare_structured(PrepareStructured {
            access: fixture.access.clone(),
            operation_id: "structured-whole-prepare".into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: "0".into(),
            blocks,
            body,
        })
        .unwrap();
    let applied = fixture
        .project()
        .apply_proposal(ApplyProposal {
            access: fixture.access.clone(),
            operation_id: "structured-whole-apply".into(),
            proposal_id: proposal.id,
            prepared_id: prepared.id,
            expected: fixture.document.head.clone(),
            result_hash: prepared.body_hash,
            local_generation: "1".into(),
        })
        .unwrap();
    assert_eq!(
        applied.document.body["body"]["content"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn nonchapter_develop_uses_author_room_whole_document_scope_and_retains_candidate() {
    let fixture = Fixture::new();
    let note = fixture
        .project()
        .create_document(CreateDocument {
            access: fixture.access.clone(),
            operation_id: "create-world-note".into(),
            document_id: "world-note".into(),
            title: "World rules".into(),
            kind: "world".into(),
            body: source_body(),
        })
        .unwrap();
    let scope = whole_scope(&note.body);
    let started = fixture
        .project()
        .start_discussion(StartDiscussion {
            access: fixture.access.clone(),
            operation_id: "develop-world-note".into(),
            expected: note.head.clone(),
            instruction: "Develop these rules while preserving their existing ideas.".into(),
            intent: FeedbackIntent::ProposeEdits,
            basis: None,
            scope: Some(DiscussionScopeInput {
                kind: scope.kind,
                start: None,
                end: None,
                quote: scope.quote.clone(),
                source_body_hash: scope.source_hash.clone(),
            }),
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: Some(ProviderBinding::codex_luna()),
            previous_run_id: None,
            lookup: None,
        })
        .unwrap();
    let frozen = fixture
        .project()
        .story_snapshot(
            fixture.access.clone(),
            started.packet.receipt.snapshot_id.clone(),
        )
        .unwrap();
    assert_eq!(
        frozen.policy.audience,
        webnovel_core::context::Audience::AuthorRoom
    );
    assert!(frozen.policy.reader_frontier.is_none());
    let target_source = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.source.document_id == note.head.document_id)
        .expect("non-chapter target is present in the frozen source manifest");
    assert!(target_source.disclosure.author_only);
    assert_eq!(
        started.packet.options.provider_binding,
        Some(ProviderBinding::codex_luna())
    );

    let dispatch = fixture
        .project()
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    fixture
        .project()
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "world-provider-complete".into(),
            assistant_text: structured_output(vec![TypedReplacementBlock::Paragraph {
                content: vec![TypedReplacementInline::Text {
                    text: "Developed world rule.".into(),
                    marks: vec![],
                }],
            }]),
            binding: ProviderBinding::codex_luna(),
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: serialized_input(
                &dispatch.packet.messages,
                &dispatch.packet.options,
            )
            .unwrap()
            .len()
            .to_string(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
            effective_identity: None,
            reported_model: None,
            delivery: None,
        })
        .unwrap();
    let proposals = fixture
        .project()
        .proposals(fixture.access.clone(), note.head.document_id.clone())
        .unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].kind, ProposalKind::Structured);
    assert!(proposals[0].current);
}

#[test]
fn nonchapter_passage_develop_is_rejected_before_context_compilation() {
    let fixture = Fixture::new();
    let note = fixture
        .project()
        .create_document(CreateDocument {
            access: fixture.access.clone(),
            operation_id: "create-world-passage".into(),
            document_id: "world-passage".into(),
            title: "World rules".into(),
            kind: "world".into(),
            body: source_body(),
        })
        .unwrap();
    let mut scope = block_scope(&note.body);
    scope.kind = ScopeKind::Passage;
    let error = fixture
        .project()
        .start_discussion(StartDiscussion {
            access: fixture.access.clone(),
            operation_id: "develop-world-passage".into(),
            expected: note.head.clone(),
            instruction: "Revise only this phrase.".into(),
            intent: FeedbackIntent::ProposeEdits,
            basis: None,
            scope: Some(DiscussionScopeInput {
                kind: scope.kind,
                start: scope.start,
                end: scope.end,
                quote: scope.quote,
                source_body_hash: scope.source_hash,
            }),
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: Some(ProviderBinding::codex_luna()),
            previous_run_id: None,
            lookup: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidScope");
}

#[test]
fn schema22_rebuild_preserves_legacy_candidate_payload_receipt_and_decision() {
    let mut fixture = Fixture::new();
    let passage = fixture.start(
        &passage_scope(&fixture.document.body),
        "migration-legacy-passage-start",
    );
    fixture.finish(&passage, legacy_output());
    let continuation = fixture.start_continuation("migration-legacy-continuation-start");
    fixture.finish(
        &continuation,
        r#"{"schemaVersion":"continuation-output.v1","suggestions":[{"title":"Legacy continuation","paragraphs":["A preserved continuation."],"explanation":"Keeps the old append contract."}]}"#.into(),
    );
    let proposals = fixture
        .project()
        .proposals(fixture.access.clone(), "chapter".into())
        .unwrap();
    let passage_proposal = proposals
        .iter()
        .find(|proposal| proposal.kind == ProposalKind::Passage)
        .unwrap()
        .clone();
    let continuation_proposal = proposals
        .iter()
        .find(|proposal| proposal.kind == ProposalKind::Continuation)
        .unwrap()
        .clone();
    let passage_body = json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type":"paragraph","attrs":{"id":"before"},"content":[{"type":"text","text":"Changed legacy."}]},
            {"type":"heading","attrs":{"id":"old-heading","level":2},"content":[{"type":"text","text":"Old heading"}]},
            {"type":"paragraph","attrs":{"id":"old-body"},"content":[{"type":"text","marks":[{"type":"bold"}],"text":"Old body."}]},
            {"type":"paragraph","attrs":{"id":"after"},"content":[{"type":"text","text":"After."}]}
        ]}
    });
    let prepared_passage = fixture
        .project()
        .prepare_proposal(PrepareProposal {
            access: fixture.access.clone(),
            operation_id: "migration-legacy-passage-prepare".into(),
            proposal_id: passage_proposal.id.clone(),
            expected_prepared_version: "0".into(),
            replacement_text: "Changed legacy.".into(),
            body: passage_body,
        })
        .unwrap();
    let prepared_continuation = fixture
        .project()
        .prepare_continuation(PrepareContinuation {
            access: fixture.access.clone(),
            operation_id: "migration-legacy-continuation-prepare".into(),
            proposal_id: continuation_proposal.id.clone(),
            expected_prepared_version: "0".into(),
            paragraphs: vec!["A preserved continuation.".into()],
            body: append_body(&["A preserved continuation."]),
        })
        .unwrap();
    let _decision = fixture
        .project()
        .reject_proposal(RejectProposal {
            access: fixture.access.clone(),
            operation_id: "migration-legacy-reject".into(),
            proposal_id: continuation_proposal.id.clone(),
        })
        .unwrap();
    let passage_id = passage_proposal.id.clone();
    let continuation_id = continuation_proposal.id.clone();
    let passage_prepared_id = prepared_passage.id.clone();
    let continuation_prepared_id = prepared_continuation.id.clone();
    drop(fixture.project.take());
    let path = fixture.root.join("project");
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    let legacy_candidate_json: String = connection
        .query_row(
            "SELECT candidate_json FROM proposals WHERE id=?",
            params![passage_id],
            |row| row.get(0),
        )
        .unwrap();
    let legacy_passage_version: (String, String, String, String) = connection
        .query_row(
            "SELECT id,body_hash,body_json,created_at FROM proposal_versions WHERE id=?",
            params![passage_prepared_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let legacy_continuation_payload: (String, String, String) = connection
        .query_row(
            "SELECT id,payload_json,created_at FROM proposal_versions WHERE id=?",
            params![continuation_prepared_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let legacy_prepare_receipt: (String, String, String) = connection
        .query_row(
            "SELECT kind,payload_hash,result_id FROM proposal_receipts WHERE operation_id=?",
            params!["migration-legacy-continuation-prepare"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let legacy_decision: (String, String, String) = connection
        .query_row(
            "SELECT id,kind,created_at FROM proposal_decisions WHERE proposal_id=?",
            params![continuation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    legacy_schema::remove_schema24_features(&connection).unwrap();
    legacy_schema21_proposal_tables(&connection);
    drop(connection);
    let reopened = ProjectSession::open(&path).unwrap();
    let access = reopened.attach("migration-reader".into()).unwrap();
    let restored = reopened.proposals(access, "chapter".into()).unwrap();
    let restored_passage = restored
        .into_iter()
        .find(|candidate| candidate.id == passage_id)
        .unwrap();
    assert_eq!(restored_passage.kind, ProposalKind::Passage);
    assert_eq!(
        restored_passage.prepared.unwrap().id,
        legacy_passage_version.0
    );
    drop(reopened);

    let migrated = Connection::open(path.join("project.sqlite3")).unwrap();
    let candidate_json: String = migrated
        .query_row(
            "SELECT candidate_json FROM proposals WHERE id=?",
            params![passage_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(candidate_json, legacy_candidate_json);
    let passage_version: (String, String, String, String) = migrated
        .query_row(
            "SELECT id,body_hash,body_json,created_at FROM proposal_versions WHERE id=?",
            params![passage_prepared_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(passage_version, legacy_passage_version);
    let continuation_payload: (String, String, String) = migrated
        .query_row(
            "SELECT id,payload_json,created_at FROM proposal_versions WHERE id=?",
            params![continuation_prepared_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(continuation_payload, legacy_continuation_payload);
    let prepare_receipt: (String, String, String) = migrated
        .query_row(
            "SELECT kind,payload_hash,result_id FROM proposal_receipts WHERE operation_id=?",
            params!["migration-legacy-continuation-prepare"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(prepare_receipt, legacy_prepare_receipt);
    let decision_row: (String, String, String) = migrated
        .query_row(
            "SELECT id,kind,created_at FROM proposal_decisions WHERE proposal_id=?",
            params![continuation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(decision_row, legacy_decision);
    let schema: i64 = migrated
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(schema, 32);
    let (run_id, ordinal): (String, i64) = migrated
        .query_row(
            "SELECT run_id,ordinal FROM proposals WHERE id=?",
            params![passage_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let duplicate = migrated.execute(
        "INSERT INTO proposals(id,run_id,ordinal,candidate_json,kind) VALUES(?,?,?,?,?)",
        params![
            Uuid::new_v4().to_string(),
            run_id,
            ordinal,
            legacy_candidate_json,
            "passage"
        ],
    );
    assert!(
        duplicate.is_err(),
        "schema22 must retain run/ordinal uniqueness"
    );
}
