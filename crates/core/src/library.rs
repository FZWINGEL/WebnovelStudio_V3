//! The project library index and app preferences, moved to `wns-library` (L7).
//!
//! The catalogue types, the preference CAS and the Codex transport preference
//! all live there now. What is left here is the one thing `wns-library` cannot
//! name: the project type its `create`, `register` and `finish` methods take
//! and return. The alias below fixes the crate's factory parameter to core's,
//! so every existing `webnovel_core::library::Library` path resolves and no
//! caller ever writes a factory.
//!
//! L7 rather than L5: the library's import workflow drives `wns-transfer`'s V2
//! installer, so it sits above the portability crate. Nothing depends on it but
//! the app shell, which the layer table does not register.

pub use wns_library::codex_transport;
pub use wns_library::library::*;

/// The library index, with this crate's project factory fixed.
pub type Library = wns_library::library::Library<crate::transfer::CoreProjectFactory>;
