//! One opt-in, one-process comparison of the current Codex exec and app-server
//! transports.  This is a small diagnostic sample, not Stage E qualification.
//!
//! The default invocation is inert.  Set `WNS_V3_ALLOW_TRANSPORT_COMPARISON=1`
//! to permit one synthetic prompt through exec, a cold app-server thread, and
//! a warm app-server thread.  The harness never retries, substitutes a model,
//! or writes author/project state.  It writes only a local JSON observation.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(not(windows))]
fn main() {
    println!("Codex transport comparison is supported only on Windows hosts");
}

#[cfg(windows)]
mod windows {
    use serde_json::{Map, Value, json};
    use sha2::{Digest, Sha256};
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use uuid::Uuid;
    use webnovel_core::context::packet::{
        CODEX_MAINTENANCE_MODEL_ID, CODEX_MAINTENANCE_REASONING, CODEX_SERVICE_TIER,
        ProviderBinding,
    };
    use webnovel_core::projects::{CoreError, CoreResult};
    use webnovel_core::providers::cli::windows_process::StopSignal;
    use webnovel_core::providers::codex_app_server::connection::ManagedAppServer;
    use webnovel_core::providers::codex_app_server::runtime::AppServerStreamEvent;
    use webnovel_core::providers::codex_exec::CodexUsage;
    use webnovel_core::providers::codex_runner::{CodexRunStatus, CodexStreamEvent};
    use webnovel_core::providers::codex_runtime::CodexConnection;
    use webnovel_core::providers::preferences::ModelSelection;

    const ALLOW_FLAG: &str = "WNS_V3_ALLOW_TRANSPORT_COMPARISON";
    const REPORT_DIR: &str = ".local";
    const SOFT_DEADLINE: Duration = Duration::from_secs(90);
    const SETTLEMENT_DEADLINE: Duration = Duration::from_secs(30);
    const MAX_REPORTED_TEXT_BYTES: usize = 8 * 1024;
    const SYNTHETIC_PACKET: &str = "Summarize this fictional note in one concise paragraph. The lantern market opens at dusk, the archivist records one new promise, and the courier leaves before the rain. Return at most 100 words, with no bullet list, no tools, and no questions.";

    #[derive(Clone)]
    struct ReportStore {
        path: PathBuf,
        value: Arc<Mutex<Value>>,
    }

    impl ReportStore {
        fn new(path: PathBuf, value: Value) -> Result<Self, String> {
            let store = Self {
                path,
                value: Arc::new(Mutex::new(value)),
            };
            store.update(|_| {})?;
            Ok(store)
        }

        fn update(&self, update: impl FnOnce(&mut Value)) -> Result<(), String> {
            let mut value = self
                .value
                .lock()
                .map_err(|_| "comparison report lock was poisoned".to_owned())?;
            update(&mut value);
            persist_report(&self.path, &value)
        }

        fn callback_update(&self, update: impl FnOnce(&mut Value)) -> CoreResult<()> {
            self.update(update)
                .map_err(|error| CoreError::new("QualificationReportUnavailable", &error))
        }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn elapsed_ms(started: &Instant) -> u64 {
        started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn selection() -> ModelSelection {
        ModelSelection {
            provider_id: "codex".into(),
            model_id: CODEX_MAINTENANCE_MODEL_ID.into(),
            reasoning: Some(CODEX_MAINTENANCE_REASONING.into()),
            service_tier: Some(CODEX_SERVICE_TIER.into()),
        }
    }

    fn requested_selection_value() -> Value {
        json!({
            "providerId": "codex",
            "modelId": CODEX_MAINTENANCE_MODEL_ID,
            "reasoning": CODEX_MAINTENANCE_REASONING,
            "serviceTier": CODEX_SERVICE_TIER,
        })
    }

    fn new_case(name: &str, input: &[u8]) -> Value {
        json!({
            "name": name,
            "status": "pending",
            "attempted": false,
            "startCommitted": false,
            "inputBytes": input.len(),
            "inputSha256": sha256_hex(input),
            "requested": requested_selection_value(),
            "firstVisibleMeaning": Value::Null,
            "firstVisibleMs": Value::Null,
            "terminalMs": Value::Null,
            "cleanupMs": Value::Null,
            "outputDeltaBytes": 0,
            "usage": Value::Null,
            "failure": Value::Null,
            "localFailure": Value::Null,
            "effectiveModel": Value::Null,
            "effectiveReasoning": Value::Null,
            "effectiveServiceTier": Value::Null,
            "inferenceCount": Value::Null,
            "turnCount": 0,
        })
    }

    #[derive(Clone, Copy, Default)]
    struct CaseOutcome {
        ok: bool,
        attempted: bool,
    }

    fn update_case(
        store: &ReportStore,
        index: usize,
        update: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<(), String> {
        store.update(|report| {
            let Some(case) = report
                .get_mut("cases")
                .and_then(Value::as_array_mut)
                .and_then(|cases| cases.get_mut(index))
                .and_then(Value::as_object_mut)
            else {
                return;
            };
            update(case);
        })
    }

    fn update_case_callback(
        store: &ReportStore,
        index: usize,
        update: impl FnOnce(&mut Map<String, Value>),
    ) -> CoreResult<()> {
        store.callback_update(|report| {
            let Some(case) = report
                .get_mut("cases")
                .and_then(Value::as_array_mut)
                .and_then(|cases| cases.get_mut(index))
                .and_then(Value::as_object_mut)
            else {
                return;
            };
            update(case);
        })
    }

    fn usage_value(usage: Option<&CodexUsage>) -> Value {
        usage.map_or(Value::Null, |usage| {
            json!({
                "inputTokens": usage.input_tokens,
                "cachedInputTokens": usage.cached_input_tokens,
                "cacheWriteInputTokens": usage.cache_write_input_tokens,
                "outputTokens": usage.output_tokens,
                "reasoningOutputTokens": usage.reasoning_output_tokens,
            })
        })
    }

    fn safe_status(status: &CodexRunStatus) -> &'static str {
        match status {
            CodexRunStatus::Completed => "completed",
            CodexRunStatus::Stopped => "stopped",
            CodexRunStatus::TimedOut => "timed-out",
            CodexRunStatus::OutputLimit => "output-limit",
            CodexRunStatus::ProtocolFailure(_) => "protocol-failure",
            CodexRunStatus::ConsumerTooSlow => "consumer-too-slow",
            CodexRunStatus::ProcessUnavailable => "process-unavailable",
            CodexRunStatus::CleanupUnresolved => "cleanup-unresolved",
        }
    }

    fn report_text(text: &str) -> Value {
        let mut end = text.len().min(MAX_REPORTED_TEXT_BYTES);
        while !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        json!({
            "text": &text[..end],
            "bytes": text.len(),
            "sha256": sha256_hex(text.as_bytes()),
        })
    }

    fn run_exec_case(
        store: &ReportStore,
        index: usize,
        connection: &CodexConnection,
        binding: &ProviderBinding,
        packet: &[u8],
        comparison_started: &Instant,
    ) -> CaseOutcome {
        let case_started = Instant::now();
        if update_case(store, index, |case| {
            case.insert("status".into(), json!("preparing"));
            case.insert("transport".into(), json!("exec"));
            case.insert(
                "caseStartedAtMs".into(),
                json!(elapsed_ms(comparison_started)),
            );
        })
        .is_err()
        {
            return CaseOutcome::default();
        }
        let prep_started = Instant::now();
        if update_case(store, index, |case| {
            case.insert("status".into(), json!("prepared"));
            case.insert("prepMs".into(), json!(elapsed_ms(&prep_started)));
            case.insert("submissionDurability".into(), json!("written-before-start"));
            case.insert("startCommitted".into(), json!(true));
            case.insert("attempted".into(), json!(true));
        })
        .is_err()
        {
            return CaseOutcome::default();
        }
        let attempted = true;
        let stop = StopSignal::new();
        let start_started = Instant::now();
        let mut stream = match connection.start_bound(binding, packet.to_vec(), stop.clone()) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("start-failed"));
                    case.insert("startMs".into(), json!(elapsed_ms(&start_started)));
                    case.insert("failure".into(), json!({"code": error.code}));
                });
                return CaseOutcome {
                    ok: false,
                    attempted,
                };
            }
        };
        let mut first_visible_ms = None;
        let mut delta_bytes = 0_u64;
        let _ = update_case(store, index, |case| {
            case.insert("status".into(), json!("running"));
            case.insert("startMs".into(), json!(elapsed_ms(&start_started)));
            case.insert("firstVisibleMeaning".into(), json!("completed-message"));
        });

        let soft_deadline = Instant::now() + SOFT_DEADLINE;
        let mut settlement_deadline = None;
        loop {
            let now = Instant::now();
            if settlement_deadline.is_none() && now >= soft_deadline {
                stream.request_stop();
                settlement_deadline = Some(now + SETTLEMENT_DEADLINE);
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("stopping"));
                    case.insert("stopRequestedMs".into(), json!(elapsed_ms(&case_started)));
                });
            }
            if settlement_deadline.is_some_and(|deadline| now >= deadline) {
                stop.request_stop();
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("timed-out-unsettled"));
                    case.insert("cleanupSettled".into(), json!(false));
                    case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                });
                return CaseOutcome {
                    ok: false,
                    attempted,
                };
            }
            let wait = settlement_deadline.map_or(Duration::from_millis(100), |deadline| {
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(100))
            });
            match stream.next_event(wait) {
                Ok(None) => {}
                Ok(Some(CodexStreamEvent::AssistantDelta(delta))) => {
                    delta_bytes = delta_bytes.saturating_add(delta.len() as u64);
                    if !delta.is_empty() && first_visible_ms.is_none() {
                        first_visible_ms = Some(elapsed_ms(&case_started));
                    }
                }
                Ok(Some(CodexStreamEvent::Finished(result))) => {
                    let terminal_ms = elapsed_ms(&case_started);
                    if first_visible_ms.is_none() && !result.assistant_text.is_empty() {
                        first_visible_ms = Some(terminal_ms);
                    }
                    let _ = update_case(store, index, |case| {
                        case.insert("status".into(), json!(safe_status(&result.status)));
                        case.insert("terminalMs".into(), json!(terminal_ms));
                        case.insert(
                            "firstVisibleMs".into(),
                            first_visible_ms.map_or(Value::Null, Value::from),
                        );
                        case.insert("cleanupSettled".into(), json!(result.cleanup_settled));
                        case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                        case.insert(
                            "confirmedStdinBytes".into(),
                            json!(result.confirmed_stdin_bytes),
                        );
                        case.insert("outputDeltaBytes".into(), json!(delta_bytes));
                        case.insert("warningCount".into(), json!(result.warning_count));
                        case.insert("usage".into(), usage_value(result.usage.as_ref()));
                        case.insert("output".into(), report_text(&result.assistant_text));
                        case.insert("turnCount".into(), json!(1));
                        if !matches!(result.status, CodexRunStatus::Completed) {
                            case.insert(
                                "failure".into(),
                                json!({"status": format!("{:?}", result.status)}),
                            );
                        }
                    });
                    return CaseOutcome {
                        ok: result.status == CodexRunStatus::Completed && result.cleanup_settled,
                        attempted,
                    };
                }
                Err(error) => {
                    let _ = update_case(store, index, |case| {
                        case.insert("status".into(), json!("stream-failed"));
                        case.insert("cleanupSettled".into(), json!(false));
                        case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                        case.insert("outputDeltaBytes".into(), json!(delta_bytes));
                        case.insert("failure".into(), json!({"code": error}));
                    });
                    return CaseOutcome {
                        ok: false,
                        attempted,
                    };
                }
            }
        }
    }

    fn run_app_server_case(
        store: &ReportStore,
        index: usize,
        server: &ManagedAppServer,
        binding: &ProviderBinding,
        packet: &str,
        comparison_started: &Instant,
    ) -> CaseOutcome {
        let case_started = Instant::now();
        if update_case(store, index, |case| {
            case.insert("status".into(), json!("preparing"));
            case.insert("transport".into(), json!("app-server"));
            case.insert(
                "caseStartedAtMs".into(),
                json!(elapsed_ms(comparison_started)),
            );
        })
        .is_err()
        {
            return CaseOutcome::default();
        }
        let prep_started = Instant::now();
        let request = match server.reserve(binding) {
            Ok(request) => request,
            Err(error) => {
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("reserve-failed"));
                    case.insert("prepMs".into(), json!(elapsed_ms(&prep_started)));
                    case.insert("failure".into(), json!({"code": error.code}));
                });
                return CaseOutcome::default();
            }
        };
        if update_case(store, index, |case| {
            case.insert("status".into(), json!("prepared"));
            case.insert("prepMs".into(), json!(elapsed_ms(&prep_started)));
            case.insert(
                "submissionDurability".into(),
                json!("written-before-reservation-start"),
            );
            case.insert("startCommitted".into(), json!(true));
            case.insert("attempted".into(), json!(true));
        })
        .is_err()
        {
            return CaseOutcome::default();
        }
        let attempted = true;
        let dispatch_holder = Arc::new(Mutex::new(None));
        let turn_holder = Arc::new(Mutex::new(None));
        let before_store = store.clone();
        let before_dispatch = Arc::clone(&dispatch_holder);
        let before_turn =
            move |dispatch: &webnovel_core::providers::codex_app_server::AppServerDispatch| {
                if let Ok(mut saved) = before_dispatch.lock() {
                    *saved = Some(dispatch.clone());
                }
                update_case_callback(&before_store, index, |case| {
                    case.insert("status".into(), json!("dispatch-claimed"));
                    case.insert(
                        "dispatchClaimed".into(),
                        serde_json::to_value(dispatch).unwrap_or(Value::Null),
                    );
                    case.insert("dispatchClaimedMs".into(), json!(elapsed_ms(&case_started)));
                })
            };
        let turn_store = store.clone();
        let on_turn_holder = Arc::clone(&turn_holder);
        let on_turn =
            move |dispatch: &webnovel_core::providers::codex_app_server::AppServerDispatch,
                  turn_id: &str| {
                if let Ok(mut saved) = on_turn_holder.lock() {
                    *saved = Some(turn_id.to_owned());
                }
                update_case_callback(&turn_store, index, |case| {
                    case.insert("status".into(), json!("turn-claimed"));
                    case.insert("turnClaimed".into(), json!(turn_id));
                    case.insert(
                        "dispatchAtTurnClaim".into(),
                        serde_json::to_value(dispatch).unwrap_or(Value::Null),
                    );
                    case.insert("turnClaimedMs".into(), json!(elapsed_ms(&case_started)));
                })
            };
        let stop = StopSignal::new();
        let start_started = Instant::now();
        let mut stream = match request.reservation.start(
            request.binding,
            packet.to_owned(),
            request.thread,
            stop.clone(),
            before_turn,
            on_turn,
        ) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("start-failed"));
                    case.insert("startMs".into(), json!(elapsed_ms(&start_started)));
                    case.insert("failure".into(), json!({"code": error.code}));
                });
                return CaseOutcome {
                    ok: false,
                    attempted,
                };
            }
        };
        let mut first_visible_ms = None;
        let mut delta_bytes = 0_u64;
        let _ = update_case(store, index, |case| {
            case.insert("status".into(), json!("running"));
            case.insert("startMs".into(), json!(elapsed_ms(&start_started)));
            case.insert("firstVisibleMeaning".into(), json!("assistant-delta"));
        });

        let soft_deadline = Instant::now() + SOFT_DEADLINE;
        let mut settlement_deadline = None;
        loop {
            let now = Instant::now();
            if settlement_deadline.is_none() && now >= soft_deadline {
                stream.request_stop();
                settlement_deadline = Some(now + SETTLEMENT_DEADLINE);
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("stopping"));
                    case.insert("stopRequestedMs".into(), json!(elapsed_ms(&case_started)));
                });
            }
            if settlement_deadline.is_some_and(|deadline| now >= deadline) {
                stop.request_stop();
                let _ = update_case(store, index, |case| {
                    case.insert("status".into(), json!("timed-out-unsettled"));
                    case.insert("cleanupSettled".into(), json!(false));
                    case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                });
                return CaseOutcome {
                    ok: false,
                    attempted,
                };
            }
            let wait = settlement_deadline.map_or(Duration::from_millis(100), |deadline| {
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(100))
            });
            match stream.next_event(wait) {
                Ok(None) => {}
                Ok(Some(AppServerStreamEvent::AssistantDelta(delta))) => {
                    delta_bytes = delta_bytes.saturating_add(delta.len() as u64);
                    if !delta.is_empty() && first_visible_ms.is_none() {
                        first_visible_ms = Some(elapsed_ms(&case_started));
                    }
                }
                Ok(Some(AppServerStreamEvent::Finished(finished))) => {
                    let finished = *finished;
                    let result = finished.result;
                    let delivery = finished.delivery;
                    let failure = finished.failure;
                    let local_failure = finished.local_failure;
                    let terminal_ms = elapsed_ms(&case_started);
                    let _ = update_case(store, index, |case| {
                        case.insert("status".into(), json!(safe_status(&result.status)));
                        case.insert("terminalMs".into(), json!(terminal_ms));
                        case.insert("cleanupSettled".into(), json!(result.cleanup_settled));
                        case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                        case.insert(
                            "firstVisibleMs".into(),
                            first_visible_ms.map_or(Value::Null, Value::from),
                        );
                        case.insert("outputDeltaBytes".into(), json!(delta_bytes));
                        case.insert(
                            "confirmedStdinBytes".into(),
                            json!(result.confirmed_stdin_bytes),
                        );
                        case.insert("warningCount".into(), json!(result.warning_count));
                        case.insert("usage".into(), usage_value(result.usage.as_ref()));
                        case.insert(
                            "delivery".into(),
                            serde_json::to_value(&delivery).unwrap_or(Value::Null),
                        );
                        case.insert("output".into(), report_text(&result.assistant_text));
                        case.insert("turnCount".into(), json!(u8::from(delivery.submission == webnovel_core::providers::codex_app_server::AppServerSubmission::Acknowledged)));
                        if let Some(failure) = failure {
                            case.insert(
                                "failure".into(),
                                json!({
                                    "code": failure.code,
                                    "httpStatusCode": failure.http_status_code,
                                }),
                            );
                        }
                        if let Some(local_failure) = local_failure {
                            case.insert("localFailure".into(), json!(local_failure.as_str()));
                        } else if !matches!(result.status, CodexRunStatus::Completed) {
                            case.insert(
                                "failure".into(),
                                json!({"status": format!("{:?}", result.status)}),
                            );
                        }
                    });
                    return CaseOutcome {
                        ok: result.status == CodexRunStatus::Completed && result.cleanup_settled,
                        attempted,
                    };
                }
                Err(error) => {
                    let dispatch = dispatch_holder.lock().ok().and_then(|saved| saved.clone());
                    let turn_id = turn_holder.lock().ok().and_then(|saved| saved.clone());
                    let _ = update_case(store, index, |case| {
                        case.insert("status".into(), json!("stream-failed"));
                        case.insert("cleanupSettled".into(), json!(false));
                        case.insert("cleanupMs".into(), json!(elapsed_ms(&case_started)));
                        case.insert("outputDeltaBytes".into(), json!(delta_bytes));
                        case.insert("failure".into(), json!({"code": error}));
                        case.insert(
                            "deliveryEvidence".into(),
                            json!({"dispatch": dispatch, "turnId": turn_id}),
                        );
                    });
                    return CaseOutcome {
                        ok: false,
                        attempted,
                    };
                }
            }
        }
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
        Ok(directory.join(format!("codex-transport-comparison-{run_key}.json")))
    }

    fn persist_report(path: &Path, report: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    }

    fn run() -> Result<(), String> {
        if std::env::var(ALLOW_FLAG).ok().as_deref() != Some("1") {
            println!(
                "Codex transport comparison skipped; set {ALLOW_FLAG}=1 to permit up to three synthetic turns"
            );
            return Ok(());
        }
        let run_key = Uuid::new_v4().simple().to_string();
        let packet = SYNTHETIC_PACKET.as_bytes().to_vec();
        let selection = selection();
        let path = report_path(&run_key)?;
        let report = json!({
            "schemaVersion": "codex-transport-comparison.v1",
            "status": "started",
            "scope": "one synthetic summary-like workload; one exec, one cold app-server, one warm app-server; modest diagnostic sample, not Stage E acceptance",
            "allowFlag": ALLOW_FLAG,
            "runKey": run_key,
            "reportPath": path,
            "maxTurns": 3,
            "packet": {
                "text": SYNTHETIC_PACKET,
                "bytes": packet.len(),
                "sha256": sha256_hex(&packet),
            },
            "requested": requested_selection_value(),
            "effectiveModel": Value::Null,
            "effectiveReasoning": Value::Null,
            "effectiveServiceTier": Value::Null,
            "inferenceCount": Value::Null,
            "cases": [
                new_case("exec", &packet),
                new_case("cold-server", &packet),
                new_case("warm-server", &packet),
            ],
        });
        let store = ReportStore::new(path.clone(), report)?;
        let comparison_started = Instant::now();
        let connection = match CodexConnection::check_installed() {
            Ok(connection) => connection,
            Err(error) => {
                store.update(|report| {
                    report["status"] = json!("refused");
                    report["refusal"] = json!({"code": error.code});
                })?;
                println!(
                    "Codex transport comparison refused; report={}",
                    path.display()
                );
                return Ok(());
            }
        };
        if !connection.catalog().supports(&selection) {
            store.update(|report| {
                report["status"] = json!("refused");
                report["refusal"] =
                    json!("Astra/low/priority is unavailable in the checked catalog.");
                report["provider"] = json!({
                    "cliVersion": connection.version(),
                    "executableSha256": connection.fingerprint(),
                });
            })?;
            println!(
                "Codex transport comparison refused; report={}",
                path.display()
            );
            return Ok(());
        }
        let exec_binding = ProviderBinding::codex_maintenance_runtime(
            connection.version(),
            connection.fingerprint(),
        );
        store.update(|report| {
            report["provider"] = json!({
                "cliVersion": connection.version(),
                "executableSha256": connection.fingerprint(),
                "catalogModelIds": connection.catalog().models.iter().map(|model| model.model_id.clone()).collect::<Vec<_>>(),
            });
            report["connectionCheckMs"] = json!(elapsed_ms(&comparison_started));
        })?;
        let server_started = Instant::now();
        let server = match ManagedAppServer::start(connection.clone(), &selection) {
            Ok(server) => server,
            Err(error) => {
                store.update(|report| {
                    report["status"] = json!("partial-failure");
                    report["serverStartupMs"] = json!(elapsed_ms(&server_started));
                    report["serverFailure"] = json!({"code": error.code});
                })?;
                let exec_outcome = run_exec_case(
                    &store,
                    0,
                    &connection,
                    &exec_binding,
                    &packet,
                    &comparison_started,
                );
                store.update(|report| {
                    report["status"] = json!(if exec_outcome.ok { "partial" } else { "failed" });
                    report["turnsAttempted"] = json!(u8::from(exec_outcome.attempted));
                })?;
                println!(
                    "Codex transport comparison finished; report={}",
                    path.display()
                );
                return Ok(());
            }
        };
        let server_start_ms = elapsed_ms(&server_started);
        let app_binding = match server.maintenance_binding() {
            Ok(binding) => binding,
            Err(error) => {
                let _ = server.shutdown_idle();
                store.update(|report| {
                    report["status"] = json!("failed");
                    report["serverStartupMs"] = json!(server_start_ms);
                    report["bindingFailure"] = json!({"code": error.code});
                })?;
                println!(
                    "Codex transport comparison failed; report={}",
                    path.display()
                );
                return Ok(());
            }
        };
        store.update(|report| {
            report["serverStartupMs"] = json!(server_start_ms);
            report["serverReady"] = json!(server.healthy());
        })?;
        let exec_outcome = run_exec_case(
            &store,
            0,
            &connection,
            &exec_binding,
            &packet,
            &comparison_started,
        );
        let cold_outcome = run_app_server_case(
            &store,
            1,
            &server,
            &app_binding,
            SYNTHETIC_PACKET,
            &comparison_started,
        );
        let warm_outcome = if server.active_count() == 0 {
            run_app_server_case(
                &store,
                2,
                &server,
                &app_binding,
                SYNTHETIC_PACKET,
                &comparison_started,
            )
        } else {
            let _ = update_case(&store, 2, |case| {
                case.insert("status".into(), json!("skipped"));
                case.insert(
                    "failure".into(),
                    json!("cold-server did not become idle; no warm turn was sent."),
                );
            });
            CaseOutcome::default()
        };
        let cleanup_started = Instant::now();
        let cleanup = if server.active_count() == 0 {
            server.shutdown_idle().is_ok()
        } else {
            false
        };
        store.update(|report| {
            report["serverCleanupMs"] = json!(elapsed_ms(&cleanup_started));
            report["serverCleanup"] = json!(if cleanup {
                "idle-shutdown-confirmed"
            } else {
                "unresolved-active"
            });
            report["turnsAttempted"] = json!(
                u8::from(exec_outcome.attempted)
                    + u8::from(cold_outcome.attempted)
                    + u8::from(warm_outcome.attempted)
            );
            report["status"] =
                json!(
                    if exec_outcome.ok && cold_outcome.ok && warm_outcome.ok && cleanup {
                        "completed"
                    } else {
                        "partial-failure"
                    }
                );
        })?;
        println!(
            "Codex transport comparison finished; report={}",
            path.display()
        );
        Ok(())
    }

    pub fn main() {
        if let Err(error) = run() {
            eprintln!("Codex transport comparison failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn main() {
    windows::main();
}
