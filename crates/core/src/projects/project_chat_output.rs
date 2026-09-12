//! Project-chat assistant response contract — moved to `wns-conversation`.
//!
//! This module has **no `impl` blocks at all**: it is free functions and record
//! shapes, with no `ProjectSession` half and no actor-side half, so it needed no
//! host trait and no command vocabulary. It is the first module of step 7 that
//! was a pure move — the pattern the step is *supposed* to look like, and the
//! only one in the tree where the answer to "what does this need from the
//! actor?" is "nothing".
//!
//! Everything public is re-exported, so `projects::project_chat_output::X`
//! resolves for `discussions` and the seven files under `project_chat/` that
//! call into it, exactly as it did before the move.

pub use wns_context::project_chat_output::*;
