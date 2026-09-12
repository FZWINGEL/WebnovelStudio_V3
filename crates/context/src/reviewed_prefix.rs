//! One member of the earlier reviewed prefix a summary is bound to.
//!
//! Moved down from `projects/reviewed_story.rs` together with
//! [`reviewed_summary`](crate::reviewed_summary), which embeds it: a summary's
//! dependencies are the exact reviewed prefix it was authored against, so the
//! type is part of the vocabulary a compiled packet carries rather than story
//! behaviour.

use serde::{Deserialize, Serialize};
use wns_kernel::Head;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewPrefixItem {
    pub document_id: String,
    pub title: String,
    pub bundle_id: String,
    pub revision_id: String,
    pub head: Head,
}
