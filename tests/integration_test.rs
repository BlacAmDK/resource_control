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
        .args(["run", "--", "-l", "60", "-u", "50"])
        .output()
        .expect("Failed to run cargo");

    // Should fail with error (min >= max)
    assert!(!output.status.success() || String::from_utf8_lossy(&output.stderr).contains("Error"));
}
