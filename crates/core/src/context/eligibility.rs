use super::contracts::{
    Audience, BasisKind, ContextPurpose, EligibilityRequest, InformationPolicy, SourceDescriptor,
    SourceKind, SourceRef, StorySnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum EligibilityErrorCode {
    InvalidSnapshot,
    InvalidPolicy,
    NoRequestedSources,
    TargetNotSelected,
    UnknownSource,
    DuplicateSource,
    CrossProjectSource,
    SourceNotInBasis,
    StaleSource,
    BasisMismatch,
    UnknownDependency,
    DependencyCycle,
    DisclosureDenied,
    DisclosureUnknown,
    FutureContamination,
    PrivateSource,
    AlternativeNotSelected,
    HistoryNotSelected,
    DirectoryOnly,
    InvalidDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EligibilityError {
    pub code: EligibilityErrorCode,
    pub message: String,
    pub handle: Option<String>,
    /// Boxed because a source reference contains several owned strings and
    /// errors cross many small validation helpers.
    pub dependency: Option<Box<SourceRef>>,
}

impl EligibilityError {
    fn new(code: EligibilityErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            handle: None,
            dependency: None,
        }
    }
    fn for_handle(code: EligibilityErrorCode, handle: &str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            handle: Some(handle.to_owned()),
            dependency: None,
        }
    }
    fn for_dependency(
        code: EligibilityErrorCode,
        handle: &str,
        dependency: SourceRef,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            handle: Some(handle.to_owned()),
            dependency: Some(Box::new(dependency)),
        }
    }
}

impl fmt::Display for EligibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for EligibilityError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EligibleSource {
    pub handle: String,
    pub source: SourceRef,
    pub kind: SourceKind,
    pub coverage: super::contracts::CoverageLabel,
    pub dependency_handles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EligibilityReceipt {
    pub snapshot_id: String,
    pub project_id: String,
    pub audience: Audience,
    pub purpose: ContextPurpose,
    pub eligible: Vec<EligibleSource>,
    pub all_dependency_handles: Vec<String>,
    /// Always false. Eligibility and author-room discussion never authorize
    /// prose Apply; the existing scoped Apply contract remains separate.
    pub can_authorize_apply: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visit {
    Visiting,
    Done,
}

/// Evaluate a serialized request whose snapshot and policy were resolved by
/// the Rust project owner. This convenience entry point performs no I/O and
/// makes no semantic truth claim.
pub fn evaluate_eligibility(
    request: &EligibilityRequest,
) -> Result<EligibilityReceipt, EligibilityError> {
    evaluate_sources(
        &request.snapshot,
        &request.policy,
        request.purpose,
        &request.requested_handles,
    )
}

/// Borrowed form for Rust owners that already hold a frozen snapshot. It
/// avoids cloning the full source manifest when trying several candidate
/// handles. The same source binding and policy checks as
/// [`evaluate_eligibility`] are applied.
pub fn evaluate_sources(
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
    requested_handles: &[String],
) -> Result<EligibilityReceipt, EligibilityError> {
    validate_snapshot(snapshot)?;
    validate_policy(policy)?;
    if purpose == ContextPurpose::MemoryAnalysis
        && (snapshot.basis != BasisKind::Working
            || snapshot.sources.len() != 1
            || snapshot.sources[0].source != snapshot.target
            || snapshot.sources[0].kind != SourceKind::CurrentDraft
            || snapshot.sources[0].coverage != super::contracts::CoverageLabel::Verbatim
            || !snapshot.sources[0].current
            || !snapshot.sources[0].dependencies.is_empty()
            || snapshot.sources[0].disclosure.author_only
            || snapshot.sources[0].disclosure.future_private
            || snapshot.sources[0].disclosure.reader_position.is_none()
            || policy.audience != Audience::AuthorRoom
            || policy.reader_frontier.is_some()
            || policy.character_id.is_some()
            || !policy.character_grants.is_empty()
            || policy.allow_alternatives
            || policy.allow_historical)
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            "Chapter memory analysis requires only its exact working chapter and an author-room boundary.",
        ));
    }
    if policy.audience == Audience::AuthorRoom
        && matches!(purpose, ContextPurpose::Revise | ContextPurpose::Continue)
        && !author_room_structured_revision_allowed(snapshot, policy, purpose)
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            "Prose generation needs a separately approved writing boundary.",
        ));
    }
    if policy.version != snapshot.disclosure_policy_version {
        return Err(EligibilityError::new(
            EligibilityErrorCode::BasisMismatch,
            "The information policy does not match the snapshot policy version.",
        ));
    }
    if requested_handles.is_empty() {
        return Err(EligibilityError::new(
            EligibilityErrorCode::NoRequestedSources,
            "A context request must select at least one resolved source.",
        ));
    }

    if snapshot.basis == BasisKind::Reviewed {
        if purpose != ContextPurpose::Continue || policy.audience != Audience::RestrictedWriting {
            return Err(EligibilityError::new(
                EligibilityErrorCode::InvalidPolicy,
                "Reviewed authority is available only for restricted continuation.",
            ));
        }
        if policy.character_id.is_some() || !policy.character_grants.is_empty() {
            return Err(EligibilityError::new(
                EligibilityErrorCode::InvalidPolicy,
                "Reviewed continuation has no character-specific knowledge grants.",
            ));
        }
    }
    validate_basis_manifest(snapshot, policy, purpose)?;

    let mut by_handle = HashMap::with_capacity(snapshot.sources.len());
    let mut by_source = HashMap::with_capacity(snapshot.sources.len());
    for descriptor in &snapshot.sources {
        if by_handle
            .insert(descriptor.handle.clone(), descriptor)
            .is_some()
            || by_source
                .insert(descriptor.source.clone(), descriptor.handle.clone())
                .is_some()
        {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::DuplicateSource,
                &descriptor.handle,
                "A frozen snapshot contains a duplicate source handle or exact source.",
            ));
        }
    }
    let target_handle = snapshot
        .sources
        .iter()
        .find(|descriptor| descriptor.source == snapshot.target)
        .map(|descriptor| descriptor.handle.clone())
        .ok_or_else(|| {
            EligibilityError::new(
                EligibilityErrorCode::InvalidSnapshot,
                "The snapshot target is missing from its resolved source manifest.",
            )
        })?;

    let mut selected = Vec::new();
    for handle in requested_handles {
        if !by_handle.contains_key(handle) {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::UnknownSource,
                handle,
                "The requested source handle is not in the frozen snapshot.",
            ));
        }
        if selected.iter().any(|value: &String| value == handle) {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::DuplicateSource,
                handle,
                "A source handle was selected more than once.",
            ));
        }
        selected.push(handle.clone());
    }
    if !selected.iter().any(|handle| handle == &target_handle) {
        return Err(EligibilityError::new(
            EligibilityErrorCode::TargetNotSelected,
            "The exact snapshot target must be selected for context preparation.",
        ));
    }

    for descriptor in &snapshot.sources {
        validate_descriptor(snapshot, descriptor, &by_source)?;
    }
    for grant in &policy.character_grants {
        if !by_handle.contains_key(&grant.source_handle) {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::InvalidPolicy,
                &grant.source_handle,
                "A character grant must name a source in the frozen snapshot.",
            ));
        }
    }

    let mut visits = HashMap::with_capacity(snapshot.sources.len());
    let mut ordered_handles = Vec::new();
    let walk_context = WalkContext {
        by_handle: &by_handle,
        policy,
        basis: snapshot.basis,
        by_source: &by_source,
        target_handle: &target_handle,
        purpose,
    };
    for handle in &selected {
        walk(handle, &walk_context, &mut visits, &mut ordered_handles)?;
    }

    let eligible = ordered_handles
        .iter()
        .map(|handle| {
            let descriptor = by_handle
                .get(handle)
                .expect("walk only emits known handles");
            let dependency_handles = descriptor
                .dependencies
                .iter()
                .map(|dependency| {
                    by_source
                        .get(dependency)
                        .expect("validated dependencies are in the source map")
                        .clone()
                })
                .collect();
            EligibleSource {
                handle: descriptor.handle.clone(),
                source: descriptor.source.clone(),
                kind: descriptor.kind,
                coverage: descriptor.coverage,
                dependency_handles,
            }
        })
        .collect::<Vec<_>>();

    Ok(EligibilityReceipt {
        snapshot_id: snapshot.snapshot_id.clone(),
        project_id: snapshot.project_id.clone(),
        audience: policy.audience,
        purpose,
        eligible,
        all_dependency_handles: ordered_handles,
        can_authorize_apply: false,
    })
}

/// Author-room development is intentionally narrower than ordinary prose
/// generation. It may revise only the current author-only working document;
/// restricted chapter writing and continuation continue to use their existing
/// disclosure boundary.
pub(crate) fn author_room_structured_revision_allowed(
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> bool {
    purpose == ContextPurpose::Revise
        && snapshot.basis == BasisKind::Working
        && policy.audience == Audience::AuthorRoom
        && policy.reader_frontier.is_none()
        && snapshot.sources.iter().any(|descriptor| {
            descriptor.source == snapshot.target
                && descriptor.kind == SourceKind::CurrentDraft
                && descriptor.current
                && descriptor.disclosure.author_only
                && descriptor.disclosure.reader_position.is_none()
        })
}

fn validate_snapshot(snapshot: &StorySnapshot) -> Result<(), EligibilityError> {
    nonempty(
        &snapshot.snapshot_id,
        EligibilityErrorCode::InvalidSnapshot,
        "snapshotId",
    )?;
    nonempty(
        &snapshot.project_id,
        EligibilityErrorCode::InvalidSnapshot,
        "projectId",
    )?;
    decimal(&snapshot.context_source_epoch).map_err(|message| {
        EligibilityError::new(
            EligibilityErrorCode::InvalidSnapshot,
            format!("contextSourceEpoch is invalid: {message}"),
        )
    })?;
    decimal(&snapshot.ordering_epoch).map_err(|message| {
        EligibilityError::new(
            EligibilityErrorCode::InvalidSnapshot,
            format!("orderingEpoch is invalid: {message}"),
        )
    })?;
    nonempty(
        &snapshot.disclosure_policy_version,
        EligibilityErrorCode::InvalidSnapshot,
        "disclosurePolicyVersion",
    )?;
    validate_source_ref(&snapshot.target, &snapshot.project_id)?;
    if snapshot.sources.is_empty() {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidSnapshot,
            "A story snapshot must contain its resolved source manifest.",
        ));
    }
    Ok(())
}

fn validate_policy(policy: &InformationPolicy) -> Result<(), EligibilityError> {
    decimal(&policy.version).map_err(|message| {
        EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            format!("Policy version is invalid: {message}"),
        )
    })?;
    if policy.audience == Audience::RestrictedWriting && policy.reader_frontier.is_none() {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            "Restricted writing requires an explicit reader frontier.",
        ));
    }
    if let Some(frontier) = &policy.reader_frontier {
        decimal(frontier).map_err(|message| {
            EligibilityError::new(
                EligibilityErrorCode::InvalidPolicy,
                format!("readerFrontier is invalid: {message}"),
            )
        })?;
    }
    if let Some(character_id) = &policy.character_id {
        nonempty(
            character_id,
            EligibilityErrorCode::InvalidPolicy,
            "characterId",
        )?;
    }
    for grant in &policy.character_grants {
        nonempty(
            &grant.character_id,
            EligibilityErrorCode::InvalidPolicy,
            "characterGrants.characterId",
        )?;
        nonempty(
            &grant.source_handle,
            EligibilityErrorCode::InvalidPolicy,
            "characterGrants.sourceHandle",
        )?;
        decimal(&grant.reader_frontier).map_err(|message| {
            EligibilityError::new(
                EligibilityErrorCode::InvalidPolicy,
                format!("character grant frontier is invalid: {message}"),
            )
        })?;
    }
    Ok(())
}

fn validate_descriptor(
    snapshot: &StorySnapshot,
    descriptor: &SourceDescriptor,
    by_source: &HashMap<SourceRef, String>,
) -> Result<(), EligibilityError> {
    nonempty(
        &descriptor.handle,
        EligibilityErrorCode::InvalidDescriptor,
        "source.handle",
    )?;
    nonempty(
        &descriptor.display_name,
        EligibilityErrorCode::InvalidDescriptor,
        "source.displayName",
    )?;
    validate_source_ref(&descriptor.source, &snapshot.project_id)?;
    for dependency in &descriptor.dependencies {
        if let Err(error) = validate_source_ref(dependency, &snapshot.project_id) {
            let code = if error.code == EligibilityErrorCode::CrossProjectSource {
                EligibilityErrorCode::CrossProjectSource
            } else {
                EligibilityErrorCode::InvalidDescriptor
            };
            return Err(EligibilityError::for_dependency(
                code,
                &descriptor.handle,
                dependency.clone(),
                if code == EligibilityErrorCode::CrossProjectSource {
                    "A source dependency belongs to another project."
                } else {
                    "A source dependency is malformed."
                },
            ));
        }
        if !by_source.contains_key(dependency) {
            return Err(EligibilityError::for_dependency(
                EligibilityErrorCode::UnknownDependency,
                &descriptor.handle,
                dependency.clone(),
                "Every influential dependency must be present in the frozen manifest.",
            ));
        }
    }
    if descriptor.disclosure.future_private && !descriptor.disclosure.author_only {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::InvalidDescriptor,
            &descriptor.handle,
            "A future-private source must also be author-only.",
        ));
    }
    if let Some(disclosure) = &descriptor.disclosure.reader_position {
        decimal(disclosure).map_err(|message| {
            EligibilityError::for_handle(
                EligibilityErrorCode::InvalidDescriptor,
                &descriptor.handle,
                format!("reader disclosure position is invalid: {message}"),
            )
        })?;
    }
    if let Some(story_time) = &descriptor.story_time {
        nonempty(
            &story_time.label,
            EligibilityErrorCode::InvalidDescriptor,
            "source.storyTime.label",
        )?;
        if let Some(position) = &story_time.position {
            decimal(position).map_err(|message| {
                EligibilityError::for_handle(
                    EligibilityErrorCode::InvalidDescriptor,
                    &descriptor.handle,
                    format!("story time position is invalid: {message}"),
                )
            })?;
        }
    }
    Ok(())
}

struct WalkContext<'a> {
    by_handle: &'a HashMap<String, &'a SourceDescriptor>,
    policy: &'a InformationPolicy,
    basis: BasisKind,
    by_source: &'a HashMap<SourceRef, String>,
    target_handle: &'a str,
    purpose: ContextPurpose,
}

fn walk(
    handle: &str,
    context: &WalkContext<'_>,
    visits: &mut HashMap<String, Visit>,
    ordered_handles: &mut Vec<String>,
) -> Result<(), EligibilityError> {
    if visits.get(handle) == Some(&Visit::Visiting) {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::DependencyCycle,
            handle,
            "Source dependencies contain a cycle.",
        ));
    }
    if visits.get(handle) == Some(&Visit::Done) {
        return Ok(());
    }
    let descriptor = context.by_handle.get(handle).ok_or_else(|| {
        EligibilityError::for_handle(
            EligibilityErrorCode::UnknownSource,
            handle,
            "A source dependency refers to an unknown handle.",
        )
    })?;
    visits.insert(handle.to_owned(), Visit::Visiting);
    apply_basis_policy(
        descriptor,
        context.policy,
        context.basis,
        context.purpose,
        handle == context.target_handle,
    )?;
    apply_disclosure_policy(descriptor, context.policy)?;
    for dependency in &descriptor.dependencies {
        let dependency_handle = context
            .by_source
            .get(dependency)
            .map(String::as_str)
            .ok_or_else(|| {
                EligibilityError::for_dependency(
                    EligibilityErrorCode::UnknownDependency,
                    handle,
                    dependency.clone(),
                    "A dependency has no resolved source handle.",
                )
            })?;
        walk(dependency_handle, context, visits, ordered_handles)?;
    }
    visits.insert(handle.to_owned(), Visit::Done);
    ordered_handles.push(handle.to_owned());
    Ok(())
}

fn apply_basis_policy(
    descriptor: &SourceDescriptor,
    policy: &InformationPolicy,
    basis: BasisKind,
    purpose: ContextPurpose,
    is_target: bool,
) -> Result<(), EligibilityError> {
    match basis {
        BasisKind::Working => {
            if !descriptor.current {
                return Err(EligibilityError::for_handle(
                    EligibilityErrorCode::StaleSource,
                    &descriptor.handle,
                    "A working-basis source is not the selected current revision.",
                ));
            }
            if matches!(
                descriptor.kind,
                SourceKind::Historical | SourceKind::PlanAlternative
            ) {
                return Err(EligibilityError::for_handle(
                    EligibilityErrorCode::BasisMismatch,
                    &descriptor.handle,
                    "Historical or alternative material requires explicit selection.",
                ));
            }
        }
        BasisKind::Reviewed => {
            if !descriptor.current {
                return Err(EligibilityError::for_handle(
                    EligibilityErrorCode::StaleSource,
                    &descriptor.handle,
                    "A reviewed-basis source is not current in the selected authority basis.",
                ));
            }
            let allowed_target = is_target
                && purpose == ContextPurpose::Continue
                && descriptor.kind == SourceKind::CurrentDraft;
            let allowed_authority = !is_target && descriptor.kind == SourceKind::ReviewedAuthority;
            if !allowed_target && !allowed_authority {
                return Err(EligibilityError::for_handle(
                    EligibilityErrorCode::BasisMismatch,
                    &descriptor.handle,
                    if is_target {
                        "A reviewed continuation target must be the current draft."
                    } else {
                        "Reviewed continuation sources must be selected reviewed authority."
                    },
                ));
            }
        }
        BasisKind::ExplicitHistory => {
            if !matches!(
                descriptor.kind,
                SourceKind::Historical | SourceKind::PlanAlternative
            ) || (!policy.allow_historical && descriptor.kind == SourceKind::Historical)
                || (!policy.allow_alternatives && descriptor.kind == SourceKind::PlanAlternative)
            {
                return Err(EligibilityError::for_handle(
                    if descriptor.kind == SourceKind::PlanAlternative {
                        EligibilityErrorCode::AlternativeNotSelected
                    } else {
                        EligibilityErrorCode::HistoryNotSelected
                    },
                    &descriptor.handle,
                    "This historical or alternative source was not explicitly selected.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_basis_manifest(
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
) -> Result<(), EligibilityError> {
    if snapshot.basis != BasisKind::Reviewed {
        if snapshot.reviewed_basis.is_some() {
            return Err(EligibilityError::new(
                EligibilityErrorCode::InvalidSnapshot,
                "A reviewed basis manifest requires a reviewed snapshot basis.",
            ));
        }
        return Ok(());
    }
    let manifest = snapshot.reviewed_basis.as_ref().ok_or_else(|| {
        EligibilityError::new(
            EligibilityErrorCode::BasisMismatch,
            "A reviewed snapshot needs an exact reviewed basis manifest.",
        )
    })?;
    if manifest.project_id != snapshot.project_id || manifest.operation_namespace.is_empty() {
        return Err(EligibilityError::new(
            EligibilityErrorCode::CrossProjectSource,
            "The reviewed basis manifest belongs to another project namespace.",
        ));
    }
    if manifest.prefix.is_empty() {
        return Err(EligibilityError::new(
            EligibilityErrorCode::BasisMismatch,
            "A reviewed continuation needs at least one earlier reviewed chapter.",
        ));
    }
    if manifest.prefix.len() > 4096 {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidSnapshot,
            "The reviewed basis prefix is too large.",
        ));
    }
    let mut documents = std::collections::HashSet::new();
    let mut bundles = std::collections::HashSet::new();
    let mut revisions = std::collections::HashSet::new();
    for member in &manifest.prefix {
        nonempty(
            &member.document_id,
            EligibilityErrorCode::InvalidSnapshot,
            "reviewedBasis.prefix.documentId",
        )?;
        nonempty(
            &member.bundle_id,
            EligibilityErrorCode::InvalidSnapshot,
            "reviewedBasis.prefix.bundleId",
        )?;
        nonempty(
            &member.revision_id,
            EligibilityErrorCode::InvalidSnapshot,
            "reviewedBasis.prefix.revisionId",
        )?;
        decimal(&member.version).map_err(|message| {
            EligibilityError::new(
                EligibilityErrorCode::InvalidSnapshot,
                format!("reviewed basis head version is invalid: {message}"),
            )
        })?;
        validate_source_ref(
            &SourceRef {
                project_id: manifest.project_id.clone(),
                document_id: member.document_id.clone(),
                revision_id: member.revision_id.clone(),
                body_hash: member.body_hash.clone(),
            },
            &snapshot.project_id,
        )?;
        if !documents.insert(&member.document_id)
            || !bundles.insert(&member.bundle_id)
            || !revisions.insert(&member.revision_id)
        {
            return Err(EligibilityError::new(
                EligibilityErrorCode::DuplicateSource,
                "The reviewed basis prefix contains duplicate or mismatched members.",
            ));
        }
    }
    let target = snapshot
        .sources
        .iter()
        .find(|source| source.source == snapshot.target)
        .ok_or_else(|| {
            EligibilityError::new(
                EligibilityErrorCode::InvalidSnapshot,
                "The reviewed target is missing from its source manifest.",
            )
        })?;
    if purpose == ContextPurpose::Continue
        && (target.kind != SourceKind::CurrentDraft || !target.current)
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::BasisMismatch,
            "A reviewed continuation target must be the current draft.",
        ));
    }
    if purpose == ContextPurpose::Continue
        && target.disclosure.reader_position.as_deref() != policy.reader_frontier.as_deref()
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            "A reviewed continuation target must match the reader frontier exactly.",
        ));
    }
    let authority_sources = snapshot
        .sources
        .iter()
        .filter(|source| source.kind == SourceKind::ReviewedAuthority)
        .collect::<Vec<_>>();
    if authority_sources.len() != manifest.prefix.len()
        || manifest.prefix.iter().any(|member| {
            !authority_sources.iter().any(|source| {
                source.source.document_id == member.document_id
                    && source.source.revision_id == member.revision_id
                    && source.source.body_hash == member.body_hash
            })
        })
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidSnapshot,
            "Every reviewed authority source must be bound to the exact basis manifest.",
        ));
    }
    Ok(())
}

fn apply_disclosure_policy(
    descriptor: &SourceDescriptor,
    policy: &InformationPolicy,
) -> Result<(), EligibilityError> {
    if policy.audience == Audience::AuthorRoom {
        return Ok(());
    }
    if descriptor.coverage == super::contracts::CoverageLabel::DirectoryOnly {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::DirectoryOnly,
            &descriptor.handle,
            "A directory entry cannot be used as writing evidence.",
        ));
    }
    if matches!(
        descriptor.kind,
        SourceKind::PrivateFuture | SourceKind::AuthorRoomDiscussion
    ) || descriptor.disclosure.author_only
        || descriptor.disclosure.future_private
    {
        return Err(EligibilityError::for_handle(
            if descriptor.disclosure.future_private {
                EligibilityErrorCode::FutureContamination
            } else {
                EligibilityErrorCode::PrivateSource
            },
            &descriptor.handle,
            "Author-room or future-private material is outside restricted writing policy.",
        ));
    }
    let frontier = policy.reader_frontier.as_deref().ok_or_else(|| {
        EligibilityError::new(
            EligibilityErrorCode::InvalidPolicy,
            "Restricted writing requires an explicit reader frontier.",
        )
    })?;
    let disclosure = descriptor
        .disclosure
        .reader_position
        .as_deref()
        .ok_or_else(|| {
            EligibilityError::for_handle(
                EligibilityErrorCode::DisclosureUnknown,
                &descriptor.handle,
                "This source has no known reader disclosure position.",
            )
        })?;
    if !decimal_le(disclosure, frontier) {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::DisclosureDenied,
            &descriptor.handle,
            "This source is disclosed after the selected reader frontier.",
        ));
    }
    if descriptor.kind == SourceKind::PlanAlternative && !policy.allow_alternatives {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::AlternativeNotSelected,
            &descriptor.handle,
            "Alternative material requires an explicit policy selection.",
        ));
    }
    if let Some(character) = policy.character_id.as_deref() {
        if !descriptor.disclosure.visible_to_characters.is_empty()
            && !descriptor
                .disclosure
                .visible_to_characters
                .iter()
                .any(|value| value == character)
        {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::DisclosureDenied,
                &descriptor.handle,
                "This source is not disclosed to the selected character.",
            ));
        }
        if !policy.character_grants.iter().any(|grant| {
            grant.character_id == character
                && grant.source_handle == descriptor.handle
                && decimal_le(&grant.reader_frontier, frontier)
        }) {
            return Err(EligibilityError::for_handle(
                EligibilityErrorCode::DisclosureDenied,
                &descriptor.handle,
                "A limited-POV source requires an explicit character grant.",
            ));
        }
    } else if !descriptor.disclosure.visible_to_characters.is_empty() {
        return Err(EligibilityError::for_handle(
            EligibilityErrorCode::DisclosureDenied,
            &descriptor.handle,
            "Character-specific knowledge requires a selected character policy.",
        ));
    }
    Ok(())
}

fn validate_source_ref(source: &SourceRef, project_id: &str) -> Result<(), EligibilityError> {
    for (label, value) in [
        ("projectId", source.project_id.as_str()),
        ("documentId", source.document_id.as_str()),
        ("revisionId", source.revision_id.as_str()),
    ] {
        nonempty(value, EligibilityErrorCode::InvalidDescriptor, label)?;
        if value.len() > 256 {
            return Err(EligibilityError::new(
                EligibilityErrorCode::InvalidDescriptor,
                format!("{label} is too long."),
            ));
        }
    }
    if source.project_id != project_id {
        return Err(EligibilityError::new(
            EligibilityErrorCode::CrossProjectSource,
            "A source belongs to another project.",
        ));
    }
    if source.body_hash.len() != 64
        || !source
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(EligibilityError::new(
            EligibilityErrorCode::InvalidDescriptor,
            "A source body hash must be 64 hexadecimal characters.",
        ));
    }
    Ok(())
}

fn nonempty(value: &str, code: EligibilityErrorCode, field: &str) -> Result<(), EligibilityError> {
    if value.is_empty() {
        return Err(EligibilityError::new(
            code,
            format!("{field} must not be empty."),
        ));
    }
    Ok(())
}

fn decimal(value: &str) -> Result<(), String> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("expected a canonical nonnegative decimal string".into());
    }
    Ok(())
}

fn decimal_le(left: &str, right: &str) -> bool {
    if decimal(left).is_err() || decimal(right).is_err() {
        return false;
    }
    left.len() < right.len() || (left.len() == right.len() && left.as_bytes() <= right.as_bytes())
}
