use super::*;

pub(crate) const PREVIEW_KIND: &str = "adoptionPreview";
pub(crate) const DECISION_KIND: &str = "adoptionDecision";
pub(crate) const RECEIPT_KIND: &str = "adoptChatPreview";
pub(crate) const MAX_TARGETS: usize = 3;

pub(crate) struct GroupOrigin {
    pub(crate) output_hash: String,
    pub(crate) effects: Option<ChatGroupEffectsOutput>,
    pub(crate) handles: HashMap<String, (Head, String)>,
    pub(crate) draft_keys: HashMap<String, (String, Head, String)>,
}
