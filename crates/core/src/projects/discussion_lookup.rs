//! Durable discussion-lookup state -- moved to `wns-conversation`.
//!
//! It is a pure module: no `impl ProjectSession`, no `impl OwnedProject`, so no
//! host trait and no command vocabulary. The move is import rewrites and a
//! re-export. Re-exported at the historical path so every caller resolves.

pub use wns_conversation::discussion_lookup::*;
