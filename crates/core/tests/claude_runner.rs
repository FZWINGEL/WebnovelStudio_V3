#![cfg(windows)]

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use webnovel_core::providers::claude_exec::ClaudeFailureCode;
use webnovel_core::providers::claude_runner::{
    ClaudeRunResult, ClaudeRunStatus, ClaudeStream, ClaudeStreamEvent,
};
use webnovel_core::providers::cli::windows_process::{
    ChildLimits, CliInvocation, EnvironmentPolicy, StopSignal,
};

fn new_marker() -> PathBuf {
    std::env::temp_dir().join(format!(
        "webnovel-claude-fixture-{}-{}.bin",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

fn invocation(mode: &str, packet: Vec<u8>, marker: &Path) -> CliInvocation {
    invocation_with_extra(mode, packet, marker, &[])
}

fn invocation_with_extra(
    mode: &str,
    packet: Vec<u8>,
    marker: &Path,
    extra: &[&Path],
) -> CliInvocation {
    let mut arguments = vec![
        OsString::from("--claude-jsonl"),
        OsString::from(mode),
        marker.as_os_str().to_owned(),
    ];
    arguments.extend(extra.iter().map(|path| path.as_os_str().to_owned()));
    arguments.extend([OsString::from("--model"), OsString::from("claude-sonnet-5")]);
    CliInvocation {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_windows-process-fixture")),
        arguments,
        cwd: std::env::current_dir().expect("test cwd"),
        environment: EnvironmentPolicy::Clear,
        packet,
        limits: ChildLimits {
            overall: Duration::from_secs(5),
            stop_grace: Duration::from_millis(100),
            max_total_output_bytes: 4 * 1024 * 1024,
        },
    }
}

fn collect(stream: &mut ClaudeStream) -> (String, ClaudeRunResult) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut text = String::new();
    loop {
        assert!(Instant::now() < deadline, "Claude fixture must settle");
        match stream
            .next_event(Duration::from_millis(100))
            .expect("stream worker remains connected")
        {
            Some(ClaudeStreamEvent::AssistantDelta(delta)) => text.push_str(&delta),
            Some(ClaudeStreamEvent::Finished(result)) => return (text, result),
            None => {}
        }
    }
}

#[test]
fn completed_stream_preserves_exact_stdin_usage_and_reported_identity() {
    let packet = b"exact story packet\nwith bytes\0".to_vec();
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("complete", packet.clone(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (text, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::Completed);
    assert_eq!(text, "The promise matters.");
    assert_eq!(result.assistant_text, text);
    assert_eq!(result.requested_model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(result.reported_model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(result.confirmed_stdin_bytes, packet.len());
    assert_eq!(
        std::fs::read(&marker).expect("fixture stdin marker"),
        packet
    );
    assert_eq!(result.usage.expect("reported usage").output_tokens, Some(2));
    assert!(!format!("{result:?}").contains("PRIVATE_CLAUDE_DIAGNOSTIC"));
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn stop_after_valid_delta_keeps_partial_text_and_settles_cleanup() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("stop", b"stop packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    loop {
        if let Some(ClaudeStreamEvent::AssistantDelta(delta)) = stream
            .next_event(Duration::from_secs(1))
            .expect("stream worker remains connected")
        {
            assert_eq!(delta, "The promise matters.");
            break;
        }
    }
    stream.request_stop();
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::Stopped);
    assert_eq!(result.assistant_text, "The promise matters.");
    assert!(result.usage.is_none());
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn malformed_and_tool_records_in_one_chunk_retain_only_valid_prefix() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("malformed-tool", b"malformed packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(
        result.status,
        ClaudeRunStatus::ProtocolFailure(ClaudeFailureCode::InvalidJson)
    );
    assert_eq!(result.assistant_text, "The promise matters.");
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn standalone_tool_record_is_rejected_after_valid_prefix() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("tool", b"tool packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(
        result.status,
        ClaudeRunStatus::ProtocolFailure(ClaudeFailureCode::UnsupportedEvent)
    );
    assert_eq!(result.assistant_text, "The promise");
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn nonzero_exit_after_valid_terminal_records_is_not_success() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("nonzero", b"nonzero packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(
        result.status,
        ClaudeRunStatus::ProtocolFailure(ClaudeFailureCode::NonZeroExit)
    );
    assert_eq!(result.assistant_text, "The promise matters.");
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn output_cap_retains_utf8_safe_prefix_and_unterminated_result_completes() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("output-cap", b"large packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::OutputLimit);
    assert!(result.assistant_text.len() <= 64 * 1024);
    assert!(
        result
            .assistant_text
            .is_char_boundary(result.assistant_text.len())
    );
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");

    let marker2 = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("unterminated", b"unterminated packet".to_vec(), &marker2),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::Completed);
    assert_eq!(result.assistant_text, "The promise matters.");
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker2).expect("remove fixture marker");
}

#[test]
fn provider_reported_model_mismatch_remains_inspectable_without_retry() {
    let marker = new_marker();
    let mut stream = ClaudeStream::start(
        invocation("mismatch", b"mismatch packet".to_vec(), &marker),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::ModelMismatch);
    assert_eq!(result.requested_model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(result.reported_model.as_deref(), Some("claude-opus-5"));
    assert_eq!(result.assistant_text, "The promise matters.");
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
}

#[test]
fn stalled_consumer_gets_bounded_failure() {
    let marker = new_marker();
    let release = new_marker();
    let pid_file = new_marker();
    let mut stream = ClaudeStream::start(
        invocation_with_extra(
            "flood",
            b"backpressure packet".to_vec(),
            &marker,
            &[&release, &pid_file],
        ),
        StopSignal::new(),
    )
    .expect("start Claude fixture");
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "flood fixture did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!raw.is_null(), "flood fixture must be alive before release");
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    std::fs::write(&release, b"go").expect("release flood fixture");
    assert_eq!(
        unsafe { WaitForSingleObject(process.as_raw_handle() as _, 6_000) },
        WAIT_OBJECT_0,
        "flood fixture must settle after release"
    );
    let (_, result) = collect(&mut stream);
    assert_eq!(result.status, ClaudeRunStatus::ConsumerTooSlow);
    assert!(result.assistant_text.len() < 64 * 1024);
    assert!(result.cleanup_settled);
    std::fs::remove_file(marker).expect("remove fixture marker");
    std::fs::remove_file(release).expect("remove flood release marker");
    std::fs::remove_file(pid_file).expect("remove flood pid marker");
}
