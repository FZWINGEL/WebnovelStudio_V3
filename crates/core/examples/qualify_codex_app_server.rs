//! Opt-in local qualification for the native Codex app-server transport.
//!
//! The default invocation is intentionally inert. `--check` performs only
//! installed-runtime discovery, isolated app-server initialization, and
//! thread-start/pre-turn-abort checks. The callback rejects before `turn/start`
//! so this harness never sends author prose or performs a paid generation.

#[cfg(windows)]
mod windows {
    use serde_json::{Map, Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};
    use webnovel_core::context::packet::CODEX_MAINTENANCE_MODEL_ID;
    use webnovel_core::projects::{CoreError, CoreResult};
    use webnovel_core::providers::cli::windows_process::StopSignal;
    use webnovel_core::providers::codex_app_server::connection::{
        AppServerRequest, ManagedAppServer,
    };
    use webnovel_core::providers::codex_app_server::runtime::AppServerStreamEvent;
    use webnovel_core::providers::codex_runner::CodexRunStatus;
    use webnovel_core::providers::codex_runtime::CodexConnection;
    use webnovel_core::providers::preferences::ModelSelection;

    const LUNA_MODEL_ID: &str = "gpt-5.6-luna";
    const PRIORITY_TIER: &str = "priority";
    const QUALIFICATION_PACKET: &str =
        "WebnovelStudio app-server qualification: no story text; abort before turn/start.";

    struct Timing {
        started: Instant,
        entries: Map<String, Value>,
    }

    impl Timing {
        fn new() -> Self {
            Self {
                started: Instant::now(),
                entries: Map::new(),
            }
        }

        fn record(&mut self, name: &str, started: Instant) {
            self.entries
                .insert(name.to_owned(), json!(started.elapsed().as_millis()));
        }

        fn finish(self) -> Value {
            json!({
                "totalMs": self.started.elapsed().as_millis(),
                "steps": self.entries,
            })
        }
    }

    fn choice(
        model_id: &str,
        reasoning: Option<String>,
        service_tier: Option<String>,
    ) -> ModelSelection {
        ModelSelection {
            provider_id: "codex".into(),
            model_id: model_id.into(),
            reasoning,
            service_tier,
        }
    }

    fn collect_abort(request: AppServerRequest, count: Arc<AtomicUsize>) -> CoreResult<bool> {
        let mut stream = request.reservation.start(
            request.binding,
            QUALIFICATION_PACKET.to_owned(),
            request.thread,
            StopSignal::new(),
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Err(CoreError::new(
                    "QualificationOnly",
                    "The qualification harness aborts before turn/start.",
                ))
            },
            |_, _| Ok(()),
        )?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if Instant::now() >= deadline {
                return Err(CoreError::new(
                    "QualificationTimeout",
                    "The app-server pre-turn qualification did not settle.",
                ));
            }
            match stream.recv_timeout(Duration::from_millis(100))? {
                Some(AppServerStreamEvent::Finished(finished)) => {
                    let valid = finished.delivery.validate().is_ok()
                        && finished.delivery.dispatch.is_none()
                        && finished.delivery.submission
                            == webnovel_core::providers::codex_app_server::AppServerSubmission::NotSent
                        && finished.result.status
                            == CodexRunStatus::ProtocolFailure(
                                webnovel_core::providers::codex_exec::CodexFailureCode::ProviderFailure,
                            );
                    return Ok(valid);
                }
                Some(AppServerStreamEvent::AssistantDelta(_)) | None => {}
            }
        }
    }

    fn transition(
        app: &ManagedAppServer,
        selection: &ModelSelection,
        callback_count: Arc<AtomicUsize>,
    ) -> CoreResult<bool> {
        let request = app.reserve(&app.author_binding(selection)?)?;
        collect_abort(request, callback_count)
    }

    fn available_reasoning(
        connection: &CodexConnection,
        model_id: &str,
        preferred: Option<&str>,
    ) -> Option<String> {
        let model = connection.catalog().model(model_id)?;
        preferred
            .filter(|value| {
                model
                    .reasoning_levels
                    .iter()
                    .any(|candidate| candidate == *value)
            })
            .map(str::to_owned)
            .or_else(|| model.default_reasoning.clone())
            .or_else(|| model.reasoning_levels.first().cloned())
    }

    fn run() -> Value {
        let mut timing = Timing::new();
        let mut checks = Map::new();
        let mut counts = Map::new();
        let mut report = Map::new();
        let installed_started = Instant::now();
        let connection = match CodexConnection::check_installed() {
            Ok(connection) => {
                timing.record("installedCheck", installed_started);
                checks.insert("installedCheck".into(), json!(true));
                report.insert("version".into(), json!(connection.version()));
                report.insert("executableSha256".into(), json!(connection.fingerprint()));
                report.insert(
                    "catalogModelIds".into(),
                    json!(
                        connection
                            .catalog()
                            .models
                            .iter()
                            .map(|model| model.model_id.clone())
                            .collect::<Vec<_>>()
                    ),
                );
                connection
            }
            Err(error) => {
                timing.record("installedCheck", installed_started);
                checks.insert("installedCheck".into(), json!(false));
                report.insert("errorCode".into(), json!(error.code));
                report.insert("checks".into(), Value::Object(checks));
                report.insert("counts".into(), Value::Object(counts));
                report.insert("timingsMs".into(), timing.finish());
                report.insert("passed".into(), json!(false));
                return Value::Object(report);
            }
        };

        let astra_reasoning = Some("low".to_owned());
        let astra_priority = choice(
            CODEX_MAINTENANCE_MODEL_ID,
            astra_reasoning.clone(),
            Some(PRIORITY_TIER.into()),
        );
        // `None` means the catalog default, which may itself be priority.
        // Exercise Standard only when the live catalog explicitly exposes it.
        let default_tier = connection
            .catalog()
            .model(CODEX_MAINTENANCE_MODEL_ID)
            .and_then(|model| model.default_service_tier.as_ref())
            .map(|_| "default".to_owned());
        let astra_default = choice(CODEX_MAINTENANCE_MODEL_ID, astra_reasoning, default_tier);
        let astra_priority_available = connection.catalog().supports(&astra_priority);
        let astra_default_available = connection.catalog().supports(&astra_default);
        checks.insert(
            "astraLowPriorityAvailable".into(),
            json!(astra_priority_available),
        );
        checks.insert(
            "astraLowDefaultAvailable".into(),
            json!(astra_default_available),
        );
        if !astra_priority_available {
            report.insert("errorCode".into(), json!("AstraLowPriorityUnavailable"));
            report.insert("checks".into(), Value::Object(checks));
            report.insert("counts".into(), Value::Object(counts));
            report.insert("timingsMs".into(), timing.finish());
            report.insert("passed".into(), json!(false));
            return Value::Object(report);
        }

        let server_started = Instant::now();
        let app = match ManagedAppServer::start(connection.clone(), &astra_priority) {
            Ok(app) => {
                timing.record("managedServerStart", server_started);
                checks.insert("managedServerReady".into(), json!(app.healthy()));
                app
            }
            Err(error) => {
                timing.record("managedServerStart", server_started);
                report.insert("errorCode".into(), json!(error.code));
                // Only locally authored diagnostics may enter the report.
                let stage = match error.detail.as_str() {
                    "The Codex app-server initialize request failed." => "initialize-rejected",
                    "The Codex app-server authentication request failed." => {
                        "authentication-rejected"
                    }
                    "The Codex app-server closed during initialization." => {
                        "initialization-process-exit"
                    }
                    "The Codex app-server did not become ready." => "initialization-timeout",
                    _ => "other-local-failure",
                };
                report.insert("failureStage".into(), json!(stage));
                report.insert("checks".into(), Value::Object(checks));
                report.insert("counts".into(), Value::Object(counts));
                report.insert("timingsMs".into(), timing.finish());
                report.insert("passed".into(), json!(false));
                return Value::Object(report);
            }
        };

        let callback_count = Arc::new(AtomicUsize::new(0));
        let first_started = Instant::now();
        let first = transition(&app, &astra_priority, Arc::clone(&callback_count));
        timing.record("astraPriorityAbort", first_started);
        checks.insert(
            "threadStartAndPreTurnAbort".into(),
            json!(first.as_ref().is_ok_and(|value| *value)),
        );
        let mut requests = 1_u64;
        let mut passed = first.as_ref().is_ok_and(|value| *value);

        if astra_default_available {
            let effective_binding = app.author_binding(&astra_default);
            let standard = effective_binding.as_ref().is_ok_and(|binding| {
                binding
                    .service_tier
                    .as_deref()
                    .is_none_or(|tier| tier == "default")
            });
            checks.insert("standardDoesNotInheritPriority".into(), json!(standard));
            passed &= standard;
            let default_started = Instant::now();
            let second = transition(&app, &astra_default, Arc::clone(&callback_count));
            timing.record("astraDefaultAbort", default_started);
            checks.insert(
                "astraDefaultAbort".into(),
                json!(second.as_ref().is_ok_and(|value| *value)),
            );
            passed &= second.as_ref().is_ok_and(|value| *value);
            requests += 1;
        } else {
            checks.insert("astraDefaultAbort".into(), json!(null));
        }

        let priority_again_started = Instant::now();
        let priority_again = transition(&app, &astra_priority, Arc::clone(&callback_count));
        timing.record("astraPriorityAgainAbort", priority_again_started);
        checks.insert(
            "astraPriorityAgainAbort".into(),
            json!(priority_again.as_ref().is_ok_and(|value| *value)),
        );
        passed &= priority_again.as_ref().is_ok_and(|value| *value);
        requests += 1;

        let luna = available_reasoning(&connection, LUNA_MODEL_ID, None).and_then(|reasoning| {
            let model = connection.catalog().model(LUNA_MODEL_ID)?;
            let selection = choice(
                LUNA_MODEL_ID,
                Some(reasoning),
                model.default_service_tier.clone(),
            );
            connection
                .catalog()
                .supports(&selection)
                .then_some(selection)
        });
        if let Some(luna) = luna {
            let luna_started = Instant::now();
            let luna_result = transition(&app, &luna, Arc::clone(&callback_count));
            timing.record("lunaAbort", luna_started);
            checks.insert(
                "lunaAbort".into(),
                json!(luna_result.as_ref().is_ok_and(|value| *value)),
            );
            passed &= luna_result.as_ref().is_ok_and(|value| *value);
            requests += 1;
        } else {
            checks.insert("lunaAbort".into(), json!(null));
        }

        counts.insert("requests".into(), json!(requests));
        counts.insert(
            "preTurnCallbacks".into(),
            json!(callback_count.load(Ordering::SeqCst)),
        );
        checks.insert(
            "sameServerWarmReuse".into(),
            json!(app.healthy() && app.active_count() == 0),
        );
        checks.insert(
            "noGenerationTurnStarted".into(),
            json!(callback_count.load(Ordering::SeqCst) == requests as usize),
        );
        passed &= app.healthy()
            && app.active_count() == 0
            && callback_count.load(Ordering::SeqCst) == requests as usize;

        let shutdown_started = Instant::now();
        let shutdown = app.shutdown_idle();
        timing.record("shutdown", shutdown_started);
        checks.insert("shutdownCleanup".into(), json!(shutdown.is_ok()));
        passed &= shutdown.is_ok();
        if let Err(error) = shutdown {
            report.insert("errorCode".into(), json!(error.code));
        }

        report.insert("checks".into(), Value::Object(checks));
        report.insert("counts".into(), Value::Object(counts));
        report.insert("timingsMs".into(), timing.finish());
        report.insert("passed".into(), json!(passed));
        Value::Object(report)
    }

    pub fn main() {
        if std::env::args().nth(1).as_deref() != Some("--check") {
            println!(
                "{}",
                json!({"status":"skipped","detail":"Pass --check to run the opt-in local Codex app-server qualification."})
            );
            return;
        }
        println!("{}", run());
    }
}

#[cfg(windows)]
fn main() {
    windows::main();
}

#[cfg(not(windows))]
fn main() {
    println!(
        "{}",
        serde_json::json!({"status":"skipped","detail":"The native Codex app-server qualification is Windows-only."})
    );
}
