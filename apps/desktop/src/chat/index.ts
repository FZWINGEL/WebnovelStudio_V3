// The public surface of this feature. Everything another feature may use
// is re-exported here; §4.1 makes the edge reviewable by making it a path.

export { ChapterRangeReview, suggestedChapterRange } from './ChapterRangeReview';
export { ConversationHistoryPanel } from './ConversationHistoryPanel';
export { documentBlocks } from './DraftReviewDiff';
export { ProjectConversation } from './ProjectConversation';
export type { ProjectConversationHandle } from './ProjectConversation';
export { confirmChapterRange } from './confirmChapterRange';
