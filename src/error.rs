//! Error types for the application.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("CPU operation failed: {0}")]
    Cpu(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),
}
