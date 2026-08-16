//! Integration tests for CLI argument parsing.

use std::process::Command;

#[test]
fn test_help_flag() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either stdout or stderr should contain help text
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("cpu-target") || combined.contains("ram_min"),
        "Help text should mention CLI arguments"
    );
}

#[test]
fn test_invalid_cpu_target() {
    let output = Command::new("cargo")
        .args(["run", "--", "--cpu-target", "150"])
        .output()
        .expect("Failed to run cargo");

    // Should fail with error
    assert!(!output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Error"));
}

#[test]
fn test_invalid_ram_range() {
    let output = Command::new("cargo")
        .args(["run", "--", "--ram", "60-50"])
        .output()
        .expect("Failed to run cargo");

    // Should fail with error (min >= max)
    assert!(!output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Error"));
}

#[test]
fn test_help_shows_nice() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to run cargo");

    let combined = format!("{}", String::from_utf8_lossy(&output.stdout));

    assert!(
        combined.contains("--nice") || combined.contains("nice"),
        "Help text should mention --nice argument"
    );
}

#[test]
fn test_valid_ram_range_no_error() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "--ram", "30-70"])
        .spawn()
        .expect("Failed to run cargo");

    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = child.kill();
    let _ = child.wait();

    // The daemon double-forks, so killing `cargo run` leaves the daemon alive.
    // Clean it up explicitly to avoid leaking a CPU/RAM-burning process.
    let stop = Command::new("cargo")
        .args(["run", "--", "--stop"])
        .output()
        .expect("Failed to run --stop");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    // Either it was stopped cleanly, or the PID file was stale/absent.
    assert!(
        combined.contains("Stopped") || combined.contains("no running instance"),
        "unexpected --stop output: {}",
        combined
    );
}
