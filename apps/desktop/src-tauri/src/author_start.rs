//! Starting an author request through the native dispatch lifecycle.
//!
//! Split out of `discussion_commands`, which was 2,232 lines: the twelve Tauri
//! commands are a thin surface, and what filled the file was this — the typed
//! acceptance enum, the shared dispatch, the mock path and the outcome
//! recording. `http_discussion` and `project_chat_commands` start author
//! requests through here too, so it was never the discussion module's private
//! machinery.

// Defined with the commands it serves; the dispatch below is its only caller.
use crate::discussion_commands::dispatch_started;
use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome};
use crate::library_commands::DesktopLibrary;
use crate::provider_runtime::{DesktopProviders, binding_matches_author_choice};
use webnovel_core::context::packet::{CompiledPacket, MOCK_MODEL_ID, ProviderBinding, packet_input_hash};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::project_chat::{StartProjectChapter, StartProjectChat};
use webnovel_core::projects::workshop_generation::StartWorkshop;
use webnovel_core::projects::project_chat_output::{
    CHAPTER_DISCUSSION_RESPONSE_CONTRACT, CHAPTER_TARGET_HEAD_MARKER,
};
use webnovel_core::projects::{CoreError, CoreResult, Head, ProjectSession};
#[cfg(windows)]
use webnovel_core::providers::codex_app_server::is_app_server;
#[cfg(windows)]
use webnovel_core::library::codex_transport::CodexTransport;
use webnovel_core::providers::preferences::ModelSelection;

/// Typed acceptance into the same native dispatch lifecycle. The enum changes
/// the core command only; transport, admission, ownership and Stop are shared.
pub(crate) enum AuthorStart {
    Workshop(StartWorkshop),
    ProjectChat(StartProjectChat),
    ProjectChapter(StartProjectChapter),
}
impl AuthorStart {
    pub(crate) fn set_binding(&mut self, binding: Option<ProviderBinding>) {
        match self {
            Self::Workshop(r) => r.provider_binding = binding,
            Self::ProjectChat(r) => r.provider_binding = binding,
            Self::ProjectChapter(r) => r.provider_binding = binding,
        }
    }
    pub(crate) fn saved(&self, project: &ProjectSession) -> CoreResult<Option<DiscussionRun>> {
        match self {
            Self::Workshop(r) => saved_workshop_request(project, r),
            Self::ProjectChat(r) => project.find_project_chat_request(
                r.access.clone(),
                r.conversation_id.clone(),
                r.operation_id.clone(),
            ),
            Self::ProjectChapter(r) => project.find_project_chapter_request(
                r.access.clone(),
                r.conversation_id.clone(),
                r.operation_id.clone(),
            ),
        }
    }
    pub(crate) fn accept(self, project: &ProjectSession) -> CoreResult<DiscussionStart> {
        match self {
            Self::Workshop(r) => project.workshop().start(r),
            Self::ProjectChat(r) => project.start_project_chat(r),
            Self::ProjectChapter(r) => project.start_project_chapter(r),
        }
    }
    pub(crate) fn intent(&self) -> FeedbackIntent {
        match self {
            Self::Workshop(_) => FeedbackIntent::WorkshopExplore,
            Self::ProjectChat(_) => FeedbackIntent::Discuss,
            Self::ProjectChapter(request) => request
                .composer
                .chapter
                .as_ref()
                .map(|chapter| chapter.intent)
                .unwrap_or(FeedbackIntent::Discuss),
        }
    }
}

pub(crate) fn start_author_native(
    mut request: AuthorStart,
    selected: ModelSelection,
    project: ProjectSession,
    recovery: DiscussionRecovery,
    library: DesktopLibrary,
    runtime: DesktopProviders,
) -> CoreResult<DiscussionStart> {
    let _admission = runtime.admit_request()?;
    let saved = request.saved(&project)?;

    // Resolve a lost-acknowledgment retry through the durable actor receipt
    // before touching provider connections. Terminal runs return immediately;
    // queued runs are dispatched only when their original native provider can
    // be reacquired. recovery.claim remains the ownership gate, while an
    // unavailable provider leaves the durable queue for a later retry.
    if let Some(saved_run) = saved.as_ref() {
        request.set_binding(saved_run.provider_binding.clone());
        let started = request.accept(&project)?;
        if started.run.status != DiscussionRunStatus::Queued {
            return Ok(started);
        }
        #[cfg(windows)]
        {
            match started
                .packet
                .options
                .provider_binding
                .as_ref()
                .map(|binding| binding.provider_id.as_str())
            {
                Some("codex") => {
                    let connection = runtime.connection().ok();
                    let app_server = started
                        .packet
                        .options
                        .provider_binding
                        .as_ref()
                        .filter(|binding| is_app_server(binding))
                        .map(|binding| runtime.app_server()?.reserve(binding))
                        .transpose()?;
                    if connection.is_none() {
                        return Ok(started);
                    }
                    return dispatch_started(
                        project, recovery, runtime, started, connection, None, app_server,
                    );
                }
                Some("claude") => {
                    let claude_connection = runtime.claude_connection().ok();
                    if claude_connection.is_none() {
                        return Ok(started);
                    }
                    return dispatch_started(
                        project,
                        recovery,
                        runtime,
                        started,
                        None,
                        claude_connection,
                        None,
                    );
                }
                // A saved HTTP request belongs to the HTTP command; an
                // unknown native binding must never be dispatched here.
                Some(_) => return Ok(started),
                None => return dispatch_mock(project, recovery, runtime, started),
            }
        }
        #[cfg(not(windows))]
        {
            if started.packet.options.provider_binding.is_some() {
                return Ok(started);
            }
            return dispatch_mock(project, recovery, runtime, started);
        }
    }

    let library_guard = library
        .0
        .lock()
        .map_err(|_| crate::provider_commands::unavailable())?;
    {
        let active = library_guard.provider_state()?.settings.active;
        if active != selected {
            return Err(CoreError::new(
                "ModelChoiceChanged",
                "The selected model changed before this request started. Check the model selector and send again.",
            ));
        }
    }
    #[cfg(windows)]
    let mut app_server_request = None;
    #[cfg(windows)]
    let connection = if selected.provider_id == "codex" {
        Some(runtime.connection().map_err(|_| {
            CoreError::new(
                "ProviderUnavailable",
                "Check the Codex connection in Settings before sending this request.",
            )
        })?)
    } else {
        None
    };
    #[cfg(windows)]
    let claude_connection = if selected.provider_id == "claude" {
        Some(runtime.claude_connection().map_err(|_| {
            CoreError::new(
                "ProviderUnavailable",
                "Check the Claude Code connection in Settings before sending this request.",
            )
        })?)
    } else {
        None
    };
    let binding = if selected.provider_id == "mock" {
        None
    } else if selected.provider_id == "codex" {
        #[cfg(windows)]
        {
            if library_guard.codex_transport_settings()?.transport == CodexTransport::AppServer {
                let server = runtime.app_server()?;
                let binding = server.author_binding(&selected)?;
                app_server_request = Some(server.reserve(&binding)?);
                Some(binding)
            } else {
                Some(crate::provider_runtime::connection_author_binding(
                    connection.as_ref().expect("Codex connection selected"),
                    &selected,
                )?)
            }
        }
        #[cfg(not(windows))]
        {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "The Codex connection is currently available on Windows only.",
            ));
        }
    } else if selected.provider_id == "claude" {
        #[cfg(windows)]
        {
            Some(crate::provider_runtime::claude_binding_for_choice(
                claude_connection
                    .as_ref()
                    .expect("Claude connection selected"),
                &selected,
            )?)
        }
        #[cfg(not(windows))]
        {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "The Claude Code connection is currently available on Windows only.",
            ));
        }
    } else {
        return Err(CoreError::new(
            "ProviderUnavailable",
            "This provider is unavailable. Check Settings before sending.",
        ));
    };
    request.set_binding(binding);
    let started = request.accept(&project)?;
    drop(library_guard);
    #[cfg(windows)]
    {
        dispatch_started(
            project,
            recovery,
            runtime,
            started,
            connection,
            claude_connection,
            app_server_request,
        )
    }
    #[cfg(not(windows))]
    {
        dispatch_started(project, recovery, runtime, started)
    }
}

#[cfg(windows)]
pub(crate) fn app_server_lookup_unavailable() -> CoreError {
    CoreError::new(
        "UnsupportedProviderFeature",
        "Story lookup still requires the Exec transport to preserve its invocation allowance. Select Exec in Settings before sending a lookup request.",
    )
}

pub(crate) fn saved_workshop_request(
    project: &ProjectSession,
    request: &StartWorkshop,
) -> CoreResult<Option<DiscussionRun>> {
    let anchor_id = format!("workshop-{}", request.exploration.session_id);
    match project.read_discussion(request.access.clone(), anchor_id) {
        Ok(view) => Ok(view
            .runs
            .into_iter()
            .find(|run| run.operation_id == request.operation_id)),
        Err(error) if error.code == "DocumentNotFound" => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
pub(crate) fn dispatch_started(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    started: DiscussionStart,
) -> CoreResult<DiscussionStart> {
    dispatch_mock(project, recovery, runtime, started)
}

pub(crate) fn dispatch_mock(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    started: DiscussionStart,
) -> CoreResult<DiscussionStart> {
    if started.run.status == DiscussionRunStatus::Queued {
        let worker_registration = runtime.track_local_worker()?;
        // A duplicate lost-ack retry can reach here. Only one worker can
        // claim the durable queued run before spawning its local worker.
        if let Some(dispatch) = recovery.claim(&project, &started.run) {
            let failure_project = project.clone();
            let failure_run = dispatch.run.clone();
            let worker_recovery = recovery.clone();
            if std::thread::Builder::new()
                .name("webnovel-test-response".into())
                .spawn(move || {
                    let _worker_registration = worker_registration;
                    run_mock(project, worker_recovery, dispatch);
                })
                .is_err()
            {
                record_worker_failure(
                    &failure_project,
                    &recovery,
                    &failure_run,
                    CoreError::new(
                        "WorkerUnavailable",
                        "The local test worker could not start.",
                    ),
                );
            }
        }
    }
    Ok(started)
}

pub(crate) fn check_model_choice(
    project: &ProjectSession,
    request: &StartDiscussion,
    requested: Option<&ModelSelection>,
    active: &ModelSelection,
) -> CoreResult<()> {
    let local = ModelSelection::local_mock();
    // Older native callers have an explicit mock-only packet contract.
    let requested = requested.unwrap_or(&local);
    let saved = has_saved_request(project, request)?;
    if requested != &local
        && !(request.provider_binding.as_ref().is_some_and(|binding| {
            binding_matches_author_choice(binding, requested)
                || (saved
                    && crate::provider_runtime::binding_matches_saved_model(binding, requested))
        }))
    {
        return Err(CoreError::new(
            "ProviderUnavailable",
            "This model or its selected settings are unavailable. Check Settings before sending.",
        ));
    }
    if active == requested {
        return Ok(());
    }
    // An old uncertain acknowledgment must still be resolvable after a model
    // preference change. Core verifies the immutable operation payload before
    // returning its receipt; a new operation cannot use this exception.
    if saved {
        return Ok(());
    }
    Err(CoreError::new(
        "ModelChoiceChanged",
        "The selected model changed before this request started. Check the model selector and send again.",
    ))
}

pub(crate) fn has_saved_request(project: &ProjectSession, request: &StartDiscussion) -> CoreResult<bool> {
    Ok(saved_request(project, request)?.is_some())
}

pub(super) fn saved_request(
    project: &ProjectSession,
    request: &StartDiscussion,
) -> CoreResult<Option<DiscussionRun>> {
    Ok(project
        .read_discussion(request.access.clone(), request.expected.document_id.clone())?
        .runs
        .into_iter()
        .find(|run| {
            run.operation_id == request.operation_id
                && run.owner.operation_namespace == request.access.operation_namespace
                && run.owner.project_id == request.access.project_id
        }))
}

pub(crate) fn run_mock(project: ProjectSession, recovery: DiscussionRecovery, dispatch: DiscussionDispatch) {
    if dispatch.run.lookup.is_some() {
        crate::lookup_discussion::run_mock(project, recovery, dispatch);
        return;
    }
    run_mock_with_pause(project, recovery, dispatch, || {
        std::thread::sleep(std::time::Duration::from_millis(150));
    });
}

pub(crate) fn run_mock_with_pause(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    dispatch: DiscussionDispatch,
    mut pause: impl FnMut(),
) {
    let owner = dispatch.run.owner.clone();
    let chunks = match mock_output(&dispatch.packet, dispatch.run.intent) {
        Ok(chunks) => chunks,
        Err(error) => {
            record_worker_failure(&project, &recovery, &dispatch.run, error);
            return;
        }
    };
    let mut run = match project.mark_discussion_delivered(owner.clone()) {
        Ok(run) => run,
        Err(error) => {
            record_worker_failure(&project, &recovery, &dispatch.run, error);
            return;
        }
    };
    for chunk in &chunks {
        pause();
        match project.append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: run.sequence.clone(),
            event_id: format!("{}-part-{}", owner.run_id, run.sequence),
            chunk: chunk.clone(),
        }) {
            Ok(updated) => run = updated,
            Err(error) => {
                record_worker_failure(&project, &recovery, &run, error);
                return;
            }
        }
    }
    let event_id = format!("{}-finish", owner.run_id);
    let finish = DiscussionFinish {
        owner: owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id,
        assistant_text: chunks.concat(),
    };
    save_worker_outcome(
        &project,
        &recovery,
        PendingSave {
            run,
            outcome: SaveOutcome::Complete(finish),
        },
    );
}

pub(crate) fn record_worker_failure(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: &DiscussionRun,
    error: CoreError,
) {
    if error.code == "RunStopping" {
        let mut current = run.clone();
        current.status = DiscussionRunStatus::Stopping;
        save_worker_outcome(
            project,
            recovery,
            PendingSave {
                run: current,
                outcome: SaveOutcome::Stop,
            },
        );
        return;
    }
    // An uncertain commit remains fenced by the core; never replay output or
    // bypass reconciliation. A completed terminal record is immutable.
    if error.code == "RunSealed" {
        return;
    }
    save_worker_outcome(
        project,
        recovery,
        PendingSave {
            run: run.clone(),
            outcome: SaveOutcome::Fail,
        },
    );
}

pub(crate) fn save_worker_outcome(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    mut pending: PendingSave,
) {
    // The mock owns no external process or asynchronous I/O. All its output
    // work is finished here; only then may the actor acknowledge cleanup.
    if let Err(error) = pending.attempt(project, &pending.run) {
        if error.code == "RunSealed" {
            return;
        }
        if error.code == "RunStopping" {
            pending.run.status = DiscussionRunStatus::Stopping;
            pending.outcome = SaveOutcome::Stop;
            if pending.attempt(project, &pending.run).is_ok() {
                return;
            }
        }
        recovery.retain(pending);
    }
}

/// Fixed local fixtures, deliberately labelled; no external model is invoked.
pub(crate) fn mock_output(packet: &CompiledPacket, intent: FeedbackIntent) -> CoreResult<Vec<String>> {
    let invalid = || {
        CoreError::new(
            "InvalidMockInput",
            "The local test request failed its exact-input check.",
        )
    };
    if packet.options.model_id != MOCK_MODEL_ID
        || packet_input_hash(&packet.messages, &packet.options).map_err(|_| invalid())?
            != packet.receipt.input_hash
    {
        return Err(invalid());
    }
    if intent == FeedbackIntent::Continue {
        return Ok(vec![serde_json::json!({
            "schemaVersion": "continuation-output.v1",
            "suggestions": [{
                "title": "Local test continuation",
                "paragraphs": ["A knock broke the silence. She folded the letter, keeping its final line hidden beneath her thumb.", "Beyond the door, someone was waiting for an answer."],
                "explanation": "Fixed local test paragraphs. Review or edit them before applying; no live AI model was called."
            }]
        }).to_string()]);
    }
    let full_instruction = packet.messages.last().ok_or_else(invalid)?.content.clone();
    let instruction = full_instruction.chars().take(220).collect::<String>();
    let envelope: serde_json::Value =
        serde_json::from_str(&packet.messages.get(1).ok_or_else(invalid)?.content)
            .map_err(|_| invalid())?;
    let allowance = packet
        .options
        .max_output_tokens
        .parse::<usize>()
        .map_err(|_| invalid())?;
    let chapter_discussion_contract = packet.messages.first().is_some_and(|message| {
        message
            .content
            .contains(CHAPTER_DISCUSSION_RESPONSE_CONTRACT)
    });
    if chapter_discussion_contract {
        let target_head = full_instruction
            .split(CHAPTER_TARGET_HEAD_MARKER)
            .nth(1)
            .and_then(|tail| tail.lines().find(|line| !line.trim().is_empty()))
            .ok_or_else(invalid)
            .and_then(|json| serde_json::from_str::<Head>(json.trim()).map_err(|_| invalid()))?;
        let target = envelope.get("target").ok_or_else(invalid)?;
        let range = first_nonempty_target_block(target);
        let output = serde_json::json!({
            "schemaVersion": CHAPTER_DISCUSSION_RESPONSE_CONTRACT,
            "answer": "The local test response identifies the first nonempty chapter paragraph for review; no live AI model was called.",
            "rangeProposal": range.map(|(block_id, quote)| serde_json::json!({
                "sourceHead": target_head,
                "firstBlockId": block_id,
                "lastBlockId": block_id,
                "quote": quote,
            })),
        });
        let encoded = serde_json::to_string(&output).map_err(|_| invalid())?;
        if encoded.len() > allowance {
            return Err(CoreError::new(
                "OutputBudgetTooSmall",
                "The reserved response allowance is too small for the local chapter discussion response.",
            ));
        }
        return Ok(vec![encoded]);
    }
    if envelope.get("projectChat").is_some() {
        if full_instruction
            .to_ascii_lowercase()
            .contains("write the first chapter")
        {
            return Ok(vec![serde_json::json!({
                "schemaVersion":"project-assistant-output.v1",
                "answer":"The local test model proposes a chapter-writing task. Choose its destination and approve the brief before sending it.",
                "questions":[], "assumptions":[], "drafts":[],
                "chapterHandoff": {"targetHandle":null,"proposedTitle":"The Cloud Bridge","instruction":"Write the first chapter at the cloud bridge.","brief":"Keep the ending hopeful. The healer is choosing whether to cross the bridge."}
            }).to_string()]);
        }
        return Ok(vec![serde_json::json!({
            "schemaVersion":"project-assistant-output.v1",
            "answer":"I have prepared two isolated planning drafts from your idea. Open either draft to edit it, or review them together. This is a fixed local test response; no live model was called.",
            "questions":[{"key":"central-choice","text":"What choice should put the protagonist under the most pressure? You can leave this open and continue with another idea."}],
            "assumptions":[],
            "drafts":[
                {"key":"foundation","title":"Story foundation","kind":"world","changeSummary":"A starting point for the setting, pending your review.","blocks":[{"type":"paragraph","content":[{"type":"text","text":format!("Local test draft based on your request: {instruction}")}]}]},
                {"key":"protagonist","title":"Main character","kind":"character","changeSummary":"A provisional character direction, pending your review.","blocks":[{"type":"paragraph","content":[{"type":"text","text":"The protagonist wants a quiet life, but a promise draws them into the conflict. Their motives and history are still open for discussion."}]}]}
            ]
        }).to_string()]);
    }
    let focus = envelope
        .get("scope")
        .and_then(|scope| scope.get("quote"))
        .and_then(|quote| quote.as_str())
        .map(|quote| {
            format!(
                "Selected passage: “{}”\n\n",
                quote.chars().take(180).collect::<String>()
            )
        })
        .unwrap_or_else(|| "Focus: the whole document.\n\n".into());
    let structured_scope = envelope
        .get("scope")
        .and_then(|scope| scope.get("kind"))
        .and_then(|kind| kind.as_str())
        .is_some_and(|kind| matches!(kind, "blocks" | "wholeDocument"));
    if intent == FeedbackIntent::WorkshopExplore {
        let workshop = envelope.get("workshop").ok_or_else(invalid)?;
        let request: serde_json::Value =
            serde_json::from_str(&full_instruction).map_err(|_| invalid())?;
        let action = request
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("directions");
        let kind = if matches!(
            action,
            "directions" | "explore" | "findDirection" | "findDirections"
        ) {
            "directions"
        } else {
            "refinement"
        };
        let is_voice_guidance = action == "voiceGuidance";
        let selected_text = request
            .get("selectedText")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let current = workshop
            .get("currentElement")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the current story element");
        let mut preserved = vec![current.to_owned()];
        if !selected_text.trim().is_empty() {
            preserved.push(selected_text.to_owned());
        }
        if let Some(details) = workshop
            .get("selectedDetails")
            .and_then(serde_json::Value::as_array)
        {
            preserved.extend(details.iter().filter_map(|detail| {
                detail
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            }));
        }
        preserved.sort();
        preserved.dedup();
        let candidates = (0..if kind == "directions" || is_voice_guidance || action == "moment" {
            3
        } else {
            1
        })
            .map(|index| {
                let dimension = if is_voice_guidance {
                    ["restrained", "brisk", "lyrical"][index].to_owned()
                } else {
                    format!("Local test axis {}", index + 1)
                };
                let (title, content, preserved_details, changed_details) =
                    if is_voice_guidance {
                        let style = dimension.as_str();
                        (
                            format!("Local STYLE guidance set {}", index + 1),
                            format!(
                                "STYLE guidance set {}.\n\
                                 Sentence density: use {} sentence lengths.\n\
                                 Viewpoint distance: stay close to the author's perception.\n\
                                 Humor: use restrained warmth only when it supports the scene.\n\
                                 Exposition: reveal context through selected detail, not event claims.\n\
                                 Dialogue rhythm: let turns breathe and vary interruption.\n\
                                 Events from the sample remain noncanon and are not adopted.",
                                index + 1,
                                style
                            ),
                            vec!["The sample remains voice evidence only.".to_owned()],
                            vec![format!("{} style qualities", style)],
                        )
                    } else {
                        (
                            format!("Local workshop direction {}", index + 1),
                            format!(
                                "{}\n{}\nThis is a deterministic workshop alternative for axis {}.",
                                current,
                                preserved.join("\n"),
                                index + 1
                            ),
                            preserved.clone(),
                            vec![format!("Local test axis {}", index + 1)],
                        )
                    };
                serde_json::json!({
                    "id": "",
                    "title": title,
                    "content": content,
                    "dimensionValue": dimension,
                    "implications": [{"text": "This could change a related decision.", "basis": "the selected workshop material", "assumption": "the author wants the change explored"}],
                    "assumptions": ["This is a proposed alternative, not canon."],
                    "affectedTargets": [],
                    "preservedDetails": preserved_details,
                    "changedDetails": changed_details
                })
            })
            .collect::<Vec<_>>();
        let output = serde_json::json!({
            "schemaVersion": "story-workshop-output.v1",
            "requestKind": kind,
            "question": workshop.get("focusQuestion").and_then(serde_json::Value::as_str).unwrap_or("What should we explore next?"),
            "questionReason": workshop.get("focusReason").and_then(serde_json::Value::as_str).unwrap_or("The local fixture preserves the current focus."),
            "dimension": "Local test axis",
            "interpretation": {
                "youSaid": workshop.get("direction").and_then(serde_json::Value::as_str).unwrap_or_default(),
                "possibleDirection": if is_voice_guidance {
                    "Review style qualities only; sample events remain untouched."
                } else {
                    current
                },
                "stillOpen": workshop.get("stillOpen").and_then(serde_json::Value::as_str).unwrap_or_default()
            },
            "candidates": candidates
        });
        let encoded = serde_json::to_string(&output).map_err(|_| invalid())?;
        if encoded.len() > allowance {
            return Err(CoreError::new(
                "OutputBudgetTooSmall",
                "The reserved response allowance is too small for the local workshop response.",
            ));
        }
        return Ok(vec![encoded]);
    }
    let chunks = vec![
        "Local test response — no live AI model is connected.\n\n".to_owned(),
        format!("{focus}Your request: {instruction}\n\n"),
        format!(
            "This request supplied {} story sources. Open Story context to inspect their exact saved versions and any omissions. This test confirms discussion and context handling; it does not evaluate or rewrite your story.",
            packet.receipt.source_handles.len()
        ),
    ];
    if intent == FeedbackIntent::ProposeEdits {
        if structured_scope {
            let output = serde_json::json!({
                "schemaVersion": "structured-proposal-output.v1",
                "suggestions": [{
                    "title": "Mock structured option",
                    "blocks": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "A clearer local test opening.", "marks": [{"type": "bold"}]}]},
                        {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "A structured local test beat."}]}
                    ],
                    "explanation": "Deterministic local block alternative; no live AI model was called."
                }]
            });
            let encoded = serde_json::to_string(&output).map_err(|_| invalid())?;
            if encoded.len() > allowance {
                return Err(CoreError::new(
                    "OutputBudgetTooSmall",
                    "The reserved response allowance is too small for the local test response.",
                ));
            }
            return Ok(vec![encoded]);
        }
        let output = serde_json::json!({
            "suggestions": [
                {
                    "title": "Mock clarity option",
                    "replacementText": "A clearer test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                },
                {
                    "title": "Mock focus option",
                    "replacementText": "A more focused test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                },
                {
                    "title": "Mock simplicity option",
                    "replacementText": "A simpler test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                }
            ]
        });
        let encoded = serde_json::to_string(&output).map_err(|_| invalid())?;
        if encoded.len() > allowance {
            return Err(CoreError::new(
                "OutputBudgetTooSmall",
                "The reserved response allowance is too small for the local test response.",
            ));
        }
        return Ok(vec![encoded]);
    }
    if chunks.iter().map(String::len).sum::<usize>() > allowance {
        return Err(CoreError::new(
            "OutputBudgetTooSmall",
            "The reserved response allowance is too small for the local test response.",
        ));
    }
    Ok(chunks)
}

pub(crate) fn first_nonempty_target_block(target: &serde_json::Value) -> Option<(String, String)> {
    let blocks = target
        .get("body")
        .and_then(|body| body.get("body"))
        .and_then(|body| body.get("content"))
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            target
                .get("body")
                .and_then(|body| body.get("content"))
                .and_then(serde_json::Value::as_array)
        })?;
    blocks.iter().find_map(|block| {
        let block_id = block
            .get("attrs")
            .and_then(|attrs| attrs.get("id"))
            .and_then(serde_json::Value::as_str)?;
        let content = block.get("content")?.as_array()?;
        let quote = content
            .iter()
            .filter_map(
                |inline| match inline.get("type").and_then(serde_json::Value::as_str) {
                    Some("text") => inline
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    Some("hardBreak") => Some("\n".to_owned()),
                    _ => None,
                },
            )
            .collect::<String>();
        (!quote.trim().is_empty()).then(|| (block_id.to_owned(), quote))
    })
}
