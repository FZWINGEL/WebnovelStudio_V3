#![cfg(windows)]

use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc,
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


fn invocation(mode: &str) -> CliInvocation {
    invocation_with_record(mode, None)
}

fn invocation_with_record(mode: &str, record_path: Option<PathBuf>) -> CliInvocation {
    invocation_with_gate(mode, record_path, None)
}

fn invocation_with_gate(
    mode: &str,
    record_path: Option<PathBuf>,
    release_path: Option<PathBuf>,
) -> CliInvocation {
    let mut arguments = vec![OsString::from("--codex-app-server"), OsString::from(mode)];
    arguments.push(
        record_path
            .map(|p| p.into_os_string())
            .unwrap_or_else(|| OsString::from("")),
    );
    if let Some(release_path) = release_path {
        arguments.push(release_path.into_os_string());
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
    let record_path = std::env::temp_dir().join(format!(
        "wns-app-server-lost-ack-{}-{}.log",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let connection = AppServerConnection::start(
        invocation_with_record("lost-start", Some(record_path.clone())),
        (),
    )
    .expect("start app-server fixture");
    let stop = StopSignal::new();
    let mut stream = start_request(&connection, stop);

    // Wait until fixture confirms turn/start was received before requesting stop
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if std::fs::read_to_string(&record_path)
            .map(|content| content.lines().any(|line| line == "turn/start"))
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        std::fs::read_to_string(&record_path)
            .map(|content| content.lines().any(|line| line == "turn/start"))
            .unwrap_or(false),
        "fixture did not receive turn/start within 5-second deadline"
    );

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
    let final_content = std::fs::read_to_string(&record_path).expect("read final method log");
    let turn_start_count = final_content.lines().filter(|line| *line == "turn/start").count();
    assert_eq!(turn_start_count, 1, "turn/start was replayed or duplicated");
    let _ = std::fs::remove_file(&record_path);
}

#[test]
fn concurrent_distinct_connections_isolate_interruption_and_completion() {
    let release_marker = std::env::temp_dir().join(format!(
        "wns-app-server-release-{}-{}.marker",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    if release_marker.exists() {
        let _ = std::fs::remove_file(&release_marker);
    }

    let connection_a =
        AppServerConnection::start(invocation("stop"), ()).expect("start connection A");
    let connection_b = AppServerConnection::start(
        invocation_with_gate("gated", None, Some(release_marker.clone())),
        (),
    )
    .expect("start connection B");

    let stop_a = StopSignal::new();
    let mut stream_a = start_request(&connection_a, stop_a);
    let mut stream_b = start_request(&connection_b, StopSignal::new());

    // Both A and B acknowledge their requests and produce initial output
    let deadline_a = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline_a,
            "stream A initial delta timeout"
        );
        match stream_a.recv_timeout(Duration::from_millis(200)).expect("stream A event") {
            Some(AppServerStreamEvent::AssistantDelta(ref text)) if text == "Hello" => break,
            Some(AppServerStreamEvent::Finished(_)) => {
                panic!("stream A finished before producing expected initial delta");
            }
            _ => {}
        }
    }

    let deadline_b = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline_b,
            "stream B initial delta timeout"
        );
        match stream_b.recv_timeout(Duration::from_millis(200)).expect("stream B event") {
            Some(AppServerStreamEvent::AssistantDelta(ref text)) if text == "Hello" => break,
            Some(AppServerStreamEvent::Finished(_)) => {
                panic!("stream B completed prematurely before gate release");
            }
            _ => {}
        }
    }

    // Connection B remains active behind the test-controlled gate while A is interrupted and shut down
    assert_eq!(connection_b.active_count(), 1, "connection B must still be actively executing");
    assert_eq!(connection_b.health(), AppServerHealth::Ready);

    // Interrupt A
    stream_a.request_stop();
    let finished_a = collect(&mut stream_a);

    // Connection A stopped with partial output
    assert_eq!(finished_a.result.status, CodexRunStatus::Stopped);
    assert_eq!(finished_a.result.assistant_text, "Hello");
    assert_eq!(
        finished_a.delivery.terminal,
        Some(AppServerTerminal::Interrupted)
    );
    assert_eq!(
        finished_a.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert!(finished_a.delivery.request_settled);

    // Shut down A while B is still active behind the gate
    connection_a.shutdown().expect("shutdown connection A");
    assert_eq!(connection_b.active_count(), 1, "connection B must remain active after connection A shutdown");
    assert_eq!(connection_b.health(), AppServerHealth::Ready);

    // Release B's completion gate
    std::fs::File::create(&release_marker).expect("create release marker");

    // Collect B's finished result and verify complete execution
    let finished_b = collect(&mut stream_b);
    assert_eq!(finished_b.result.status, CodexRunStatus::Completed);
    assert_eq!(finished_b.result.assistant_text, "Hello world");
    assert!(finished_b.result.cleanup_settled);
    assert_eq!(
        finished_b.delivery.submission,
        AppServerSubmission::Acknowledged
    );
    assert_eq!(
        finished_b.delivery.terminal,
        Some(AppServerTerminal::Completed)
    );

    connection_b.shutdown().expect("shutdown connection B");
    let _ = std::fs::remove_file(&release_marker);
}

#[test]
fn malformed_frame_poison_does_not_report_success() {
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
    let connection = AppServerConnection::start_with_thread_threshold(
        invocation("complete"),
        (),
        8,
    )
    .expect("start app-server fixture");
    for _ in 0..8 {
        let mut stream = start_request(&connection, StopSignal::new());
        let finished = collect(&mut stream);
        assert_eq!(finished.result.status, CodexRunStatus::Completed);
    }
    // The last request's teardown is asynchronous: the fixture closes the
    // completed thread from its own side, so `active_count` can still read 1 at
    // the moment the final stream finishes. Wait for the recycle instead of
    // racing it — asserting immediately made this test fail roughly one run in
    // twenty under full-suite parallelism, with `left: 1, right: 0`.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while connection.active_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the completed thread was not recycled within the deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // Reaching zero is the recycle. `health()` and the reservation refusal are
    // then immediate, so they stay as plain assertions.
    assert_eq!(connection.health(), AppServerHealth::Closed);
    assert!(connection.try_reserve().is_err());
    connection.shutdown().expect("idle recycle cleanup settles");
}

#[test]
fn before_turn_rejection_is_not_sent_and_leaves_a_valid_not_sent_receipt() {
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
