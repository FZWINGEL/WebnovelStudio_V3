//! Project-chat context ownership — moved to `wns-conversation`.
//!
//! It is pure: no `impl` blocks, so no `ProjectSession` half and no actor-side
//! half, and the move is import rewrites and a re-export. Its callers are
//! `discussions` and `project_chat`, both bound for the same crate, and it
//! reaches only `story_context` (L4) and `wns-context` (L3) — both legal
//! downward calls from L5. That it can move at all is the return on the frozen
//! validation split: what used to be an L4→L5 upward edge is now an L3 call.

pub use wns_conversation::project_chat_context::*;
