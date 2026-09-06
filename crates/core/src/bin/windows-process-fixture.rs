//! Owned child-tree fixture for the Windows process-boundary tests.
//!
//! This binary is only a test fixture.  It never reads author files or starts
//! a provider.  The root mode creates a child, which creates a grandchild;
//! all three remain alive until the root receives EOF or the enclosing Job
//! Object terminates them.

use std::fs::OpenOptions;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "--child" => child(),
        "--grandchild" => grandchild(),
        "--oneshot" => oneshot(),
        "--early-output" => early_output(),
        "--two-streams" => two_streams(),
        "--early-exit" => println!("EARLY_EXIT"),
        "--args-and-stdin" => args_and_stdin(),
        "--flood" => flood(),
        "--fast-flood" => fast_flood(),
        "--ignore-stdin" => ignore_stdin(),
        "--exit-with-descendant" => exit_with_descendant(),
        "--check-sentinel" => check_sentinel(),
        "--codex-jsonl" => codex_jsonl(),
        "--interactive" => interactive(),
        _ => root(),
    }
}

fn codex_jsonl() {
    let mode = std::env::args().nth(2).unwrap_or_default();
    let mut packet = Vec::new();
    io::stdin().read_to_end(&mut packet).expect("fixture stdin");
    if mode == "flood" {
        announce_pid("root");
        let release = PathBuf::from(std::env::args_os().nth(3).expect("flood release marker"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !release.exists() {
            assert!(Instant::now() < deadline, "flood fixture was not released");
            thread::sleep(Duration::from_millis(5));
        }
    }
    let emit = |value: serde_json::Value| {
        println!("{value}");
        io::stdout().flush().expect("fixture stdout");
    };
    emit(serde_json::json!({"type":"thread.started","thread_id":"fixture-thread"}));
    emit(serde_json::json!({"type":"turn.started"}));
    emit(
        serde_json::json!({"type":"item.started","item":{"id":"answer","type":"agent_message","text":"The promise"}}),
    );
    if mode == "wait" {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "tool" {
        emit(
            serde_json::json!({"type":"item.started","item":{"id":"tool","type":"command_execution","command":"INERT_FIXTURE_NEVER_EXECUTED"}}),
        );
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "broken" {
        println!("{{malformed fixture record");
        return;
    }
    if mode == "flood" {
        let mut text = "The promise".to_owned();
        for _ in 0..300 {
            text.push('.');
            emit(
                serde_json::json!({"type":"item.updated","item":{"id":"answer","type":"agent_message","text":text}}),
            );
        }
        // Only process containment ends this fixture. A stalled consumer must
        // stop the provider, independent of machine startup/scheduling speed.
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    emit(
        serde_json::json!({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":"The promise matters."}}),
    );
    emit(
        serde_json::json!({"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":7,"reasoning_output_tokens":2}}),
    );
    eprintln!("PRIVATE_DIAGNOSTIC_FIXTURE_DO_NOT_EXPOSE");
    if mode == "nonzero" {
        std::process::exit(2);
    }
}

fn root() {
    announce_pid("root");
    let executable = std::env::current_exe().expect("fixture executable");
    let mut child = Command::new(executable)
        .arg("--child")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child");
    println!("ROOT_READY {} {}", std::process::id(), child.id());
    let _ = io::stdout().flush();

    // Keep the root alive until the process boundary closes stdin.  The
    // wrapper writes the packet on stdin; a test can also use a never-ending
    // packet/child to exercise timeout and Stop cleanup.
    let mut packet = Vec::new();
    let _ = io::stdin().read_to_end(&mut packet);
    loop {
        let _ = child.try_wait();
        thread::sleep(Duration::from_secs(60));
    }
}

fn oneshot() {
    let mut packet = Vec::new();
    let _ = io::stdin().read_to_end(&mut packet);
    println!("ONESHOT_READY {}", packet.len());
}

fn interactive() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line.expect("interactive fixture stdin").as_str() {
            "FIRST" => println!("FIRST_ACK"),
            "SECOND" => println!("SECOND_ACK"),
            _ => println!("UNKNOWN_ACK"),
        }
        io::stdout().flush().expect("interactive fixture stdout");
    }
}

fn early_output() {
    let release_marker = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .expect("early-output release marker");
    println!("EARLY_OUTPUT");
    let _ = io::stdout().flush();
    // Keep the root alive after the first output until the observer explicitly
    // releases it. The timeout prevents a broken test from hanging forever.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !release_marker.exists() {
        if Instant::now() >= deadline {
            eprintln!("early-output release marker timeout");
            std::process::exit(2);
        }
        thread::sleep(Duration::from_millis(5));
    }
    let mut packet = Vec::new();
    let _ = io::stdin().read_to_end(&mut packet);
    println!("EARLY_DONE {}", packet.len());
    let _ = io::stdout().flush();
}

fn two_streams() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    stdout
        .write_all(b"STDOUT_OBSERVER_PREFIX")
        .expect("write stdout fixture output");
    stdout.flush().expect("flush stdout fixture output");
    stderr
        .write_all(b"STDERR_OBSERVER_PREFIX")
        .expect("write stderr fixture output");
    stderr.flush().expect("flush stderr fixture output");
    let mut packet = Vec::new();
    let _ = io::stdin().read_to_end(&mut packet);
}

fn args_and_stdin() {
    let arguments: Vec<_> = std::env::args().skip(2).collect();
    println!("ARGS {}", arguments.join("|"));
    let _ = io::stdout().flush();
    let mut packet = Vec::new();
    let _ = io::stdin().read_to_end(&mut packet);
    println!("PACKET_LEN {}", packet.len());
}

fn ignore_stdin() {
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

#[allow(clippy::zombie_processes)]
fn exit_with_descendant() {
    announce_pid("root");
    let executable = std::env::current_exe().expect("fixture executable");
    let grandchild = Command::new(executable)
        .arg("--grandchild")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn descendant");
    println!("ROOT_EXIT_READY {}", grandchild.id());
    let _ = io::stdout().flush();
}

fn flood() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let output = vec![b'x'; 16 * 1024];
    loop {
        let _ = stdout.write_all(&output);
        let _ = stdout.flush();
        let _ = stderr.write_all(&output);
        let _ = stderr.flush();
    }
}

fn fast_flood() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let output = vec![b'x'; 128 * 1024];
    let _ = stdout.write_all(&output);
    let _ = stdout.flush();
    let _ = stderr.write_all(&output);
    let _ = stderr.flush();
}

#[cfg(windows)]
fn check_sentinel() {
    use std::os::windows::io::RawHandle;
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    let value = std::env::var("WNS_FIXTURE_SENTINEL").unwrap_or_default();
    let parsed = value.parse::<usize>().unwrap_or_default();
    let result = unsafe { WaitForSingleObject(parsed as RawHandle as HANDLE, 0) };
    if result == WAIT_TIMEOUT {
        println!("SENTINEL_INHERITED");
    } else {
        println!("SENTINEL_UNAVAILABLE");
    }
}

#[cfg(not(windows))]
fn check_sentinel() {}

#[allow(clippy::zombie_processes)]
fn child() {
    announce_pid("child");
    let executable = std::env::current_exe().expect("fixture executable");
    let grandchild = Command::new(executable)
        .arg("--grandchild")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn grandchild");
    println!("CHILD_READY {} {}", std::process::id(), grandchild.id());
    let _ = io::stdout().flush();
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn grandchild() {
    announce_pid("grandchild");
    println!("GRANDCHILD_READY {}", std::process::id());
    let _ = io::stdout().flush();
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn announce_pid(role: &str) {
    let Some(path) = std::env::var_os("WNS_FIXTURE_PID_FILE") else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{role} {}", std::process::id());
}
