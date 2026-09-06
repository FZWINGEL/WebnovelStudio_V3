//! Provider boundaries.
//!
//! Pure protocol parsers and a bounded Windows Codex runtime. The native
//! application dispatches frozen packets and reports terminal results back to
//! the discussion actor; providers never own manuscript mutations.

pub mod adapter;
pub mod catalog;
pub mod claude_exec;
pub mod claude_profile;
#[cfg(windows)]
pub mod claude_runner;
#[cfg(windows)]
pub mod claude_runtime;
pub mod cli;
pub mod codex_catalog;
#[cfg(windows)]
pub mod codex_discovery;
pub mod codex_exec;
pub mod codex_profile;
#[cfg(windows)]
pub mod codex_runner;
#[cfg(windows)]
pub mod codex_runtime;
pub mod credentials;
pub mod endpoints;
pub mod http_request;
pub mod openai_compatible;
pub mod preferences;
