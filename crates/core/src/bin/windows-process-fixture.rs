//! Owned child-tree fixture for the Windows process-boundary tests.
//!
//! This binary is only a test fixture.  It never reads author files or starts
//! a provider.  The root mode creates a child, which creates a grandchild;
//! all three remain alive until the root receives EOF or the enclosing Job
//! Object terminates them.

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "--child" => child(),
        "--grandchild" => grandchild(),
        "--oneshot" => oneshot(),
        "--early-exit" => println!("EARLY_EXIT"),
        "--args-and-stdin" => args_and_stdin(),
        "--flood" => flood(),
        "--fast-flood" => fast_flood(),
        "--ignore-stdin" => ignore_stdin(),
        "--exit-with-descendant" => exit_with_descendant(),
        "--check-sentinel" => check_sentinel(),
        _ => root(),
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
