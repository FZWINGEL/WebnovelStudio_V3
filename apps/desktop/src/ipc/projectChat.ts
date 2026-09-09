import { invoke } from '@tauri-apps/api/core';
import type { WnsDocument } from '../editor/document';
import type { CheckpointRequest, DocumentRecord, Head, ProjectAccess, ReconcileRequest, ReconciledDocument, Revision, SaveAck, SaveSnapshot } from './projects';
import type { DiscussionRun, DiscussionStart, DiscussionScope, FeedbackIntent, ContinuationBasis, SafeBriefInput } from './discussions';
import type { MockContextBudget } from './context';
import type { ModelSelection } from './providers';

export interface ProjectChatDraftRef { head: Head; dispositionVersion: string }
export interface ProjectBriefOrigin {
  version: 'project-conversation-brief.v1'; projectId: string; operationNamespace: string;
  conversationId: string; messageId: string; target: Head; scopeHash: string; textHash: string;
}
export interface ProjectChapterComposer {
  target: Head; intent: FeedbackIntent; basis?: ContinuationBasis | null;
  scope?: DiscussionScope | null; safeBrief?: SafeBriefInput | null;
}
export interface ProjectComposer { text: string; sourceRefs: Head[]; taskDraftRefs: ProjectChatDraftRef[]; focusedDocumentRef?: Head; chapter?: ProjectChapterComposer | null }
export interface ProjectComposerSnapshot { conversationId: string; version: string; body: ProjectComposer }
export interface ConversationItem { id: string; sequence: string; kind: string; referenceId: string | null; payload: Record<string, unknown>; createdAt: string }
export interface AssistantDraft {
  document: DocumentRecord; conversationId: string; originRunId: string; packetId: string;
  initialRevisionId: string; predecessorDocumentId?: string | null; target: Head | null; disposition: 'pending' | 'rejected' | 'adopted' | 'superseded'; dispositionVersion: string; stale: boolean;
}
export interface ProjectConversationView {
  id: string; composer: ProjectComposerSnapshot; items: ConversationItem[]; olderBefore: string | null;
  activeRun: DiscussionRun | null; drafts: AssistantDraft[]; sourceEpoch: string; policyEpoch: string; earlierWorkshop: boolean;
  workerIssues: { runId: string; detail: string }[];
}
export interface SaveProjectComposer { access: ProjectAccess; operationId: string; conversationId: string; expectedVersion: string; body: ProjectComposer }
export interface StartProjectChat { access: ProjectAccess; operationId: string; conversationId: string; expectedComposerVersion: string; composer: ProjectComposer; budget: MockContextBudget }
export interface StartProjectChapter extends StartProjectChat {}
export interface ChapterRangeProposal {
  sourceHead: Head;
  firstBlockId: string;
  lastBlockId: string;
  quote: string;
}
export interface ChapterDiscussionFeedback {
  runId: string;
  target: Head;
  answer: string;
  rangeProposal?: ChapterRangeProposal | null;
  rangeError?: string | null;
}
export interface ChatAdoptionTarget { draft: ProjectChatDraftRef; draftRevisionId: string; documentId: string; title: string; kind: string; before: DocumentRecord | null; body: WnsDocument }
export interface ChatAdoptionPreview {
  id: string; version: string; digest: string; projectId: string; operationNamespace: string; conversationId: string;
  sourceEpoch: string; policyEpoch: string; workshopVersion: string; targets: ChatAdoptionTarget[];
}
export interface ChatAdoptionAck { previewId: string; documents: DocumentRecord[]; decisionId: string }
export type ChatDispositionScopeKind = 'project' | 'task' | 'chapter' | 'document';
export interface ChatDispositionScope { kind: ChatDispositionScopeKind; referenceId?: string }
export type ChatUnknownTo = 'author' | 'reader' | 'both';
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
