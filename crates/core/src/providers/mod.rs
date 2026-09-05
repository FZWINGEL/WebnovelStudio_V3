//! Provider boundaries.
//!
//! Pure protocol parsers and a bounded Windows Codex runtime. The native
//! application dispatches frozen packets and reports terminal results back to
//! the discussion actor; providers never own manuscript mutations.

pub mod catalog;
pub mod claude_exec;
pub mod cli;
pub mod codex_exec;
pub mod codex_profile;
#[cfg(windows)]
pub mod codex_runner;
#[cfg(windows)]
pub mod codex_runtime;
pub mod preferences;
