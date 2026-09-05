use serde_json::Value;
use webnovel_core::context::{
    Audience, BasisKind, CharacterGrant, ContextPurpose, Disclosure, EligibilityErrorCode,
    EligibilityRequest, InformationPolicy, SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
    StoryTime, evaluate_eligibility,
};

const PROJECT: &str = "project-a";

fn source_ref(project_id: &str, handle: &str) -> SourceRef {
    SourceRef {
        project_id: project_id.to_owned(),
        document_id: format!("doc-{handle}"),
        revision_id: format!("revision-{handle}"),
        body_hash: "a".repeat(64),
    }
}

fn source(
    project_id: &str,
    handle: &str,
    kind: SourceKind,
    current: bool,
    reader_position: Option<&str>,
    dependencies: Vec<SourceRef>,
) -> SourceDescriptor {
    SourceDescriptor {
        handle: handle.to_owned(),
        source: source_ref(project_id, handle),
        display_name: format!("Source {handle} — café 🧭"),
        kind,
        current,
        coverage: webnovel_core::context::CoverageLabel::Verbatim,
        disclosure: Disclosure {
            reader_position: reader_position.map(str::to_owned),
            visible_to_characters: Vec::new(),
            author_only: false,
            future_private: false,
        },
        story_time: None,
        dependencies,
    }
}

fn snapshot(basis: BasisKind, sources: Vec<SourceDescriptor>) -> StorySnapshot {
    StorySnapshot {
        snapshot_id: "snapshot-1".into(),
        project_id: PROJECT.into(),
        basis,
        target: sources[0].source.clone(),
        context_source_epoch: "12".into(),
        ordering_epoch: "4".into(),
        disclosure_policy_version: "1".into(),
        sources,
    }
}

fn policy(audience: Audience) -> InformationPolicy {
    InformationPolicy {
        version: "1".into(),
        audience,
        reader_frontier: (audience == Audience::RestrictedWriting).then(|| "10".into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn request(
    snapshot: StorySnapshot,
    policy: InformationPolicy,
    requested_handles: &[&str],
) -> EligibilityRequest {
    let purpose = if policy.audience == Audience::AuthorRoom {
        ContextPurpose::StoryQuestion
    } else {
        ContextPurpose::Continue
    };
    EligibilityRequest {
        snapshot,
        policy,
        purpose,
        requested_handles: requested_handles
            .iter()
            .map(|handle| (*handle).into())
            .collect(),
    }
}

fn error(request: &EligibilityRequest) -> EligibilityErrorCode {
    evaluate_eligibility(request)
        .expect_err("eligibility should reject this request")
        .code
}

#[test]
fn working_basis_accepts_current_draft_and_keeps_apply_separate() {
    let draft = source(
        PROJECT,
        "draft-😀",
        SourceKind::CurrentDraft,
        true,
        Some("4"),
        Vec::new(),
    );
    let request = request(
        snapshot(BasisKind::Working, vec![draft]),
        policy(Audience::RestrictedWriting),
        &["draft-😀"],
    );
    let receipt = evaluate_eligibility(&request).expect("current draft is eligible");
    assert_eq!(receipt.eligible.len(), 1);
    assert_eq!(receipt.eligible[0].kind, SourceKind::CurrentDraft);
    assert!(!receipt.can_authorize_apply);
}

#[test]
fn reviewed_basis_accepts_reviewed_authority_and_adopted_guidance_only() {
    let authority = source(
        PROJECT,
        "reviewed",
        SourceKind::ReviewedAuthority,
        true,
        Some("3"),
        Vec::new(),
    );
    let accepted = request(
        snapshot(BasisKind::Reviewed, vec![authority]),
        policy(Audience::AuthorRoom),
        &["reviewed"],
    );
    assert!(evaluate_eligibility(&accepted).is_ok());

    let draft = source(
        PROJECT,
        "draft",
        SourceKind::CurrentDraft,
        true,
        Some("3"),
        Vec::new(),
    );
    let rejected = request(
        snapshot(BasisKind::Reviewed, vec![draft]),
        policy(Audience::AuthorRoom),
        &["draft"],
    );
    assert_eq!(error(&rejected), EligibilityErrorCode::BasisMismatch);

    let guidance = source(
        PROJECT,
        "guidance",
        SourceKind::AdoptedGuidance,
        true,
        Some("3"),
        Vec::new(),
    );
    let guidance_request = request(
        snapshot(BasisKind::Reviewed, vec![guidance]),
        policy(Audience::AuthorRoom),
        &["guidance"],
    );
    assert!(evaluate_eligibility(&guidance_request).is_ok());
}

#[test]
fn alternatives_and_history_require_explicit_basis_and_policy_flags() {
    let alternative = source(
        PROJECT,
        "alternative",
        SourceKind::PlanAlternative,
        true,
        Some("2"),
        Vec::new(),
    );
    let working = request(
        snapshot(BasisKind::Working, vec![alternative.clone()]),
        policy(Audience::AuthorRoom),
        &["alternative"],
    );
    assert_eq!(error(&working), EligibilityErrorCode::BasisMismatch);

    let mut explicit_policy = policy(Audience::AuthorRoom);
    explicit_policy.allow_alternatives = true;
    let explicit = request(
        snapshot(BasisKind::ExplicitHistory, vec![alternative]),
        explicit_policy,
        &["alternative"],
    );
    assert!(evaluate_eligibility(&explicit).is_ok());

    let historical = source(
        PROJECT,
        "old-revision",
        SourceKind::Historical,
        false,
        Some("1"),
        Vec::new(),
    );
    let not_selected = request(
        snapshot(BasisKind::ExplicitHistory, vec![historical.clone()]),
        policy(Audience::AuthorRoom),
        &["old-revision"],
    );
    assert_eq!(
        error(&not_selected),
        EligibilityErrorCode::HistoryNotSelected
    );

    let mut history_policy = policy(Audience::AuthorRoom);
    history_policy.allow_historical = true;
    let selected = request(
        snapshot(BasisKind::ExplicitHistory, vec![historical]),
        history_policy,
        &["old-revision"],
    );
    assert!(evaluate_eligibility(&selected).is_ok());
}

#[test]
fn author_room_can_discuss_private_future_but_never_authorizes_apply() {
    let mut future = source(
        PROJECT,
        "future-secret",
        SourceKind::PrivateFuture,
        true,
        None,
        Vec::new(),
    );
    future.disclosure.author_only = true;
    future.disclosure.future_private = true;
    let author_request = request(
        snapshot(BasisKind::Working, vec![future.clone()]),
        policy(Audience::AuthorRoom),
        &["future-secret"],
    );
    let receipt =
        evaluate_eligibility(&author_request).expect("author room may inspect private notes");
    assert!(!receipt.can_authorize_apply);

    let restricted_request = request(
        snapshot(BasisKind::Working, vec![future]),
        policy(Audience::RestrictedWriting),
        &["future-secret"],
    );
    assert_eq!(
        error(&restricted_request),
        EligibilityErrorCode::FutureContamination
    );
}

#[test]
fn future_derived_digest_cannot_be_relabelled_safe() {
    let mut secret = source(
        PROJECT,
        "private-note",
        SourceKind::PrivateFuture,
        true,
        None,
        Vec::new(),
    );
    secret.disclosure.author_only = true;
    secret.disclosure.future_private = true;
    let digest = source(
        PROJECT,
        "digest",
        SourceKind::GeneratedDigest,
        true,
        Some("2"),
        vec![secret.source.clone()],
    );
    let request = request(
        snapshot(BasisKind::Working, vec![digest, secret]),
        policy(Audience::RestrictedWriting),
        &["digest"],
    );
    assert_eq!(error(&request), EligibilityErrorCode::FutureContamination);
}

#[test]
fn source_and_dependency_project_ownership_is_enforced() {
    let foreign_dependency = source_ref("project-b", "foreign");
    let target = source(
        PROJECT,
        "target",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        vec![foreign_dependency.clone()],
    );
    let dependency_request = request(
        snapshot(BasisKind::Working, vec![target]),
        policy(Audience::AuthorRoom),
        &["target"],
    );
    assert_eq!(
        error(&dependency_request),
        EligibilityErrorCode::CrossProjectSource
    );

    let foreign = source(
        "project-b",
        "foreign",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    let request = request(
        snapshot(BasisKind::Working, vec![foreign]),
        policy(Audience::AuthorRoom),
        &["foreign"],
    );
    assert_eq!(error(&request), EligibilityErrorCode::CrossProjectSource);
}

#[test]
fn missing_disclosure_timestamp_and_flashback_story_time_do_not_bypass_frontier() {
    let mut unknown = source(
        PROJECT,
        "unknown-disclosure",
        SourceKind::CurrentDraft,
        true,
        None,
        Vec::new(),
    );
    unknown.story_time = Some(StoryTime {
        label: "flashback before the reader frontier".into(),
        position: Some("1".into()),
    });
    let unknown_request = request(
        snapshot(BasisKind::Working, vec![unknown]),
        policy(Audience::RestrictedWriting),
        &["unknown-disclosure"],
    );
    assert_eq!(
        error(&unknown_request),
        EligibilityErrorCode::DisclosureUnknown
    );

    let mut future = source(
        PROJECT,
        "flashback-future-disclosure",
        SourceKind::CurrentDraft,
        true,
        Some("11"),
        Vec::new(),
    );
    future.story_time = Some(StoryTime {
        label: "flashback".into(),
        position: Some("1".into()),
    });
    let future_request = request(
        snapshot(BasisKind::Working, vec![future]),
        policy(Audience::RestrictedWriting),
        &["flashback-future-disclosure"],
    );
    assert_eq!(
        error(&future_request),
        EligibilityErrorCode::DisclosureDenied
    );
}

#[test]
fn every_dependency_is_checked_even_when_not_cited() {
    let mut private = source(
        PROJECT,
        "uncited-private",
        SourceKind::PrivateFuture,
        true,
        None,
        Vec::new(),
    );
    private.disclosure.author_only = true;
    private.disclosure.future_private = true;
    let target = source(
        PROJECT,
        "visible-draft",
        SourceKind::CurrentDraft,
        true,
        Some("2"),
        vec![private.source.clone()],
    );
    let request = request(
        snapshot(BasisKind::Working, vec![target, private]),
        policy(Audience::RestrictedWriting),
        &["visible-draft"],
    );
    assert_eq!(error(&request), EligibilityErrorCode::FutureContamination);
}

#[test]
fn dependency_cycles_and_unknown_dependencies_are_rejected() {
    let mut first = source(
        PROJECT,
        "first",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    let mut second = source(
        PROJECT,
        "second",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    first.dependencies.push(second.source.clone());
    second.dependencies.push(first.source.clone());
    let cycle_request = request(
        snapshot(BasisKind::Working, vec![first, second]),
        policy(Audience::AuthorRoom),
        &["first"],
    );
    assert_eq!(error(&cycle_request), EligibilityErrorCode::DependencyCycle);

    let unknown = source(
        PROJECT,
        "unknown-parent",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        vec![source_ref(PROJECT, "missing")],
    );
    let unknown_request = request(
        snapshot(BasisKind::Working, vec![unknown]),
        policy(Audience::AuthorRoom),
        &["unknown-parent"],
    );
    assert_eq!(
        error(&unknown_request),
        EligibilityErrorCode::UnknownDependency
    );
}

#[test]
fn character_specific_sources_require_membership_and_an_explicit_grant() {
    let mut character_source = source(
        PROJECT,
        "character-memory",
        SourceKind::ReviewedAuthority,
        true,
        Some("5"),
        Vec::new(),
    );
    character_source.disclosure.visible_to_characters = vec!["mei".into()];

    let mut wrong_character = policy(Audience::RestrictedWriting);
    wrong_character.character_id = Some("lin".into());
    wrong_character.character_grants.push(CharacterGrant {
        character_id: "lin".into(),
        source_handle: "character-memory".into(),
        reader_frontier: "5".into(),
    });
    let wrong_request = request(
        snapshot(BasisKind::Reviewed, vec![character_source.clone()]),
        wrong_character,
        &["character-memory"],
    );
    assert_eq!(
        error(&wrong_request),
        EligibilityErrorCode::DisclosureDenied
    );

    let mut missing_grant = policy(Audience::RestrictedWriting);
    missing_grant.character_id = Some("mei".into());
    let missing_request = request(
        snapshot(BasisKind::Reviewed, vec![character_source.clone()]),
        missing_grant,
        &["character-memory"],
    );
    assert_eq!(
        error(&missing_request),
        EligibilityErrorCode::DisclosureDenied
    );

    let mut granted = policy(Audience::RestrictedWriting);
    granted.character_id = Some("mei".into());
    granted.character_grants.push(CharacterGrant {
        character_id: "mei".into(),
        source_handle: "character-memory".into(),
        reader_frontier: "5".into(),
    });
    let granted_request = request(
        snapshot(BasisKind::Reviewed, vec![character_source]),
        granted,
        &["character-memory"],
    );
    assert!(evaluate_eligibility(&granted_request).is_ok());
}

#[test]
fn stale_source_directory_only_and_reviewed_digest_are_rejected() {
    let stale = source(
        PROJECT,
        "stale",
        SourceKind::CurrentDraft,
        false,
        Some("1"),
        Vec::new(),
    );
    let stale_request = request(
        snapshot(BasisKind::Working, vec![stale]),
        policy(Audience::AuthorRoom),
        &["stale"],
    );
    assert_eq!(error(&stale_request), EligibilityErrorCode::StaleSource);

    let mut directory = source(
        PROJECT,
        "directory",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    directory.coverage = webnovel_core::context::CoverageLabel::DirectoryOnly;
    let directory_request = request(
        snapshot(BasisKind::Working, vec![directory]),
        policy(Audience::RestrictedWriting),
        &["directory"],
    );
    assert_eq!(
        error(&directory_request),
        EligibilityErrorCode::DirectoryOnly
    );

    let digest = source(
        PROJECT,
        "unreviewed-digest",
        SourceKind::GeneratedDigest,
        true,
        Some("1"),
        Vec::new(),
    );
    let digest_request = request(
        snapshot(BasisKind::Reviewed, vec![digest]),
        policy(Audience::AuthorRoom),
        &["unreviewed-digest"],
    );
    assert_eq!(error(&digest_request), EligibilityErrorCode::BasisMismatch);
}

#[test]
fn exact_target_selection_policy_version_and_descriptor_hash_are_required() {
    let source = source(
        PROJECT,
        "target",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    let target_missing = request(
        snapshot(BasisKind::Working, vec![source.clone()]),
        policy(Audience::AuthorRoom),
        &["other"],
    );
    assert_eq!(error(&target_missing), EligibilityErrorCode::UnknownSource);

    let mut no_target = request(
        snapshot(BasisKind::Working, vec![source.clone()]),
        policy(Audience::AuthorRoom),
        &["target"],
    );
    no_target.snapshot.target.revision_id = "different-revision".into();
    assert_eq!(error(&no_target), EligibilityErrorCode::InvalidSnapshot);

    let mut wrong_policy = request(
        snapshot(BasisKind::Working, vec![source.clone()]),
        policy(Audience::AuthorRoom),
        &["target"],
    );
    wrong_policy.policy.version = "2".into();
    assert_eq!(error(&wrong_policy), EligibilityErrorCode::BasisMismatch);

    let mut bad_hash = source;
    bad_hash.source.body_hash = "not-a-hash".into();
    let bad_hash_request = request(
        snapshot(BasisKind::Working, vec![bad_hash]),
        policy(Audience::AuthorRoom),
        &["target"],
    );
    assert_eq!(
        error(&bad_hash_request),
        EligibilityErrorCode::InvalidDescriptor
    );
}

#[test]
fn contracts_round_trip_camel_case_and_preserve_unicode_source_names() {
    let unicode = source(
        PROJECT,
        "révision-👩‍🚀",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    let request = request(
        snapshot(BasisKind::Working, vec![unicode]),
        policy(Audience::AuthorRoom),
        &["révision-👩‍🚀"],
    );
    let json = serde_json::to_value(&request).expect("serialize request");
    assert_eq!(
        json["snapshot"]["sources"][0]["displayName"],
        "Source révision-👩‍🚀 — café 🧭"
    );
    assert!(json.get("requested_handles").is_none());
    assert!(json.get("requestedHandles").is_some());
    let decoded: EligibilityRequest = serde_json::from_value(json).expect("round trip request");
    assert_eq!(decoded, request);
}

#[test]
fn invalid_private_descriptor_and_unknown_grant_fail_closed() {
    let mut invalid = source(
        PROJECT,
        "invalid-private",
        SourceKind::PrivateFuture,
        true,
        None,
        Vec::new(),
    );
    invalid.disclosure.future_private = true;
    let invalid_request = request(
        snapshot(BasisKind::Working, vec![invalid]),
        policy(Audience::AuthorRoom),
        &["invalid-private"],
    );
    assert_eq!(
        error(&invalid_request),
        EligibilityErrorCode::InvalidDescriptor
    );

    let source = source(
        PROJECT,
        "target",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        Vec::new(),
    );
    let mut bad_policy = policy(Audience::AuthorRoom);
    bad_policy.character_grants.push(CharacterGrant {
        character_id: "mei".into(),
        source_handle: "missing".into(),
        reader_frontier: "1".into(),
    });
    let bad_grant_request = request(
        snapshot(BasisKind::Working, vec![source]),
        bad_policy,
        &["target"],
    );
    assert_eq!(
        error(&bad_grant_request),
        EligibilityErrorCode::InvalidPolicy
    );
}

#[test]
fn receipt_dependencies_are_dependency_first_and_include_non_cited_inputs() {
    let dependency = source(
        PROJECT,
        "dependency",
        SourceKind::ReviewedAuthority,
        true,
        Some("1"),
        Vec::new(),
    );
    let target = source(
        PROJECT,
        "target",
        SourceKind::CurrentDraft,
        true,
        Some("1"),
        vec![dependency.source.clone()],
    );
    let request = request(
        snapshot(BasisKind::Working, vec![target, dependency]),
        policy(Audience::RestrictedWriting),
        &["target"],
    );
    let receipt = evaluate_eligibility(&request).expect("safe dependency graph");
    assert_eq!(receipt.all_dependency_handles, vec!["dependency", "target"]);
    assert_eq!(receipt.eligible[0].handle, "dependency");
    assert_eq!(receipt.eligible[1].dependency_handles, vec!["dependency"]);
}

#[test]
fn packet_and_budget_contracts_keep_cross_boundary_counters_as_strings() {
    let receipt = webnovel_core::context::PacketReceipt {
        packet_id: "packet".into(),
        session_id: "session".into(),
        snapshot_id: "snapshot".into(),
        invocation_ordinal: "9007199254740993".into(),
        source_handles: vec!["target".into()],
        guidance_handles: Vec::new(),
        conversation_message_ids: Vec::new(),
        omitted_discussion_turns: 0,
        coverage: Vec::new(),
        omissions: Vec::new(),
        input_hash: "b".repeat(64),
        input_tokens: "12345678901234567890".into(),
        token_accounting_method: "utf8-estimate".into(),
    };
    let value: Value = serde_json::to_value(&receipt).expect("serialize receipt");
    assert_eq!(value["invocationOrdinal"], "9007199254740993");
    assert_eq!(value["inputTokens"], "12345678901234567890");
}
