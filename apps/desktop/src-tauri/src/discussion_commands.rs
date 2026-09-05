//! Author commands use renderer leases; the local test worker owns only its run.
use crate::project_commands::{DesktopProjects, execute};
use tauri::State;
use webnovel_core::context::packet::{CompiledPacket, MOCK_MODEL_ID, packet_input_hash};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::{CoreError, CoreResult, ProjectAccess, ProjectSession};

#[tauri::command]
pub async fn read_discussion(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionView> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_discussion(access, document_id)).await
}

#[tauri::command]
pub async fn save_discussion_draft(
    request: SaveDiscussionDraft,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionDraft> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_discussion_draft(request)).await
}

#[tauri::command]
pub async fn stop_discussion(
    access: ProjectAccess,
    run_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionStop> {
    let project = state.project(&access.project_id)?;
    execute(move || project.stop_discussion(access, run_id)).await
}

#[tauri::command]
pub async fn start_discussion(
    request: StartDiscussion,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionStart> {
    let project = state.project(&request.access.project_id)?;
    execute(move || {
        let started = project.start_discussion(request)?;
        if started.run.status == DiscussionRunStatus::Queued {
            let owner = started.run.owner.clone();
            // A duplicate lost-ack retry can reach here. Only one worker can
            // claim the durable queued run; none may replay an existing claim.
            let failure_project = project.clone();
            let failure_owner = owner.clone();
            if std::thread::Builder::new()
                .name("webnovel-test-response".into())
                .spawn(move || run_mock(project, owner))
                .is_err()
            {
                let event_id = format!("{}-launch-failed", failure_owner.run_id);
                let _ = failure_project.fail_discussion_run(DiscussionFail {
                    owner: failure_owner,
                    expected_sequence: started.run.sequence.clone(),
                    event_id,
                    reason: "The local test worker could not start.".into(),
                });
            }
        }
        Ok(started)
    })
    .await
}

fn run_mock(project: ProjectSession, owner: RunOwner) {
    let dispatch = match project.begin_discussion_run(DiscussionBegin {
        owner: owner.clone(),
    }) {
        Ok(dispatch) => dispatch,
        Err(_) => return, // Already claimed, stopped, stale, or unavailable.
    };
    let chunks = match mock_output(&dispatch.packet) {
        Ok(chunks) => chunks,
        Err(error) => {
            let event_id = format!("{}-input-failed", owner.run_id);
            let _ = project.fail_discussion_run(DiscussionFail {
                owner,
                expected_sequence: dispatch.run.sequence,
                event_id,
                reason: error.detail,
            });
            return;
        }
    };
    let mut run = match project.mark_discussion_delivered(owner.clone()) {
        Ok(run) => run,
        Err(error) => {
            record_worker_failure(&project, &owner, &dispatch.run.sequence, error);
            return;
        }
    };
    for chunk in &chunks {
        std::thread::sleep(std::time::Duration::from_millis(150));
        match project.append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: run.sequence.clone(),
            event_id: format!("{}-part-{}", owner.run_id, run.sequence),
            chunk: chunk.clone(),
        }) {
            Ok(updated) => run = updated,
            Err(error) => {
                record_worker_failure(&project, &owner, &run.sequence, error);
                return;
            }
        }
    }
    let event_id = format!("{}-finish", owner.run_id);
    if let Err(error) = project.finish_discussion(DiscussionFinish {
        owner: owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id,
        assistant_text: chunks.concat(),
    }) {
        record_worker_failure(&project, &owner, &run.sequence, error);
    }
}

fn record_worker_failure(
    project: &ProjectSession,
    owner: &RunOwner,
    sequence: &str,
    error: CoreError,
) {
    // Stop already has a durable terminal outcome. An uncertain commit remains
    // fenced by the core; never replay the response or bypass reconciliation.
    if matches!(error.code.as_str(), "RunSealed" | "RunStopping") {
        return;
    }
    let _ = project.fail_discussion_run(DiscussionFail {
        owner: owner.clone(),
        expected_sequence: sequence.to_owned(),
        event_id: format!("{}-storage-failed-{sequence}", owner.run_id),
        reason: format!(
            "The local test response could not be saved: {}",
            error.detail
        ),
    });
}

/// A fixed local fixture, deliberately labelled and incapable of writing prose.
fn mock_output(packet: &CompiledPacket) -> CoreResult<Vec<String>> {
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
    let instruction = packet
        .messages
        .last()
        .ok_or_else(invalid)?
        .content
        .chars()
        .take(220)
        .collect::<String>();
    let envelope: serde_json::Value =
        serde_json::from_str(&packet.messages.get(1).ok_or_else(invalid)?.content)
            .map_err(|_| invalid())?;
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
    let chunks = vec![
        "Local test response — no live AI model is connected.\n\n".to_owned(),
        format!("{focus}Your request: {instruction}\n\n"),
        format!(
            "This request supplied {} story sources. Open Story context to inspect their exact saved versions and any omissions. This test confirms discussion and context handling; it does not evaluate or rewrite your story.",
            packet.receipt.source_handles.len()
        ),
    ];
    let allowance = packet
        .options
        .max_output_tokens
        .parse::<usize>()
        .map_err(|_| invalid())?;
    if chunks.iter().map(String::len).sum::<usize>() > allowance {
        return Err(CoreError::new(
            "OutputBudgetTooSmall",
            "The reserved response allowance is too small for the local test response.",
        ));
    }
    Ok(chunks)
}
