//! Actor-owned inspection and stop coordination for local background work.
//!
//! Moved to `wns-conversation`; this is the re-export at the historical path,
//! so `webnovel_core::projects::background_work::{…}` and the `work_api`
//! façade keep resolving.

pub use wns_conversation::background_work::*;
