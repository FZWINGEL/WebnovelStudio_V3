import { invoke } from '@tauri-apps/api/core';
import type { ConversationItem } from './projectChat';
import type { SourceDescriptor } from './context';
import type { DiscussionMessage, DiscussionRun } from './discussions';
import type { ProjectAccess, Revision } from './projects';

/** An immutable conversation identity retained by a recovered or duplicated project. */
export interface HistoricalConversationRef {
  projectId: string;
  operationNamespace: string;
  conversationId: string;
}

export interface ReadProjectChatHistory {
  access: ProjectAccess;
  conversation: HistoricalConversationRef;
  before?: string | null;
  limit?: number;
}

export interface HistoricalSourceRevision {
  handle: string;
  kind: SourceDescriptor['kind'];
  descriptor: SourceDescriptor;
  revision: Revision;
}

export interface HistoricalDraftRevision {
  documentId: string;
  initial: boolean;
  revision: Revision;
}

export interface HistoricalConversationItem {
  item: ConversationItem;
  run?: DiscussionRun;
  messages: DiscussionMessage[];
  sourceRevisions: HistoricalSourceRevision[];
  draftRevisions: HistoricalDraftRevision[];
}

export interface HistoricalConversation {
  conversation: HistoricalConversationRef;
  anchorDocumentId: string;
  items: HistoricalConversationItem[];
  olderBefore: string | null;
}

export interface HistoricalConversationSummary {
  conversation: HistoricalConversationRef;
  anchorDocumentId: string;
  itemCount: number;
  current: boolean;
}

export const listProjectChatHistory = (
  access: ProjectAccess,
): Promise<HistoricalConversationSummary[]> => invoke('list_project_chat_history', { access });

export const readProjectChatHistory = (
  request: ReadProjectChatHistory,
): Promise<HistoricalConversation> => invoke('read_project_chat_history', { request });

export const readHistoricalProjectChat = (
  access: ProjectAccess,
  conversation: HistoricalConversationRef,
  before: string | null = null,
  limit = 40,
): Promise<HistoricalConversation> => readProjectChatHistory({ access, conversation, before, limit });
