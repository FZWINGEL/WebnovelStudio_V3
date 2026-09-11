//! Frozen project-chat vocabulary carried inside an immutable context manifest.
//!
//! Moved down from `projects/project_chat.rs` and
//! `projects/project_chat_context.rs` as part of the packet-compiler inversion
//! (`docs/V3_ARCHITECTURE_MODULAR.md` §3.4).
//!
//! [`FrozenContext`](crate::frozen::FrozenContext) embeds `FrozenProjectChat`,
//! so the compiler could not be extracted while this vocabulary sat above it.
//! These are shapes a compiled packet *carries* — packet input — which puts them
//! at the compiler's layer rather than above it. The distinction matters: it is
//! exactly the mistake the inversion corrects, where the compiler had to reach
//! upward for its own input types.
//!
//! The operations on these shapes — `freeze_project_chat_at` above all — stay
//! where they are, because they read SQLite and belong to the story layer.

use serde::{Deserialize, Serialize};
use wns_kernel::{CoreError, CoreResult, Head, check_id};

/// The bounded story surface to which a question decision applies. A missing
/// scope on an older client means `project`; references are authenticated
/// against the producing conversation before the event is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatDispositionScopeKind {
    Project,
    Task,
    Chapter,
    Document,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatDispositionScope {
    pub kind: ChatDispositionScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
}

impl ChatDispositionScope {
    pub fn validate_shape(&self) -> CoreResult<()> {
        match self.kind {
            ChatDispositionScopeKind::Project => {
                if self.reference_id.is_some() {
                    return Err(CoreError::new(
                        "InvalidDisposition",
                        "A project disposition scope cannot carry a reference.",
                    ));
                }
            }
            ChatDispositionScopeKind::Task
            | ChatDispositionScopeKind::Chapter
            | ChatDispositionScopeKind::Document => {
                let reference = self.reference_id.as_deref().ok_or_else(|| {
                    CoreError::new(
                        "InvalidDisposition",
                        "A non-project disposition scope requires a reference.",
                    )
                })?;
                check_id(reference)?;
            }
        }
        Ok(())
    }
}

impl Default for ChatDispositionScope {
    fn default() -> Self {
        Self {
            kind: ChatDispositionScopeKind::Project,
            reference_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatUnknownTo {
    Author,
    Reader,
    Both,
}

/// An unadopted assistant draft may be attached only by its exact current
/// draft head and disposition version.  A draft reference is never inferred
/// from a title, a document ID supplied by the renderer, or a quoted answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectChatDraftRef {
    pub head: Head,
    pub disposition_version: String,
}

/// Frozen project-chat identity stored inside the immutable context manifest.
/// `source_refs` and `task_draft_refs` retain the exact request claims after
/// the project owner has authenticated them against SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenProjectChat {
    pub conversation_id: String,
    pub anchor_document_id: String,
    pub operation_namespace: String,
    pub source_refs: Vec<Head>,
    pub task_draft_refs: Vec<ProjectChatDraftRef>,
    /// Optional for immutable compatibility with snapshots written before
    /// project-chat prompt recipes were versioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_recipe_version: Option<String>,
    /// Latest author decisions for response questions and assumptions. The
    /// text is resolved from the immutable producing output; chat history is
    /// never copied into a story packet a second time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dispositions: Vec<FrozenProjectChatDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenProjectChatDisposition {
    pub item_id: String,
    pub payload_hash: String,
    pub reference_id: String,
    pub producer_run_id: String,
    pub key: String,
    pub item_kind: String,
    pub text: String,
    pub disposition: String,
    pub version: String,
    pub rationale: String,
    #[serde(default)]
    pub scope: ChatDispositionScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_to: Option<ChatUnknownTo>,
}
