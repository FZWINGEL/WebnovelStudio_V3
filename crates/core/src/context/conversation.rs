//! Exact prior discussion turns. These are contextual evidence, not guidance
//! or accepted story facts. The first selector deliberately uses recency only.
use super::{Audience, ContextPurpose};
use crate::documents::ScopeGrant;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_CONTEXT_TURNS: usize = 4;
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessage {
    pub id: String,
    pub content: String,
    pub scope: Option<ScopeGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationTurn {
    pub run_id: String,
    pub packet_id: String,
    pub source_snapshot_id: String,
    pub policy_version: String,
    pub user: ConversationMessage,
    pub assistant: ConversationMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenConversation {
    pub project_id: String,
    pub operation_namespace: String,
    pub document_id: String,
    pub thread_id: String,
    /// Newest first for deterministic prefix selection. Delivered turns are
    /// reversed into chronological order without splitting a turn.
    pub turns: Vec<ConversationTurn>,
    /// Older complete eligible turns retained locally but outside this cap.
    pub omitted_turns: u32,
}

pub(crate) fn validate_conversation(
    conversation: Option<&FrozenConversation>,
    project_id: &str,
    document_id: &str,
    policy_version: &str,
    audience: Audience,
    purpose: ContextPurpose,
) -> Result<(), String> {
    let Some(conversation) = conversation else {
        return Ok(());
    };
    if audience != Audience::AuthorRoom || purpose != ContextPurpose::Discuss {
        return Err(
            "Recent discussion is available only to author-room discussion requests.".into(),
        );
    }
    if conversation.project_id != project_id
        || conversation.document_id != document_id
        || conversation.operation_namespace.is_empty()
        || conversation.thread_id.is_empty()
        || conversation.turns.len() > MAX_CONTEXT_TURNS
    {
        return Err(
            "The discussion context belongs to another thread or exceeds its turn limit.".into(),
        );
    }
    let mut ids = HashSet::new();
    let mut runs = HashSet::new();
    let mut size = 0usize;
    for turn in &conversation.turns {
        if turn.run_id.is_empty()
            || !runs.insert(&turn.run_id)
            || turn.packet_id.is_empty()
            || turn.source_snapshot_id.is_empty()
            || turn.policy_version != policy_version
            || turn.assistant.scope.is_some()
        {
            return Err("A prior turn has an invalid identity, policy, or message scope.".into());
        }
        for message in [&turn.user, &turn.assistant] {
            if message.id.is_empty()
                || !ids.insert(&message.id)
                || message.content.trim().is_empty()
            {
                return Err("A prior discussion turn is incomplete or repeats a message.".into());
            }
        }
        size = size
            .checked_add(serde_json::to_vec(turn).map_err(|e| e.to_string())?.len())
            .ok_or("The prior discussion size overflowed.")?;
    }
    if size > MAX_CONTEXT_BYTES {
        return Err("The exact prior discussion exceeds its local selection limit.".into());
    }
    Ok(())
}
