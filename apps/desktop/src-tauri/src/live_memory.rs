//! One bounded memory refresh: one frozen packet, one Codex process, one local settlement.
#![cfg(windows)]

use crate::memory_recovery::MemoryRecovery;
use crate::provider_runtime::DesktopProviders;
use std::time::Duration;
use webnovel_core::context::packet::{ProviderBinding, packet_input_hash, serialized_input};
use webnovel_core::projects::ProjectSession;
use webnovel_core::projects::discussions::{ProviderCleanup, ProviderOutcomeStatus, ProviderUsage};
use webnovel_core::projects::memory::{CompleteMemory, MemoryDispatch, MemoryJobStatus};
use webnovel_core::providers::cli::windows_process::StopSignal;
use webnovel_core::providers::codex_runner::{CodexRunResult, CodexRunStatus, CodexStreamEvent};
use webnovel_core::providers::codex_runtime::CodexConnection;

pub fn run_live(
    project: ProjectSession,
    recovery: MemoryRecovery,
    runtime: DesktopProviders,
    connection: Option<CodexConnection>,
    dispatch: MemoryDispatch,
    stop: StopSignal,
) {
    let owner = dispatch.job.owner.clone();
    let document_id = dispatch.job.target.document_id.clone();
    let _registration = Registration {
        runtime,
        owner: owner.clone(),
    };

    let binding = match dispatch.packet.options.provider_binding.clone() {
        Some(binding) if binding == ProviderBinding::codex_luna() => binding,
        _ => {
            save_failure(
                &project,
                &recovery,
                owner,
                document_id,
                "The refresh's saved model settings could not be validated.",
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
                owner,
                document_id,
                "The exact saved refresh request could not be validated.",
            );
            return;
        }
    };

    let current = match project.read_memory_job(owner.clone()) {
        Ok(current) => current,
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                owner,
                document_id,
                "The refresh's saved state could not be checked before starting.",
            );
            return;
        }
    };
    if current.status == MemoryJobStatus::Stopping {
        stop.request_stop();
    }
    if !matches!(
        current.status,
        MemoryJobStatus::Running | MemoryJobStatus::Stopping
    ) {
        return;
    }
    if stop.is_requested() {
        save_result(&project, &recovery, stopped(&owner, 0), document_id);
        return;
    }
    // A policy revocation may happen after the durable claim but before the
    // worker starts.  The running-job replay path revalidates the frozen
    // policy without submitting a second provider request.
    if let Err(error) = project.begin_memory(owner.clone()) {
        let detail = if error.code == "ContextPolicyChanged" {
            "Story-context permissions changed before this refresh could start."
        } else {
            "The refresh could not be revalidated before starting."
        };
        save_failure(&project, &recovery, owner, document_id, detail);
        return;
    }
    let Some(connection) = connection else {
        save_failure(
            &project,
            &recovery,
            owner,
            document_id,
            "Codex is unavailable. Check the connection in Settings before refreshing story memory.",
        );
        return;
    };
    let mut stream = match connection.start(input.into_bytes(), stop.clone()) {
        Ok(stream) => stream,
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                owner,
                document_id,
                "Codex could not start this refresh. Check its connection in Settings.",
            );
            return;
        }
    };
    let output_limit = binding.output_limit().unwrap_or(64 * 1024);
    let mut observed = String::new();
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
                observed.push_str(prefix(&delta, remaining));
            }
            Ok(Some(CodexStreamEvent::Finished(mut result))) => {
                if !result.assistant_text.starts_with(&observed) {
                    result.status = CodexRunStatus::ProtocolFailure(
                        webnovel_core::providers::codex_exec::CodexFailureCode::Protocol,
                    );
                    result.assistant_text = observed.clone();
                }
                if output_limited || result.assistant_text.len() > output_limit {
                    result.status = CodexRunStatus::OutputLimit;
                    result.assistant_text = prefix(&result.assistant_text, output_limit).into();
                }
                save_result(&project, &recovery, report(&owner, result), document_id);
                return;
            }
            Err(_) => {
                let mut result = failed(CodexRunStatus::CleanupUnresolved);
                result.assistant_text = observed;
                result.cleanup_settled = false;
                save_result(&project, &recovery, report(&owner, result), document_id);
                return;
            }
        }
    }
}

pub fn worker_unavailable(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    owner: webnovel_core::projects::memory::MemoryOwner,
    document_id: String,
    detail: &'static str,
) {
    save_failure(project, recovery, owner, document_id, detail);
}

fn save_failure(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    owner: webnovel_core::projects::memory::MemoryOwner,
    document_id: String,
    detail: &'static str,
) {
    let mut result = failed(CodexRunStatus::ProcessUnavailable);
    result.cleanup_settled = true;
    save_result(
        project,
        recovery,
        report_with_error(&owner, result, detail),
        document_id,
    );
}

fn save_result(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    completion: CompleteMemory,
    document_id: String,
) {
    let _ = recovery.save_or_retain(project, completion, document_id);
}

fn report(
    owner: &webnovel_core::projects::memory::MemoryOwner,
    result: CodexRunResult,
) -> CompleteMemory {
    let error = match result.status {
        CodexRunStatus::Completed | CodexRunStatus::Stopped => None,
        CodexRunStatus::TimedOut => {
            Some("The memory refresh timed out before the response finished.")
        }
        CodexRunStatus::OutputLimit => Some("The memory refresh reached the app's output limit."),
        CodexRunStatus::ProtocolFailure(_) => {
            Some("Codex returned an incomplete or unsupported memory response.")
        }
        CodexRunStatus::ConsumerTooSlow => Some(
            "The memory refresh stopped because progress could not be processed quickly enough.",
        ),
        CodexRunStatus::ProcessUnavailable => {
            Some("Codex could not start this refresh. Check its connection in Settings.")
        }
        CodexRunStatus::CleanupUnresolved => Some("Local process cleanup could not be confirmed."),
    };
    report_with_error(owner, result, error.unwrap_or(""))
}

fn report_with_error(
    owner: &webnovel_core::projects::memory::MemoryOwner,
    result: CodexRunResult,
    detail: &str,
) -> CompleteMemory {
    let error = if detail.is_empty() {
        None
    } else {
        Some(detail.to_owned())
    };
    let outcome = match result.status {
        CodexRunStatus::Completed => ProviderOutcomeStatus::Completed,
        CodexRunStatus::Stopped => ProviderOutcomeStatus::Stopped,
        CodexRunStatus::TimedOut => ProviderOutcomeStatus::TimedOut,
        CodexRunStatus::OutputLimit => ProviderOutcomeStatus::OutputLimit,
        CodexRunStatus::ProtocolFailure(_)
        | CodexRunStatus::ConsumerTooSlow
        | CodexRunStatus::ProcessUnavailable
        | CodexRunStatus::CleanupUnresolved => ProviderOutcomeStatus::Failed,
    };
    CompleteMemory {
        owner: owner.clone(),
        event_id: format!("{}-provider-finish", owner.job_id),
        raw_output: result.assistant_text,
        outcome,
        confirmed_stdin_bytes: Some(result.confirmed_stdin_bytes.to_string()),
        usage: result.usage.map(|usage| ProviderUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_write_input_tokens: usage.cache_write_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        }),
        cleanup: Some(if result.cleanup_settled {
            ProviderCleanup::Settled
        } else {
            ProviderCleanup::Unresolved
        }),
        error,
        effective_identity: None,
    }
}

fn failed(status: CodexRunStatus) -> CodexRunResult {
    CodexRunResult {
        status,
        assistant_text: String::new(),
        usage: None,
        confirmed_stdin_bytes: 0,
        warning_count: 0,
        cleanup_settled: true,
    }
}

fn stopped(owner: &webnovel_core::projects::memory::MemoryOwner, bytes: usize) -> CompleteMemory {
    report_with_error(
        owner,
        CodexRunResult {
            status: CodexRunStatus::Stopped,
            assistant_text: String::new(),
            usage: None,
            confirmed_stdin_bytes: bytes,
            warning_count: 0,
            cleanup_settled: true,
        },
        "",
    )
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

struct Registration {
    runtime: DesktopProviders,
    owner: webnovel_core::projects::memory::MemoryOwner,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.runtime.release_memory(&self.owner);
    }
}
