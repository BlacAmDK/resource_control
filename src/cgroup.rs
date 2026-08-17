use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use sysinfo::System;

use crate::error::AppError;

/// A simple cgroup v2 manager for the current process.
///
/// Creates a per-process cgroup under /sys/fs/cgroup/resource_control_<pid>,
/// writes the current pid into cgroup.procs, applies cpu.max and memory.max,
/// and exposes a cleanup() method to move the process back to the root cgroup
/// and remove the created directory.
pub struct Cgroup {
    path: PathBuf,
}

impl Cgroup {
    /// Create a cgroup for the current process and apply limits.
    ///
    /// cpu_target: percentage 0.0..=100.0 (>=100 => no cpu limit)
    /// ram_max_percent: upper memory limit in percent (0..=100)
    pub fn setup(cpu_target: f32, ram_max_percent: u64) -> Result<Self, AppError> {
        let cgroup_base = Path::new("/sys/fs/cgroup");
        let controllers_file = cgroup_base.join("cgroup.controllers");
        if !controllers_file.exists() {
            return Err(AppError::Cpu(
                "cgroup v2 not detected at /sys/fs/cgroup (cgroup.controllers missing)".into(),
            ));
        }

        let pid = std::process::id();
        let cg_name = format!("resource_control_{}", pid);
        let cg_path = cgroup_base.join(&cg_name);

        fs::create_dir_all(&cg_path).map_err(|e| {
            AppError::Cpu(format!(
                "failed to create cgroup dir {}: {}",
                cg_path.display(),
                e
            ))
        })?;

        // Write pid into cgroup.procs
        let procs_path = cg_path.join("cgroup.procs");
        let mut procs = OpenOptions::new()
            .write(true)
            .append(true)
            .open(&procs_path)
            .map_err(|e| {
                AppError::Cpu(format!(
                    "failed to open {} for writing: {}",
                    procs_path.display(),
                    e
                ))
            })?;
        writeln!(procs, "{}", pid).map_err(|e| {
            AppError::Cpu(format!(
                "failed to write pid to {}: {}",
                procs_path.display(),
                e
            ))
        })?;

        // Memory limit
        if ram_max_percent < 100 {
            let mut sys = System::new();
            sys.refresh_memory();
            // sysinfo returns kilobytes
            let total_kb = sys.total_memory();
            let limit_bytes = total_kb
                .saturating_mul(ram_max_percent)
                .saturating_div(100)
                .saturating_mul(1024);
            let mem_max_path = cg_path.join("memory.max");
            fs::write(&mem_max_path, limit_bytes.to_string()).map_err(|e| {
                AppError::Cpu(format!(
                    "failed to write memory.max ({}): {}",
                    mem_max_path.display(),
                    e
                ))
            })?;
        } else {
            let mem_max_path = cg_path.join("memory.max");
            let _ = fs::write(&mem_max_path, "max");
        }

        // CPU limit
        let cpu_max_path = cg_path.join("cpu.max");
        if cpu_target >= 100.0 {
            fs::write(&cpu_max_path, "max").map_err(|e| {
                AppError::Cpu(format!(
                    "failed to write cpu.max ({}): {}",
                    cpu_max_path.display(),
                    e
                ))
            })?;
        } else {
            let period: u64 = 100_000; // 100ms in us
            let mut sys = System::new_all();
            let cpu_count = sys.cpus().len() as u64;
            let quota = ((cpu_target as f64) * (cpu_count as f64) * (period as f64) / 100.0)
                .round() as u64;
            let quota = quota.max(1);
            let content = format!("{} {}", quota, period);
            fs::write(&cpu_max_path, content).map_err(|e| {
                AppError::Cpu(format!(
                    "failed to write cpu.max ({}): {}",
                    cpu_max_path.display(),
                    e
                ))
            })?;
        }

        Ok(Self { path: cg_path })
    }

    /// Attempt to move the current process back to the parent cgroup (root)
    /// and remove the created directory. Non-fatal: errors are returned but
    /// cleanup callers should log and continue.
    pub fn cleanup(&self) -> Result<(), AppError> {
        let pid = std::process::id();
        let parent_procs = Path::new("/sys/fs/cgroup").join("cgroup.procs");

        // Move ourselves back to the root cgroup
        let mut parent = OpenOptions::new()
            .write(true)
            .append(true)
            .open(&parent_procs)
            .map_err(|e| {
                AppError::Cpu(format!(
                    "failed to open parent cgroup.procs {}: {}",
                    parent_procs.display(),
                    e
                ))
            })?;
        writeln!(parent, "{}", pid).map_err(|e| {
            AppError::Cpu(format!(
                "failed to write pid to parent cgroup.procs {}: {}",
                parent_procs.display(),
                e
            ))
        })?;

        // Now remove our cgroup directory
        fs::remove_dir(&self.path).map_err(|e| {
            AppError::Cpu(format!(
                "failed to remove cgroup dir {}: {}",
                self.path.display(),
                e
            ))
        })?;

        Ok(())
    }
}
