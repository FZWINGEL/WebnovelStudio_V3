//! Bounded recent-turn selection over retained discussion records — moved to
//! `wns-context::conversation`.
//!
//! The second cycle discharged by moving one half down rather than both modules
//! together. `conversation_context` rebuilds a turn by decoding a stored
//! manifest, and `story_context` freezes a conversation by selecting one — so
//! whichever crate held this file sat above the other, and `story_context`
//! (destined for `wns-story`, L4) could not move while it was in core.
//!
//! Both halves of what looked like a module-level cycle turned out to be
//! separate: `decode_snapshot` went to `wns-context::frozen` beside the type it
//! constructs, and the selectors came here beside `FrozenConversation`, the type
//! they produce. Neither needs a host trait, because neither has an `impl`
//! block — this is a pure module like `project_chat_output`.
//!
//! Re-exported at the historical path for `story_context` and
//! `project_chat_context`.
