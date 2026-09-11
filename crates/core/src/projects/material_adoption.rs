//! Transaction-local writes for author-approved material — moved to
//! `wns-documents`.
//!
//! The second pure move of step 7: no `impl` blocks, so no host trait and no
//! command vocabulary. It went to `wns-documents` rather than to either of its
//! consumers because it has two — `project_chat/adoption.rs` and `workshop.rs` —
//! and those are siblings at L5. A helper two siblings both need has to sit
//! below both of them, or the layering rule turns it into a sideways edge.
//!
//! It is document machinery regardless of that argument: it takes a caller-owned
//! transaction and writes a document body with a before/after checkpoint pair,
//! which is the same concern `history` expresses for restore.

pub(crate) use wns_documents::material_adoption::*;
