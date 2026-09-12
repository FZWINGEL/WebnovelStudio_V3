//! One module per feature, and nothing else.
//!
//! The shell's job is to expose the facades over IPC. Each module here owns
//! one feature's commands and the adapter state only that feature needs; the
//! shared state is `crate::app_state::AppState`. What is *not* here is as
//! deliberate: `provider_runtime`, `author_start`, the recovery trackers and
//! the live-session adapters are reached by several features, so they sit
//! above this directory rather than inside it.

pub mod app_close_commands;
pub mod codex_transport_commands;
pub mod context_commands;
pub mod discussion_commands;
pub mod endpoint_commands;
pub mod endpoint_discovery;
pub mod export_commands;
pub mod guidance_commands;
pub mod library_commands;
pub mod memory_commands;
pub mod project_chat_commands;
pub mod project_commands;
pub mod provider_commands;
pub mod recovery_commands;
pub mod review_commands;
pub mod source_pin_commands;
pub mod v2_import_commands;
pub mod workshop_commands;
pub mod workshop_generation_commands;
