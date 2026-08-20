//! Veyra core: shared data models, configuration and error types used across
//! all Veyra crates. Contains no I/O, GUI or filesystem operations.

#![forbid(unsafe_code)]

pub mod config;
pub mod crash_report;
pub mod error;
pub mod logging_sanitizer;
pub mod security;

pub use config::XdgDirs;
pub use error::VeyraError;
