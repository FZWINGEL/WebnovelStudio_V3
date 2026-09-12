import type {
  AssistantDraft as WireAssistantDraft,
  ChapterDiscussionFeedback,
  ChatAdoptionAck,
  ChatAdoptionEffects,
  ChatAdoptionImpact,
  ChatAdoptionPlacement,
  ChatAdoptionPreview,
  ChatAdoptionRelationship,
  ChatAdoptionSupersession,
  ChatAdoptionTarget as WireChatAdoptionTarget,
  ChatDocumentSave,
  ChatProtectedContent,
  ChatRelationshipDependency,
  ConversationItem as WireConversationItem,
  ProjectChapterComposer,
  ProjectComposer,
  ProjectComposerSnapshot,
  SaveProjectComposer,
  StartProjectChapter,
  StartProjectChat,
} from './generated/conversation';

// Rust carries a document body as `serde_json::Value` and an event payload as
// an opaque value, so the generated shapes say `any`. The editor knows better,
// and saying so once here is narrower than the hand-written mirror it replaces:
// every *other* field still comes from Rust, so a rename there is still a
// compile error rather than a runtime `undefined`. Same treatment as `Revision`
// in `ipc/projects`.
export type ConversationItem = Omit<WireConversationItem, 'payload'> & { payload: Record<string, unknown> };
export type ChatAdoptionTarget = Omit<WireChatAdoptionTarget, 'body'> & { body: WnsDocument };
export type AssistantDraft = Omit<WireAssistantDraft, 'document'> & { document: DocumentRecord };
export type {
  ChapterDiscussionFeedback,
  ChatAdoptionAck,
  ChatAdoptionEffects,
  ChatAdoptionImpact,
  ChatAdoptionPlacement,
  ChatAdoptionPreview,
  ChatAdoptionRelationship,
  ChatAdoptionSupersession,
  ChatDocumentSave,
  ChatProtectedContent,
  ChatRelationshipDependency,
  ProjectChapterComposer,
  ProjectComposer,
  ProjectComposerSnapshot,
  SaveProjectComposer,
  StartProjectChapter,
  StartProjectChat,
};

import type {
  ChatDispositionScope,
  ChatDispositionScopeKind,
  ChatUnknownTo,
  ProjectBriefOrigin,
  ProjectChatDraftRef,
} from './generated/context';
export type {
  ChatDispositionScope,
  ChatDispositionScopeKind,
  ChatUnknownTo,
  ProjectBriefOrigin,
  ProjectChatDraftRef,
};

import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor';
import type { CheckpointRequest, DocumentRecord, Head, ProjectAccess, ReconcileRequest, ReconciledDocument, Revision, SaveAck, SaveSnapshot } from './projects';
import type { DiscussionRun, DiscussionStart, DiscussionScope, FeedbackIntent, SafeBriefInput } from './discussions';
import type { MockContextBudget } from './context';
import type { ModelSelection } from './providers';

export interface ProjectConversationView {
  id: string; composer: ProjectComposerSnapshot; items: ConversationItem[]; olderBefore: string | null;
  activeRun: DiscussionRun | null; drafts: AssistantDraft[]; sourceEpoch: string; policyEpoch: string; earlierWorkshop: boolean;
  workerIssues: { runId: string; detail: string }[];
  documentSaves?: ChatDocumentSave[];
}
export interface ChapterRangeProposal {
  sourceHead: Head;
  firstBlockId: string;
  lastBlockId: string;
  quote: string;
}
export interface ChatDispositionOptions { scope?: ChatDispositionScope; unknownTo?: ChatUnknownTo }
export const readChatAdoptionPreview = (access: ProjectAccess, conversationId: string, previewId: string): Promise<ChatAdoptionPreview> => invoke('read_chat_adoption_preview', { access, conversationId, previewId });
export const emptyProjectComposer = (): ProjectComposer => ({ text: '', sourceRefs: [], taskDraftRefs: [] });
export const readProjectConversation = (access: ProjectAccess, before: string | null = null, limit = 40): Promise<ProjectConversationView> => invoke('read_project_conversation', { request: { access, before, limit } });
export const saveProjectComposer = (request: SaveProjectComposer): Promise<ProjectComposerSnapshot> => invoke('save_project_composer', { request });
export const startProjectChat = (request: StartProjectChat, modelSelection: ModelSelection | null): Promise<DiscussionStart> => invoke('start_project_chat', { request, modelSelection });
export const startProjectChapter = (request: StartProjectChapter, modelSelection: ModelSelection | null): Promise<DiscussionStart> => invoke('start_project_chapter', { request, modelSelection });
export const readProjectChapterFeedback = (access: ProjectAccess, runId: string): Promise<ChapterDiscussionFeedback | null> => invoke('read_project_chapter_feedback', { access, runId });
export const retryProjectChatSave = (access: ProjectAccess, conversationId: string, runId: string): Promise<void> => invoke('retry_project_chat_save', { access, conversationId, runId });
export const readAssistantDraft = (access: ProjectAccess, conversationId: string, documentId: string): Promise<AssistantDraft> => invoke('read_assistant_draft', { access, conversationId, documentId });
export const saveAssistantDraft = (conversationId: string, dispositionVersion: string, snapshot: SaveSnapshot): Promise<SaveAck> => invoke('save_assistant_draft', { request: { conversationId, dispositionVersion, snapshot } });
export const checkpointAssistantDraft = (conversationId: string, request: CheckpointRequest): Promise<Revision> => invoke('checkpoint_assistant_draft', { conversationId, request });
export const reconcileAssistantDraft = (conversationId: string, request: ReconcileRequest): Promise<ReconciledDocument> => invoke('reconcile_assistant_draft', { conversationId, request });
export const setChatDisposition = (access: ProjectAccess, conversationId: string, referenceId: string, expectedVersion: string, disposition: string, rationale = '', operationId = crypto.randomUUID(), options: ChatDispositionOptions = {}): Promise<ConversationItem> => invoke('set_chat_disposition', { request: { access, conversationId, referenceId, expectedVersion, disposition, rationale, operationId, ...options } });
export const prepareChatAdoption = (access: ProjectAccess, conversationId: string, drafts: ProjectChatDraftRef[], operationId = crypto.randomUUID()): Promise<ChatAdoptionPreview> => invoke('prepare_chat_adoption', { request: { access, conversationId, drafts, operationId } });
export const adoptChatPreview = (access: ProjectAccess, preview: ChatAdoptionPreview, operationId = crypto.randomUUID()): Promise<ChatAdoptionAck> => invoke('adopt_chat_preview', { request: { access, conversationId: preview.conversationId, previewId: preview.id, previewVersion: preview.version, previewDigest: preview.digest, operationId } });
