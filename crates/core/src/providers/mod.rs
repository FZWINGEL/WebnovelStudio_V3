//! Provider boundaries.
//!
//! Provider execution is intentionally not wired into the discussion actor yet.
//! The Windows child-process primitive is kept independent so it can be
//! qualified before a real adapter is allowed to call it.

pub mod catalog;
pub mod claude_exec;
pub mod cli;
pub mod codex_exec;
pub mod codex_profile;
pub mod preferences;
