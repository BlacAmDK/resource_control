# Agent Guidelines for resource_control

Rust binary project for controlling server CPU/memory usage through multi-threaded
resource management.

## Project Structure

```
src/
├── main.rs          # Entry point, CLI parsing with clap, thread management
├── cpu.rs           # CPU control module
├── ram.rs           # RAM control module
└── error.rs         # Unified error types using thiserror

tests/
└── integration_test.rs  # Integration tests for CLI
```

## Dependencies

- **clap** (with derive): CLI argument parsing
- **sysinfo**: System information (CPU, memory)
- **core_affinity**: CPU core binding
- **jemallocator**: Global memory allocator
- **thiserror**: Error type derivation

## CLI Usage

```bash
cargo run -- [--OPTIONS]

Options:
  -c, --cpu-target <FLOAT>  Target CPU usage percentage (default: 55.0)
  -l, --ram-min <UINT>      Minimum RAM usage percentage (default: 45)
  -u, --ram-max <UINT>       Maximum RAM usage percentage (default: 55)
  -v, --verbose              Enable verbose logging
  -h, --help                 Print help
```

Example:
```bash
cargo run -- --cpu-target 60 -l 40 -u 60 --verbose
```

## Build/Lint/Test Commands

### Build
```bash
cargo build [--release]       # Build project
cargo run [--release]        # Build and run
```

### Testing
```bash
cargo test                    # Run all tests (unit + integration)
cargo test <test_name>       # Run a single test
cargo test cpu::              # Run tests in cpu module
cargo test ram::             # Run tests in ram module
```

### Linting
```bash
cargo clippy                  # Run linter
cargo clippy -- -D warnings   # Treat warnings as errors
```

### Formatting
```bash
cargo fmt -- --check          # Check formatting
cargo fmt                     # Format code
```

### Type Checking
```bash
cargo check                   # Quick syntax/type check
```

---

## Code Style Guidelines

### 1. Imports
Use qualified imports for clarity:
```rust
use std::cmp::min;
use std::thread;
use clap::Parser;
use crate::error::AppError;
```

### 2. Naming Conventions
| Element | Convention | Example |
|---------|------------|---------|
| Constants | SCREAMING_SNAKE_CASE | `TARGET_CPU_USAGE` |
| Types/Structs/Enums | PascalCase | `AppError`, `RamController` |
| Functions/Methods | snake_case | `adjust()`, `refresh()` |
| Variables | snake_case | `ram_pool`, `cpu_usage` |
| CLI Arguments | kebab-case | `cpu-target`, `ram-min` |

### 3. Error Handling
Use `thiserror` for error types:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Memory operation failed: {0}")]
    Memory(String),

    #[error("CPU operation failed: {0}")]
    Cpu(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),
}
```

Guidelines:
- Return `Result<T, Error>` from fallible operations
- Use `eprintln!` for error output to stderr
- Return errors rather than panicking
- Handle errors gracefully in main loops (log and continue)
- Validate CLI arguments early and exit with code 1 on failure

### 4. Documentation
- Add doc comments for public APIs (`///`)
- Comment complex logic, especially arithmetic
- Explain "why" for non-obvious code paths

### 5. Concurrency
Use thread spawning with vector to collect handles:
```rust
let mut handles = vec![];

// Spawn threads
let handle = thread::spawn(move || { /* ... */ });
handles.push(handle);

// Join all handles
for handle in handles {
    let _ = handle.join();
}
```

### 6. Formatting
- 4-space indentation
- Follow rustfmt defaults (100 char max line)
- No trailing whitespace

### 7. Type Annotations
- Prefer explicit annotations for clarity
- Use `u64`/`i64` for memory calculations
- Use `usize` for collection indices

```rust
let memory_one_percent: u64 = total_memory / 100;
let sum: u64 = 0;
```

### 8. Safety and Edge Cases
Prevent overflow and division by zero:
```rust
// Use checked_div for fallible division
let diff_bytes = total_memory_i64
    .saturating_mul(target_diff)
    .checked_div(100)
    .ok_or(AppError::Overflow)?;

// Guard against zero
if total_memory > 0 {
    self.usage_percent = used_memory * 100 / total_memory;
}
```

Key practices:
- Use `checked_div` instead of `/` for potentially zero denominators
- Use `saturating_mul` to prevent overflow
- Use `wrapping_add` when overflow is acceptable
- Handle edge cases explicitly (zero memory, empty pools)
- Limit per-iteration adjustments

### 9. Module Design
Each module should have:
- Clear public API with doc comments
- Unit tests in `#[cfg(test)]` module
- Error types in `error.rs`

```rust
// src/cpu.rs
//! CPU control module.
//! ...
pub struct CpuController { ... }
pub fn spawn_cpu_threads(target: f32) -> Result<Vec<JoinHandle<()>>, AppError> { ... }

#[cfg(test)]
mod tests { ... }
```

### 10. Global Allocator
When using jemalloc (place at end of main.rs):
```rust
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;
```

---

## CI/CD

GitHub Actions CI handles build and test. See `.github/workflows/rust.yml`.
```yaml
- cargo build --verbose
- cargo test --verbose
```
