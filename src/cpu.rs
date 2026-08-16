//! CPU control module.
//!
//! Controls CPU usage by spawning threads that bind to each CPU core
//! and adjusting work/sleep ratio based on target usage.

use std::hint::black_box;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{System, MINIMUM_CPU_UPDATE_INTERVAL};

use crate::error::AppError;

/// Controller for managing CPU usage on a single core.
pub struct CpuController {
    core_id: usize,
    target_usage: f32,
    integral_error: f32,
    consecutive_over_target: u32,
}

impl CpuController {
    /// Creates a new CPU controller for the specified core.
    pub fn new(core_id: usize, target_usage: f32) -> Self {
        Self {
            core_id,
            target_usage,
            integral_error: 0.0,
            consecutive_over_target: 0,
        }
    }

    /// Runs the CPU control loop indefinitely.
    pub fn run(&mut self) {
        // Bind thread to the specific CPU core (silently ignore failures:
        // daemonized stderr is redirected to /dev/null anyway)
        core_affinity::set_for_current(core_affinity::CoreId { id: self.core_id });

        let mut system = System::new();

        loop {
            system.refresh_cpu_usage();
            let cpu_usage = system.cpus()[self.core_id].cpu_usage();

            // 误差 = 目标 - 当前测量值
            // 正值表示低于目标，负值表示高于目标
            let error = self.target_usage - cpu_usage;

            // 追踪连续高于目标的周期数，用于强制让出
            // 限制最大值防止溢出，锁定后不再累加
            if error < 0.0 {
                self.consecutive_over_target = self.consecutive_over_target.saturating_add(1);
            } else {
                self.consecutive_over_target = 0;
            }

            // I (积分) 项：累积误差，消除稳态偏移
            // 限制在 [-0.1, 0.1] 防止积分饱和
            self.integral_error += error * 0.05;
            self.integral_error = self.integral_error.clamp(-0.1, 0.1);

            let work_ratio = compute_work_ratio(
                self.target_usage,
                cpu_usage,
                self.integral_error,
                self.consecutive_over_target,
            );

            let cycle_duration = MINIMUM_CPU_UPDATE_INTERVAL;
            let work_duration =
                Duration::from_secs_f64(work_ratio as f64 * cycle_duration.as_secs_f64());
            let sleep_duration = cycle_duration - work_duration;

            let start = Instant::now();
            while start.elapsed() < work_duration {
                let mut sum = 0u64;
                for x in 1..10000 {
                    sum = black_box(sum.wrapping_add(x));
                }
            }

            thread::sleep(sleep_duration);
        }
    }
}

/// Computes the work ratio for a cycle from measured usage and controller state.
///
/// Returns the work ratio in [0, upper]. Lower bound is 0 so that when the core
/// is saturated by other processes, the controller can fully yield instead of
/// competing with business workloads.
fn compute_work_ratio(
    target_usage: f32,
    cpu_usage: f32,
    integral_error: f32,
    consecutive_over_target: u32,
) -> f32 {
    // 误差 = 目标 - 当前测量值（正值=低于目标，负值=高于目标）
    let error = target_usage - cpu_usage;

    // P (比例) 项 + I (积分) 项，限制幅度防止震荡/饱和
    let p_term = error * 0.1;
    let i_term = (integral_error + error * 0.05).clamp(-0.1, 0.1);
    let adjustment = (p_term + i_term).clamp(-0.15, 0.15);

    // 上限允许短时超调；下限为 0，过载时完全让出
    let base_ratio = target_usage / 100.0;
    let upper = (base_ratio * 1.5).min(1.0);
    let mut work_ratio = (base_ratio + adjustment).clamp(0.0, upper);

    // 连续 3 个周期高于目标，强制让出所有 CPU 资源
    if consecutive_over_target >= 3 {
        work_ratio = 0.0;
    }

    work_ratio
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
        let mut controller = CpuController::new(cpu_id, target_usage);
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

    #[test]
    fn test_work_ratio_never_exceeds_upper_bound() {
        // target=60: upper = min(0.9, 1.0) = 0.9
        let ratio = compute_work_ratio(60.0, 0.0, 0.0, 0);
        assert!(ratio <= 0.9);
    }

    #[test]
    fn test_work_ratio_forced_to_zero_after_three_over_target() {
        // Three consecutive cycles above target must release all CPU
        let ratio = compute_work_ratio(60.0, 100.0, 0.0, 3);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn test_work_ratio_increases_when_below_target() {
        // Below target (0% usage) should push ratio above base 0.6
        let ratio = compute_work_ratio(60.0, 0.0, 0.0, 0);
        assert!(ratio > 0.6);
    }

    #[test]
    fn test_work_ratio_reduces_when_over_target() {
        // Over target (100% usage) should reduce ratio below base 0.6
        let ratio = compute_work_ratio(60.0, 100.0, 0.0, 0);
        assert!(ratio < 0.6);
    }
}
