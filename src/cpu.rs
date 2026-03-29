//! CPU control module.
//!
//! Controls CPU usage by spawning threads that bind to each CPU core
//! and adjusting work/sleep ratio based on target usage.

use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{System, MINIMUM_CPU_UPDATE_INTERVAL};

use crate::error::AppError;

/// Controller for managing CPU usage on a single core.
pub struct CpuController {
    core_id: usize,
    target_usage: f32,
}

impl CpuController {
    /// Creates a new CPU controller for the specified core.
    pub fn new(core_id: usize, target_usage: f32) -> Self {
        Self {
            core_id,
            target_usage,
        }
    }

    /// Runs the CPU control loop indefinitely.
    pub fn run(&self) {
        // Bind thread to the specific CPU core
        core_affinity::set_for_current(core_affinity::CoreId { id: self.core_id });

        let mut system = System::new();
        system.refresh_cpu_usage();
        thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);

        // Target is percentage (0-100), convert to ratio (0.0-1.0)
        let target_ratio = self.target_usage as f64 / 100.0;

        loop {
            system.refresh_cpu_usage();
            let _cpu_usage = system.cpus()[self.core_id].cpu_usage();

            // Calculate work/sleep duration based on target
            // Total cycle time remains constant at ~200ms
            let cycle_duration = Duration::from_millis(200);
            let work_duration =
                Duration::from_secs_f64(cycle_duration.as_secs_f64() * target_ratio);
            let sleep_duration = cycle_duration - work_duration;

            // Perform work for calculated duration
            let start = Instant::now();
            while start.elapsed() < work_duration {
                let mut sum = 0u64;
                for x in 1..10000 {
                    sum = sum.wrapping_add(x);
                }
                std::hint::black_box(sum);
            }

            // Sleep for the remaining time
            thread::sleep(sleep_duration);
        }
    }
}

/// Spawns CPU control threads for all available cores.
pub fn spawn_cpu_threads(target_usage: f32) -> Result<Vec<std::thread::JoinHandle<()>>, AppError> {
    let system = System::new_all();
    let cpu_count = system.cpus().len();

    if cpu_count == 0 {
        return Err(AppError::Cpu("No CPU cores found".into()));
    }

    let mut handles = Vec::with_capacity(cpu_count);

    for cpu_id in 0..cpu_count {
        let controller = CpuController::new(cpu_id, target_usage);
        let handle = thread::spawn(move || {
            controller.run();
        });
        handles.push(handle);
    }

    Ok(handles)
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_controller_creation() {
        let controller = CpuController::new(0, 60.0);
        assert_eq!(controller.core_id, 0);
        assert_eq!(controller.target_usage, 60.0);
    }

    #[test]
    fn test_target_ratio_calculation() {
        // Verify target percentage is stored correctly
        let controller = CpuController::new(0, 90.0);
        let ratio = controller.target_usage / 100.0;
        assert!((ratio - 0.9).abs() < f32::EPSILON);

        let controller = CpuController::new(1, 55.0);
        let ratio = controller.target_usage / 100.0;
        assert!((ratio - 0.55).abs() < f32::EPSILON);
    }
}
