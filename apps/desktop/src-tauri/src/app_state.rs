//! The one piece of state the Tauri shell manages.
//!
//! The shell used to register six independent managed states — the project
//! registry, the library, the provider runtime and three recovery trackers —
//! and every command named whichever subset it needed. With one state owning
//! all six, a command takes `State<'_, AppState>` and binds the facades it
//! wants from it: the set of managed types is a property of the app rather
//! than of each command's signature.
//!
//! This is the shell's half of the architecture's step 9. The facades
//! themselves are unchanged; the fields are the same types `main.rs` used to
//! `manage()` individually, held rather than registered separately.
//!
//! The state is built inside `setup` rather than on the `Builder`, because the
//! library's root depends on the app's resolved local-data directory and
//! opening it can fail. Nothing invokes a command before `setup` returns.

/// Every facade a command can reach.
pub struct AppState {
    /// Open projects, keyed by project id.
    pub projects: crate::commands::project_commands::DesktopProjects,
    /// The app-local library index. Opened in `setup`; see `main.rs`.
    pub library: crate::commands::library_commands::DesktopLibrary,
    /// Live provider work and its cancellation tokens.
    pub providers: crate::provider_runtime::DesktopProviders,
    /// Discussion saves awaiting a materialization decision.
    pub discussion_recovery: crate::discussion_recovery::DiscussionRecovery,
    /// Memory writes awaiting the same.
    pub memory_recovery: crate::memory_recovery::MemoryRecovery,
    /// In-flight endpoint discovery probes.
    pub endpoint_discovery: crate::commands::endpoint_discovery::EndpointDiscovery,
}
