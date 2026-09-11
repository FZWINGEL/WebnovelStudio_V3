//! One saved Claude request, one contained invocation, then a retryable local
//! settlement.  This worker never retries provider work.
#![cfg(windows)]

use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome};
use crate::provider_runtime::DesktopProviders;
use std::time::Duration;
use webnovel_core::context::packet::{ProviderBinding, packet_input_hash, serialized_input};
use webnovel_core::projects::{ProjectSession, discussions::*};
use webnovel_core::providers::{
    claude_profile::ClaudeLaunchProfile,
    claude_runner::{ClaudeRunResult, ClaudeRunStatus, ClaudeStreamEvent},
    claude_runtime::ClaudeConnection,
    cli::windows_process::StopSignal,
};

pub fn run_live(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    connection: Option<ClaudeConnection>,
    dispatch: DiscussionDispatch,
    stop: StopSignal,
) {
    let owner = dispatch.run.owner.clone();
    let _registration = Registration {
        runtime,
        owner: owner.clone(),
    };
    let mut run = dispatch.run;

    // The command boundary rejects this combination before dispatch. Keep a
    // worker guard so a malformed or historical dispatch cannot enter a
    // provider lookup loop or leave a claimed run active forever.
    if run.lookup.is_some() {
        save_local_failure(
            &project,
            &recovery,
            run,
            "Claude does not support bounded story lookups.",
        );
        return;
    }

    if connection.is_none() {
        save_local_failure(
            &project,
            &recovery,
            run,
            "This saved Claude request has no active provider invocation and cannot resume automatically. Send a new request to try again.",
        );
        return;
    }

    let binding = match dispatch.packet.options.provider_binding.clone() {
        Some(binding)
            if binding.is_current_claude_profile()
                && connection.as_ref().is_some_and(|connection| {
                    crate::provider_runtime::claude_connection_matches_binding(connection, &binding)
                }) =>
        {
            binding
        }
        _ => {
            save_local_failure(
                &project,
                &recovery,
                run,
                "The request's saved Claude settings could not be validated.",
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
            save_local_failure(
                &project,
                &recovery,
                run,
                "The exact saved Claude request could not be validated.",
            );
            return;
        }
    };

    match project.read_discussion_run(owner.clone()) {
        Ok(current) => {
            if current.status == DiscussionRunStatus::Stopping {
                stop.request_stop();
            }
            run = current;
        }
        Err(_) => {
            save_local_failure(
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
        let mut result = failed(Some(binding.model_id.clone()));
        result.status = ClaudeRunStatus::Stopped;
        save(&project, &recovery, run, result);
        return;
    }
    let Some(connection) = connection else {
        save_local_failure(
            &project,
            &recovery,
            run,
            "Claude is unavailable. Check the Claude connection in Settings before sending this request.",
        );
        return;
    };
    let profile = match ClaudeLaunchProfile::for_version(
        connection.version(),
        &binding.model_id,
        binding.reasoning.as_deref(),
    ) {
        Ok(profile) => profile,
        Err(_) => {
            save_local_failure(
                &project,
                &recovery,
                run,
                "The request's saved Claude launch settings are invalid.",
            );
            return;
        }
    };
    let mut stream = match connection.start(&profile, input.into_bytes(), stop.clone()) {
        Ok(stream) => stream,
        Err(_) => {
            save_local_failure(
                &project,
                &recovery,
                run,
                "Claude could not start this request. Check its connection in Settings.",
            );
            return;
        }
    };
    let output_limit = binding.output_limit().expect("trusted Claude binding");
    let mut observed = String::new();
    let mut local_write_failed = false;
    let mut output_limited = false;

    loop {
        match stream.next_event(Duration::from_millis(100)) {
            Ok(None) => {}
            Ok(Some(ClaudeStreamEvent::AssistantDelta(delta))) => {
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
                    event_id: format!("{}-claude-part-{}", owner.run_id, run.sequence),
                    chunk: chunk.into(),
                }) {
                    Ok(updated) => run = updated,
                    Err(error) => {
                        local_write_failed = error.code != "RunStopping";
                        stop.request_stop();
                    }
                }
            }
            Ok(Some(ClaudeStreamEvent::Finished(mut result))) => {
                if !result.assistant_text.starts_with(&observed) {
                    result.status = ClaudeRunStatus::ProtocolFailure(
                        webnovel_core::providers::claude_exec::ClaudeFailureCode::Protocol,
                    );
                    result.assistant_text = observed.clone();
                }
                if output_limited || result.assistant_text.len() > output_limit {
                    result.status = ClaudeRunStatus::OutputLimit;
                    result.assistant_text = prefix(&result.assistant_text, output_limit).into();
                }
                // A local append error may have committed its exact chunk. The
                // terminal provider report remains available for explicit local
                // reconciliation and never starts another Claude request.
                if local_write_failed && result.status == ClaudeRunStatus::Stopped {
                    result.status = ClaudeRunStatus::ProtocolFailure(
                        webnovel_core::providers::claude_exec::ClaudeFailureCode::Incomplete,
                    );
                }
                if local_write_failed {
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
                let mut result = failed(Some(binding.model_id.clone()));
                result.status = ClaudeRunStatus::CleanupUnresolved;
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

fn failed(requested_model: Option<String>) -> ClaudeRunResult {
    ClaudeRunResult {
        status: ClaudeRunStatus::ProcessUnavailable,
        assistant_text: String::new(),
        requested_model,
        reported_model: None,
        usage: None,
        confirmed_stdin_bytes: 0,
        cleanup_settled: true,
    }
}

fn classified_result(
    binding: &ProviderBinding,
    result: &ClaudeRunResult,
) -> (ProviderOutcomeStatus, Option<String>) {
    let mut status = outcome(&result.status);
    let mut error = error_for(&result.status).map(str::to_owned);
    if result.status == ClaudeRunStatus::Completed {
        match result.reported_model.as_deref() {
            Some(reported) if reported == binding.model_id => {}
            Some(_) => {
                status = ProviderOutcomeStatus::Failed;
                error = Some(
                    "Claude reported a different model than requested. The original response is retained for inspection."
                        .into(),
                );
            }
            None => {
                status = ProviderOutcomeStatus::Failed;
                error = Some("Claude did not report its terminal model identity.".into());
            }
        }
    }
    (status, error)
}

fn outcome(status: &ClaudeRunStatus) -> ProviderOutcomeStatus {
    match status {
        ClaudeRunStatus::Completed => ProviderOutcomeStatus::Completed,
        ClaudeRunStatus::Stopped => ProviderOutcomeStatus::Stopped,
        ClaudeRunStatus::TimedOut => ProviderOutcomeStatus::TimedOut,
        ClaudeRunStatus::OutputLimit => ProviderOutcomeStatus::OutputLimit,
        ClaudeRunStatus::InputLimit
        | ClaudeRunStatus::ModelMismatch
        | ClaudeRunStatus::ProtocolFailure(_)
        | ClaudeRunStatus::ConsumerTooSlow
        | ClaudeRunStatus::ProcessUnavailable
        | ClaudeRunStatus::CleanupUnresolved => ProviderOutcomeStatus::Failed,
    }
}

fn error_for(status: &ClaudeRunStatus) -> Option<&'static str> {
    match status {
        ClaudeRunStatus::Completed | ClaudeRunStatus::Stopped => None,
        ClaudeRunStatus::TimedOut => {
            Some("The Claude request timed out before the response finished.")
        }
        ClaudeRunStatus::OutputLimit => Some("The response reached the app's output limit."),
        ClaudeRunStatus::InputLimit => Some("The request exceeds the app's Claude input limit."),
        ClaudeRunStatus::ModelMismatch => Some(
            "Claude reported a different model than requested. The original response is retained for inspection.",
        ),
        ClaudeRunStatus::ProtocolFailure(_) => {
            Some("Claude returned an incomplete or unsupported response.")
        }
        ClaudeRunStatus::ConsumerTooSlow => {
            Some("The response stopped because progress could not be processed quickly enough.")
        }
        ClaudeRunStatus::ProcessUnavailable => {
            Some("Claude could not start this request. Check its connection in Settings.")
        }
        ClaudeRunStatus::CleanupUnresolved => Some("Local process cleanup could not be confirmed."),
    }
}

fn report(run: &DiscussionRun, result: ClaudeRunResult) -> ProviderTerminalReport {
    let binding = run
        .provider_binding
        .as_ref()
        .expect("a Claude provider report requires its frozen binding");
    let (status, error) = classified_result(binding, &result);
    ProviderTerminalReport {
        app_server: None,
        owner: run.owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-claude-finish", run.id),
        assistant_text: result.assistant_text,
        binding: binding.clone(),
        status,
        confirmed_stdin_bytes: result.confirmed_stdin_bytes.to_string(),
        // Claude's optional counters do not map to the all-required Codex
        // usage contract, so retaining them as fabricated Codex counters would
        // be misleading.
        usage: None,
        cleanup: if result.cleanup_settled {
            ProviderCleanup::Settled
        } else {
            ProviderCleanup::Unresolved
        },
        error,
        effective_identity: None,
        // Preserve safe unknown IDs for diagnosis, but never store arbitrary
        // provider text as an identity. Classification above still sees the
        // original value, so filtering cannot turn a mismatch into success.
        reported_model: result.reported_model.filter(|value| {
            webnovel_core::providers::claude_profile::valid_reported_model_id(value)
        }),
        delivery: None,
    }
}

fn save(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    result: ClaudeRunResult,
) {
    save_report(project, recovery, run.clone(), report(&run, result));
}

fn save_report(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    report: ProviderTerminalReport,
) {
    let mut pending = PendingSave {
        outcome: SaveOutcome::Provider(Box::new(report)),
        run,
    };
    if let Ok(current) = project.read_discussion_run(pending.run.owner.clone()) {
        pending.run = current;
    }
    if pending.attempt(project, &pending.run).is_err() {
        recovery.retain(pending);
    }
}

fn save_local_failure(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    detail: &'static str,
) {
    let failure = DiscussionFail {
        owner: run.owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-claude-rejected", run.id),
        reason: detail.to_owned(),
    };
    if project.fail_discussion_run(failure).is_err() {
        let mut pending = PendingSave {
            run,
            outcome: SaveOutcome::Fail,
        };
        // A Stop can win before this unsent failure is saved. No process was
        // started, so settle that Stop locally without requiring a retry click.
        if let Ok(current) = project.read_discussion_run(pending.run.owner.clone()) {
            pending.run = current;
            if pending.run.status == DiscussionRunStatus::Stopping
                && pending.attempt(project, &pending.run).is_ok()
            {
                return;
            }
        }
        recovery.retain(pending);
    }
}

pub fn worker_unavailable(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
) {
    save_local_failure(
        project,
        recovery,
        run,
        "The Claude response worker could not start. No model request was sent.",
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
    use webnovel_core::providers::claude_exec::ClaudeFailureCode;

    #[test]
    fn orphaned_dispatch_without_a_connection_seals_locally_and_respects_stop() {
        use webnovel_core::context::packet::MockContextBudget;
        use webnovel_core::projects::CreateDocument;
        for stopping in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "wns-claude-unsent-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            ));
            let project = ProjectSession::create(&root, "Synthetic Claude recovery").unwrap();
            let access = project.documents().attach("fixture".into()).unwrap();
            let document = project.documents().create(CreateDocument {
                access: access.clone(), operation_id: "create-chapter".into(),
                document_id: "chapter".into(), title: "The key".into(), kind: "chapter".into(),
                body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"Mei returned the key."}]}]}}),
            }).unwrap();
            let request = StartDiscussion {
                access: access.clone(),
                operation_id: "accepted-claude-request".into(),
                expected: document.head,
                instruction: "Discuss the key.".into(),
                intent: FeedbackIntent::Discuss,
                basis: None,
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "100", "100"),
                provider_binding: Some(ProviderBinding::claude_author_runtime(
                    "claude-sonnet-5",
                    "high",
                    "2.1.220",
                    &"a".repeat(64),
                )),
                previous_run_id: None,
                lookup: None,
            };
            let started = project.start_discussion(request.clone()).unwrap();
            let runtime = DesktopProviders::default();
            let stop = runtime.register(&started.run.owner).unwrap();
            let dispatch = project
                .begin_discussion_run(DiscussionBegin {
                    owner: started.run.owner.clone(),
                })
                .unwrap();
            if stopping {
                project
                    .stop_discussion(access, started.run.id.clone())
                    .unwrap();
            }
            run_live(
                project.clone(),
                DiscussionRecovery::default(),
                runtime.clone(),
                None,
                dispatch,
                stop,
            );
            let settled = project
                .read_discussion_run(started.run.owner.clone())
                .unwrap();
            assert_eq!(
                settled.status,
                if stopping {
                    DiscussionRunStatus::Stopped
                } else {
                    DiscussionRunStatus::Failed
                }
            );
            assert!(settled.provider_result.is_none());
            assert!(settled.output_text.is_empty());
            // Lost acknowledgment resolves the terminal operation; it cannot
            // turn the original saved request into a new provider invocation.
            assert_eq!(
                project.start_discussion(request).unwrap().run.status,
                settled.status
            );
            assert!(runtime.register(&started.run.owner).is_ok());
            runtime.release(&started.run.owner);
            drop(project);
            let canonical = std::fs::canonicalize(&root).unwrap();
            assert_eq!(
                canonical.parent(),
                Some(
                    std::fs::canonicalize(std::env::temp_dir())
                        .unwrap()
                        .as_path()
                )
            );
            assert!(
                canonical
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("wns-claude-unsent-")
            );
            std::fs::remove_dir_all(canonical).unwrap();
        }
    }

    #[test]
    fn result_conversion_keeps_model_mismatch_failed_and_usage_unknown() {
        let status = ClaudeRunStatus::ModelMismatch;
        assert_eq!(outcome(&status), ProviderOutcomeStatus::Failed);
        assert!(error_for(&status).is_some());
        let status = ClaudeRunStatus::Completed;
        assert_eq!(outcome(&status), ProviderOutcomeStatus::Completed);
        let status = ClaudeRunStatus::ProtocolFailure(ClaudeFailureCode::Incomplete);
        assert_eq!(outcome(&status), ProviderOutcomeStatus::Failed);
    }

    #[test]
    fn completed_result_requires_the_exact_reported_model() {
        let binding = ProviderBinding::claude_author_runtime(
            "claude-sonnet-5",
            "high",
            "2.1.220",
            &"a".repeat(64),
        );
        let mut result = failed(Some(binding.model_id.clone()));
        result.status = ClaudeRunStatus::Completed;
        result.assistant_text = "A retained answer.".into();
        result.reported_model = Some("claude-opus-5".into());
        let (status, error) = classified_result(&binding, &result);
        assert_eq!(status, ProviderOutcomeStatus::Failed);
        assert!(error.is_some());
        assert_eq!(result.reported_model.as_deref(), Some("claude-opus-5"));

        result.reported_model = None;
        let (status, error) = classified_result(&binding, &result);
        assert_eq!(status, ProviderOutcomeStatus::Failed);
        assert_eq!(
            error.as_deref(),
            Some("Claude did not report its terminal model identity.")
        );

        result.reported_model = Some(binding.model_id.clone());
        let (status, error) = classified_result(&binding, &result);
        assert_eq!(status, ProviderOutcomeStatus::Completed);
        assert_eq!(error, None);
    }

    #[test]
    fn stopped_result_has_no_usage_or_reported_identity() {
        let result = failed(Some("claude-sonnet-5".into()));
        assert_eq!(result.usage, None);
        assert_eq!(result.reported_model, None);
        assert_eq!(result.requested_model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(result.confirmed_stdin_bytes, 0);
    }

    #[test]
    fn output_scope_never_splits_unicode_or_exceeds_the_cap() {
        assert_eq!(prefix("Aé🌙", 2), "A");
        assert_eq!(prefix("Aé🌙", 6), "Aé");
        let text = prefix("Aé🌙", 7);
        assert!(text.len() <= 7);
        assert!(text.is_char_boundary(text.len()));
    }

    #[test]
    fn protocol_failures_are_failed_provider_outcomes() {
        assert_eq!(
            outcome(&ClaudeRunStatus::ProtocolFailure(
                ClaudeFailureCode::InvalidJson
            )),
            ProviderOutcomeStatus::Failed
        );
        assert!(error_for(&ClaudeRunStatus::CleanupUnresolved).is_some());
    }
}
