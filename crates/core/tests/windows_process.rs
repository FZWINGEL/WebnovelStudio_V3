#![cfg(windows)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use webnovel_core::providers::cli::windows_process::{
    ChildLimits, ChildStream, ChildTermination, CliInvocation, EnvironmentPolicy,
    InteractiveAction, MAX_PACKET_BYTES, MAX_PERSISTENT_DIAGNOSTIC_BYTES,
    MAX_PERSISTENT_PENDING_BYTES, PersistentEvent, StopSignal, spawn, spawn_interactive,
    spawn_persistent,
};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Threading::{
    CreateEventW, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

fn fixture() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_windows-process-fixture")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_windows_process_fixture"))
        .map(PathBuf::from)
        .expect("Cargo must provide the owned fixture path")
}

fn invocation(arguments: &[&str], limits: ChildLimits) -> CliInvocation {
    CliInvocation {
        executable: fixture(),
        arguments: arguments.iter().map(OsString::from).collect(),
        cwd: std::env::current_dir().expect("test cwd"),
        environment: EnvironmentPolicy::Clear,
        packet: b"packet-marker-must-stay-on-stdin".to_vec(),
        limits,
    }
}

fn limits() -> ChildLimits {
    ChildLimits {
        overall: Duration::from_secs(5),
        stop_grace: Duration::from_millis(100),
        max_total_output_bytes: 64 * 1024,
    }
}

#[test]
fn stop_contains_fixture_child_and_grandchild() {
    let pid_file = new_pid_file();
    let running = spawn(invocation_with_pid_file(&[], limits(), &pid_file)).expect("spawn fixture");
    let handles = retained_fixture_handles(&pid_file, &["root", "child", "grandchild"]);
    let stop = StopSignal::new();
    let request = stop.clone();
    let worker = thread::spawn(move || running.finish_or_stop(request));
    thread::sleep(Duration::from_millis(250));
    stop.request_stop();

    let outcome = worker
        .join()
        .expect("finish worker")
        .expect("cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
    assert_all_signaled(&handles, "Stop cleanup");
    let stdout = String::from_utf8_lossy(&outcome.output.stdout);
    assert!(stdout.contains("ROOT_READY"), "{stdout}");
    assert!(stdout.contains("CHILD_READY"), "{stdout}");
    assert!(stdout.contains("GRANDCHILD_READY"), "{stdout}");
    let _ = fs::remove_file(pid_file);
}

fn retained_process_handle(pid: u32) -> OwnedHandle {
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(
        !process.is_null(),
        "OpenProcess failed for live fixture {pid}"
    );
    let handle = unsafe { OwnedHandle::from_raw_handle(process.cast()) };
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle() as _, 0) },
        WAIT_TIMEOUT,
        "fixture {pid} was not alive when retained"
    );
    handle
}

fn new_pid_file() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "webnovel-studio-fixture-{}-{}.txt",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    File::create(&path).expect("create fixture PID file");
    path
}

fn new_marker_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "webnovel-studio-fixture-release-{}-{}.txt",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

fn invocation_with_pid_file(
    arguments: &[&str],
    limits: ChildLimits,
    pid_file: &std::path::Path,
) -> CliInvocation {
    let mut request = invocation(arguments, limits);
    let mut environment = BTreeMap::new();
    environment.insert(
        OsString::from("WNS_FIXTURE_PID_FILE"),
        pid_file.as_os_str().to_owned(),
    );
    request.environment = EnvironmentPolicy::Explicit(environment);
    request
}

fn retained_fixture_handles(pid_file: &std::path::Path, roles: &[&str]) -> Vec<OwnedHandle> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let contents = fs::read_to_string(pid_file).unwrap_or_default();
        let mut handles = Vec::with_capacity(roles.len());
        let mut complete = true;
        for role in roles {
            let pid = contents
                .lines()
                .find(|line| line.starts_with(&format!("{role} ")))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok());
            let Some(pid) = pid else {
                complete = false;
                break;
            };
            let handle = retained_process_handle(pid);
            handles.push(handle);
        }
        if complete {
            return handles;
        }
        if std::time::Instant::now() >= deadline {
            panic!("fixture did not publish live roles {roles:?}: {contents}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_all_signaled(handles: &[OwnedHandle], operation: &str) {
    for handle in handles {
        assert_eq!(
            unsafe { WaitForSingleObject(handle.as_raw_handle() as _, 0) },
            WAIT_OBJECT_0,
            "retained process handle did not observe {operation}"
        );
    }
}

#[test]
fn timeout_preserves_partial_output_and_kills_tree() {
    let mut bounded = limits();
    bounded.overall = Duration::from_millis(250);
    let pid_file = new_pid_file();
    let running = spawn(invocation_with_pid_file(&[], bounded, &pid_file)).expect("spawn fixture");
    let handles = retained_fixture_handles(&pid_file, &["root", "child", "grandchild"]);
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("timeout cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::TimedOut);
    assert!(!outcome.output.stdout.is_empty());
    assert_all_signaled(&handles, "timeout cleanup");
    let _ = fs::remove_file(pid_file);
}

#[test]
fn output_cap_is_combined_across_stdout_and_stderr() {
    let mut bounded = limits();
    bounded.max_total_output_bytes = 32 * 1024;
    let running = spawn(invocation(&["--flood"], bounded)).expect("spawn flood fixture");
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("output-limit cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::OutputLimitExceeded);
    assert!(outcome.output.truncated);
    assert!(!outcome.output.stdout.is_empty());
    assert!(!outcome.output.stderr.is_empty());
    assert!(outcome.output.stdout.len() + outcome.output.stderr.len() <= 32 * 1024);
}

#[test]
fn fast_exit_output_cap_is_not_reported_as_completed() {
    let mut bounded = limits();
    bounded.max_total_output_bytes = 32 * 1024;
    let running = spawn(invocation(&["--fast-flood"], bounded)).expect("spawn fast flood fixture");
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("fast output-limit cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::OutputLimitExceeded);
    assert!(outcome.output.truncated);
    assert!(outcome.output.stdout.len() + outcome.output.stderr.len() <= 32 * 1024);
}

#[test]
fn normal_completion_reports_packet_only_on_stdin_path() {
    let mut bounded = limits();
    bounded.overall = Duration::from_secs(2);
    let running = spawn(invocation(&["--oneshot"], bounded)).expect("spawn oneshot fixture");
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("oneshot cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert!(String::from_utf8_lossy(&outcome.output.stdout).contains("ONESHOT_READY 32"));
    assert_eq!(outcome.output.stdin_bytes_written, 32);
    assert!(!String::from_utf8_lossy(&outcome.output.stdout).contains("packet-marker"));
}

#[test]
fn observer_sees_first_chunk_before_normal_child_exit() {
    let mut bounded = limits();
    bounded.overall = Duration::from_secs(5);
    let release_marker = new_marker_path();
    let mut request = invocation(&["--early-output"], bounded);
    request
        .arguments
        .push(release_marker.as_os_str().to_owned());
    let running = spawn(request).expect("spawn early output fixture");
    let process = retained_process_handle(running.process_id());
    let mut observed = Vec::new();
    let mut alive_for_first_chunk = false;
    let outcome = running
        .finish_or_stop_with_output(StopSignal::new(), |stream, bytes| {
            assert_eq!(stream, ChildStream::Stdout);
            if observed.is_empty() {
                alive_for_first_chunk =
                    unsafe { WaitForSingleObject(process.as_raw_handle() as _, 0) == WAIT_TIMEOUT };
                File::create(&release_marker).expect("release early-output fixture");
            }
            observed.extend_from_slice(bytes);
        })
        .expect("early output cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert!(
        alive_for_first_chunk,
        "first chunk arrived after process exit"
    );
    assert_eq!(outcome.output.exit_code, Some(0));
    assert_eq!(observed, outcome.output.stdout);
    let observed_text = String::from_utf8_lossy(&observed);
    assert!(observed_text.contains("EARLY_OUTPUT"));
    assert!(observed_text.contains("EARLY_DONE 32"));
    let _ = fs::remove_file(release_marker);
}

#[test]
fn observer_stop_cleans_tree_without_later_observer_chunks() {
    let pid_file = new_pid_file();
    let running = spawn(invocation_with_pid_file(&[], limits(), &pid_file)).expect("spawn fixture");
    let handles = retained_fixture_handles(&pid_file, &["root", "child", "grandchild"]);
    let stop = StopSignal::new();
    let callback_stop = stop.clone();
    let mut observed_chunks = 0_usize;
    let mut callbacks_after_stop = 0_usize;
    let outcome = running
        .finish_or_stop_with_output(stop, |_, bytes| {
            if callback_stop.is_requested() {
                callbacks_after_stop += 1;
            } else {
                observed_chunks += 1;
                assert!(!bytes.is_empty());
                callback_stop.request_stop();
            }
        })
        .expect("observer stop cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
    assert_eq!(observed_chunks, 1);
    assert_eq!(callbacks_after_stop, 0);
    assert_all_signaled(&handles, "observer Stop cleanup");
    let _ = fs::remove_file(pid_file);
}

#[test]
fn observer_receives_fast_exit_final_tail() {
    let running = spawn(invocation(&["--oneshot"], limits())).expect("spawn oneshot fixture");
    let mut observed = Vec::new();
    let outcome = running
        .finish_or_stop_with_output(StopSignal::new(), |stream, bytes| {
            assert_eq!(stream, ChildStream::Stdout);
            observed.extend_from_slice(bytes);
        })
        .expect("oneshot observer cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert_eq!(observed, outcome.output.stdout);
    assert!(String::from_utf8_lossy(&observed).contains("ONESHOT_READY 32"));
}

#[test]
fn interactive_observer_appends_bounded_packets_before_closing_stdin() {
    let mut request = invocation(&["--interactive"], limits());
    request.packet = b"FIRST\n".to_vec();
    let running = spawn_interactive(request).expect("spawn interactive fixture");
    let mut observed = Vec::new();
    let outcome = running
        .finish_interactive(StopSignal::new(), |stream, bytes| {
            assert_eq!(stream, ChildStream::Stdout);
            observed.extend_from_slice(bytes);
            if observed.ends_with(b"FIRST_ACK\n") {
                InteractiveAction::Send(b"SECOND\n".to_vec())
            } else if observed.ends_with(b"SECOND_ACK\n") {
                InteractiveAction::Close
            } else {
                InteractiveAction::KeepOpen
            }
        })
        .expect("interactive cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert_eq!(outcome.output.stdin_bytes_written, b"FIRST\nSECOND\n".len());
    assert_eq!(observed, b"FIRST_ACK\nSECOND_ACK\n");
    assert_eq!(observed, outcome.output.stdout);
}

#[test]
fn persistent_observer_polls_idle_and_reuses_one_process_for_multiple_packets() {
    let mut request = invocation(&["--persistent"], limits());
    request.packet.clear();
    let running = spawn_persistent(request).expect("spawn persistent fixture");
    let process = retained_process_handle(running.process_id());
    let started = std::time::Instant::now();
    let mut ticks = Vec::new();
    let mut sent_first = false;
    let mut observed = Vec::new();
    let outcome = running
        .finish_persistent(StopSignal::new(), |event| match event {
            PersistentEvent::Tick => {
                ticks.push(std::time::Instant::now());
                if !sent_first && started.elapsed() >= Duration::from_millis(80) {
                    sent_first = true;
                    InteractiveAction::Send(b"FIRST\n".to_vec())
                } else {
                    InteractiveAction::KeepOpen
                }
            }
            PersistentEvent::Output(stream, bytes) => {
                assert_eq!(stream, ChildStream::Stdout);
                observed.extend_from_slice(bytes);
                if observed.ends_with(b"FIRST_ACK\n") {
                    InteractiveAction::Send(b"SECOND\n".to_vec())
                } else if observed.ends_with(b"SECOND_ACK\n") {
                    InteractiveAction::Close
                } else {
                    InteractiveAction::KeepOpen
                }
            }
        })
        .expect("persistent cleanup settles");
    assert_eq!(
        outcome.termination,
        ChildTermination::Completed,
        "observed={:?} ticks={} stdout={} stderr={}",
        String::from_utf8_lossy(&observed),
        ticks.len(),
        String::from_utf8_lossy(&outcome.output.stdout),
        String::from_utf8_lossy(&outcome.output.stderr)
    );
    assert_eq!(outcome.output.stdin_bytes_written, b"FIRST\nSECOND\n".len());
    assert_eq!(observed, b"FIRST_ACK\nSECOND_ACK\n");
    assert_eq!(observed, outcome.output.stdout);
    assert!(
        ticks.len() >= 3,
        "idle polling did not run: {}",
        ticks.len()
    );
    let largest_gap = ticks
        .windows(2)
        .map(|pair| pair[1].duration_since(pair[0]))
        .max()
        .unwrap_or_default();
    assert!(
        largest_gap <= Duration::from_millis(100),
        "idle poll gap was {largest_gap:?}"
    );
    assert_eq!(
        unsafe { WaitForSingleObject(process.as_raw_handle() as _, 0) },
        WAIT_OBJECT_0,
        "Close did not settle the owned process"
    );
}

#[test]
fn persistent_stop_settles_idle_owned_process() {
    let pid_file = new_pid_file();
    let running = spawn_persistent(invocation_with_pid_file(
        &["--persistent"],
        limits(),
        &pid_file,
    ))
    .expect("spawn persistent fixture");
    let handles = retained_fixture_handles(&pid_file, &["persistent"]);
    let stop = StopSignal::new();
    let request_stop = stop.clone();
    let worker = thread::spawn(move || {
        running.finish_persistent(request_stop, |event| match event {
            PersistentEvent::Tick => InteractiveAction::KeepOpen,
            PersistentEvent::Output(_, _) => InteractiveAction::KeepOpen,
        })
    });
    thread::sleep(Duration::from_millis(100));
    stop.request_stop();
    let outcome = worker
        .join()
        .expect("persistent stop worker")
        .expect("persistent stop cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
    assert_all_signaled(&handles, "persistent Stop cleanup");
    let _ = fs::remove_file(pid_file);
}

#[test]
fn persistent_lifetime_output_is_observed_beyond_diagnostic_prefix_without_kill() {
    let mut request = invocation(&["--persistent"], limits());
    request.packet = b"FLOOD\n".to_vec();
    let running = spawn_persistent(request).expect("spawn persistent fixture");
    let mut observed = Vec::new();
    let outcome = running
        .finish_persistent(StopSignal::new(), |event| match event {
            PersistentEvent::Tick => InteractiveAction::KeepOpen,
            PersistentEvent::Output(_, bytes) => {
                observed.extend_from_slice(bytes);
                InteractiveAction::Close
            }
        })
        .expect("persistent output cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert!(observed.len() > MAX_PERSISTENT_DIAGNOSTIC_BYTES);
    assert!(outcome.output.truncated);
    assert!(
        outcome.output.stdout.len() + outcome.output.stderr.len()
            <= MAX_PERSISTENT_DIAGNOSTIC_BYTES
    );
}

#[test]
fn persistent_input_queue_rejects_unbounded_backpressure() {
    let running = spawn_persistent(invocation(&["--persistent"], limits()))
        .expect("spawn persistent fixture");
    let mut sends = 0;
    let error = running
        .finish_persistent(StopSignal::new(), |event| match event {
            PersistentEvent::Tick => {
                sends += 1;
                if sends == 1 {
                    InteractiveAction::Send(vec![b'x'; MAX_PERSISTENT_PENDING_BYTES])
                } else {
                    InteractiveAction::Send(vec![b'y'; MAX_PERSISTENT_PENDING_BYTES])
                }
            }
            PersistentEvent::Output(_, _) => InteractiveAction::KeepOpen,
        })
        .expect_err("persistent queue must remain bounded");
    assert!(
        error.to_string().contains("persistent stdin queue exceeds"),
        "unexpected queue error: {error}"
    );
}

#[test]
fn observer_stop_on_fast_exit_is_not_reported_completed() {
    let running = spawn(invocation(&["--oneshot"], limits())).expect("spawn oneshot fixture");
    let stop = StopSignal::new();
    let callback_stop = stop.clone();
    let outcome = running
        .finish_or_stop_with_output(stop, |_, _| callback_stop.request_stop())
        .expect("observer stop cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
}

#[test]
fn observer_never_receives_bytes_beyond_combined_cap() {
    let mut bounded = limits();
    bounded.max_total_output_bytes = 32 * 1024;
    let running = spawn(invocation(&["--fast-flood"], bounded)).expect("spawn fast flood fixture");
    let mut observed_stdout = Vec::new();
    let mut observed_stderr = Vec::new();
    let outcome = running
        .finish_or_stop_with_output(StopSignal::new(), |stream, bytes| match stream {
            ChildStream::Stdout => observed_stdout.extend_from_slice(bytes),
            ChildStream::Stderr => observed_stderr.extend_from_slice(bytes),
        })
        .expect("fast output-limit observer cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::OutputLimitExceeded);
    assert!(outcome.output.truncated);
    assert!(observed_stdout.len() + observed_stderr.len() <= bounded.max_total_output_bytes);
    assert_eq!(observed_stdout, outcome.output.stdout);
    assert_eq!(observed_stderr, outcome.output.stderr);
}

#[test]
fn observer_preserves_each_stream_prefix() {
    let running =
        spawn(invocation(&["--two-streams"], limits())).expect("spawn two-stream fixture");
    let mut observed_stdout = Vec::new();
    let mut observed_stderr = Vec::new();
    let outcome = running
        .finish_or_stop_with_output(StopSignal::new(), |stream, bytes| match stream {
            ChildStream::Stdout => observed_stdout.extend_from_slice(bytes),
            ChildStream::Stderr => observed_stderr.extend_from_slice(bytes),
        })
        .expect("two-stream observer cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert_eq!(observed_stdout, b"STDOUT_OBSERVER_PREFIX");
    assert_eq!(observed_stderr, b"STDERR_OBSERVER_PREFIX");
    assert_eq!(observed_stdout, outcome.output.stdout);
    assert_eq!(observed_stderr, outcome.output.stderr);
}

#[test]
fn quotes_arguments_and_keeps_packet_on_stdin() {
    let request = invocation(
        &["--args-and-stdin", "a b", "quote\"part", "trail\\"],
        limits(),
    );
    let running = spawn(request).expect("spawn argument fixture");
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("argument cleanup settles");
    let stdout = String::from_utf8_lossy(&outcome.output.stdout);
    assert!(stdout.contains("ARGS a b|quote\"part|trail\\"), "{stdout}");
    assert!(stdout.contains("PACKET_LEN 32"), "{stdout}");
    assert!(!stdout.contains("packet-marker"), "{stdout}");
}

#[test]
fn normal_root_exit_settles_a_descendant_holding_output() {
    let pid_file = new_pid_file();
    let mut request = invocation_with_pid_file(&["--exit-with-descendant"], limits(), &pid_file);
    request.packet.clear(); // This fixture only exercises descendant cleanup.
    let running = spawn(request).expect("spawn descendant fixture");
    let handles = retained_fixture_handles(&pid_file, &["grandchild"]);
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("descendant cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    let stdout = String::from_utf8_lossy(&outcome.output.stdout);
    assert!(stdout.contains("ROOT_EXIT_READY"));
    assert_all_signaled(&handles, "completion cleanup");
    let _ = fs::remove_file(pid_file);
}

#[test]
fn ignored_stdin_is_cancelled_without_a_blocking_writer() {
    let mut request = invocation(&["--ignore-stdin"], limits());
    request.packet = vec![b'p'; MAX_PACKET_BYTES];
    let running = spawn(request).expect("spawn ignored-stdin fixture");
    let stop = StopSignal::new();
    let request_stop = stop.clone();
    let worker = thread::spawn(move || running.finish_or_stop(request_stop));
    thread::sleep(Duration::from_millis(100));
    stop.request_stop();
    let outcome = worker
        .join()
        .expect("finish worker")
        .expect("writer cancellation settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
}

#[test]
fn dropping_ignored_stdin_closes_workers_and_root() {
    let mut request = invocation(&["--ignore-stdin"], limits());
    request.packet = vec![b'p'; MAX_PACKET_BYTES];
    let running = spawn(request).expect("spawn ignored-stdin fixture");
    let pid = running.process_id();
    let root_handle = retained_process_handle(pid);
    drop(running);
    assert_eq!(
        unsafe { WaitForSingleObject(root_handle.as_raw_handle() as _, 0) },
        WAIT_OBJECT_0,
        "root survived RunningChild Drop"
    );
}

#[test]
fn dropping_flood_closes_workers_without_queue_deadlock() {
    let running = spawn(invocation(&["--flood"], limits())).expect("spawn flood fixture");
    let root_handle = retained_process_handle(running.process_id());
    drop(running);
    assert_eq!(
        unsafe { WaitForSingleObject(root_handle.as_raw_handle() as _, 0) },
        WAIT_OBJECT_0,
        "flood fixture survived RunningChild Drop"
    );
}

#[test]
fn handle_list_excludes_an_unrelated_inheritable_sentinel() {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let sentinel = unsafe { CreateEventW(&attributes, 0, 0, std::ptr::null()) };
    assert!(!sentinel.is_null());
    let sentinel = unsafe { OwnedHandle::from_raw_handle(sentinel.cast()) };

    let mut environment = BTreeMap::new();
    environment.insert(
        OsString::from("WNS_FIXTURE_SENTINEL"),
        OsString::from((sentinel.as_raw_handle() as usize).to_string()),
    );
    let mut request = invocation(&["--check-sentinel"], limits());
    request.packet.clear(); // This fixture only checks handle inheritance.
    request.environment = EnvironmentPolicy::Explicit(environment);
    let running = spawn(request).expect("spawn sentinel fixture");
    let outcome = running
        .finish_or_stop(StopSignal::new())
        .expect("sentinel cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Completed);
    assert!(
        String::from_utf8_lossy(&outcome.output.stdout).contains("SENTINEL_UNAVAILABLE"),
        "{}",
        String::from_utf8_lossy(&outcome.output.stdout)
    );
}

#[test]
fn rejects_non_exe_and_relative_paths_before_creating_a_process() {
    let mut request = invocation(&["--oneshot"], limits());
    request.executable = PathBuf::from("fixture.exe");
    assert!(spawn(request).is_err());

    let mut request = invocation(&["--oneshot"], limits());
    request.executable = std::env::current_dir().expect("test cwd");
    assert!(spawn(request).is_err());
}

#[test]
fn zero_exit_with_incomplete_input_retains_delivery_evidence() {
    let mut request = invocation(&["--early-exit"], limits());
    request.packet = vec![b'p'; MAX_PACKET_BYTES];
    let error = spawn(request)
        .expect("spawn early-exit fixture")
        .finish_or_stop(StopSignal::new())
        .expect_err("an unread packet cannot be a completed delivery");
    let webnovel_core::providers::cli::windows_process::ContainmentError::Cleanup {
        stage,
        partial,
    } = error
    else {
        panic!("expected explicit delivery failure");
    };
    assert_eq!(stage, "incomplete stdin delivery");
    let partial = partial.expect("retained output");
    assert_eq!(partial.exit_code, Some(0));
    assert!(partial.stdin_bytes_written < MAX_PACKET_BYTES);
    assert!(String::from_utf8_lossy(&partial.stdout).contains("EARLY_EXIT"));
}

#[test]
fn stop_requested_before_wait_does_not_begin_packet_delivery() {
    let running = spawn(invocation(&["--oneshot"], limits())).expect("spawn fixture");
    let stop = StopSignal::new();
    stop.request_stop();
    let outcome = running.finish_or_stop(stop).expect("stop cleanup settles");
    assert_eq!(outcome.termination, ChildTermination::Stopped);
    assert_eq!(outcome.output.stdin_bytes_written, 0);
}
