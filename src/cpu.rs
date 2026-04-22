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
        // Bind thread to the specific CPU core
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
                if self.consecutive_over_target < u32::MAX {
                    self.consecutive_over_target += 1;
                }
            } else {
                self.consecutive_over_target = 0;
            }

            // P (比例) 项：快速响应当前误差
            // 系数 0.1 避免剧烈震荡
            // 当 error 为负（高于目标）时，此项会使 adjustment 变负，降低 work_ratio
            let p_term = error * 0.1;

            // I (积分) 项：累积误差，消除稳态偏移
            // 例如：持续低于目标时，integral_error 会累积增大
            // 限制在 [-0.1, 0.1] 防止积分饱和
            self.integral_error += error * 0.05;
            let i_term = self.integral_error.clamp(-0.1, 0.1);

            // 合成调整量 = P + I
            // 限制在 [-0.15, 0.15] 避免单步调整过大
            let adjustment = (p_term + i_term).clamp(-0.15, 0.15);

            // 基础比例 = 目标百分比
            let base_ratio = self.target_usage / 100.0;

            // 计算工作比例
            // 上限：min(target * 1.5, 100%)，允许短时超调以快速逼近目标
            // 下限：min(target * 1.5, 5%)，确保不会因 target 过低导致 min > max
            let mut work_ratio = (base_ratio + adjustment)
                .clamp((base_ratio * 1.5).min(0.05), (base_ratio * 1.5).min(1.0));

            // 如果连续 3 个周期高于目标，强制让出所有 CPU 资源
            if self.consecutive_over_target >= 3 {
                work_ratio = 0.0;
            }

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
}
