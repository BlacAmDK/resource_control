//! Resource Control - Server CPU and Memory Usage Controller
//!
//! This program maintains server CPU and memory usage within configurable
//! target ranges by spawning control threads for each resource.

mod cpu;
mod error;
mod ram;

use std::path::Path;
use std::time::Duration;
use clap::Parser;
use cpu::spawn_cpu_threads;
use daemonize::Daemonize;
use ram::spawn_ram_thread;
use rustix::process::setpriority_process;

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

fn stop_instance() {
    let contents = std::fs::read_to_string(PID_FILE).unwrap_or_else(|_| {
        eprintln!("Error: no running instance found");
        std::process::exit(1);
    });

    let pid = contents.trim().parse::<u32>().unwrap_or_else(|_| {
        eprintln!("Error: invalid PID file");
        std::process::exit(1);
    });

    let proc_path = format!("/proc/{}", pid);
    if !Path::new(&proc_path).exists() {
        let _ = std::fs::remove_file(PID_FILE);
        eprintln!("Error: no running instance found (stale PID file removed)");
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

    if Path::new(&proc_path).exists() {
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

    if args.cpu_target > 100.0 || args.cpu_target < 0.0 {
        eprintln!("Error: cpu_target must be between 0 and 100");
        std::process::exit(1);
    }

    let ram_parts: Vec<&str> = args.ram.split('-').collect();
    if ram_parts.len() != 2 {
        eprintln!("Error: ram must be in format \"min-max\" (e.g., \"45-55\")");
        std::process::exit(1);
    }

    let ram_min: u64 = ram_parts[0].parse().unwrap_or_else(|_| {
        eprintln!("Error: invalid ram min value");
        std::process::exit(1);
    });

    let ram_max: u64 = ram_parts[1].parse().unwrap_or_else(|_| {
        eprintln!("Error: invalid ram max value");
        std::process::exit(1);
    });

    if ram_min >= ram_max || ram_max > 100 {
        eprintln!("Error: ram min must be less than ram max, and ram max must be <= 100");
        std::process::exit(1);
    }

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

    match spawn_ram_thread((ram_min, ram_max)) {
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
