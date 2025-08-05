use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, MemoryRefreshKind, System};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

const TARGET_CPU_USAGE: f32 = 70.0; // Target CPU usage percentage
const TARGET_RAM_USAGE_RANGE: (u64, u64) = (55, 70); // Target RAM usage percentage range
const WORK_DURATION: Duration = MINIMUM_CPU_UPDATE_INTERVAL; // Duration of work
const SLEEP_DURATION: Duration = MINIMUM_CPU_UPDATE_INTERVAL; // Duration of sleep

fn main() {
    let mut handles = vec![];

    // RAM Control
    let handle = thread::spawn(move || {
        let mut ram_pool = Ram::new();
        loop {
            thread::sleep(SLEEP_DURATION);
            ram_pool.adjust();
        }
    });
    handles.push(handle);

    // CPU Control
    for cpu_id in 0..System::new_all().cpus().len() {
        if cpu_id <= 1 {
            continue;
        }
        let handle = thread::spawn(move || {
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
                        let mut _sum = 0;
                        for x in 1..100 {
                            _sum += x;
                            _sum -= x;
                        }
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
}

impl Ram {
    fn new() -> Ram {
        let mut system = System::new();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        Ram {
            pool: Vec::with_capacity(
                system.total_memory() as usize / 100 * TARGET_RAM_USAGE_RANGE.1 as usize,
            ),
            target_usage_range: TARGET_RAM_USAGE_RANGE,
            target_usage_range_mid: (TARGET_RAM_USAGE_RANGE.0 + TARGET_RAM_USAGE_RANGE.1) / 2,
            usage_percent: system.used_memory() * 100 / system.total_memory(),
            system,
        }
    }
    fn refresh(&mut self) {
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        self.usage_percent = self.system.used_memory() * 100 / self.system.total_memory();
    }
    fn adjust(&mut self) {
        self.refresh();
        if self.usage_percent >= self.target_usage_range.0
            && self.usage_percent <= self.target_usage_range.1
        { // RAM usage in range, do nothing
        } else if self.usage_percent < self.target_usage_range.0 {
            // allocate ram(add 1G per call)
            let diff = self.system.total_memory() / 100
                * (self.target_usage_range_mid - self.usage_percent)
                / 1024
                / 1024
                / 1024;
            self.adjust_pool(diff.try_into().unwrap_or(0));
        } else {
            // free ram
            let diff = self.system.total_memory() / 100
                * (self.usage_percent - self.target_usage_range_mid)
                / 1024
                / 1024
                / 1024;
            self.adjust_pool(-(diff).try_into().unwrap_or(0));
        }
    }
    fn adjust_pool(&mut self, multiplier: i32) {
        // if need allocate ram(multiplier>0), only allocate 1G
        // if need free ram(multiplier<0), free -multiplier times
        if multiplier > 0 {
            if let Some(block) = self.pool.first() {
                self.pool.push(block.clone());
            } else {
                let size: u32 = 1024 * 1024 * 1024 / 4; // size of 1GB u32
                let mut ram = Vec::with_capacity(size as usize);
                for i in 0..size {
                    ram.push(i);
                }
                self.pool.push(ram);
            }
        } else if multiplier < 0 {
            for _ in 0..(-multiplier) {
                self.pool.pop();
            }
        }
    }
}
