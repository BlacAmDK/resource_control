use std::cmp::min;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, MemoryRefreshKind, System};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

const TARGET_CPU_USAGE: f32 = 55.0; // Target CPU usage percentage
const TARGET_RAM_USAGE_RANGE: (u64, u64) = (45, 55); // Target RAM usage percentage range
const WORK_DURATION: Duration = MINIMUM_CPU_UPDATE_INTERVAL; // Duration of work
const SLEEP_DURATION: Duration = MINIMUM_CPU_UPDATE_INTERVAL; // Duration of sleep

fn main() {
    let mut handles = vec![];

    // RAM Control
    let handle = thread::spawn(move || {
        let mut ram_pool = match Ram::new() {
            Ok(ram) => ram,
            Err(e) => {
                eprintln!("Failed to initialize RAM controller: {:?}", e);
                return;
            }
        };

        loop {
            thread::sleep(SLEEP_DURATION);
            if let Err(e) = ram_pool.adjust() {
                eprintln!("RAM adjustment error: {:?}", e);
                // Continue attempting to adjust despite errors
            }
        }
    });
    handles.push(handle);

    // CPU Control
    for cpu_id in 0..System::new_all().cpus().len() {
        // if cpu_id <= 1 {
        //     continue;
        // }
        let handle = thread::spawn(move || {
            // Bind the current thread to the specific CPU core
            core_affinity::set_for_current(core_affinity::CoreId { id: cpu_id });
            let mut system = System::new();
            system.refresh_cpu_usage();
            thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
            loop {
                system.refresh_cpu_usage();
                let cpu_usage = system.cpus()[cpu_id].cpu_usage();
                if cpu_usage < TARGET_CPU_USAGE {
                    // println!("CPU{} is Working", cpu_id);
                    let start = Instant::now();
                    while start.elapsed() < WORK_DURATION {
                        // Simulate some CPU-bound work
                        let mut sum = 0u64;
                        for x in 1..10000 {
                            sum = sum.wrapping_add(x);
                        }
                        // Prevent compiler optimization
                        std::hint::black_box(sum);
                    }
                } else {
                    // Sleep to reduce CPU usage
                    thread::sleep(SLEEP_DURATION);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to finish (they won't in this infinite loop)
    for handle in handles {
        let _ = handle.join();
    }
}

struct Ram {
    pool: Vec<Vec<u32>>,
    system: System,
    target_usage_range: (u64, u64),
    target_usage_range_mid: u64,
    usage_percent: u64,
    memory_one_percent: u64,
}

#[derive(Debug)]
enum MemoryError {
    DivisionByZero,
    InvalidMemorySize,
    Overflow,
}

impl Ram {
    fn new() -> Result<Ram, MemoryError> {
        let mut system = System::new();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        let total_memory = system.total_memory();
        if total_memory == 0 {
            return Err(MemoryError::InvalidMemorySize);
        }

        let memory_one_percent = min(total_memory / 100, 1024 * 1024 * 1024); // 1% of total memory, max 1G
        if memory_one_percent == 0 {
            return Err(MemoryError::DivisionByZero);
        }

        let usage_percent = if total_memory > 0 {
            system.used_memory() * 100 / total_memory
        } else {
            0
        };

        Ok(Ram {
            pool: Vec::with_capacity(
                (total_memory as usize / 100 * TARGET_RAM_USAGE_RANGE.1 as usize)
                    .max(1),
            ),
            target_usage_range: TARGET_RAM_USAGE_RANGE,
            target_usage_range_mid: (TARGET_RAM_USAGE_RANGE.0 + TARGET_RAM_USAGE_RANGE.1) / 2,
            usage_percent,
            memory_one_percent,
            system,
        })
    }
    fn refresh(&mut self) {
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        self.usage_percent = if total_memory > 0 {
            used_memory * 100 / total_memory
        } else {
            0
        };
    }

    fn adjust(&mut self) -> Result<(), MemoryError> {
        self.refresh();

        if self.usage_percent >= self.target_usage_range.0
            && self.usage_percent <= self.target_usage_range.1
        {
            // RAM usage in range, do nothing
            return Ok(());
        }

        // 使用 i64 进行计算以避免溢出
        let target_diff = if self.usage_percent < self.target_usage_range.0 {
            // Need to allocate: target_mid - current_usage (positive)
            (self.target_usage_range_mid as i64) - (self.usage_percent as i64)
        } else {
            // Need to free: current_usage - target_mid (negative)
            (self.usage_percent as i64) - (self.target_usage_range_mid as i64)
        };

        // 计算需要调整的字节数: total_memory * target_diff / 100
        let total_memory_i64 = self.system.total_memory() as i64;
        let diff_bytes = total_memory_i64
            .saturating_mul(target_diff)
            .checked_div(100)
            .ok_or(MemoryError::Overflow)?;

        // 转换为需要调整的内存块数
        let memory_one_percent_i64 = self.memory_one_percent as i64;
        let diff_blocks = diff_bytes
            .checked_div(memory_one_percent_i64)
            .ok_or(MemoryError::DivisionByZero)?;

        self.adjust_pool(diff_blocks)?;

        Ok(())
    }

    fn adjust_pool(&mut self, multiplier: i64) -> Result<(), MemoryError> {
        if multiplier > 0 {
            // Need to allocate memory
            let blocks_to_allocate = (multiplier as usize).min(100); // Limit to max 100 blocks per adjustment

            for _ in 0..blocks_to_allocate {
                if let Some(block) = self.pool.first() {
                    self.pool.push(block.clone());
                } else {
                    // Allocate initial block (1% of memory)
                    let size = (self.memory_one_percent / 4) as usize;
                    if size == 0 {
                        return Err(MemoryError::InvalidMemorySize);
                    }
                    let ram = vec![0u32; size];
                    self.pool.push(ram);
                }
            }
        } else if multiplier < 0 && !self.pool.is_empty() {
            // Need to free memory
            let blocks_to_free = (-multiplier as usize).min(self.pool.len());

            for _ in 0..blocks_to_free {
                self.pool.pop();
            }
        }

        Ok(())
    }
}
