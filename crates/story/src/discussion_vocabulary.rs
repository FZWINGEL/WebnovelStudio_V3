//! Discussion-start vocabulary shared below both conversation crates.
//!
//! Moved down from `webnovel-core::projects::discussions`. `project_chat` and
//! `discussions` (both bound for `wns-conversation`, L5) need `StartDiscussion`
//! and `FeedbackIntent`, and so do `workshop` and `workshop_generation` (both
//! bound for `wns-workshop`, L5). Vocabulary two future sibling crates both
//! need has to sit below both — the argument that put the provider delivery
//! types in `wns-providers` and the workshop vocabulary in this crate.
//!
//! `discussions` re-exports all four items at their historical paths.

use serde::{Deserialize, Serialize};
use wns_context::lookup::LookupAllowance;
use wns_context::packet::{MockContextBudget, ProviderBinding};
use wns_context::{BasisKind, ContextPurpose, SafeBriefInput};
use wns_documents::{Endpoint, ScopeKind};
use wns_kernel::{CoreError, CoreResult, Head, ProjectAccess};

/// The two author-room actions supported by a discussion request.  `Discuss`
/// is intentionally the wire default so older clients produce the same
/// request hash they did before intent was added to the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackIntent {
    #[default]
    Discuss,
    ProposeEdits,
    Continue,
    WorkshopExplore,
}

impl FeedbackIntent {
    pub fn is_discuss(self) -> bool {
        self == Self::Discuss
    }

    pub fn purpose(self) -> ContextPurpose {
        match self {
            Self::Discuss => ContextPurpose::Discuss,
            Self::ProposeEdits => ContextPurpose::Revise,
            Self::Continue => ContextPurpose::Continue,
            Self::WorkshopExplore => ContextPurpose::StoryQuestion,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discuss => "discuss",
            Self::ProposeEdits => "proposeEdits",
            Self::Continue => "continue",
            Self::WorkshopExplore => "workshopExplore",
        }
    }

    pub fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "discuss" => Ok(Self::Discuss),
            "proposeEdits" => Ok(Self::ProposeEdits),
            "continue" => Ok(Self::Continue),
            "workshopExplore" => Ok(Self::WorkshopExplore),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved discussion draft has an unknown intent.",
            )),
        }
    }

    pub fn from_purpose(purpose: ContextPurpose) -> CoreResult<Self> {
        match purpose {
            ContextPurpose::Discuss => Ok(Self::Discuss),
            ContextPurpose::Revise => Ok(Self::ProposeEdits),
            ContextPurpose::Continue => Ok(Self::Continue),
            ContextPurpose::StoryQuestion => Ok(Self::WorkshopExplore),
            ContextPurpose::Plan | ContextPurpose::MemoryAnalysis => Err(CoreError::new(
                "InvalidContext",
                "The discussion snapshot has an unsupported purpose.",
            )),
        }
    }
}

pub fn skip_default_feedback_intent(intent: &FeedbackIntent) -> bool {
    intent.is_discuss()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionScopeInput {
    pub kind: ScopeKind,
    pub start: Option<Endpoint>,
    pub end: Option<Endpoint>,
    pub quote: String,
    pub source_body_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartDiscussion {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub instruction: String,
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
    pub previous_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}
