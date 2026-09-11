//! Bounded fresh invocations over one durable story snapshot.
//! Intermediate protocol output is retained as evidence, never streamed into chat.
use crate::discussion_recovery::{
    DiscussionRecovery, PendingSave, SaveOutcome, invalid_lookup_report,
};
use webnovel_core::context::lookup::{
    LOOKUP_SCHEMA_VERSION, LookupEnvelope, LookupRead, LookupReadResult,
};
use webnovel_core::context::packet::{CompiledPacket, packet_input_hash, serialized_input};
use webnovel_core::projects::discussion_lookup::{
    LookupAdvance, LookupAdvanceRequest, LookupDispatch, LookupHaltRequest, LookupInvocationReport,
};
use webnovel_core::projects::discussions::{
    DiscussionDispatch, DiscussionRun, DiscussionRunStatus, ProviderCleanup, ProviderOutcomeStatus,
};
use webnovel_core::projects::{CoreError, CoreResult, ProjectSession};

fn active(run: &DiscussionRun) -> bool {
    matches!(
        run.status,
        DiscussionRunStatus::Queued | DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    )
}

fn exact_input(packet: &CompiledPacket) -> CoreResult<String> {
    let input = serialized_input(&packet.messages, &packet.options).map_err(|_| {
        CoreError::new(
            "InvalidLookupInput",
            "The saved lookup input could not be read.",
        )
    })?;
    if packet_input_hash(&packet.messages, &packet.options)
        .ok()
        .as_ref()
        != Some(&packet.receipt.input_hash)
    {
        return Err(CoreError::new(
            "InvalidLookupInput",
            "The saved lookup input could not be verified.",
        ));
    }
    Ok(input)
}

fn halt(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: &DiscussionRun,
    reason: &str,
) {
    if project
        .halt_lookup(LookupHaltRequest {
            owner: run.owner.clone(),
            reason: reason.into(),
        })
        .is_err()
    {
        recovery.retain(PendingSave {
            run: run.clone(),
            outcome: SaveOutcome::LookupHalt(reason.into()),
        });
    }
}

fn run_loop(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    dispatch: DiscussionDispatch,
    mut invoke: impl FnMut(&LookupDispatch, String) -> LookupInvocationReport,
) {
    let mut run = dispatch.run;
    let mut ordinal = "0".to_owned();
    // Rust also enforces the persisted allowance. This bound prevents a bad
    // worker response from turning a programming error into an unbounded loop.
    for _ in 0..3 {
        let grant = match project.claim_lookup_invocation(run.owner.clone(), ordinal.clone()) {
            Ok(grant) => grant,
            Err(_) => {
                halt(
                    &project,
                    &recovery,
                    &run,
                    "The next lookup could not start. No model call was replayed.",
                );
                return;
            }
        };
        run = grant.run.clone();
        let input = match exact_input(&grant.packet) {
            Ok(input) => input,
            Err(_) => {
                halt(
                    &project,
                    &recovery,
                    &run,
                    "The exact saved lookup input could not be verified.",
                );
                return;
            }
        };
        let report = invoke(&grant, input);
        run = match project.settle_lookup_invocation(report.clone()) {
            Ok(run) => run,
            Err(error) if invalid_lookup_report(&error) => {
                halt(
                    &project,
                    &recovery,
                    &run,
                    "The lookup response could not be accepted. Its external outcome remains unconfirmed; no model call was replayed.",
                );
                return;
            }
            Err(_) => {
                // No next invocation after an uncertain result write. An
                // explicit local retry records the receipt and ends the chain.
                recovery.retain(PendingSave {
                    run,
                    outcome: SaveOutcome::Lookup(Box::new(report)),
                });
                return;
            }
        };
        if !active(&run) {
            return;
        }
        match project.advance_lookup(LookupAdvanceRequest {
            owner: run.owner.clone(),
            completed_ordinal: ordinal,
        }) {
            Ok(LookupAdvance::Prepared { dispatch }) => {
                ordinal = dispatch.ordinal;
                run = dispatch.run;
            }
            Ok(LookupAdvance::Finished { .. }) => return,
            Err(_) => {
                halt(
                    &project,
                    &recovery,
                    &run,
                    "The next story lookup could not be prepared. The saved evidence is retained.",
                );
                return;
            }
        }
    }
    halt(
        &project,
        &recovery,
        &run,
        "The request reached its lookup allowance. No further model call was made.",
    );
}

pub(super) fn run_mock(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    dispatch: DiscussionDispatch,
) {
    run_loop(project, recovery, dispatch, |grant, input| {
        std::thread::sleep(std::time::Duration::from_millis(150));
        LookupInvocationReport {
            owner: grant.run.owner.clone(),
            ordinal: grant.ordinal.clone(),
            event_id: format!("{}-lookup-{}-finish", grant.run.id, grant.ordinal),
            assistant_text: mock_envelope(&grant.packet),
            binding: None,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: input.len().to_string(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
        }
    });
}

fn mock_envelope(packet: &CompiledPacket) -> String {
    let Some(lookup) = &packet.receipt.lookup else {
        return String::new();
    };
    if webnovel_core::context::lookup::reviewed_memory_enabled(Some(lookup))
        && packet.messages.last().is_some_and(|message| {
            message
                .content
                .to_lowercase()
                .contains("look up reviewed memory")
        })
    {
        return mock_memory_envelope(packet, lookup);
    }
    let schema_version = LOOKUP_SCHEMA_VERSION.into();
    let response = if lookup.completed_invocations == 0
        && lookup.allowance.max_additional_invocations > 0
    {
        let instruction = packet
            .messages
            .last()
            .map(|message| message.content.as_str())
            .unwrap_or("");
        let query = instruction
            .split('"')
            .nth(1)
            .filter(|query| !query.trim().is_empty() && query.len() <= 512)
            .unwrap_or("promise")
            .to_owned();
        LookupEnvelope::NeedsContext {
            schema_version,
            reads: vec![LookupRead::Search {
                id: "local-search".into(),
                query,
                mode: webnovel_core::projects::story_context::SearchMode::Literal,
                limit: 3,
            }],
        }
    } else if lookup.completed_invocations < lookup.allowance.max_additional_invocations
        && let Some(hit) = lookup
            .exchanges
            .iter()
            .find_map(|exchange| match &exchange.result {
                LookupReadResult::Search { result } => result.hits.first(),
                _ => None,
            })
    {
        LookupEnvelope::NeedsContext {
            schema_version,
            reads: vec![LookupRead::Read {
                id: "local-read".into(),
                handle: hit.passage.handle.clone(),
                block_ids: Some(vec![hit.passage.block_id.clone()]),
            }],
        }
    } else {
        let evidence = lookup
            .exchanges
            .iter()
            .rev()
            .find_map(|exchange| match &exchange.result {
                LookupReadResult::Read { passages, .. } => {
                    passages.first().map(|passage| passage.text.as_str())
                }
                LookupReadResult::Search { result } => {
                    result.hits.first().map(|hit| hit.passage.text.as_str())
                }
                _ => None,
            });
        let text = match evidence {
            Some(text) => format!("Local test assistant: I read this exact saved passage:\n\n{text}\n\nThis demonstrates read-only story lookup. No live AI model was called. These observations do not establish complete story state."),
            None => "Local test assistant: no matching passage was supplied by this lookup. A missing match does not establish that an event never happened. No live AI model was called.".into(),
        };
        LookupEnvelope::Discussion {
            schema_version,
            text,
        }
    };
    serde_json::to_string(&response).expect("fixed local lookup envelope")
}

/// Deterministic offline exercise of the same read boundary as the live
/// adapter. Exact entity identities come from returned catalog entries.
fn mock_memory_envelope(
    packet: &CompiledPacket,
    lookup: &webnovel_core::context::lookup::LookupPacketInput,
) -> String {
    use webnovel_core::context::lookup::MemoryEntityKind;
    let mut reads = Vec::new();
    if lookup.completed_invocations < lookup.allowance.max_additional_invocations {
        if lookup.completed_invocations == 0 {
            let instruction = packet
                .messages
                .last()
                .map(|item| item.content.as_str())
                .unwrap_or("");
            let queries: Vec<_> = instruction
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|query| !query.trim().is_empty() && query.len() <= 512)
                .collect();
            for (index, (entity_kind, fallback)) in [
                (MemoryEntityKind::Character, "Mei"),
                (MemoryEntityKind::Object, "key"),
                (MemoryEntityKind::Promise, "return"),
            ]
            .into_iter()
            .enumerate()
            {
                reads.push(LookupRead::FindEntities {
                    id: format!("local-entity-{index}"),
                    entity_kind,
                    query: queries.get(index).copied().unwrap_or(fallback).to_owned(),
                    offset: 0,
                    limit: 3,
                });
            }
        } else {
            for (index, exchange) in lookup.exchanges.iter().enumerate() {
                let LookupReadResult::FindEntities {
                    entity_kind,
                    entries,
                    ..
                } = &exchange.result
                else {
                    continue;
                };
                let Some(entry) = entries.first() else {
                    continue;
                };
                let id = format!("local-history-{index}");
                reads.push(match entity_kind {
                    MemoryEntityKind::Character => LookupRead::KnowledgeHistory {
                        id,
                        character_id: entry.entity.id.clone(),
                        topic_id: None,
                        offset: 0,
                        limit: 3,
                    },
                    MemoryEntityKind::Object => LookupRead::PossessionHistory {
                        id,
                        object_id: entry.entity.id.clone(),
                        offset: 0,
                        limit: 3,
                    },
                    MemoryEntityKind::Promise => LookupRead::PromiseHistory {
                        id,
                        promise_id: entry.entity.id.clone(),
                        offset: 0,
                        limit: 3,
                    },
                    MemoryEntityKind::Topic => continue,
                });
            }
        }
    }
    let response = if reads.is_empty() {
        let mut evidence = Vec::new();
        for exchange in &lookup.exchanges {
            match &exchange.result {
                LookupReadResult::KnowledgeHistory { history, .. } => {
                    if let Some(item) = history.observations.first() {
                        evidence.push(format!(
                            "{}: {} Recorded attitude: {:?}. Evidence: “{}”",
                            item.source_display_name,
                            item.statement,
                            item.attitude,
                            item.evidence.quote
                        ));
                    }
                }
                LookupReadResult::PossessionHistory { history, .. } => {
                    if let Some(item) = history.observations.first() {
                        evidence.push(format!(
                            "{}: {} is recorded with {}. Evidence: “{}”",
                            item.source_display_name,
                            item.object.label,
                            item.holder
                                .as_ref()
                                .map(|holder| holder.label.as_str())
                                .unwrap_or("no named holder"),
                            item.evidence.quote
                        ));
                    }
                }
                LookupReadResult::PromiseHistory { history, .. } => {
                    if let Some(item) = history.observations.first() {
                        evidence.push(format!(
                            "{}: {} ({:?}). Evidence: “{}”",
                            item.source_display_name,
                            item.promise.label,
                            item.phase,
                            item.evidence.quote
                        ));
                    }
                }
                _ => {}
            }
        }
        LookupEnvelope::Discussion {
            schema_version: LOOKUP_SCHEMA_VERSION.into(),
            text: format!(
                "Local test assistant: reviewed story lookup.\n\n{}\n\nRecorded history is incomplete. Beliefs are not world facts, and missing transfers or payoffs do not establish absence. No live AI model was called.",
                if evidence.is_empty() {
                    "No history observations were delivered.".into()
                } else {
                    evidence.join("\n\n")
                }
            ),
        }
    } else {
        LookupEnvelope::NeedsContext {
            schema_version: LOOKUP_SCHEMA_VERSION.into(),
            reads,
        }
    };
    serde_json::to_string(&response).expect("fixed local memory lookup envelope")
}

#[cfg(windows)]
pub(super) fn run_live(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: crate::provider_runtime::DesktopProviders,
    connection: Option<webnovel_core::providers::codex_runtime::CodexConnection>,
    dispatch: DiscussionDispatch,
    stop: webnovel_core::providers::cli::windows_process::StopSignal,
) {
    struct Registration(
        crate::provider_runtime::DesktopProviders,
        webnovel_core::projects::discussions::RunOwner,
    );
    impl Drop for Registration {
        fn drop(&mut self) {
            self.0.release(&self.1);
        }
    }
    let _registration = Registration(runtime, dispatch.run.owner.clone());
    run_loop(project, recovery, dispatch, |grant, input| {
        let matched = connection.as_ref().filter(|connection| {
            grant
                .packet
                .options
                .provider_binding
                .as_ref()
                .is_some_and(|binding| {
                    crate::provider_runtime::connection_matches_binding(connection, binding)
                })
        });
        let result = collect_live(
            matched,
            grant.packet.options.provider_binding.as_ref(),
            input,
            stop.clone(),
        );
        let legacy = crate::live_discussion::report(&grant.run, result);
        LookupInvocationReport {
            owner: grant.run.owner.clone(),
            ordinal: grant.ordinal.clone(),
            event_id: format!("{}-lookup-{}-finish", grant.run.id, grant.ordinal),
            assistant_text: legacy.assistant_text,
            binding: grant.packet.options.provider_binding.clone(),
            status: legacy.status,
            confirmed_stdin_bytes: legacy.confirmed_stdin_bytes,
            usage: legacy.usage,
            cleanup: legacy.cleanup,
            error: legacy.error,
        }
    });
}

#[cfg(windows)]
fn collect_live(
    connection: Option<&webnovel_core::providers::codex_runtime::CodexConnection>,
    binding: Option<&webnovel_core::context::packet::ProviderBinding>,
    input: String,
    stop: webnovel_core::providers::cli::windows_process::StopSignal,
) -> webnovel_core::providers::codex_runner::CodexRunResult {
    use std::time::Duration;
    use webnovel_core::providers::codex_runner::{
        CodexRunResult, CodexRunStatus, CodexStreamEvent,
    };
    let failed = |status, assistant_text, cleanup_settled| CodexRunResult {
        status,
        assistant_text,
        usage: None,
        confirmed_stdin_bytes: 0,
        warning_count: 0,
        cleanup_settled,
    };
    if stop.is_requested() {
        return failed(CodexRunStatus::Stopped, String::new(), true);
    }
    let (Some(connection), Some(binding)) = (connection, binding) else {
        return failed(CodexRunStatus::ProcessUnavailable, String::new(), true);
    };
    let mut stream = match connection.start_bound(binding, input.into_bytes(), stop.clone()) {
        Ok(stream) => stream,
        Err(_) => return failed(CodexRunStatus::ProcessUnavailable, String::new(), true),
    };
    let mut observed = String::new();
    let mut limited = false;
    loop {
        match stream.next_event(Duration::from_millis(100)) {
            Ok(None) => {}
            Ok(Some(CodexStreamEvent::AssistantDelta(delta))) => {
                let remaining = (64 * 1024usize).saturating_sub(observed.len());
                let mut end = delta.len().min(remaining);
                while !delta.is_char_boundary(end) {
                    end -= 1;
                }
                observed.push_str(&delta[..end]);
                if end != delta.len() {
                    limited = true;
                    stop.request_stop();
                }
            }
            Ok(Some(CodexStreamEvent::Finished(mut result))) => {
                if !result.assistant_text.starts_with(&observed) {
                    result.status = CodexRunStatus::ProtocolFailure(
                        webnovel_core::providers::codex_exec::CodexFailureCode::Protocol,
                    );
                    result.assistant_text = observed;
                }
                if limited || result.assistant_text.len() > 64 * 1024 {
                    result.status = CodexRunStatus::OutputLimit;
                    let mut end = result.assistant_text.len().min(64 * 1024);
                    while !result.assistant_text.is_char_boundary(end) {
                        end -= 1;
                    }
                    result.assistant_text.truncate(end);
                }
                return result;
            }
            Err(_) => {
                stop.request_stop();
                return failed(CodexRunStatus::CleanupUnresolved, observed, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webnovel_core::context::lookup::LookupAllowance;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::projects::discussions::{FeedbackIntent, StartDiscussion};
    use webnovel_core::projects::{CreateDocument, ProjectAccess};

    fn fixture(
        label: &str,
    ) -> (
        ProjectSession,
        ProjectAccess,
        DiscussionDispatch,
        DiscussionRecovery,
    ) {
        let path = std::env::temp_dir().join(format!(
            "wns-lookup-worker-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Lookup worker test").unwrap();
        let access = project.documents().attach("lookup-session".into()).unwrap();
        let document = project.documents().create(CreateDocument {
            access: access.clone(), operation_id: "create".into(), document_id: "chapter".into(), title: "Promise".into(), kind: "chapter".into(),
            body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"Mei made a promise to return the brass key."}]}]}}),
        }).unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                access: access.clone(),
                operation_id: "start".into(),
                expected: document.head,
                instruction: "Find the old \"promise\" and read its exact passage.".into(),
                intent: FeedbackIntent::Discuss,
                basis: None,
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                previous_run_id: None,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: None,
                lookup: Some(LookupAllowance::default()),
            })
            .unwrap();
        let recovery = DiscussionRecovery::default();
        let dispatch = recovery.claim(&project, &started.run).unwrap();
        (project, access, dispatch, recovery)
    }

    fn completed(grant: &LookupDispatch, input: &str) -> LookupInvocationReport {
        LookupInvocationReport {
            owner: grant.run.owner.clone(),
            ordinal: grant.ordinal.clone(),
            event_id: format!("test-{}", grant.ordinal),
            assistant_text: mock_envelope(&grant.packet),
            binding: None,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: input.len().to_string(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
        }
    }

    fn clean(project: ProjectSession) {
        let path = project.path.clone();
        let canonical = std::fs::canonicalize(&path).unwrap();
        assert!(canonical.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()));
        assert!(
            canonical
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("wns-lookup-worker-")
        );
        drop(project);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn lookup_worker_uses_three_saved_packets_and_only_publishes_final_text() {
        let (project, access, dispatch, recovery) = fixture("complete");
        let initial = dispatch.packet.receipt.packet_id.clone();
        let target = dispatch.run.target.clone();
        let mut calls = 0;
        run_loop(project.clone(), recovery, dispatch, |grant, input| {
            calls += 1;
            let view = project
                .read_discussion(access.clone(), "chapter".into())
                .unwrap();
            assert_eq!(
                view.messages.len(),
                1,
                "intermediate JSON must not enter chat"
            );
            completed(grant, &input)
        });
        assert_eq!(calls, 3);
        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
        assert_eq!(view.messages.len(), 2);
        assert!(view.messages[1].content.contains("Mei made a promise"));
        assert!(!view.messages[1].content.contains("needsContext"));
        assert!(view.messages[1].packet_id.is_some());
        assert_ne!(view.messages[1].packet_id.as_ref(), Some(&initial));
        assert_eq!(
            project.documents().read(access, "chapter".into()).unwrap().head,
            target
        );
        let path = project.path.clone();
        drop(project);
        let reopened = ProjectSession::open(path).unwrap();
        let access = reopened.documents().attach("reopened-session".into()).unwrap();
        let saved = reopened.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(saved.messages[1].content, view.messages[1].content);
        assert_eq!(saved.runs[0].lookup.as_ref().unwrap().invocations.len(), 3);
        clean(reopened);
    }

    #[test]
    fn failed_lookup_receipt_write_and_local_retry_never_dispatch_another_call() {
        let (project, access, dispatch, recovery) = fixture("receipt-fault");
        let id = dispatch.run.id.clone();
        let db = rusqlite::Connection::open(project.path.join("project.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER lookup_fault BEFORE INSERT ON discussion_lookup_results BEGIN SELECT RAISE(ABORT,'synthetic lookup result failure'); END;").unwrap();
        let mut calls = 0;
        run_loop(
            project.clone(),
            recovery.clone(),
            dispatch,
            |grant, input| {
                calls += 1;
                completed(grant, &input)
            },
        );
        assert_eq!(calls, 1);
        assert!(
            recovery
                .retry(&project, access.clone(), "chapter".into(), id.clone())
                .is_err()
        );
        db.execute_batch("DROP TRIGGER lookup_fault;").unwrap();
        recovery
            .retry(&project, access.clone(), "chapter".into(), id.clone())
            .unwrap();
        recovery
            .retry(&project, access.clone(), "chapter".into(), id)
            .unwrap();
        let view = project.read_discussion(access, "chapter".into()).unwrap();
        assert!(!active(&view.runs[0]));
        assert_eq!(view.runs[0].lookup.as_ref().unwrap().invocations.len(), 1);
        assert!(
            view.messages
                .iter()
                .all(|message| !message.content.contains("needsContext"))
        );
        assert_eq!(calls, 1);
        drop(db);
        clean(project);
    }

    #[test]
    fn stop_while_response_finishes_prevents_an_expansion_call() {
        let (project, access, dispatch, recovery) = fixture("stop");
        let mut calls = 0;
        run_loop(project.clone(), recovery, dispatch, |grant, input| {
            calls += 1;
            project
                .stop_discussion(access.clone(), grant.run.id.clone())
                .unwrap();
            completed(grant, &input)
        });
        assert_eq!(calls, 1);
        let view = project.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Stopped);
        assert!(
            view.messages
                .iter()
                .all(|message| !message.content.contains("needsContext"))
        );
        assert_eq!(view.runs[0].lookup.as_ref().unwrap().invocations.len(), 1);
        clean(project);
    }

    #[test]
    fn rejected_provider_report_halts_instead_of_offering_an_impossible_save_retry() {
        let (project, access, dispatch, recovery) = fixture("rejected-report");
        let id = dispatch.run.id.clone();
        let mut calls = 0;
        run_loop(
            project.clone(),
            recovery.clone(),
            dispatch,
            |grant, input| {
                calls += 1;
                let mut report = completed(grant, &input);
                report.confirmed_stdin_bytes = "1".into();
                report
            },
        );
        assert_eq!(calls, 1);
        let current = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert!(!active(&current.runs[0]));
        let view = recovery
            .retry(&project, access, "chapter".into(), id)
            .unwrap();
        assert_eq!(
            serde_json::to_value(view).unwrap()["workerIssues"],
            serde_json::json!([])
        );
        clean(project);
    }
}
