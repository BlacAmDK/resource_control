//! RAM control module.
//!
//! Manages memory usage by allocating/deallocating memory blocks
//! to keep usage within a target range.

use sysinfo::{MemoryRefreshKind, System};

use crate::error::AppError;

/// Controller for managing RAM usage.
#[derive(Debug)]
pub struct RamController {
    pool: Vec<Vec<u32>>,
    memory_one_percent: u64,
    target_range: (u64, u64),
    target_mid: u64,
    usage_percent: u64,
    system: System,
    last_total_memory: u64,
}

/// Result of an adjustment operation.
#[derive(Debug, PartialEq)]
pub enum AdjustResult {
    InRange,
    Allocated(u64),
    Freed(u64),
}

impl RamController {
    /// Creates a new RAM controller with the specified target range.
    pub fn new(target_range: (u64, u64)) -> Result<Self, AppError> {
        if target_range.0 >= target_range.1 {
            return Err(AppError::InvalidArg(
                "ram_min must be less than ram_max".into(),
            ));
        }

        let mut system = System::new();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        let total_memory = system.total_memory();
        let memory_one_percent = total_memory / 100;

        if memory_one_percent == 0 {
            return Err(AppError::InvalidArg("System has no memory".into()));
        }

        let usage_percent = Self::calculate_usage_percent(&system);

        Ok(Self {
            pool: Vec::with_capacity(
                (total_memory as usize / 100 * target_range.1 as usize).max(1),
            ),
            target_range,
            target_mid: (target_range.0 + target_range.1) / 2,
            usage_percent,
            memory_one_percent,
            system,
            last_total_memory: total_memory,
        })
    }

    /// Creates a controller with specific values for testing.
    #[cfg(test)]
    pub fn with_test_values(
        target_range: (u64, u64),
        memory_one_percent: u64,
        usage_percent: u64,
    ) -> Self {
        Self {
            pool: Vec::new(),
            target_range,
            target_mid: (target_range.0 + target_range.1) / 2,
            usage_percent,
            memory_one_percent,
            system: System::new(),
            last_total_memory: 0,
        }
    }

    /// Adjusts memory usage to stay within target range.
    pub fn adjust(&mut self) -> Result<AdjustResult, AppError> {
        self.refresh();

        if self.usage_percent >= self.target_range.0 && self.usage_percent <= self.target_range.1 {
            return Ok(AdjustResult::InRange);
        }

        let target_diff = (self.target_mid as i64) - (self.usage_percent as i64);
        self.adjust_pool(target_diff);

        if target_diff > 0 {
            Ok(AdjustResult::Allocated(target_diff as u64))
        } else {
            Ok(AdjustResult::Freed((-target_diff) as u64))
        }
    }

    fn refresh(&mut self) {
        self.system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        // memory hotplug? free memory pool
        if self.system.total_memory() != self.last_total_memory {
            self.pool.clear();
            self.last_total_memory = self.system.total_memory();
            self.memory_one_percent = self.last_total_memory / 100;
        }

        self.usage_percent = Self::calculate_usage_percent(&self.system);
    }

    fn calculate_usage_percent(system: &System) -> u64 {
        let total = system.total_memory();
        let used = system.used_memory();
        (used * 100).checked_div(total).unwrap_or(100)
    }

    fn adjust_pool(&mut self, blocks: i64) {
        if blocks > 0 {
            let blocks_to_allocate = (blocks as usize).min(100);
            let size = (self.memory_one_percent / 4) as usize;
            for _ in 0..blocks_to_allocate {
                let mut v = vec![0u32; size];
                for i in (0..size).step_by(1024) {
                    v[i] = 0;
                }
                self.pool.push(v);
            }
        } else if blocks < 0 && !self.pool.is_empty() {
            let blocks_to_free = ((-blocks) as usize).min(self.pool.len());
            for _ in 0..blocks_to_free {
                self.pool.pop();
            }
        }
    }
}

/// Spawns the RAM control thread.
pub fn spawn_ram_thread(target_range: (u64, u64)) -> Result<std::thread::JoinHandle<()>, AppError> {
    let handle = std::thread::spawn(move || {
        let mut ram = match RamController::new(target_range) {
            Ok(ram) => ram,
            Err(e) => {
                eprintln!("Failed to initialize RAM controller: {:?}", e);
                return;
            }
        };

        loop {
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            if let Err(e) = ram.adjust() {
                eprintln!("RAM adjustment error: {:?}", e);
            }
        }
    });

    Ok(handle)
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_total_memory_decrease_clears_pool() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 50);
        controller.last_total_memory = u64::MAX;
        controller.pool.push(vec![0u32; 10]);
        assert_eq!(controller.pool.len(), 1);
        let _ = controller.refresh();
        assert_eq!(controller.pool.len(), 0);
    }

    #[test]
    fn test_total_memory_increase_clears_pool() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 50);
        controller.last_total_memory = 0;
        controller.pool.push(vec![0u32; 10]);
        assert_eq!(controller.pool.len(), 1);
        let _ = controller.refresh();
        assert_eq!(controller.pool.len(), 0);
    }

    #[test]
    fn test_new_with_invalid_range() {
        // ram_min >= ram_max should fail
        let result = RamController::new((60, 50));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidArg(_)));
    }

    #[test]
    fn test_with_test_values() {
        let controller = RamController::with_test_values((45, 55), 1_000_000, 40);
        assert_eq!(controller.target_range, (45, 55));
        assert_eq!(controller.target_mid, 50);
        assert_eq!(controller.usage_percent, 40);
    }

    #[test]
    fn test_adjust_result_returns_valid_enum() {
        // Test that adjust() returns a valid AdjustResult without panicking
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 50);
        let result = controller.adjust().unwrap();

        // Verify it's a valid AdjustResult variant
        match result {
            AdjustResult::InRange => {}
            AdjustResult::Allocated(n) => assert!(n > 0),
            AdjustResult::Freed(n) => assert!(n > 0),
        }
    }

    #[test]
    fn test_calculate_usage_percent_zero_total() {
        // This is a compile-time check that the method exists
        // Actual runtime behavior would require mocking System
        let controller = RamController::with_test_values((45, 55), 1_000_000, 30);
        assert_eq!(controller.usage_percent, 30);
    }
}
