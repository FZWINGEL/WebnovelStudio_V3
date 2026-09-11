//! One saved request, one Codex invocation, then a retryable local settlement.
#![cfg(windows)]
use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome};
use crate::provider_runtime::DesktopProviders;
use std::time::Duration;
use webnovel_core::context::packet::{ProviderBinding, packet_input_hash, serialized_input};
use webnovel_core::projects::{ProjectSession, discussions::*};
use webnovel_core::providers::{
    cli::windows_process::StopSignal,
    codex_runner::{CodexRunResult, CodexRunStatus, CodexStreamEvent},
    codex_runtime::CodexConnection,
};

pub fn run_live(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    connection: Option<CodexConnection>,
    dispatch: DiscussionDispatch,
    stop: StopSignal,
) {
    if dispatch.run.lookup.is_some() {
        crate::lookup_discussion::run_live(project, recovery, runtime, connection, dispatch, stop);
        return;
    }
    let owner = dispatch.run.owner.clone();
    let _registration = Registration {
        runtime,
        owner: owner.clone(),
    };
    let mut run = dispatch.run;
    let binding = match dispatch.packet.options.provider_binding.clone() {
        Some(binding)
            if binding.is_current_codex_profile()
                && connection.as_ref().is_some_and(|connection| {
                    crate::provider_runtime::connection_matches_binding(connection, &binding)
                }) =>
        {
            binding
        }
        _ => {
            save_failure(
                &project,
                &recovery,
                run,
                "The request's saved model settings could not be validated.",
            );
            return;
        }
    };
    let input = match serialized_input(&dispatch.packet.messages, &dispatch.packet.options) {
        Ok(input)
            if packet_input_hash(&dispatch.packet.messages, &dispatch.packet.options)
                .ok()
                .as_ref()
                == Some(&dispatch.packet.receipt.input_hash) =>
        {
            input
        }
        _ => {
            save_failure(
                &project,
                &recovery,
                run,
                "The exact saved request could not be validated.",
            );
            return;
        }
    };
    // Recheck the owner before any external work. A Stop can arrive between
    // claim and worker startup, including before the Stop registry was read.
    match project.read_discussion_run(owner.clone()) {
        Ok(current) => {
            if current.status == DiscussionRunStatus::Stopping {
                stop.request_stop();
            }
            run = current;
        }
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                run,
                "The request's saved state could not be checked before starting.",
            );
            return;
        }
    }
    if !matches!(
        run.status,
        DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    ) {
        return;
    }
    if stop.is_requested() {
        let mut result = failed();
        result.status = CodexRunStatus::Stopped;
        save(&project, &recovery, run, result);
        return;
    }
    let Some(connection) = connection else {
        save_failure(
            &project,
            &recovery,
            run,
            "The Codex connection is unavailable. Check Settings before trying again.",
        );
        return;
    };
    let mut stream = match connection.start_bound(&binding, input.into_bytes(), stop.clone()) {
        Ok(stream) => stream,
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                run,
                "Codex could not start this request. Check its connection in Settings.",
            );
            return;
        }
    };
    let output_limit = binding.output_limit().expect("trusted fixed binding");
    let mut observed = String::new();
    let mut local_write_failed = false;
    let mut output_limited = false;
    loop {
        match stream.next_event(Duration::from_millis(100)) {
            Ok(None) => {}
            Ok(Some(CodexStreamEvent::AssistantDelta(delta))) => {
                if observed.len().saturating_add(delta.len()) > output_limit {
                    output_limited = true;
                    stop.request_stop();
                }
                let remaining = output_limit.saturating_sub(observed.len());
                let chunk = prefix(&delta, remaining);
                observed.push_str(chunk);
                if chunk.is_empty() || local_write_failed || stop.is_requested() {
                    continue;
                }
                match project.append_discussion_output(DiscussionOutputAppend {
                    owner: owner.clone(),
                    expected_sequence: run.sequence.clone(),
                    event_id: format!("{}-part-{}", owner.run_id, run.sequence),
                    chunk: chunk.into(),
                }) {
                    Ok(updated) => run = updated,
                    Err(error) => {
                        local_write_failed = error.code != "RunStopping";
                        stop.request_stop();
                    }
                }
            }
            Ok(Some(CodexStreamEvent::Finished(mut result))) => {
                if !result.assistant_text.starts_with(&observed) {
                    result.status = CodexRunStatus::ProtocolFailure(
                        webnovel_core::providers::codex_exec::CodexFailureCode::Protocol,
                    );
                    result.assistant_text = observed;
                }
                if output_limited || result.assistant_text.len() > output_limit {
                    result.status = CodexRunStatus::OutputLimit;
                    result.assistant_text = prefix(&result.assistant_text, output_limit).into();
                }
                // A local append error causes Stop, but may have committed its
                // exact chunk. The terminal transaction checks the saved prefix.
                if local_write_failed && result.status == CodexRunStatus::Stopped {
                    result.status = CodexRunStatus::ProtocolFailure(
                        webnovel_core::providers::codex_exec::CodexFailureCode::Incomplete,
                    );
                }
                if local_write_failed {
                    // Only an explicit local retry may commit output after a
                    // failed/uncertain chunk write. The model is already stopped.
                    recovery.retain(PendingSave {
                        outcome: SaveOutcome::Provider(Box::new(report(&run, result))),
                        run,
                    });
                } else {
                    save(&project, &recovery, run, result);
                }
                return;
            }
            Err(_) => {
                stop.request_stop();
                let mut result = failed();
                result.status = CodexRunStatus::CleanupUnresolved;
                result.cleanup_settled = false;
                result.assistant_text = observed;
                save(&project, &recovery, run, result);
                return;
            }
        }
    }
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
fn failed() -> CodexRunResult {
    CodexRunResult {
        status: CodexRunStatus::ProcessUnavailable,
        assistant_text: String::new(),
        usage: None,
        confirmed_stdin_bytes: 0,
        warning_count: 0,
        cleanup_settled: true,
    }
}
pub(super) fn report(run: &DiscussionRun, result: CodexRunResult) -> ProviderTerminalReport {
    let (status, error) = match result.status {
        CodexRunStatus::Completed => (ProviderOutcomeStatus::Completed, None),
        CodexRunStatus::Stopped => (ProviderOutcomeStatus::Stopped, None),
        CodexRunStatus::TimedOut => (
            ProviderOutcomeStatus::TimedOut,
            Some("The request timed out before the response finished."),
        ),
        CodexRunStatus::OutputLimit => (
            ProviderOutcomeStatus::OutputLimit,
            Some("The response reached the app's output limit."),
        ),
        CodexRunStatus::ProtocolFailure(_) => (
            ProviderOutcomeStatus::Failed,
            Some("The provider returned an incomplete or unsupported response."),
        ),
        CodexRunStatus::ConsumerTooSlow => (
            ProviderOutcomeStatus::Failed,
            Some("The response stopped because progress could not be processed quickly enough."),
        ),
        CodexRunStatus::ProcessUnavailable => (
            ProviderOutcomeStatus::Failed,
            Some("Codex could not start this request. Check its connection in Settings."),
        ),
        CodexRunStatus::CleanupUnresolved => (
            ProviderOutcomeStatus::Failed,
            Some("Local process cleanup could not be confirmed."),
        ),
    };
    ProviderTerminalReport {
        app_server: None,
        owner: run.owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-provider-finish", run.id),
        assistant_text: result.assistant_text,
        binding: run
            .provider_binding
            .clone()
            .unwrap_or_else(ProviderBinding::codex_luna),
        status,
        confirmed_stdin_bytes: result.confirmed_stdin_bytes.to_string(),
        usage: result.usage.map(|usage| ProviderUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_write_input_tokens: usage.cache_write_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        }),
        cleanup: if result.cleanup_settled {
            ProviderCleanup::Settled
        } else {
            ProviderCleanup::Unresolved
        },
        error: error.map(str::to_owned),
        effective_identity: None,
        reported_model: None,
        delivery: None,
    }
}
fn save(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    result: CodexRunResult,
) {
    save_report(project, recovery, run.clone(), report(&run, result));
}
pub(super) fn save_report(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    report: ProviderTerminalReport,
) {
    let mut pending = PendingSave {
        outcome: SaveOutcome::Provider(Box::new(report)),
        run,
    };
    // Reading this owner cannot redirect a late result after project switching.
    if let Ok(current) = project.read_discussion_run(pending.run.owner.clone()) {
        pending.run = current;
    }
    if pending.attempt(project, &pending.run).is_err() {
        recovery.retain(pending);
    }
}
fn save_failure(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    detail: &'static str,
) {
    let mut report = report(&run, failed());
    report.error = Some(detail.into());
    save_report(project, recovery, run, report);
}
pub fn worker_unavailable(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
) {
    save_failure(
        project,
        recovery,
        run,
        "The response worker could not start. No model request was sent.",
    );
}
struct Registration {
    runtime: DesktopProviders,
    owner: RunOwner,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.runtime.release(&self.owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::projects::{CreateDocument, ProjectAccess};

    fn started(label: &str) -> (ProjectSession, ProjectAccess, DiscussionDispatch) {
        let root = std::env::temp_dir().join(format!(
            "wns-live-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(root, "Live worker fixture").unwrap();
        let access = project.documents().attach("live-test".into()).unwrap();
        let document = project.documents().create(CreateDocument { access:access.clone(), operation_id:"create".into(), document_id:"chapter".into(), title:"Chapter".into(), kind:"chapter".into(),
            body:serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The ending stays."}]}]}}) }).unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                lookup: None,
                access: access.clone(),
                operation_id: "discuss".into(),
                expected: document.head,
                instruction: "Discuss the promise.".into(),
                intent: FeedbackIntent::Discuss,
                basis: None,
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("1", "0", "0"),
                provider_binding: Some(ProviderBinding::codex_luna_runtime(
                    "0.154.0",
                    &"a".repeat(64),
                )),
                previous_run_id: None,
            })
            .unwrap();
        let dispatch = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner,
            })
            .unwrap();
        (project, access, dispatch)
    }
    fn completed(dispatch: &DiscussionDispatch) -> CodexRunResult {
        CodexRunResult {
            status: CodexRunStatus::Completed,
            assistant_text: "The promise matters.".into(),
            usage: Some(webnovel_core::providers::codex_exec::CodexUsage {
                input_tokens: 100,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 9,
                reasoning_output_tokens: 3,
            }),
            confirmed_stdin_bytes: serialized_input(
                &dispatch.packet.messages,
                &dispatch.packet.options,
            )
            .unwrap()
            .len(),
            warning_count: 0,
            cleanup_settled: true,
        }
    }

    #[test]
    fn local_retry_reconciles_a_saved_chunk_and_commits_one_provider_receipt() {
        let (project, access, dispatch) = started("retry");
        let before = project.documents().read(access.clone(), "chapter".into()).unwrap();
        // The append committed but its caller retained the older sequence.
        project
            .append_discussion_output(DiscussionOutputAppend {
                owner: dispatch.run.owner.clone(),
                expected_sequence: dispatch.run.sequence.clone(),
                event_id: "chunk".into(),
                chunk: "The promise".into(),
            })
            .unwrap();
        let recovery = DiscussionRecovery::default();
        recovery.retain(PendingSave {
            run: dispatch.run.clone(),
            outcome: SaveOutcome::Provider(Box::new(report(&dispatch.run, completed(&dispatch)))),
        });
        recovery
            .retry(
                &project,
                access.clone(),
                "chapter".into(),
                dispatch.run.id.clone(),
            )
            .unwrap();
        let run = project
            .read_discussion_run(dispatch.run.owner.clone())
            .unwrap();
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        assert_eq!(run.output_text, "The promise matters.");
        assert_eq!(
            run.provider_result
                .as_ref()
                .unwrap()
                .usage
                .as_ref()
                .unwrap()
                .output_tokens,
            9
        );
        recovery
            .retry(&project, access.clone(), "chapter".into(), run.id)
            .unwrap();
        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(view.runs.len(), 1);
        assert_eq!(view.messages.len(), 2);
        assert_eq!(
            project.documents().read(access, "chapter".into()).unwrap().body,
            before.body
        );
    }

    #[test]
    fn pending_provider_cleanup_is_not_relabelled_as_a_clean_mock_stop() {
        let (project, access, dispatch) = started("unresolved");
        let stopped = project
            .stop_discussion(access, dispatch.run.id.clone())
            .unwrap();
        let mut result = completed(&dispatch);
        result.status = CodexRunStatus::CleanupUnresolved;
        result.cleanup_settled = false;
        result.usage = None;
        let pending = PendingSave {
            run: stopped.run.clone(),
            outcome: SaveOutcome::Provider(Box::new(report(&dispatch.run, result))),
        };
        pending.attempt(&project, &stopped.run).unwrap();
        let run = project.read_discussion_run(dispatch.run.owner).unwrap();
        assert_eq!(run.status, DiscussionRunStatus::Interrupted);
        assert_eq!(
            run.provider_result.unwrap().cleanup,
            ProviderCleanup::Unresolved
        );
    }
    #[test]
    fn byte_caps_never_split_unicode_or_invent_usage() {
        assert_eq!(prefix("Aé🌙", 2), "A");
        assert_eq!(prefix("Aé🌙", 6), "Aé");
        let result = failed();
        assert_eq!(result.usage, None);
        assert_eq!(result.confirmed_stdin_bytes, 0);
    }
}
