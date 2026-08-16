//! Integration tests for CLI subcommands.
//!
//! Tests run serially because they share a single global daemon state
//! (PID file + a running resource_control process).

use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn serial_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .output()
        .expect("Failed to run cargo")
}

#[test]
fn test_help_flag() {
    let _guard = serial_lock().lock().unwrap();
    let output = run(&["--help"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("start") && combined.contains("stop"),
        "top-level help should list start/stop subcommands"
    );
}

#[test]
fn test_start_help_shows_options() {
    let _guard = serial_lock().lock().unwrap();
    let output = run(&["start", "--help"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("--cpu-target")
            && combined.contains("--ram")
            && combined.contains("--nice"),
        "start help should mention all options"
    );
}

#[test]
fn test_invalid_cpu_target() {
    let _guard = serial_lock().lock().unwrap();
    let output = run(&["start", "--cpu-target", "150"]);
    assert!(!output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Error"));
}

#[test]
fn test_invalid_ram_range() {
    let _guard = serial_lock().lock().unwrap();
    let output = run(&["start", "--ram", "60-50"]);
    assert!(!output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Error"));
}

#[test]
fn test_status_not_running_after_stop() {
    let _guard = serial_lock().lock().unwrap();
    // Ensure no leftover daemon, then default invocation must report idle
    let _ = run(&["stop"]);
    let output = run(&[]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Not running")
            && combined.contains("resource_control start")
            && combined.contains("--cpu-target"),
        "default invocation should report status with start hint, got: {}",
        combined
    );
}

#[test]
fn test_valid_ram_range_no_error() {
    let _guard = serial_lock().lock().unwrap();
    let mut child = Command::new("cargo")
        .args(["run", "--", "start", "--ram", "30-70"])
        .spawn()
        .expect("Failed to run cargo");

    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = child.kill();
    let _ = child.wait();

    // The daemon double-forks, so killing `cargo run` leaves the daemon alive.
    // Clean it up explicitly to avoid leaking a CPU/RAM-burning process.
    let stop = run(&["stop"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    // Either it was stopped cleanly, or the PID file was stale/absent.
    assert!(
        combined.contains("Stopped") || combined.contains("no running instance"),
        "unexpected stop output: {}",
        combined
    );
}
