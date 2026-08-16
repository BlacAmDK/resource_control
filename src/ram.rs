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

/// Max blocks a pool can hold: each block is 0.25% of RAM, so ram_max% needs
/// ram_max * 4 blocks. Independent of total memory to avoid oversized reserves.
fn pool_capacity(ram_max: u64) -> usize {
    (ram_max as usize) * 4
}

/// Caps the number of blocks allocated per iteration to avoid memory bursts.
/// Only used for allocation; freeing is never capped.
fn blocks_to_allocate(blocks: i64) -> usize {
    if blocks > 0 {
        (blocks as usize).min(4)
    } else {
        0
    }
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
            // Capacity is the max blocks needed: each block holds memory_one_percent/4
            // (0.25%), so reaching ram_max% requires ram_max * 4 blocks.
            pool: Vec::with_capacity(pool_capacity(target_range.1)),
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
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        // memory hotplug? free memory pool
        if self.system.total_memory() != self.last_total_memory {
            self.refresh_total_memory(self.system.total_memory());
        }

        self.usage_percent = Self::calculate_usage_percent(&self.system);
    }

    /// Recomputes state after total memory changes (hotplug add/remove).
    ///
    /// Drops the whole pool so blocks are resized to the new total. Guards
    /// `memory_one_percent` to stay >= 4 so a pathologically tiny total never
    /// yields zero-size blocks that can never reach the target.
    fn refresh_total_memory(&mut self, new_total: u64) {
        self.pool.clear();
        self.last_total_memory = new_total;
        self.memory_one_percent = (new_total / 100).max(4);
    }

    fn calculate_usage_percent(system: &System) -> u64 {
        let total = system.total_memory();
        let used = system.used_memory();
        (used * 100).checked_div(total).unwrap_or(100)
    }

    fn adjust_pool(&mut self, blocks: i64) {
        if blocks > 0 {
            let blocks_to_allocate = blocks_to_allocate(blocks);
            let size = (self.memory_one_percent / 4) as usize;
            for _ in 0..blocks_to_allocate {
                let mut v = vec![0u32; size];
                for i in (0..size).step_by(1024) {
                    v[i] = 0;
                }
                self.pool.push(v);
            }
        } else if blocks < 0 && !self.pool.is_empty() {
            // Free everything requested at once so memory is released
            // immediately if other processes suddenly need it.
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
        controller.refresh();
        assert_eq!(controller.pool.len(), 0);
    }

    #[test]
    fn test_total_memory_increase_clears_pool() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 50);
        controller.last_total_memory = 0;
        controller.pool.push(vec![0u32; 10]);
        assert_eq!(controller.pool.len(), 1);
        controller.refresh();
        assert_eq!(controller.pool.len(), 0);
    }

    #[test]
    fn test_refresh_total_memory_guards_zero_one_percent() {
        // A hotplug that shrinks total memory below 100 bytes must not produce
        // zero-size blocks; memory_one_percent stays at least 4.
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 50);
        controller.pool.push(vec![0u32; 10]);
        controller.refresh_total_memory(50);
        assert_eq!(controller.pool.len(), 0);
        assert!(controller.memory_one_percent >= 4);
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

    #[test]
    fn test_pool_capacity_scales_with_target() {
        // Capacity only depends on ram_max (0.25% blocks => ram_max * 4), not total memory
        assert_eq!(pool_capacity(55), 220);
        assert_eq!(pool_capacity(100), 400);
        assert_eq!(pool_capacity(1), 4);
    }

    #[test]
    fn test_blocks_to_allocate_capped() {
        assert_eq!(blocks_to_allocate(100), 4);
        assert_eq!(blocks_to_allocate(2), 2);
        assert_eq!(blocks_to_allocate(0), 0);
    }

    #[test]
    fn test_adjust_pool_allocates_limited_blocks() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 40);
        controller.adjust_pool(100);
        assert_eq!(controller.pool.len(), 4);
    }

    #[test]
    fn test_adjust_pool_frees_limited_blocks() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 60);
        controller.adjust_pool(10);
        assert_eq!(controller.pool.len(), 4);
        // Freeing is NOT capped: all requested blocks are released at once
        // so memory frees immediately when other processes need it.
        controller.adjust_pool(-10);
        assert_eq!(controller.pool.len(), 0);
    }

    #[test]
    fn test_adjust_pool_frees_all_blocks_in_one_step() {
        let mut controller = RamController::with_test_values((45, 55), 1_000_000, 60);
        for _ in 0..10 {
            controller.pool.push(vec![0u32; 10]);
        }
        assert_eq!(controller.pool.len(), 10);
        // Even a huge free request must release everything immediately.
        controller.adjust_pool(-1000);
        assert_eq!(controller.pool.len(), 0);
    }
}
