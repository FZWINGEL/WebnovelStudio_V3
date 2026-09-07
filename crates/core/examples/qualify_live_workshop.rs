//! Explicitly opt-in, one-shot live Codex Workshop qualification.
//!
//! This is a provider/core qualification harness.  It proves packet freezing,
//! one provider dispatch, durable terminal settlement, and Workshop response
//! parsing.  It does not qualify native UI behavior, model quality, or the
//! semantic usefulness of the returned directions.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(not(windows))]
fn main() {
    println!("live Workshop qualification is supported only on Windows Codex hosts");
}

#[cfg(windows)]
mod windows {
    use serde_json::{Value, json};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use uuid::Uuid;
    use webnovel_core::context::packet::{MockContextBudget, ProviderBinding, serialized_input};
    use webnovel_core::projects::ProjectSession;
    use webnovel_core::projects::discussions::{
        DiscussionBegin, DiscussionOutputAppend, DiscussionRunStatus, ProviderCleanup,
        ProviderOutcomeStatus, ProviderTerminalReport, ProviderUsage,
    };
    use webnovel_core::projects::workshop::{
        Lens, SaveWorkshop, WorkshopBranchKind, WorkshopDepth, WorkshopSession, WorkshopState,
    };
    use webnovel_core::projects::workshop_generation::{StartWorkshop, WorkshopExploration};
    use webnovel_core::providers::cli::windows_process::StopSignal;
    use webnovel_core::providers::codex_profile::{
        CODEX_AUTHOR_PROFILE_VERSION, CODEX_LUNA_MODEL, CODEX_PRIORITY_SERVICE_TIER,
        CODEX_REASONING_EFFORT,
    };
    use webnovel_core::providers::codex_runner::{CodexRunStatus, CodexStreamEvent};
    use webnovel_core::providers::codex_runtime::CodexConnection;
    use webnovel_core::providers::preferences::ModelSelection;

    const ALLOW_FLAG: &str = "WNS_V3_ALLOW_LIVE_WORKSHOP";
    const SOURCE_SHA_ENV: &str = "WNS_V3_WORKSHOP_SOURCE_SHA";
    const REPORT_DIR: &str = ".local";
    const WORKSHOP_SESSION: &str = "live-session";
    const WORKSHOP_ANCHOR: &str = "workshop-live-session";
    const QUALIFICATION_BUDGET: (&str, &str, &str) = ("128000", "65536", "2048");

    pub fn run() -> Result<(), String> {
        if std::env::var(ALLOW_FLAG).ok().as_deref() != Some("1") {
            println!(
                "live Workshop qualification disabled; set {ALLOW_FLAG}=1 to permit one Codex dispatch"
            );
            return Ok(());
        }

        let run_key = Uuid::new_v4().simple().to_string();
        let report_path = report_path(&run_key)?;
        let source_sha = std::env::var(SOURCE_SHA_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let mut report = json!({
            "schemaVersion": "live-workshop-qualification.v1",
            "status": "started",
            "runKey": run_key,
            "sourceSha": source_sha,
            "dispatchCap": 1,
            "operationId": format!("workshop-live-{run_key}"),
            "action": "directions",
            "modelSelection": {
                "providerId": "codex",
                "modelId": CODEX_LUNA_MODEL,
                "reasoning": CODEX_REASONING_EFFORT,
                "serviceTier": CODEX_PRIORITY_SERVICE_TIER,
            },
            "reportPath": report_path,
        });
        persist_report(&report_path, &report)?;

        let result = qualify(&run_key, &report_path, &mut report);
        match result {
            Ok(()) => {
                report["status"] = Value::String("passed".into());
                persist_report(&report_path, &report)?;
                println!(
                    "Workshop qualification passed; report={}",
                    report_path.display()
                );
                Ok(())
            }
            Err(error) => {
                report["status"] = Value::String("failed".into());
                report["failure"] = Value::String(error.clone());
                // A failed run remains in its synthetic project directory and
                // its report remains the inspection/reconciliation boundary.
                persist_report(&report_path, &report)?;
                eprintln!(
                    "Workshop qualification failed; report={} error={}",
                    report_path.display(),
                    error
                );
                Err(error)
            }
        }
    }

    fn qualify(run_key: &str, report_path: &Path, report: &mut Value) -> Result<(), String> {
        // This check performs bounded version/login/catalog discovery.  It does
        // not send story text and does not read Codex authentication files.
        let connection = CodexConnection::check_installed().map_err(display_error)?;
        let selection = ModelSelection {
            provider_id: "codex".into(),
            model_id: CODEX_LUNA_MODEL.into(),
            reasoning: Some(CODEX_REASONING_EFFORT.into()),
            service_tier: Some(CODEX_PRIORITY_SERVICE_TIER.into()),
        };
        if !connection.catalog().supports(&selection) {
            return Err(
                "The checked Codex catalog does not support gpt-5.6-luna with xhigh/priority."
                    .into(),
            );
        }
        let model = connection
            .catalog()
            .model(CODEX_LUNA_MODEL)
            .ok_or_else(|| "The checked Codex catalog has no Luna model entry.".to_owned())?;
        let catalog_sha = model.fingerprint().map_err(display_error)?;
        let binding = ProviderBinding::codex_author_runtime(
            CODEX_LUNA_MODEL,
            CODEX_REASONING_EFFORT,
            Some(CODEX_PRIORITY_SERVICE_TIER),
            connection.version(),
            connection.fingerprint(),
            &catalog_sha,
        );
        report["provider"] = json!({
            "cliVersion": connection.version(),
            "executableSha256": connection.fingerprint(),
            "catalogModelSha256": catalog_sha,
            "profileVersion": CODEX_AUTHOR_PROFILE_VERSION,
        });
        persist_report(report_path, report)?;

        let project_root = canonical_temp_project(run_key)?;
        report["projectPath"] = Value::String(project_root.display().to_string());
        let project = ProjectSession::create(&project_root, "Live Workshop qualification")
            .map_err(display_error)?;
        let access = project.attach(format!("workshop-qualifier-{run_key}"));
        let access = access.map_err(display_error)?;
        let session = synthetic_session();
        let state = WorkshopState {
            schema_version: 1,
            current_session_id: Some(WORKSHOP_SESSION.into()),
            sessions: vec![session],
            ..WorkshopState::default()
        };
        let expected_state = state.clone();
        let saved = project
            .save_workshop(SaveWorkshop {
                access: access.clone(),
                operation_id: format!("workshop-state-{run_key}"),
                expected_version: "0".into(),
                state,
            })
            .map_err(display_error)?;
        report["workshop"] = json!({
            "sessionId": WORKSHOP_SESSION,
            "anchorDocumentId": WORKSHOP_ANCHOR,
            "initialVersion": saved.version,
            "workingGeneration": "0",
            "chaptersBefore": 0,
        });
        persist_report(report_path, report)?;

        let operation_id = format!("workshop-live-{run_key}");
        let started = project
            .start_workshop(StartWorkshop {
                access: access.clone(),
                operation_id: operation_id.clone(),
                exploration: WorkshopExploration {
                    session_id: WORKSHOP_SESSION.into(),
                    expected_version: saved.version.clone(),
                    working_generation: "0".into(),
                    action: "directions".into(),
                    instruction: "Offer three distinct ways to explore what this archive preserves. Keep each alternative reviewable; create no canon and no chapter.".into(),
                    selected_scope: "Whole working version".into(),
                    selected_text: String::new(),
                    working_selection: None,
                },
                budget: MockContextBudget::new(
                    QUALIFICATION_BUDGET.0,
                    QUALIFICATION_BUDGET.1,
                    QUALIFICATION_BUDGET.2,
                ),
                provider_binding: Some(binding.clone()),
            })
            .map_err(display_error)?;
        let anchor_before = project
            .document(access.clone(), WORKSHOP_ANCHOR.into())
            .map_err(display_error)?;
        if anchor_before.kind != "note" {
            return Err("The Workshop start did not create an author-room note anchor.".into());
        }
        if started.packet.options.provider_binding.as_ref() != Some(&binding) {
            return Err(
                "The frozen packet provider binding differs from the checked selection.".into(),
            );
        }
        report["start"] = json!({
            "threadId": started.thread_id,
            "run": started.run,
            "anchor": anchor_before,
        });
        report["packet"] = serde_json::to_value(&started.packet).map_err(display_error)?;
        let packet_input = serialized_input(&started.packet.messages, &started.packet.options)
            .map_err(|error| error.to_string())?;
        report["serializedInput"] = Value::String(packet_input.clone());
        report["packetInputBytes"] = Value::from(packet_input.len());
        persist_report(report_path, report)?;

        let dispatch = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone(),
            })
            .map_err(display_error)?;
        report["dispatch"] = json!({
            "run": dispatch.run,
            "preflightStoryDispatchCount": 0,
            "retryCount": 0,
        });
        persist_report(report_path, report)?;

        let stop = StopSignal::new();
        report["dispatch"]["startAttempted"] = Value::Bool(true);
        report["dispatch"]["externalInvocationCount"] = Value::Null;
        persist_report(report_path, report)?;
        let mut stream =
            match connection.start_bound(&binding, packet_input.clone().into_bytes(), stop.clone())
            {
                Ok(stream) => stream,
                Err(error) => {
                    let _ = settle_failure(
                        &project,
                        &dispatch.run,
                        ProviderOutcomeStatus::Failed,
                        ProviderCleanup::Settled,
                        0,
                        None,
                        "Codex did not start the one permitted invocation.",
                    );
                    return Err(display_error(error));
                }
            };
        report["dispatch"]["externalInvocationCount"] = Value::from(1);
        persist_report(report_path, report)?;

        let mut run = dispatch.run;
        let mut observed = String::new();
        let mut local_write_failed = false;
        let terminal = loop {
            match stream.next_event(Duration::from_millis(100)) {
                Ok(None) => continue,
                Ok(Some(CodexStreamEvent::AssistantDelta(delta))) => {
                    if observed.len().saturating_add(delta.len()) > binding_output_limit(&binding)?
                    {
                        stop.request_stop();
                    }
                    let remaining = binding_output_limit(&binding)?.saturating_sub(observed.len());
                    let chunk = prefix(&delta, remaining);
                    observed.push_str(chunk);
                    if !chunk.is_empty() && !local_write_failed && !stop.is_requested() {
                        match project.append_discussion_output(DiscussionOutputAppend {
                            owner: run.owner.clone(),
                            expected_sequence: run.sequence.clone(),
                            event_id: format!("{}-part-{}", run.id, run.sequence),
                            chunk: chunk.into(),
                        }) {
                            Ok(updated) => run = updated,
                            Err(_) => {
                                local_write_failed = true;
                                stop.request_stop();
                            }
                        }
                    }
                }
                Ok(Some(CodexStreamEvent::Finished(result))) => break result,
                Err(error) => {
                    stop.request_stop();
                    let message = "The Codex stream ended without a terminal cleanup receipt.";
                    let _ = settle_failure(
                        &project,
                        &run,
                        ProviderOutcomeStatus::Failed,
                        ProviderCleanup::Unresolved,
                        0,
                        Some(observed.clone()),
                        message,
                    );
                    return Err(format!("{message} ({error})"));
                }
            }
        };
        let mut terminal = terminal;
        let output_limit = binding_output_limit(&binding)?;
        let mut terminal_status = terminal.status.clone();
        if !terminal.assistant_text.starts_with(&observed) {
            terminal_status = CodexRunStatus::ProtocolFailure(
                webnovel_core::providers::codex_exec::CodexFailureCode::Protocol,
            );
            terminal.assistant_text = observed.clone();
        }
        if terminal.assistant_text.len() > output_limit {
            terminal_status = CodexRunStatus::OutputLimit;
            terminal.assistant_text = prefix(&terminal.assistant_text, output_limit).into();
        }
        if local_write_failed && terminal_status == CodexRunStatus::Stopped {
            terminal_status = CodexRunStatus::ProtocolFailure(
                webnovel_core::providers::codex_exec::CodexFailureCode::Incomplete,
            );
        }
        let report_status = status_for(&terminal_status);
        let report_error = error_for(&terminal_status);
        report["terminal"] = json!({
            "status": format!("{terminal_status:?}"),
            "assistantText": terminal.assistant_text.clone(),
            "confirmedStdinBytes": terminal.confirmed_stdin_bytes,
            "cleanupSettled": terminal.cleanup_settled,
            "warningCount": terminal.warning_count,
        });
        let terminal_report = ProviderTerminalReport {
            owner: run.owner.clone(),
            expected_sequence: run.sequence.clone(),
            event_id: format!("{}-provider-finish", run.id),
            assistant_text: terminal.assistant_text.clone(),
            binding: binding.clone(),
            status: report_status,
            confirmed_stdin_bytes: terminal.confirmed_stdin_bytes.to_string(),
            usage: terminal.usage.as_ref().map(provider_usage),
            cleanup: if terminal.cleanup_settled {
                ProviderCleanup::Settled
            } else {
                ProviderCleanup::Unresolved
            },
            error: report_error.map(str::to_owned),
            effective_identity: None,
            reported_model: None,
            delivery: None,
        };
        let settled = project
            .settle_provider_discussion(terminal_report)
            .map_err(display_error)?;
        report["providerResult"] = json!({
            "run": settled.run,
            "providerResult": settled.provider_result,
            "assistantText": terminal.assistant_text,
            "confirmedStdinBytes": terminal.confirmed_stdin_bytes,
            "cleanupSettled": terminal.cleanup_settled,
        });
        persist_report(report_path, report)?;

        let view = project
            .read_workshop(access.clone())
            .map_err(display_error)?;
        let result = view
            .results
            .iter()
            .find(|result| result.run.id == run.id)
            .ok_or_else(|| "The settled run is missing from the Workshop projection.".to_owned())?;
        report["workshopView"] = serde_json::to_value(&view).map_err(display_error)?;
        report["parsedDirections"] = serde_json::to_value(&result.output).map_err(display_error)?;
        assert_completed(result)?;
        if result
            .output
            .as_ref()
            .map_or(0, |output| output.candidates.len())
            != 3
        {
            return Err(
                "A completed directions run did not produce exactly three candidates.".into(),
            );
        }
        if result.output.as_ref().is_some_and(|output| {
            output
                .candidates
                .iter()
                .enumerate()
                .any(|(index, candidate)| candidate.id != format!("{}-{index}", result.run.id))
        }) {
            return Err("A completed direction candidate has an unstable durable ID.".into());
        }
        if result.run.provider_result.is_none() {
            return Err("The completed Workshop run has no immutable provider receipt.".into());
        }
        if result.run.provider_result.as_ref().is_some_and(|receipt| {
            receipt.binding != binding
                || receipt.confirmed_stdin_bytes != packet_input.len().to_string()
                || receipt.cleanup != ProviderCleanup::Settled
        }) {
            return Err("The provider receipt does not match the frozen packet evidence.".into());
        }

        let anchor_after = project
            .document(access.clone(), WORKSHOP_ANCHOR.into())
            .map_err(display_error)?;
        if anchor_after.body != anchor_before.body || anchor_after.head != anchor_before.head {
            return Err("The Workshop dispatch mutated the author-room anchor.".into());
        }
        let documents = project.documents(access.clone()).map_err(display_error)?;
        if documents.len() != 1 || documents[0].kind != "note" {
            return Err(
                "The qualification project contains an unexpected document or chapter.".into(),
            );
        }
        if view.version != saved.version || view.state != expected_state {
            return Err("The Workshop run changed manual Workshop state.".into());
        }
        report["assertions"] = json!({
            "completedDelivered": true,
            "exactlyThreeDirections": true,
            "stableCandidateIds": true,
            "providerReceiptMatchesPacket": true,
            "anchorUnchanged": true,
            "noChapterCreated": true,
            "manualStateUnchanged": true,
        });

        drop(project);
        report["projectRetained"] = Value::Bool(true);
        Ok(())
    }

    fn synthetic_session() -> WorkshopSession {
        WorkshopSession {
            id: WORKSHOP_SESSION.into(),
            title: "Archive directions".into(),
            lens: Lens::Possibilities,
            parent_session_id: None,
            branch_kind: WorkshopBranchKind::Working,
            brief: "A quiet archive preserves memories that their owners are not ready to face."
                .into(),
            direction: "Offer ways into the archive's unresolved purpose.".into(),
            still_open: "Who built the archive, and what does opening it cost?".into(),
            focus_question: "What could the archive ask of the person who finds it?".into(),
            focus_reason: "Keep the first exploration open to different story engines.".into(),
            focus_document_id: None,
            anchor_document_id: Some(WORKSHOP_ANCHOR.into()),
            depth: WorkshopDepth::Develop,
            outside_direction: false,
            included_document_ids: Vec::new(),
            working_text: "An archive keeps memories that their owners are not ready to face."
                .into(),
            working_title: "The memory archive".into(),
            working_generation: "0".into(),
            selected_details: Vec::new(),
            choices: Vec::new(),
            questions: Vec::new(),
            composer: "Find three distinct directions without establishing canon.".into(),
            selected_scope: "Whole working version".into(),
            original_notes: "Synthetic qualification fixture; no author material.".into(),
            active_run_id: None,
        }
    }

    fn assert_completed(
        result: &webnovel_core::projects::workshop::WorkshopResult,
    ) -> Result<(), String> {
        if result.run.status != DiscussionRunStatus::Completed
            || result.run.dispatch_state != "delivered"
        {
            return Err(format!(
                "Workshop run did not complete and deliver: status={:?}, dispatchState={}",
                result.run.status, result.run.dispatch_state
            ));
        }
        if result.stale || result.validation_error.is_some() || result.output.is_none() {
            return Err(format!(
                "Workshop projection is not a fresh validated output: stale={}, validationError={:?}",
                result.stale, result.validation_error
            ));
        }
        Ok(())
    }

    fn settle_failure(
        project: &ProjectSession,
        run: &webnovel_core::projects::discussions::DiscussionRun,
        status: ProviderOutcomeStatus,
        cleanup: ProviderCleanup,
        confirmed_stdin_bytes: usize,
        assistant_text: Option<String>,
        error: &str,
    ) -> Result<(), String> {
        project
            .settle_provider_discussion(ProviderTerminalReport {
                owner: run.owner.clone(),
                expected_sequence: run.sequence.clone(),
                event_id: format!("{}-provider-finish", run.id),
                assistant_text: assistant_text.unwrap_or_default(),
                binding: run
                    .provider_binding
                    .clone()
                    .ok_or_else(|| "The live run has no provider binding.".to_owned())?,
                status,
                confirmed_stdin_bytes: confirmed_stdin_bytes.to_string(),
                usage: None,
                cleanup,
                error: Some(error.to_owned()),
                effective_identity: None,
                reported_model: None,
                delivery: None,
            })
            .map(|_| ())
            .map_err(display_error)
    }

    fn status_for(status: &CodexRunStatus) -> ProviderOutcomeStatus {
        match status {
            CodexRunStatus::Completed => ProviderOutcomeStatus::Completed,
            CodexRunStatus::Stopped => ProviderOutcomeStatus::Stopped,
            CodexRunStatus::TimedOut => ProviderOutcomeStatus::TimedOut,
            CodexRunStatus::OutputLimit => ProviderOutcomeStatus::OutputLimit,
            CodexRunStatus::ProtocolFailure(_)
            | CodexRunStatus::ConsumerTooSlow
            | CodexRunStatus::ProcessUnavailable
            | CodexRunStatus::CleanupUnresolved => ProviderOutcomeStatus::Failed,
        }
    }

    fn error_for(status: &CodexRunStatus) -> Option<&'static str> {
        match status {
            CodexRunStatus::Completed | CodexRunStatus::Stopped => None,
            CodexRunStatus::TimedOut => Some("The one permitted Codex invocation timed out."),
            CodexRunStatus::OutputLimit => {
                Some("The one permitted Codex invocation reached the output limit.")
            }
            CodexRunStatus::ProtocolFailure(_)
            | CodexRunStatus::ConsumerTooSlow
            | CodexRunStatus::ProcessUnavailable => {
                Some("The one permitted Codex invocation failed.")
            }
            CodexRunStatus::CleanupUnresolved => Some("Codex cleanup could not be confirmed."),
        }
    }

    fn provider_usage(usage: &webnovel_core::providers::codex_exec::CodexUsage) -> ProviderUsage {
        ProviderUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_write_input_tokens: usage.cache_write_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        }
    }

    fn binding_output_limit(binding: &ProviderBinding) -> Result<usize, String> {
        binding.output_limit().map_err(|error| error.to_owned())
    }

    fn prefix(text: &str, limit: usize) -> &str {
        let mut end = text.len().min(limit);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    }

    fn canonical_temp_project(run_key: &str) -> Result<PathBuf, String> {
        let parent = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let path = parent.join(format!("webnovel-workshop-qualification-{run_key}"));
        if path.exists() {
            return Err("The synthetic qualification project path already exists.".into());
        }
        Ok(path)
    }

    fn report_path(run_key: &str) -> Result<PathBuf, String> {
        let repo = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .map_err(|error| error.to_string())?
            .join("..")
            .join("..")
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let directory = repo.join(REPORT_DIR);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        Ok(directory.join(format!("workshop-qualification-{run_key}.json")))
    }

    fn persist_report(path: &Path, report: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }

    fn display_error(error: impl std::fmt::Display) -> String {
        error.to_string()
    }
}

#[cfg(windows)]
fn main() {
    if windows::run().is_err() {
        std::process::exit(1);
    }
}
