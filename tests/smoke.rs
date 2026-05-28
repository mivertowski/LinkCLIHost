use std::process::{Command, Stdio};
use std::time::Duration;

/// Spawns the release binary, lets it run briefly, then sends SIGINT.
/// Confirms the process exits with status 0 and writes a non-empty log.
#[test]
#[ignore] // run with `cargo test --release -- --ignored` because it needs the release binary built
fn binary_starts_logs_and_exits_cleanly() {
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("linkclihost"))
        .expect("locate binary");
    assert!(bin.exists(), "build the binary first: cargo build --release");

    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("smoke.jsonl");

    let mut child = Command::new(&bin)
        .arg("--no-tui")
        .arg("--log")
        .arg(&log)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn linkclihost");

    std::thread::sleep(Duration::from_secs(2));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let status = child.wait().expect("wait");
    assert!(status.success(), "child exited with {status}");

    let body = std::fs::read_to_string(&log).expect("read log");
    assert!(body.contains("session_start"), "log missing session_start");
    assert!(body.contains("session_end"), "log missing session_end");
}
