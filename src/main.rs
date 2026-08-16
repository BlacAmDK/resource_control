//! Resource Control - Server CPU and Memory Usage Controller
//!
//! This program maintains server CPU and memory usage within configurable
//! target ranges by spawning control threads for each resource.

mod cpu;
mod error;
mod ram;

use clap::Parser;
use cpu::spawn_cpu_threads;
use daemonize::Daemonize;
use ram::spawn_ram_thread;
use rustix::process::setpriority_process;
use std::path::Path;
use std::time::Duration;

const PID_FILE: &str = "/tmp/resource_control.pid";

/// CLI arguments for resource control.
#[derive(Parser, Debug)]
#[command(
    name = "resource_control",
    about = "Control server CPU and memory usage"
)]
struct Args {
    /// Target CPU usage percentage (0-100)
    #[arg(short, long, default_value_t = 50.0)]
    cpu_target: f32,

    /// RAM usage range as "min-max" (e.g., "45-55")
    #[arg(short = 'm', long, default_value = "45-55")]
    ram: String,

    /// Nice value (0-19, higher = lower priority)
    #[arg(short, long, default_value_t = 19)]
    nice: i32,

    /// Stop the running instance
    #[arg(long)]
    stop: bool,
}

fn parse_ram_range(raw: &str) -> Result<(u64, u64), String> {
    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() != 2 {
        return Err("ram must be in format \"min-max\" (e.g., \"45-55\")".to_string());
    }

    let ram_min: u64 = parts[0]
        .parse()
        .map_err(|_| "invalid ram min value".to_string())?;
    let ram_max: u64 = parts[1]
        .parse()
        .map_err(|_| "invalid ram max value".to_string())?;

    if ram_min >= ram_max || ram_max > 100 {
        return Err("ram min must be less than ram max, and ram max must be <= 100".to_string());
    }

    Ok((ram_min, ram_max))
}

fn validate_cpu_target(target: f32) -> Result<(), String> {
    if !(0.0..=100.0).contains(&target) {
        return Err("cpu_target must be between 0 and 100".to_string());
    }
    Ok(())
}

fn validate_nice(nice: i32) -> Result<(), String> {
    if !(0..=19).contains(&nice) {
        return Err("nice must be between 0 and 19".to_string());
    }
    Ok(())
}

/// Verifies the PID belongs to the same program as the running binary.
///
/// Compares the executable filename of `pid` against our own executable name
/// via `/proc/<pid>/exe` and `current_exe()`. This stays correct when the
/// binary is renamed, since both sides reference the same name. Fail-closed:
/// unreadable/missing processes are treated as not ours.
fn is_resource_control_pid(pid: u32) -> bool {
    let our_name = match std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_owned()))
    {
        Some(name) => name,
        None => return false,
    };
    std::fs::read_link(format!("/proc/{}/exe", pid))
        .ok()
        .and_then(|exe| exe.file_name().map(|n| n.to_os_string()))
        .map(|name| name == our_name)
        .unwrap_or(false)
}

fn stop_instance() {
    let contents = std::fs::read_to_string(PID_FILE).unwrap_or_else(|_| {
        eprintln!("Error: no running instance found");
        std::process::exit(1);
    });

    let pid = contents.trim().parse::<u32>().unwrap_or_else(|_| {
        eprintln!("Error: invalid PID file");
        std::process::exit(1);
    });

    // Only kill the process if it is actually a resource_control instance,
    // guarding against stale/tampered PID files targeting unrelated processes.
    if !is_resource_control_pid(pid) {
        if !Path::new(&format!("/proc/{}", pid)).exists() {
            let _ = std::fs::remove_file(PID_FILE);
            eprintln!("Error: no running instance found (stale PID file removed)");
            std::process::exit(1);
        }
        eprintln!(
            "Error: PID {} is not a resource_control process; refusing to kill it",
            pid
        );
        std::process::exit(1);
    }

    println!("Stopping instance PID {}", pid);

    let status = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to execute kill: {}", e);
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("Error: failed to stop process {}", pid);
        std::process::exit(1);
    }

    std::thread::sleep(Duration::from_secs(2));

    if is_resource_control_pid(pid) {
        println!("Process still running, sending SIGKILL");
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
    }

    let _ = std::fs::remove_file(PID_FILE);
    println!("Stopped");
}

fn main() {
    let args = Args::parse();

    if args.stop {
        stop_instance();
        return;
    }

    if let Err(e) = validate_cpu_target(args.cpu_target) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let ram_range = match parse_ram_range(&args.ram) {
        Ok(range) => range,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = validate_nice(args.nice) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let (ram_min, ram_max) = ram_range;
    println!(
        "Starting resource control: CPU target={}%, RAM range={}-{}%, nice={}",
        args.cpu_target, ram_min, ram_max, args.nice
    );

    Daemonize::new()
        .pid_file(PID_FILE)
        .start()
        .unwrap_or_else(|_| {
            eprintln!("Error: another instance is already running");
            eprintln!("Hint: use --stop to stop the running instance");
            std::process::exit(1);
        });

    setpriority_process(None, args.nice).unwrap_or_else(|e| {
        eprintln!("Warning: failed to set nice value: {}", e);
    });

    let mut handles = vec![];

    match spawn_ram_thread(ram_range) {
        Ok(handle) => handles.push(handle),
        Err(e) => {
            eprintln!("Failed to spawn RAM thread: {:?}", e);
            std::process::exit(1);
        }
    }

    match spawn_cpu_threads(args.cpu_target) {
        Ok(cpu_handles) => handles.extend(cpu_handles),
        Err(e) => {
            eprintln!("Failed to spawn CPU threads: {:?}", e);
            std::process::exit(1);
        }
    }

    for handle in handles {
        let _ = handle.join();
    }
}

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ram_range_valid() {
        assert_eq!(parse_ram_range("45-55"), Ok((45, 55)));
        assert_eq!(parse_ram_range("0-100"), Ok((0, 100)));
    }

    #[test]
    fn test_parse_ram_range_invalid_format() {
        assert!(parse_ram_range("45").is_err());
        assert!(parse_ram_range("45-55-60").is_err());
        assert!(parse_ram_range("abc").is_err());
    }

    #[test]
    fn test_parse_ram_range_invalid_bounds() {
        assert!(parse_ram_range("60-50").is_err());
        assert!(parse_ram_range("45-45").is_err());
        assert!(parse_ram_range("45-101").is_err());
    }

    #[test]
    fn test_cpu_target_validation() {
        assert!(validate_cpu_target(50.0).is_ok());
        assert!(validate_cpu_target(0.0).is_ok());
        assert!(validate_cpu_target(100.0).is_ok());
        assert!(validate_cpu_target(-1.0).is_err());
        assert!(validate_cpu_target(150.0).is_err());
    }

    #[test]
    fn test_nice_validation() {
        assert!(validate_nice(0).is_ok());
        assert!(validate_nice(19).is_ok());
        assert!(validate_nice(-1).is_err());
        assert!(validate_nice(20).is_err());
    }

    #[test]
    fn test_pid_identity_rejects_other_process() {
        // Spawn a harmless process and verify it is NOT identified as ours
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(!is_resource_control_pid(pid));
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_pid_identity_rejects_nonexistent() {
        assert!(!is_resource_control_pid(u32::MAX));
    }

    #[test]
    fn test_pid_identity_accepts_same_process() {
        // The current test process runs from the same executable as current_exe(),
        // so identity must match regardless of the binary's name.
        assert!(is_resource_control_pid(std::process::id()));
    }

    #[test]
    fn test_pid_identity_rejects_different_executable() {
        // A process running a different binary must be rejected, even if it
        // happens to share our renamed executable's name.
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(!is_resource_control_pid(pid));
        let _ = child.kill();
        let _ = child.wait();
    }
}
