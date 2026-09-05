#![cfg(windows)]
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};
use webnovel_core::providers::cli::windows_process::{
    ChildLimits, CliInvocation, EnvironmentPolicy, StopSignal,
};
use webnovel_core::providers::codex_exec::CodexFailureCode;
use webnovel_core::providers::codex_runner::{
    CodexRunResult, CodexRunStatus, CodexStream, CodexStreamEvent,
};

fn invocation(mode: &str) -> CliInvocation {
    CliInvocation {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_windows-process-fixture")),
        arguments: vec![OsString::from("--codex-jsonl"), mode.into()],
        cwd: std::env::current_dir().unwrap(),
        environment: EnvironmentPolicy::Clear,
        packet: b"Only synthetic story evidence.".to_vec(),
        limits: ChildLimits {
            overall: Duration::from_secs(5),
            stop_grace: Duration::from_millis(100),
            max_total_output_bytes: 1024 * 1024,
        },
    }
}
fn collect(stream: &mut CodexStream) -> (String, CodexRunResult) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    loop {
        assert!(Instant::now() < deadline, "provider fixture must settle");
        match stream.next_event(Duration::from_millis(100)).unwrap() {
            Some(CodexStreamEvent::AssistantDelta(delta)) => text.push_str(&delta),
            Some(CodexStreamEvent::Finished(result)) => return (text, result),
            None => {}
        }
    }
}
#[test]
fn completed_stream_binds_usage_and_never_exposes_stderr() {
    let request = invocation("complete");
    let expected = request.packet.len();
    let mut stream = CodexStream::start(request, StopSignal::new()).unwrap();
    let (text, result) = collect(&mut stream);
    assert_eq!(result.status, CodexRunStatus::Completed);
    assert!(result.cleanup_settled);
    assert_eq!(text, "The promise matters.");
    assert_eq!(result.assistant_text, text);
    assert_eq!(result.confirmed_stdin_bytes, expected);
    assert_eq!(result.usage.unwrap().output_tokens, 7);
    assert!(!format!("{result:?}").contains("PRIVATE_DIAGNOSTIC"));
    assert!(stream.next_event(Duration::ZERO).is_err());
}
#[test]
fn stop_keeps_validated_partial_output_and_confirms_cleanup() {
    let mut stream = CodexStream::start(invocation("wait"), StopSignal::new()).unwrap();
    loop {
        if let Some(event) = stream.next_event(Duration::from_secs(1)).unwrap() {
            assert_eq!(
                event,
                CodexStreamEvent::AssistantDelta("The promise".into())
            );
            break;
        }
    }
    stream.request_stop();
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, CodexRunStatus::Stopped);
    assert_eq!(result.assistant_text, "The promise");
    assert!(result.cleanup_settled);
    assert!(result.usage.is_none());
}
#[test]
fn malformed_or_tool_records_fail_closed_with_partial_text() {
    for (mode, code) in [
        ("broken", CodexFailureCode::InvalidJson),
        ("tool", CodexFailureCode::UnsupportedItem),
        ("nonzero", CodexFailureCode::NonZeroExit),
    ] {
        let mut stream = CodexStream::start(invocation(mode), StopSignal::new()).unwrap();
        let (_, result) = collect(&mut stream);
        assert_eq!(
            result.status,
            CodexRunStatus::ProtocolFailure(code),
            "{mode}"
        );
        assert!(result.assistant_text.starts_with("The promise"));
        assert!(result.cleanup_settled);
    }
}
#[test]
fn timeout_and_preexisting_stop_do_not_turn_into_completed_output() {
    let mut request = invocation("wait");
    request.limits.overall = Duration::from_millis(100);
    let mut stream = CodexStream::start(request, StopSignal::new()).unwrap();
    assert_eq!(collect(&mut stream).1.status, CodexRunStatus::TimedOut);
    let stop = StopSignal::new();
    stop.request_stop();
    let mut request = invocation("complete");
    request.executable = PathBuf::from("not-an-executable");
    let mut stream = CodexStream::start(request, stop).unwrap();
    let result = collect(&mut stream).1;
    assert_eq!(result.status, CodexRunStatus::Stopped);
    assert_eq!(result.confirmed_stdin_bytes, 0);
}
#[test]
fn a_stalled_consumer_has_a_bounded_failure_instead_of_unbounded_progress() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    let directory = std::env::temp_dir().join(format!("wns-codex-runner-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let pid_file = directory.join("pid.txt");
    let release = directory.join("release");
    let mut request = invocation("flood");
    request.arguments.push(release.clone().into_os_string());
    request.environment = EnvironmentPolicy::Explicit(
        [(
            OsString::from("WNS_FIXTURE_PID_FILE"),
            pid_file.clone().into_os_string(),
        )]
        .into(),
    );
    let mut stream = CodexStream::start(request, StopSignal::new()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u32>().ok())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "fixture did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!raw.is_null(), "fixture must be alive before release");
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    std::fs::write(&release, b"go").unwrap();
    // Consume nothing until the runner terminates the still-running fixture.
    assert_eq!(
        unsafe { WaitForSingleObject(process.as_raw_handle() as _, 6000) },
        WAIT_OBJECT_0
    );
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, CodexRunStatus::ConsumerTooSlow);
    assert!(result.cleanup_settled);
    assert!(result.assistant_text.len() < 512 * 1024);
    std::fs::remove_file(pid_file).unwrap();
    std::fs::remove_file(release).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
