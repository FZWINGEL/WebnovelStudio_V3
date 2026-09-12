import type {
  HistoricalConversation,
  HistoricalConversationItem,
  HistoricalConversationRef,
  HistoricalConversationSummary,
  HistoricalDraftRevision,
  HistoricalSourceRevision,
  ReadProjectChatHistory,
} from './generated/conversation';
export type {
  HistoricalConversation,
  HistoricalConversationItem,
  HistoricalConversationRef,
  HistoricalConversationSummary,
  HistoricalDraftRevision,
  HistoricalSourceRevision,
  ReadProjectChatHistory,
};

import { invoke } from '@tauri-apps/api/core';
import type { ConversationItem } from './projectChat';
import type { SourceDescriptor } from './context';
import type { DiscussionMessage, DiscussionRun } from './discussions';
import type { ProjectAccess, Revision } from './projects';

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
