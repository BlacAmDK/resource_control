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

/// CLI arguments for resource control.
#[derive(Parser, Debug)]
#[command(
    name = "resource_control",
    about = "Control server CPU and memory usage"
)]
struct Args {
    /// Target CPU usage percentage (0-100)
    #[arg(short, long, default_value_t = 55.0)]
    cpu_target: f32,

    /// Minimum RAM usage percentage (0-100)
    #[arg(short = 'l', long, default_value_t = 45)]
    ram_min: u64,

    /// Maximum RAM usage percentage (0-100)
    #[arg(short = 'u', long, default_value_t = 55)]
    ram_max: u64,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    // Validate arguments
    if args.cpu_target > 100.0 || args.cpu_target < 0.0 {
        eprintln!("Error: cpu_target must be between 0 and 100");
        std::process::exit(1);
    }

    if args.ram_min >= args.ram_max || args.ram_max > 100 {
        eprintln!("Error: ram_min must be less than ram_max, and ram_max must be <= 100");
        std::process::exit(1);
    }

    if args.verbose {
        println!(
            "Starting resource control: CPU target={}%, RAM range={}-{}%",
            args.cpu_target, args.ram_min, args.ram_max
        );
    }

    let mut handles = vec![];

    // Spawn RAM control thread
    match spawn_ram_thread((args.ram_min, args.ram_max)) {
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
