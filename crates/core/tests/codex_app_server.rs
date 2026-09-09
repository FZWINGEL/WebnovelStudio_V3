#![cfg(windows)]

use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use webnovel_core::context::packet::ProviderBinding;
use webnovel_core::projects::CoreError;
use webnovel_core::providers::cli::windows_process::{
    ChildLimits, CliInvocation, EnvironmentPolicy, StopSignal,
};
use webnovel_core::providers::codex_app_server::protocol::ThreadStartConfig;
use webnovel_core::providers::codex_app_server::runtime::{
    AppServerAuth, AppServerConnection, AppServerFinished, AppServerHealth, AppServerStream,
    AppServerStreamEvent,
};
use webnovel_core::providers::codex_app_server::{
    AppServerConnectionSettlement, AppServerRuntimeIdentity, AppServerSubmission, AppServerTerminal,
};
use webnovel_core::providers::codex_runner::CodexRunStatus;

static FIXTURE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture_guard() -> MutexGuard<'static, ()> {
    FIXTURE_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn invocation(mode: &str) -> CliInvocation {
    invocation_with_record(mode, None)
}

fn invocation_with_record(mode: &str, record_path: Option<PathBuf>) -> CliInvocation {
    let mut arguments = vec![OsString::from("--codex-app-server"), OsString::from(mode)];
    if let Some(record_path) = record_path {
        arguments.push(record_path.into_os_string());
    }
    CliInvocation {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_windows-process-fixture")),
        arguments,
        cwd: std::env::current_dir().expect("test cwd"),
        environment: EnvironmentPolicy::Clear,
        packet: Vec::new(),
        limits: ChildLimits {
            overall: Duration::from_secs(60),
            stop_grace: Duration::from_millis(100),
            max_total_output_bytes: 1024 * 1024,
        },
    }
}

fn binding() -> ProviderBinding {
    ProviderBinding::codex_app_server_author_runtime(
        "gpt-6-astra",
        "low",
        Some("default"),
        "fixture-cli",
        &"a".repeat(64),
        &"b".repeat(64),
        AppServerRuntimeIdentity {
            account_sha256: "c".repeat(64),
            security_config_sha256: "d".repeat(64),
            restrictive_catalog_sha256: "e".repeat(64),
        },
    )
}

fn thread_config() -> ThreadStartConfig {
    ThreadStartConfig {
        cwd: Some(
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        base_instructions: None,
        developer_instructions: None,
        model: "gpt-6-astra".into(),
        reasoning_effort: Some("low".into()),
        service_tier: "default".into(),
    }
}

fn start_request(connection: &AppServerConnection, stop: StopSignal) -> AppServerStream {
    let reservation = connection
        .try_reserve()
        .expect("reserve app-server slot after start");
    reservation
        .start(
            binding(),
            "A bounded synthetic story packet.".into(),
            thread_config(),
            stop,
            |_| Ok(()),
            |_, _| Ok(()),
        )
        .expect("start app-server request")
}

fn collect(stream: &mut AppServerStream) -> AppServerFinished {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "app-server fixture stalled"
        );
        match stream
            .recv_timeout(Duration::from_millis(100))
            .expect("stream remains connected")
        {
            Some(AppServerStreamEvent::AssistantDelta(_)) => {}
            Some(AppServerStreamEvent::Finished(finished)) => return *finished,
            None => {}
        }
    }
}

#[test]
fn owned_fixture_completes_and_shutdown_verifies_cleanup() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("complete"), ()).expect("start app-server fixture");
    assert_eq!(connection.health(), AppServerHealth::Ready);
    let mut stream = start_request(&connection, StopSignal::new());
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::Completed);
    assert_eq!(finished.result.assistant_text, "Hello world");
    assert!(finished.result.cleanup_settled);
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert_eq!(
        finished.delivery.terminal,
        Some(AppServerTerminal::Completed)
    );
    assert_eq!(
        finished.delivery.connection,
        AppServerConnectionSettlement::Reusable
    );
    connection
        .shutdown()
        .expect("shutdown settles owned fixture");
}

#[test]
fn concurrent_requests_route_interleaved_notifications_by_thread() {
    let _fixture_guard = fixture_guard();
    let connection = AppServerConnection::start(invocation("interleaved"), ())
        .expect("start app-server fixture");
    let mut first = start_request(&connection, StopSignal::new());
    let mut second = start_request(&connection, StopSignal::new());
    let first_finished = collect(&mut first);
    let second_finished = collect(&mut second);
    assert_eq!(first_finished.result.status, CodexRunStatus::Completed);
    assert_eq!(second_finished.result.status, CodexRunStatus::Completed);
    assert_eq!(connection.active_count(), 0);
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn stop_waits_for_turn_terminal_and_keeps_partial_output() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("stop"), ()).expect("start app-server fixture");
    let stop = StopSignal::new();
    let mut stream = start_request(&connection, stop.clone());
    loop {
        if matches!(
            stream.recv_timeout(Duration::from_secs(1)).expect("stream"),
            Some(AppServerStreamEvent::AssistantDelta(ref text)) if text == "Hello"
        ) {
            break;
        }
    }
    stream.request_stop();
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::Stopped);
    assert_eq!(finished.result.assistant_text, "Hello");
    assert_eq!(
        finished.delivery.terminal,
        Some(AppServerTerminal::Interrupted)
    );
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert!(finished.delivery.request_settled);
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn direct_stop_signal_interrupts_one_request_while_another_completes() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("stop-first"), ()).expect("start app-server fixture");
    let stop_a = StopSignal::new();
    let mut stream_a = start_request(&connection, stop_a.clone());
    loop {
        if matches!(
            stream_a.recv_timeout(Duration::from_secs(1)).expect("stream A"),
            Some(AppServerStreamEvent::AssistantDelta(ref text)) if text == "Hello"
        ) {
            break;
        }
    }
    // Ensure the fixture's first turn is A before admitting B. A remains
    // active while the driver observes the direct StopSignal below; B must
    // still complete independently after A settles.
    let mut stream_b = start_request(&connection, StopSignal::new());
    // Simulate the desktop worker's direct StopSignal path. The driver must
    // notice this on its bounded tick and send the matching interrupt.
    stop_a.request_stop();
    let stopped = collect(&mut stream_a);
    let completed = collect(&mut stream_b);
    assert_eq!(stopped.result.status, CodexRunStatus::Stopped);
    assert_eq!(
        stopped.delivery.terminal,
        Some(AppServerTerminal::Interrupted)
    );
    assert_eq!(completed.result.status, CodexRunStatus::Completed);
    assert_eq!(
        completed.delivery.terminal,
        Some(AppServerTerminal::Completed)
    );
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn stop_before_pre_turn_claim_does_not_kill_connection_or_retain_dispatch() {
    let _fixture_guard = fixture_guard();
    let connection = AppServerConnection::start(invocation("delay-thread"), ())
        .expect("start delayed app-server fixture");
    let stop = StopSignal::new();
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback_count_for_worker = Arc::clone(&callback_count);
    let reservation = connection
        .try_reserve()
        .expect("reserve app-server slot after start");
    let mut stream = reservation
        .start(
            binding(),
            "A bounded synthetic story packet.".into(),
            thread_config(),
            stop.clone(),
            move |_| {
                callback_count_for_worker.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            |_, _| Ok(()),
        )
        .expect("start delayed app-server request");
    stop.request_stop();
    let finished = collect(&mut stream);
    assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    assert_eq!(
        finished.delivery,
        webnovel_core::providers::codex_app_server::AppServerDelivery::not_sent()
    );
    finished
        .delivery
        .validate()
        .expect("pre-claim stop receipt");
    assert_eq!(connection.health(), AppServerHealth::Ready);
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn missing_interrupt_terminal_is_bounded_and_unresolved() {
    let _fixture_guard = fixture_guard();
    let connection = AppServerConnection::start(invocation("ignore-interrupt"), ())
        .expect("start interrupt fixture");
    let mut stream = start_request(&connection, StopSignal::new());
    loop {
        if matches!(
            stream.recv_timeout(Duration::from_secs(1)).expect("stream"),
            Some(AppServerStreamEvent::AssistantDelta(ref text)) if text == "Hello"
        ) {
            break;
        }
    }
    let started = std::time::Instant::now();
    stream.request_stop();
    let finished = collect(&mut stream);
    assert!(started.elapsed() < Duration::from_secs(15));
    assert_eq!(finished.result.status, CodexRunStatus::CleanupUnresolved);
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert!(finished.delivery.turn_id.is_some());
    assert!(finished.delivery.terminal.is_none());
    assert!(!finished.delivery.request_settled);
    finished
        .delivery
        .validate()
        .expect("bounded interrupt timeout receipt");
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn lost_turn_ack_with_authoritative_events_settles_without_replay() {
    let _fixture_guard = fixture_guard();
    let connection = AppServerConnection::start(invocation("lost-start-complete"), ())
        .expect("start lost-ack fixture");
    let mut stream = start_request(&connection, StopSignal::new());
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::Completed);
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert_eq!(
        finished.delivery.terminal,
        Some(AppServerTerminal::Completed)
    );
    assert!(finished.delivery.turn_id.is_some());
    finished
        .delivery
        .validate()
        .expect("authoritative lost-ack receipt");
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn authenticated_launch_advertises_experimental_api_capability() {
    let _fixture_guard = fixture_guard();
    let auth = AppServerAuth {
        method: "account/login/start".into(),
        params: serde_json::json!({"type":"chatgptAuthTokens","accessToken":"fixture"}),
    };
    let connection = AppServerConnection::start_with_auth(invocation("auth"), (), Some(auth))
        .expect("start authenticated app-server fixture");
    assert_eq!(connection.health(), AppServerHealth::Ready);
    connection
        .shutdown()
        .expect("shutdown settles authenticated fixture");
}

#[test]
fn lost_turn_ack_is_uncertain_and_is_never_replayed() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("lost-start"), ()).expect("start app-server fixture");
    let stop = StopSignal::new();
    let mut stream = start_request(&connection, stop);
    std::thread::sleep(Duration::from_millis(100));
    stream.request_stop();
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::CleanupUnresolved);
    assert_eq!(finished.delivery.submission, AppServerSubmission::Uncertain);
    assert!(!finished.delivery.request_settled);
    assert!(!finished.result.cleanup_settled);
    assert_eq!(
        finished.delivery.connection,
        AppServerConnectionSettlement::Closed
    );
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn malformed_frame_poison_does_not_report_success() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("malformed"), ()).expect("start app-server fixture");
    let mut stream = start_request(&connection, StopSignal::new());
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::CleanupUnresolved);
    assert!(!finished.delivery.request_settled);
    assert_eq!(
        finished.delivery.connection,
        AppServerConnectionSettlement::Closed
    );
    assert_eq!(connection.health(), AppServerHealth::Poisoned);
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn acknowledged_turn_crash_preserves_turn_identity_and_validates_receipt() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("crash"), ()).expect("start app-server fixture");
    let mut stream = start_request(&connection, StopSignal::new());
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::CleanupUnresolved);
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert!(finished.delivery.turn_id.is_some());
    assert!(finished.delivery.terminal.is_none());
    assert!(!finished.delivery.request_settled);
    assert!(!finished.result.cleanup_settled);
    assert_eq!(
        finished.delivery.connection,
        AppServerConnectionSettlement::Closed
    );
    finished
        .delivery
        .validate()
        .expect("acknowledged crash receipt");
    connection.shutdown().expect("shutdown settles fixture");
}

#[test]
fn completed_thread_threshold_recycles_idle_connection() {
    let _fixture_guard = fixture_guard();
    let connection =
        AppServerConnection::start(invocation("complete"), ()).expect("start app-server fixture");
    for _ in 0..128 {
        let mut stream = start_request(&connection, StopSignal::new());
        let finished = collect(&mut stream);
        assert_eq!(finished.result.status, CodexRunStatus::Completed);
    }
    assert_eq!(connection.active_count(), 0);
    assert_eq!(connection.health(), AppServerHealth::Closed);
    assert!(connection.try_reserve().is_err());
    connection.shutdown().expect("idle recycle cleanup settles");
}

#[test]
fn before_turn_rejection_is_not_sent_and_leaves_a_valid_not_sent_receipt() {
    let _fixture_guard = fixture_guard();
    let record_path = std::env::temp_dir().join(format!(
        "wns-app-server-reject-{}-{}.log",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let connection = AppServerConnection::start(
        invocation_with_record("record-reject", Some(record_path.clone())),
        (),
    )
    .expect("start app-server fixture");
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback_count_for_worker = Arc::clone(&callback_count);
    let reservation = connection
        .try_reserve()
        .expect("reserve app-server slot after start");
    let mut stream = reservation
        .start(
            binding(),
            "A bounded synthetic story packet.".into(),
            thread_config(),
            StopSignal::new(),
            move |_| {
                callback_count_for_worker.fetch_add(1, Ordering::SeqCst);
                Err(CoreError::new(
                    "QualificationOnly",
                    "synthetic pre-turn rejection",
                ))
            },
            |_, _| Ok(()),
        )
        .expect("start rejected app-server request");
    let finished = collect(&mut stream);
    assert_eq!(callback_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        finished.delivery,
        webnovel_core::providers::codex_app_server::AppServerDelivery::not_sent()
    );
    assert!(matches!(
        finished.result.status,
        CodexRunStatus::ProtocolFailure(_)
    ));
    let methods = std::fs::read_to_string(&record_path).expect("fixture method record");
    assert!(!methods.lines().any(|method| method == "turn/start"));
    connection.shutdown().expect("shutdown settles fixture");
    std::fs::remove_file(record_path).expect("remove fixture method record");
}

#[test]
fn delayed_turn_persistence_fences_terminal_and_preserves_known_failure_receipt() {
    let _fixture_guard = fixture_guard();

    let connection = AppServerConnection::start(invocation("complete"), ())
        .expect("start delayed persistence fixture");
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let reservation = connection
        .try_reserve()
        .expect("reserve app-server slot after start");
    let mut stream = reservation
        .start(
            binding(),
            "A bounded synthetic story packet.".into(),
            thread_config(),
            StopSignal::new(),
            |_| Ok(()),
            move |_, _| {
                started_tx.send(()).expect("signal persistence callback");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("release persistence callback");
                Ok(())
            },
        )
        .expect("start delayed persistence request");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("turn persistence callback started");

    let mut saw_delta = false;
    let hold_deadline = std::time::Instant::now() + Duration::from_millis(250);
    while std::time::Instant::now() < hold_deadline {
        match stream
            .recv_timeout(Duration::from_millis(25))
            .expect("delayed stream remains connected")
        {
            Some(AppServerStreamEvent::AssistantDelta(_)) => saw_delta = true,
            Some(AppServerStreamEvent::Finished(_)) => {
                panic!("terminal result escaped before turn persistence settled")
            }
            None => {}
        }
    }
    assert!(
        saw_delta,
        "fixture should stream before persistence release"
    );
    release_tx
        .send(())
        .expect("release delayed persistence callback");
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::Completed);
    assert_eq!(finished.result.assistant_text, "Hello world");
    assert!(finished.delivery.request_settled);
    finished
        .delivery
        .validate()
        .expect("delayed successful persistence receipt");
    connection
        .shutdown()
        .expect("shutdown delayed success fixture");

    let connection = AppServerConnection::start(invocation("complete"), ())
        .expect("start failed persistence fixture");
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let reservation = connection
        .try_reserve()
        .expect("reserve app-server slot after start");
    let mut stream = reservation
        .start(
            binding(),
            "A bounded synthetic story packet.".into(),
            thread_config(),
            StopSignal::new(),
            |_| Ok(()),
            move |_, _| {
                started_tx.send(()).expect("signal failed callback");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("release failed callback");
                Err(CoreError::new(
                    "PersistenceUnavailable",
                    "synthetic callback detail must not be retained",
                ))
            },
        )
        .expect("start failed persistence request");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("failed persistence callback started");
    let mut saw_delta = false;
    let hold_deadline = std::time::Instant::now() + Duration::from_millis(250);
    while std::time::Instant::now() < hold_deadline {
        match stream
            .recv_timeout(Duration::from_millis(25))
            .expect("failed stream remains connected")
        {
            Some(AppServerStreamEvent::AssistantDelta(_)) => saw_delta = true,
            Some(AppServerStreamEvent::Finished(_)) => {
                panic!("failed persistence escaped before callback release")
            }
            None => {}
        }
    }
    assert!(
        saw_delta,
        "fixture should stream before failed persistence release"
    );
    release_tx
        .send(())
        .expect("release failed persistence callback");
    let finished = collect(&mut stream);
    assert_eq!(finished.result.status, CodexRunStatus::CleanupUnresolved);
    assert_eq!(finished.result.assistant_text, "Hello world");
    assert!(!finished.result.cleanup_settled);
    assert_eq!(
        finished.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert!(finished.delivery.turn_id.is_some());
    assert_eq!(
        finished.delivery.terminal,
        Some(AppServerTerminal::Completed)
    );
    assert!(!finished.delivery.request_settled);
    finished
        .delivery
        .validate()
        .expect("failed persistence receipt retains known terminal");
    assert_eq!(
        finished.local_failure,
        Some(
            webnovel_core::providers::codex_app_server::runtime::AppServerLocalFailure::Persistence
        )
    );
    connection
        .shutdown()
        .expect("shutdown delayed failure fixture");
}
