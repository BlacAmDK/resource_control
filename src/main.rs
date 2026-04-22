//! Resource Control - Server CPU and Memory Usage Controller
//!
//! This program maintains server CPU and memory usage within configurable
//! target ranges by spawning control threads for each resource.

mod cpu;
mod error;
mod ram;

use clap::Parser;
use cpu::spawn_cpu_threads;
use ram::spawn_ram_thread;
use rustix::process::setpriority_process;

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

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Nice value (0-19, higher = lower priority)
    #[arg(short, long, default_value_t = 19)]
    nice: i32,
}

fn main() {
    let args = Args::parse();

    // Set nice value for lower priority
    match setpriority_process(None, args.nice) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Warning: failed to set nice value: {}", e);
        }
    }

    // Validate arguments
    if args.cpu_target > 100.0 || args.cpu_target < 0.0 {
        eprintln!("Error: cpu_target must be between 0 and 100");
        std::process::exit(1);
    }

    let ram_parts: Vec<&str> = args.ram.split('-').collect();
    if ram_parts.len() != 2 {
        eprintln!("Error: ram must be in format \"min-max\" (e.g., \"45-55\")");
        std::process::exit(1);
    }

    let ram_min = match ram_parts[0].parse::<u64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: invalid ram min value");
            std::process::exit(1);
        }
    };

    let ram_max = match ram_parts[1].parse::<u64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: invalid ram max value");
            std::process::exit(1);
        }
    };

    if ram_min >= ram_max || ram_max > 100 {
        eprintln!("Error: ram min must be less than ram max, and ram max must be <= 100");
        std::process::exit(1);
    }

    if args.verbose {
        println!(
            "Starting resource control: CPU target={}%, RAM range={}-{}%",
            args.cpu_target, ram_min, ram_max
        );
    }

    let mut handles = vec![];

    // Spawn RAM control thread
    match spawn_ram_thread((ram_min, ram_max)) {
        Ok(handle) => handles.push(handle),
        Err(e) => {
            eprintln!("Failed to spawn RAM thread: {:?}", e);
            std::process::exit(1);
        }
    }

    // Spawn CPU control threads
    match spawn_cpu_threads(args.cpu_target) {
        Ok(cpu_handles) => handles.extend(cpu_handles),
        Err(e) => {
            eprintln!("Failed to spawn CPU threads: {:?}", e);
            std::process::exit(1);
        }
    }

    // Wait for all threads (they run indefinitely)
    for handle in handles {
        let _ = handle.join();
    }
}

// Global jemalloc allocator
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;
