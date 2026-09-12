//! The TypeScript bindings for every type that crosses the IPC boundary.
//!
//! Rust is the source of truth. The frontend used to carry a hand-written
//! mirror of each type — 255 of them across the `src/ipc/` modules — and
//! nothing compared the two, so a renamed Rust field became a runtime
//! `undefined` in the renderer with no failing test anywhere. That is D6.
//!
//! This crate is the single collector. It is not a layer: it depends on all of
//! them and nothing depends on it, so it is deliberately absent from the
//! layer table, like `wns-app` and `contracts`. Each crate supplies its own
//! [`Group`]; [`render`] turns them into one TypeScript file per crate, and
//! `tests/drift.rs` fails the build when the committed output is stale.
//!
//! Adding a crate is one `Group` and one line in `groups()` — with two things
//! I learned attempting the second crate and reverting it:
//!
//! * **The group must list the whole closure, not the crate's own types.** A
//!   workshop snapshot reaches `DiscussionRun`, which reaches the run
//!   vocabulary, which reaches the provider and context vocabularies. `export`
//!   renders a type and *references* the named types it depends on; it does not
//!   always declare them, so every name the generated file mentions has to be
//!   in the list. The frontend's compiler names exactly which ones are missing,
//!   which is the reliable way to close the list.
//! * **"Public" is not "crosses the wire."** A blanket pass that derives
//!   `specta::Type` on every `pub struct` also hits internal accumulators — the
//!   provider parser state — and those hold private types that cannot be
//!   derived at all. The wire surface is a choice per type; the kernel's eleven
//!   needed no skips because each was chosen.
//!
//! * **A group carries a frontend tail, and it is where the value is.** The
//!   workshop group converged at 131 types, and replacing its mirrors then
//!   failed to typecheck in ~57 places — not because the generation was wrong,
//!   but because the mirrors were. `BasisKind` and `ContinuationBasis` are
//!   distinct Rust enums the hand-written TypeScript used one name for, and
//!   several fields the mirrors declared optional are required in Rust. That is
//!   D6 becoming visible the moment something can see it. A group is finished
//!   when the frontend compiles against it, not when the file is generated; the
//!   kernel group had no tail because its eleven types were already exact.
//!
//! * **specta does not honour `rename_all_fields`, and that is the largest
//!   error class.** `LookupReadResult` and its neighbours carry
//!   `#[serde(rename_all_fields = "camelCase")]`, so their struct-variant fields
//!   really are camelCase on the wire — and the frontend was right to expect
//!   that. specta 1.0 reads only `rename_all` on the *container* and never reads
//!   serde's attributes at all, so it emitted `entity_kind` where the wire has
//!   `entityKind`. The remedy is `#[specta(rename_all = "camelCase")]` on each
//!   variant — settled against a minimal case in `tests/variant_fields.rs`, and
//!   it works. A container-level attribute does not: its `rename_all` applies to
//!   variant *names*. Applying this across the tree took the workshop group's
//!   frontend tail from 83 errors to 24 in one step, which is most of what that
//!   tail was.
//!
//! * **A field Rust omits when empty needs telling, not inferring.**
//!   `#[serde(default, skip_serializing_if = "Vec::is_empty")]` means the field
//!   is *absent* on the wire, so a generated `string[]` is a latent crash: the
//!   frontend reads `undefined` where the type promised an array. specta cannot
//!   express it — `tests/variant_fields.rs` pins the four attribute spellings
//!   that do nothing — so [`OMITTED_WHEN_EMPTY`] lists the fields and
//!   `tests/omitted.rs` re-derives the list from the Rust source and fails when
//!   the two disagree. That test found three the first scan had missed, which
//!   is the argument for deriving it rather than writing it down.
//!
//! **Let the compiler close both lists.** The workshop's derives converged in 6
//!   rounds (workshop → story vocabulary → run vocabulary → provider and context
//!   vocabularies) and the group's names in 6 more against `tsc`. Neither list
//!   should be typed by hand, and both converge quickly when the gap is read
//!   from the error output.
//!
//! ## What the frontend does with a narrower type
//!
//! Rust carries a document body as `serde_json::Value`, so specta emits `any`.
//! The editor knows the body is a `WnsDocument` and says so once, as
//! `Omit<WireRevision, 'body'> & { body: WnsDocument }`. That is narrower than
//! the mirror it replaces: every *other* field still comes from Rust, so a
//! rename there is still a build failure rather than a runtime `undefined`.

use std::collections::BTreeMap;

/// One crate's IPC-facing types.
pub struct Group {
    /// The generated file's name, without extension.
    pub file: &'static str,
    /// The crate's own name, for the file header.
    pub crate_name: &'static str,
    /// Each entry is the declared TypeScript name and the emitted text for it.
    pub types: Vec<(String, String)>,
}

/// Export one type. `specta` emits it *and* everything it references.
pub fn one<T: specta::NamedType>() -> Result<String, specta::ts::TsExportError> {
    specta::ts::export::<T>(&config())
}

fn config() -> specta::ts::ExportConfiguration {
    specta::ts::ExportConfiguration::new().bigint(specta::ts::BigIntExportBehavior::Number)
}

/// Build a group from a list of exported types, keeping one declaration per
/// name — `Head` sits inside several, and exporting each type in turn repeats
/// it.
pub fn group(
    file: &'static str,
    crate_name: &'static str,
    exported: Vec<(&'static str, Result<String, specta::ts::TsExportError>)>,
) -> Result<Group, specta::ts::TsExportError> {
    let mut declarations: BTreeMap<String, String> = BTreeMap::new();
    for (_, text) in exported {
        for block in split_declarations(&text?) {
            if let Some(name) = declaration_name(&block) {
                let marked = mark_omitted(&name, &block);
                declarations.entry(name).or_insert(marked);
            }
        }
    }
    Ok(Group { file, crate_name, types: declarations.into_iter().collect() })
}

/// Render one group as a TypeScript module.
///
/// A group's types reference types from the layers below it — the workshop's
/// snapshot embeds a `DocumentRecord` — so the header imports whatever another
/// group declares. Without it each file would re-declare them, which is the
/// drift this crate exists to remove.
pub fn render(group: &Group, others: &[Group]) -> String {
    let own: Vec<&str> = group.types.iter().map(|(n, _)| n.as_str()).collect();
    let mut imports: Vec<(&str, Vec<&str>)> = Vec::new();
    for other in others {
        if other.file == group.file {
            continue;
        }
        let referenced: Vec<&str> = other
            .types
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !own.contains(n))
            .filter(|name| {
                group.types.iter().any(|(_, block)| {
                    block.match_indices(*name).any(|(i, _)| {
                        let before = block[..i].chars().last();
                        let after = block[i + name.len()..].chars().next();
                        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    })
                })
            })
            .collect();
        if !referenced.is_empty() {
            imports.push((other.file, referenced));
        }
    }

    let mut out = format!(
        "// Generated from `{}` by `crates/bindings`. Do not edit.\n\
         // Change the Rust type and run `cargo run -p wns-bindings`.\n",
        group.crate_name
    );
    for (file, names) in &imports {
        out.push_str(&format!("import type {{ {} }} from './{}';\n", names.join(", "), file));
    }
    out.push('\n');
    for (_, block) in &group.types {
        out.push_str(block.trim_end());
        out.push_str("\n\n");
    }
    out
}

/// Render every group, each importing what it needs from the others.
pub fn render_all() -> Result<Vec<(String, String)>, specta::ts::TsExportError> {
    let groups = groups()?;
    Ok(groups
        .iter()
        .map(|g| (format!("{}.ts", g.file), render(g, &groups)))
        .collect())
}

/// Fields Rust omits when empty, as `(Rust type, Rust field)`.
///
/// `#[serde(default, skip_serializing_if = "…")]` on a non-`Option` field means
/// the field is *absent* on the wire, so the generated TypeScript has to mark
/// it optional or the frontend reads `undefined` where the type promised an
/// array. specta cannot express that — `tests/variant_fields.rs` pins the four
/// attribute spellings that do not work — so the generator is told here.
///
/// `tests/omitted.rs` re-scans the Rust source and asserts this list is exactly
/// what it finds, so a field added or changed without the list fails the build.
pub const OMITTED_WHEN_EMPTY: &[(&str, &str)] = &[
    ("ChatGroupEffectsOutput", "impacts"),
    ("ChatGroupEffectsOutput", "placements"),
    ("ChatGroupEffectsOutput", "relationships"),
    ("ChatGroupEffectsOutput", "supersessions"),
    ("DiscussionDraft", "intent"),
    ("DocumentRecord", "role"),
    ("FrozenContext", "guidance"),
    ("FrozenContext", "navigation_views"),
    ("FrozenContext", "reviewed_evidence"),
    ("FrozenContext", "reviewed_knowledge"),
    ("FrozenContext", "reviewed_promises"),
    ("FrozenContext", "reviewed_summaries"),
    ("FrozenProjectChat", "dispositions"),
    ("HistoricalConversationItem", "draft_revisions"),
    ("HistoricalConversationItem", "messages"),
    ("HistoricalConversationItem", "source_revisions"),
    ("LookupRead", "offset"),    ("PacketOptions", "max_output_tokens"),
    ("PacketReceipt", "conversation_message_ids"),
    ("PacketReceipt", "guidance_handles"),
    ("PacketReceipt", "mandatory_source_handles"),
    ("PacketReceipt", "navigation_omissions"),
    ("PacketReceipt", "navigation_views"),
    ("PacketReceipt", "omitted_discussion_turns"),
    ("PacketReceipt", "reviewed_evidence"),
    ("PacketReceipt", "reviewed_evidence_omissions"),
    ("PacketReceipt", "reviewed_knowledge_omissions"),
    ("PacketReceipt", "reviewed_summary_omissions"),
    ("PacketReceipt", "reviewed_knowledge"),
    ("PacketReceipt", "reviewed_promise_omissions"),
    ("PacketReceipt", "reviewed_promises"),
    ("PacketReceipt", "reviewed_summaries"),
    ("PreviewWorkshopAdoption", "impact_drafts"),
    ("PreviewWorkshopAdoption", "relationships"),
    ("Proposal", "kind"),
    ("SaveDiscussionDraft", "intent"),
    ("StartDiscussion", "intent"),
    ("TypedReplacementInline", "marks"),    ("WorkshopAdoptionPreview", "endpoint_sources"),
    ("WorkshopAdoptionPreview", "impacts"),
    ("WorkshopAdoptionPreview", "relationships"),
    ("WorkshopContext", "story_possibilities"),
    ("WorkshopPacketMetadata", "story_possibilities"),
    ("WorkshopSession", "story_possibilities"),
];

/// `story_possibilities` -> `storyPossibilities`.
fn camel(field: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in field.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Mark the fields of `declaration` that Rust omits when empty.
fn mark_omitted(name: &str, block: &str) -> String {
    let mut out = block.to_owned();
    for (ty, field) in OMITTED_WHEN_EMPTY {
        if *ty != name {
            continue;
        }
        let key = camel(field);
        // `storyPossibilities:` -> `storyPossibilities?:`, at EVERY occurrence.
        // A struct field appears once, but an enum's declaration is one union
        // and the same field name can appear in several variants — `LookupRead`
        // carries `offset` in four of them, and marking only the first leaves
        // three promising a value the wire omits.
        let needle = format!("{key}:");
        let mut from = 0;
        while let Some(rel) = out[from..].find(&needle) {
            let at = from + rel;
            let boundary = out[..at].chars().last();
            if boundary.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                from = at + needle.len();
                continue;
            }
            let after = at + key.len();
            if !out[after..].starts_with('?') {
                out.insert(after, '?');
            }
            from = after + 1;
        }
    }
    out
}

/// Where the generated files live, relative to the workspace root.
pub const OUTPUT_DIR: &str = "apps/desktop/src/ipc/generated";

/// One declaration per element. A declaration starts at `export`, and a doc
/// comment starts the block it belongs to.
fn split_declarations(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let starts_item = line.starts_with("export ") || line.starts_with("/**");
        if starts_item && current.contains("export ") {
            blocks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        blocks.push(current);
    }
    blocks
}

fn declaration_name(block: &str) -> Option<String> {
    let start = block.find("export ")?;
    let rest = block[start..].strip_prefix("export ")?;
    let rest = rest
        .strip_prefix("type ")
        .or_else(|| rest.strip_prefix("enum "))
        .or_else(|| rest.strip_prefix("interface "))?;
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

/// Every group the frontend has been migrated onto.
pub fn groups() -> Result<Vec<Group>, specta::ts::TsExportError> {
    Ok(vec![
        wns_groups::kernel()?,
        wns_groups::workshop()?,
        wns_groups::context()?,
        wns_groups::documents()?,
        wns_groups::conversation()?,
        wns_groups::story()?,
    ])
}

/// The workspace root, from this crate's manifest directory.
pub fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

mod wns_groups {
    use super::{group, one, Group};

    /// `wns-kernel`'s IPC types.
    pub fn kernel() -> Result<Group, specta::ts::TsExportError> {
        group(
            "kernel",
            "wns-kernel",
            vec![
                ("CoreError", one::<wns_kernel::CoreError>()),
                ("Head", one::<wns_kernel::Head>()),
                ("ProjectAccess", one::<wns_kernel::ProjectAccess>()),
                ("Revision", one::<wns_kernel::Revision>()),
                ("ProjectInfo", one::<wns_kernel::ProjectInfo>()),
                ("DocumentRecord", one::<wns_kernel::DocumentRecord>()),
                ("DocumentRole", one::<wns_kernel::DocumentRole>()),
                ("RestoredDecision", one::<wns_kernel::RestoredDecision>()),
                ("AppliedDecision", one::<wns_kernel::AppliedDecision>()),
                ("StoredResult", one::<wns_kernel::StoredResult>()),
                ("SnapshotReceipt", one::<wns_kernel::SnapshotReceipt>()),
            ],
        )
    }

    /// `wns-workshop`'s IPC closure.
    ///
    /// Not only the crate's own types: the record vocabulary moved to
    /// `wns-story` when the two crates stopped being able to reach sideways,
    /// the snapshot reaches `DiscussionRun`, and that reaches the provider and
    /// context vocabularies. `export` renders a type and *references* its named
    /// dependencies without always declaring them, so every name the generated
    /// file mentions is listed here — closed against the frontend compiler
    /// rather than by hand.
    pub fn workshop() -> Result<Group, specta::ts::TsExportError> {
        group(
            "workshop",
            "wns-workshop",
            vec![
                ("WorkshopRelationshipDraft", one::<wns_workshop::workshop::WorkshopRelationshipDraft>()),
                ("WorkshopImpactDraft", one::<wns_workshop::workshop::WorkshopImpactDraft>()),
                ("WorkshopAdoptionImpact", one::<wns_workshop::workshop::WorkshopAdoptionImpact>()),
                ("WorkshopSnapshot", one::<wns_workshop::workshop::WorkshopSnapshot>()),
                ("WorkshopCandidateImplication", one::<wns_workshop::workshop::WorkshopCandidateImplication>()),
                ("WorkshopCandidateAffectedTarget", one::<wns_workshop::workshop::WorkshopCandidateAffectedTarget>()),
                ("WorkshopCandidate", one::<wns_workshop::workshop::WorkshopCandidate>()),
                ("WorkshopOutputInterpretation", one::<wns_workshop::workshop::WorkshopOutputInterpretation>()),
                ("WorkshopOutput", one::<wns_workshop::workshop::WorkshopOutput>()),
                ("WorkshopResult", one::<wns_workshop::workshop::WorkshopResult>()),
                ("WorkshopView", one::<wns_workshop::workshop::WorkshopView>()),
                ("SaveWorkshop", one::<wns_workshop::workshop::SaveWorkshop>()),
                ("WorkshopAdoptionTarget", one::<wns_workshop::workshop::WorkshopAdoptionTarget>()),
                ("AdoptionMode", one::<wns_workshop::workshop::AdoptionMode>()),
                ("PreviewWorkshopAdoption", one::<wns_workshop::workshop::PreviewWorkshopAdoption>()),
                ("WorkshopAdoptionPreview", one::<wns_workshop::workshop::WorkshopAdoptionPreview>()),
                ("WorkshopAdoptionAck", one::<wns_workshop::workshop::WorkshopAdoptionAck>()),
                ("WorkshopDepth", one::<wns_story::workshop_vocabulary::WorkshopDepth>()),
                ("PreferencePolarity", one::<wns_story::workshop_vocabulary::PreferencePolarity>()),
                ("PreferenceStrength", one::<wns_story::workshop_vocabulary::PreferenceStrength>()),
                ("PreferenceScope", one::<wns_story::workshop_vocabulary::PreferenceScope>()),
                ("CandidateChoiceStatus", one::<wns_story::workshop_vocabulary::CandidateChoiceStatus>()),
                ("WorkshopQuestionStatus", one::<wns_story::workshop_vocabulary::WorkshopQuestionStatus>()),
                ("UnknownTo", one::<wns_story::workshop_vocabulary::UnknownTo>()),
                ("WorkshopRelationshipStatus", one::<wns_story::workshop_vocabulary::WorkshopRelationshipStatus>()),
                ("WorkshopPreference", one::<wns_story::workshop_vocabulary::WorkshopPreference>()),
                ("WorkshopQuestion", one::<wns_story::workshop_vocabulary::WorkshopQuestion>()),
                ("StoryPossibilityKind", one::<wns_story::workshop_vocabulary::StoryPossibilityKind>()),
                ("StoryPossibilityStatus", one::<wns_story::workshop_vocabulary::StoryPossibilityStatus>()),
                ("StoryPossibility", one::<wns_story::workshop_vocabulary::StoryPossibility>()),
                ("WorkshopRelationship", one::<wns_story::workshop_vocabulary::WorkshopRelationship>()),
                ("WorkshopBranchKind", one::<wns_story::workshop_vocabulary::WorkshopBranchKind>()),
                ("WorkshopDecisionStatus", one::<wns_story::workshop_vocabulary::WorkshopDecisionStatus>()),
                ("WorkshopImpactKind", one::<wns_story::workshop_vocabulary::WorkshopImpactKind>()),
                ("WorkshopImpactStatus", one::<wns_story::workshop_vocabulary::WorkshopImpactStatus>()),
                ("SelectedDetail", one::<wns_story::workshop_vocabulary::SelectedDetail>()),
                ("CandidateChoice", one::<wns_story::workshop_vocabulary::CandidateChoice>()),
                ("WorkshopSession", one::<wns_story::workshop_vocabulary::WorkshopSession>()),
                ("WorkshopDecision", one::<wns_story::workshop_vocabulary::WorkshopDecision>()),
                ("WorkshopImpact", one::<wns_story::workshop_vocabulary::WorkshopImpact>()),
                ("WorkshopPreset", one::<wns_story::workshop_vocabulary::WorkshopPreset>()),
                ("WorkshopState", one::<wns_story::workshop_vocabulary::WorkshopState>()),
                ("DiscussionRun", one::<wns_story::run_vocabulary::DiscussionRun>()),
                ("Lens", one::<wns_story::workshop_vocabulary::Lens>()),
                ("WorkshopWorkingSelection", one::<wns_story::workshop_metadata::WorkshopWorkingSelection>()),
                ("BasisKind", one::<wns_context::contracts::BasisKind>()),
                ("DiscussionRunStatus", one::<wns_story::run_vocabulary::DiscussionRunStatus>()),
                ("FeedbackIntent", one::<wns_story::discussion_vocabulary::FeedbackIntent>()),
                ("LookupRunSummary", one::<wns_story::run_vocabulary::LookupRunSummary>()),
                ("ProviderBinding", one::<wns_providers::vocabulary::ProviderBinding>()),
                ("ProviderResult", one::<wns_story::run_vocabulary::ProviderResult>()),
                ("RunOwner", one::<wns_story::run_vocabulary::RunOwner>()),
                ("AppServerDelivery", one::<wns_providers::codex_app_server::AppServerDelivery>()),
                ("HttpProviderBinding", one::<wns_providers::vocabulary::HttpProviderBinding>()),
                ("LookupAllowance", one::<wns_context::lookup::LookupAllowance>()),
                ("LookupInvocationSummary", one::<wns_story::run_vocabulary::LookupInvocationSummary>()),
                ("ProviderCleanup", one::<wns_providers::vocabulary::ProviderCleanup>()),
                ("ProviderDeliveryReceipt", one::<wns_providers::vocabulary::ProviderDeliveryReceipt>()),
                ("ProviderOutcomeStatus", one::<wns_providers::vocabulary::ProviderOutcomeStatus>()),
                ("ProviderRuntimeIdentity", one::<wns_providers::vocabulary::ProviderRuntimeIdentity>()),
                ("ProviderUsage", one::<wns_providers::vocabulary::ProviderUsage>()),
                ("AppServerConnectionSettlement", one::<wns_providers::codex_app_server::AppServerConnectionSettlement>()),
                ("AppServerDispatch", one::<wns_providers::codex_app_server::AppServerDispatch>()),
                ("AppServerRuntimeIdentity", one::<wns_providers::codex_app_server::AppServerRuntimeIdentity>()),
                ("AppServerSubmission", one::<wns_providers::codex_app_server::AppServerSubmission>()),
                ("AppServerTerminal", one::<wns_providers::codex_app_server::AppServerTerminal>()),
                ("HttpDeliverySubmission", one::<wns_providers::vocabulary::HttpDeliverySubmission>()),
                ("HttpProviderUsage", one::<wns_providers::vocabulary::HttpProviderUsage>()),
                ("HttpResponseFormat", one::<wns_providers::vocabulary::HttpResponseFormat>()),
                ("LookupInvocationState", one::<wns_story::run_vocabulary::LookupInvocationState>()),
            ],
        )
    }

    /// `context`'s IPC closure: every name its generated file mentions, closed
    /// against the frontend compiler rather than typed by hand.
    pub fn context() -> Result<Group, specta::ts::TsExportError> {
        group(
            "context",
            "context",
            vec![
                ("CompiledPacket", one::<wns_context::packet::CompiledPacket>()),
                ("ContextEpochs", one::<wns_story::story_context::ContextEpochs>()),
                ("ContextPurpose", one::<wns_context::contracts::ContextPurpose>()),
                ("ConversationMessage", one::<wns_context::conversation::ConversationMessage>()),
                ("ConversationTurn", one::<wns_context::conversation::ConversationTurn>()),
                ("EvidenceHistory", one::<wns_context::evidence_history::EvidenceHistory>()),
                ("EvidenceHistoryObservation", one::<wns_context::evidence_history::EvidenceHistoryObservation>()),
                ("FrozenContext", one::<wns_context::frozen::FrozenContext>()),
                ("FrozenConversation", one::<wns_context::conversation::FrozenConversation>()),
                ("FrozenNavigationView", one::<wns_context::navigation::FrozenNavigationView>()),
                ("InformationPolicy", one::<wns_context::contracts::InformationPolicy>()),
                ("KnowledgeHistory", one::<wns_context::knowledge_history::KnowledgeHistory>()),
                ("KnowledgeHistoryObservation", one::<wns_context::knowledge_history::KnowledgeHistoryObservation>()),
                ("LookupExchange", one::<wns_context::lookup::LookupExchange>()),
                ("LookupPacketInput", one::<wns_context::lookup::LookupPacketInput>()),
                ("LookupSourceProjection", one::<wns_context::lookup::LookupSourceProjection>()),
                ("MockContextBudget", one::<wns_context::packet::MockContextBudget>()),
                ("NavigationViewOmission", one::<wns_context::navigation::NavigationViewOmission>()),
                ("NavigationViewRef", one::<wns_context::navigation::NavigationViewRef>()),
                ("PacketReceipt", one::<wns_context::contracts::PacketReceipt>()),
                ("PreparationResult", one::<wns_story::context_packets::PreparationResult>()),
                ("PromiseHistory", one::<wns_context::promise_history::PromiseHistory>()),
                ("PromiseHistoryObservation", one::<wns_context::promise_history::PromiseHistoryObservation>()),
                ("ReviewedBasisManifest", one::<wns_context::contracts::ReviewedBasisManifest>()),
                ("ReviewedEvidenceCoverage", one::<wns_context::reviewed_evidence::ReviewedEvidenceCoverage>()),
                ("ReviewedEvidenceOmission", one::<wns_context::reviewed_evidence::ReviewedEvidenceOmission>()),
                ("ReviewedEvidenceSet", one::<wns_context::reviewed_evidence::ReviewedEvidenceSet>()),
                ("ReviewedHistoryResult", one::<wns_story::evidence_queries::ReviewedHistoryResult>()),
                ("ReviewedKnowledgeHistoryResult", one::<wns_story::evidence_queries::ReviewedKnowledgeHistoryResult>()),
                ("ReviewedKnowledgeSet", one::<wns_context::reviewed_knowledge::ReviewedKnowledgeSet>()),
                ("ReviewedPromiseHistoryResult", one::<wns_story::evidence_queries::ReviewedPromiseHistoryResult>()),
                ("ReviewedPromiseSet", one::<wns_context::reviewed_promises::ReviewedPromiseSet>()),
                ("ReviewedSummaryCoverage", one::<wns_context::reviewed_summaries::ReviewedSummaryCoverage>()),
                ("ReviewedSummaryOmission", one::<wns_context::reviewed_summaries::ReviewedSummaryOmission>()),
                ("ReviewedSummarySet", one::<wns_context::reviewed_summaries::ReviewedSummarySet>()),
                ("ScopeGrant", one::<wns_documents::scope::ScopeGrant>()),
                ("SourceDescriptor", one::<wns_context::contracts::SourceDescriptor>()),
                ("SourcePassage", one::<wns_context::frozen::SourcePassage>()),
                ("SourceRead", one::<wns_context::frozen::SourceRead>()),
                ("SourceRef", one::<wns_context::contracts::SourceRef>()),
                ("StorySnapshot", one::<wns_context::contracts::StorySnapshot>()),
                ("Audience", one::<wns_context::contracts::Audience>()),
                ("BudgetError", one::<wns_context::contracts::BudgetError>()),
                ("CharacterGrant", one::<wns_context::contracts::CharacterGrant>()),
                ("CoverageEntry", one::<wns_context::contracts::CoverageEntry>()),
                ("CoverageLabel", one::<wns_context::contracts::CoverageLabel>()),
                ("DigestCandidate", one::<wns_context::memory::DigestCandidate>()),
                ("Disclosure", one::<wns_context::contracts::Disclosure>()),
                ("Endpoint", one::<wns_documents::scope::Endpoint>()),
                ("EvidenceAnchor", one::<wns_context::story_records::EvidenceAnchor>()),
                ("EvidenceAudience", one::<wns_context::story_records::EvidenceAudience>()),
                ("EvidenceHistoryUncertainty", one::<wns_context::evidence_history::EvidenceHistoryUncertainty>()),
                ("FrozenGuidance", one::<wns_context::guidance::FrozenGuidance>()),
                ("FrozenProjectChat", one::<wns_context::chat_vocabulary::FrozenProjectChat>()),
                ("KnowledgeAttitude", one::<wns_context::story_records::KnowledgeAttitude>()),
                ("KnowledgeHistoryUncertainty", one::<wns_context::knowledge_history::KnowledgeHistoryUncertainty>()),
                ("KnowledgeRecord", one::<wns_context::story_records::KnowledgeRecord>()),
                ("LookupRead", one::<wns_context::lookup::LookupRead>()),
                ("LookupReadResult", one::<wns_context::lookup::LookupReadResult>()),
                ("LookupSourceProjectionSource", one::<wns_context::lookup::LookupSourceProjectionSource>()),
                ("NavigationOmissionReason", one::<wns_context::navigation::NavigationOmissionReason>()),
                ("PacketMessage", one::<wns_providers::vocabulary::PacketMessage>()),
                ("PacketOptions", one::<wns_providers::vocabulary::PacketOptions>()),
                ("PossessionRecord", one::<wns_context::story_records::PossessionRecord>()),
                ("PossessionTiming", one::<wns_context::story_records::PossessionTiming>()),
                ("PromiseHistoryUncertainty", one::<wns_context::promise_history::PromiseHistoryUncertainty>()),
                ("PromisePhase", one::<wns_context::story_records::PromisePhase>()),
                ("PromiseRecord", one::<wns_context::story_records::PromiseRecord>()),
                ("ReviewedBasisMember", one::<wns_context::contracts::ReviewedBasisMember>()),
                ("ReviewedEvidenceOmissionReason", one::<wns_context::reviewed_evidence::ReviewedEvidenceOmissionReason>()),
                ("ReviewedSummaryOmissionReason", one::<wns_context::reviewed_summaries::ReviewedSummaryOmissionReason>()),
                ("SafeBriefReceipt", one::<wns_context::contracts::SafeBriefReceipt>()),
                ("ScopeKind", one::<wns_documents::scope::ScopeKind>()),
                ("SourceKind", one::<wns_context::contracts::SourceKind>()),
                ("StoryEntityRef", one::<wns_context::story_records::StoryEntityRef>()),
                ("StoryTime", one::<wns_context::contracts::StoryTime>()),
                ("SummaryRevision", one::<wns_context::reviewed_summary::SummaryRevision>()),
                ("BudgetErrorCode", one::<wns_context::contracts::BudgetErrorCode>()),
                ("DigestItem", one::<wns_context::memory::DigestItem>()),
                ("FrozenProjectChatDisposition", one::<wns_context::chat_vocabulary::FrozenProjectChatDisposition>()),
                ("GuidanceVersion", one::<wns_context::guidance::GuidanceVersion>()),
                ("MemoryEntityEntry", one::<wns_context::lookup::MemoryEntityEntry>()),
                ("MemoryEntityKind", one::<wns_context::lookup::MemoryEntityKind>()),
                ("ProjectBriefOrigin", one::<wns_context::contracts::ProjectBriefOrigin>()),
                ("ProjectChatDraftRef", one::<wns_context::chat_vocabulary::ProjectChatDraftRef>()),
                ("ReviewPrefixItem", one::<wns_context::reviewed_prefix::ReviewPrefixItem>()),
                ("SearchMode", one::<wns_context::frozen::SearchMode>()),
                ("SearchResult", one::<wns_context::frozen::SearchResult>()),
                ("SummaryAudience", one::<wns_context::reviewed_summary::SummaryAudience>()),
                ("DigestEvidence", one::<wns_context::memory::DigestEvidence>()),
                ("ChatDispositionScope", one::<wns_context::chat_vocabulary::ChatDispositionScope>()),
                ("ChatUnknownTo", one::<wns_context::chat_vocabulary::ChatUnknownTo>()),
                ("GuidanceScope", one::<wns_context::guidance::GuidanceScope>()),
                ("SearchHit", one::<wns_context::frozen::SearchHit>()),
                ("ChatDispositionScopeKind", one::<wns_context::chat_vocabulary::ChatDispositionScopeKind>()),
            ],
        )
    }

    /// `documents`'s IPC closure: every name its generated file mentions, closed
    /// against the frontend compiler rather than typed by hand.
    pub fn documents() -> Result<Group, specta::ts::TsExportError> {
        group(
            "documents",
            "documents",
            vec![
                ("CheckpointRequest", one::<wns_documents::records::CheckpointRequest>()),
                ("HistoryPage", one::<wns_documents::history::HistoryPage>()),
                ("OperationReceipt", one::<wns_documents::records::OperationReceipt>()),
                ("ReconcileRequest", one::<wns_documents::records::ReconcileRequest>()),
                ("ReconciledDocument", one::<wns_documents::records::ReconciledDocument>()),
                ("RestoreAck", one::<wns_documents::history::RestoreAck>()),
                ("RestoreRevision", one::<wns_documents::history::RestoreRevision>()),
                ("RevisionSummary", one::<wns_documents::history::RevisionSummary>()),
                ("SaveAck", one::<wns_documents::records::SaveAck>()),
                ("SaveSnapshot", one::<wns_documents::records::SaveSnapshot>()),
                ("ViewState", one::<wns_documents::view_state::ViewState>()),
                ("CheckpointReason", one::<wns_documents::records::CheckpointReason>()),
                ("SaveCause", one::<wns_documents::records::SaveCause>()),
                // The frontend calls this `StructuredBlock`; nothing in
                // `ipc/*.ts` names it, so the seeding pass cannot find it and
                // it is listed here by hand. `ipc/proposals` aliases it.
                ("TypedReplacementBlock", one::<wns_documents::structured::TypedReplacementBlock>()),
                ("TypedReplacementInline", one::<wns_documents::structured::TypedReplacementInline>()),
                ("TypedReplacementHeadingAttrs", one::<wns_documents::structured::TypedReplacementHeadingAttrs>()),
                ("TypedReplacementMark", one::<wns_documents::structured::TypedReplacementMark>()),
                ("TypedReplacementLinkAttrs", one::<wns_documents::structured::TypedReplacementLinkAttrs>()),
            ],
        )
    }

    /// `conversation`'s IPC closure: every name its generated file mentions, closed
    /// against the frontend compiler rather than typed by hand.
    pub fn conversation() -> Result<Group, specta::ts::TsExportError> {
        group(
            "conversation",
            "conversation",
            vec![
                ("ApplyAck", one::<wns_conversation::proposals::ApplyAck>()),
                ("ApplyProposal", one::<wns_conversation::proposals::ApplyProposal>()),
                ("AssistantDraft", one::<wns_conversation::project_chat::AssistantDraft>()),
                ("ChapterDiscussionFeedback", one::<wns_conversation::project_chat::ChapterDiscussionFeedback>()),
                ("ChapterRangeProposal", one::<wns_context::project_chat_output::ChapterRangeProposal>()),
                ("ChatAdoptionAck", one::<wns_conversation::project_chat::ChatAdoptionAck>()),
                ("ChatAdoptionEffects", one::<wns_conversation::project_chat::ChatAdoptionEffects>()),
                ("ChatAdoptionImpact", one::<wns_conversation::project_chat::ChatAdoptionImpact>()),
                ("ChatAdoptionPlacement", one::<wns_conversation::project_chat::ChatAdoptionPlacement>()),
                ("ChatAdoptionPreview", one::<wns_conversation::project_chat::ChatAdoptionPreview>()),
                ("ChatAdoptionRelationship", one::<wns_conversation::project_chat::ChatAdoptionRelationship>()),
                ("ChatAdoptionSupersession", one::<wns_conversation::project_chat::ChatAdoptionSupersession>()),
                ("ChatAdoptionTarget", one::<wns_conversation::project_chat::ChatAdoptionTarget>()),
                ("ChatDocumentSave", one::<wns_conversation::project_chat::ChatDocumentSave>()),
                ("ChatProtectedContent", one::<wns_conversation::project_chat::ChatProtectedContent>()),
                ("ChatRelationshipDependency", one::<wns_conversation::project_chat::ChatRelationshipDependency>()),
                ("ConversationItem", one::<wns_conversation::project_chat::ConversationItem>()),
                ("DiscussionDraft", one::<wns_conversation::discussions::DiscussionDraft>()),
                ("DiscussionMessage", one::<wns_story::run_vocabulary::DiscussionMessage>()),
                ("DiscussionMessageRole", one::<wns_story::run_vocabulary::DiscussionMessageRole>()),
                ("DiscussionScopeInput", one::<wns_story::discussion_vocabulary::DiscussionScopeInput>()),
                ("DiscussionView", one::<wns_conversation::discussions::DiscussionView>()),
                ("HistoricalConversation", one::<wns_conversation::project_chat::HistoricalConversation>()),
                ("HistoricalConversationItem", one::<wns_conversation::project_chat::HistoricalConversationItem>()),
                ("HistoricalConversationRef", one::<wns_conversation::project_chat::HistoricalConversationRef>()),
                ("HistoricalConversationSummary", one::<wns_conversation::project_chat::HistoricalConversationSummary>()),
                ("HistoricalDraftRevision", one::<wns_conversation::project_chat::HistoricalDraftRevision>()),
                ("HistoricalSourceRevision", one::<wns_conversation::project_chat::HistoricalSourceRevision>()),
                ("PrepareContinuation", one::<wns_conversation::proposals::PrepareContinuation>()),
                ("PrepareProposal", one::<wns_conversation::proposals::PrepareProposal>()),
                ("PrepareStructured", one::<wns_conversation::proposals::PrepareStructured>()),
                ("PreparedProposal", one::<wns_conversation::proposals::PreparedProposal>()),
                ("ProjectChapterComposer", one::<wns_conversation::project_chat::ProjectChapterComposer>()),
                ("ProjectComposer", one::<wns_conversation::project_chat::ProjectComposer>()),
                ("ProjectComposerSnapshot", one::<wns_conversation::project_chat::ProjectComposerSnapshot>()),
                ("Proposal", one::<wns_conversation::proposals::Proposal>()),
                ("ProposalCandidate", one::<wns_conversation::proposals::ProposalCandidate>()),
                ("ProposalContent", one::<wns_conversation::proposals::ProposalContent>()),
                ("ProposalDecision", one::<wns_conversation::proposals::ProposalDecision>()),
                ("ProposalKind", one::<wns_conversation::proposals::ProposalKind>()),
                ("ReadProjectChatHistory", one::<wns_conversation::project_chat::ReadProjectChatHistory>()),
                ("SafeBriefInput", one::<wns_context::contracts::SafeBriefInput>()),
                ("SaveDiscussionDraft", one::<wns_conversation::discussions::SaveDiscussionDraft>()),
                ("SaveGuidance", one::<wns_conversation::guidance::SaveGuidance>()),
                ("SaveProjectComposer", one::<wns_conversation::project_chat::SaveProjectComposer>()),
                ("StartProjectChapter", one::<wns_conversation::project_chat::StartProjectChapter>()),
                ("StartProjectChat", one::<wns_conversation::project_chat::StartProjectChat>()),
                ("ContinuationCandidate", one::<wns_context::continuation::ContinuationCandidate>()),
                ("StructuredProposalCandidate", one::<wns_conversation::proposals::StructuredProposalCandidate>()),
            ],
        )
    }

    /// `story`'s IPC closure: every name its generated file mentions, closed
    /// against the frontend compiler rather than typed by hand.
    pub fn story() -> Result<Group, specta::ts::TsExportError> {
        group(
            "story",
            "story",
            vec![
                ("DiscussionMessage", one::<wns_story::run_vocabulary::DiscussionMessage>()),
                ("DiscussionStart", one::<wns_story::run_vocabulary::DiscussionStart>()),
                ("MarkReady", one::<wns_story::reviewed_story::MarkReady>()),
                ("MemoryDispatchState", one::<wns_story::memory::MemoryDispatchState>()),
                ("MemoryJob", one::<wns_story::memory::MemoryJob>()),
                ("MemoryJobStatus", one::<wns_story::memory::MemoryJobStatus>()),
                ("MemoryOwner", one::<wns_story::memory::MemoryOwner>()),
                ("MemoryRead", one::<wns_story::memory::MemoryRead>()),
                ("MemoryResult", one::<wns_story::memory::MemoryResult>()),
                ("PreparationResult", one::<wns_story::context_packets::PreparationResult>()),
                ("ReadyBundle", one::<wns_story::reviewed_story::ReadyBundle>()),
                ("ReviewStage", one::<wns_story::reviewed_story::ReviewStage>()),
                ("ReviewStatus", one::<wns_story::reviewed_story::ReviewStatus>()),
                ("ReviewedEntityCatalog", one::<wns_story::evidence_queries::ReviewedEntityCatalog>()),
                ("ReviewedEntityChoice", one::<wns_story::evidence_queries::ReviewedEntityChoice>()),
                ("ReviewedRecordSet", one::<wns_story::reviewed_story::ReviewedRecordSet>()),
                ("SaveSourcePins", one::<wns_story::source_pins::SaveSourcePins>()),
                ("SourcePinScope", one::<wns_story::source_pins::SourcePinScope>()),
                ("SourcePinSet", one::<wns_story::source_pins::SourcePinSet>()),
                ("SourcePinsView", one::<wns_story::source_pins::SourcePinsView>()),
                ("StageAuthorReview", one::<wns_story::reviewed_story::StageAuthorReview>()),
                ("StartDiscussion", one::<wns_story::discussion_vocabulary::StartDiscussion>()),
                ("StartMemory", one::<wns_story::memory::StartMemory>()),
                ("WorkshopExploration", one::<wns_story::workshop_metadata::WorkshopExploration>()),
                ("MemoryView", one::<wns_story::memory::MemoryView>()),
                ("ReviewState", one::<wns_story::reviewed_story::ReviewState>()),
                ("SummaryChange", one::<wns_context::reviewed_summary::SummaryChange>()),
            ],
        )
    }
}
