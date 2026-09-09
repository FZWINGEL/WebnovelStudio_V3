import type { ChatAdoptionPreview } from '../ipc/projectChat';
import type { ProjectAccess } from '../ipc/projects';

export interface ChatViewPreferences {
  rightMode?: 'document' | 'review';
  selectedDraftIds?: string[];
  editingDraftId?: string | null;
  transcriptAnchor?: string | null;
  preview?: ChatAdoptionPreview | null;
  applyOperationId?: string | null;
}

function storage(): Storage | null {
  try { return typeof localStorage === 'undefined' ? null : localStorage; } catch { return null; }
}

export function chatViewPreferenceKey(access: ProjectAccess, conversationId: string): string {
  return `webnovelstudio.chat-view.v1:${access.projectId}:${access.operationNamespace}:${conversationId}`;
}

function validPreview(value: unknown): ChatAdoptionPreview | null {
  if (!value || typeof value !== 'object') return null;
  const candidate = value as Partial<ChatAdoptionPreview>;
  const targets = Array.isArray(candidate.targets) ? candidate.targets : [];
  const validDocument = (document: unknown): boolean => {
    if (!document || typeof document !== 'object') return false;
    const body = (document as Record<string, unknown>).body;
    if (!body || typeof body !== 'object') return false;
    const inner = (body as Record<string, unknown>).body;
    return !!inner && typeof inner === 'object' && (inner as Record<string, unknown>).type === 'doc' && Array.isArray((inner as Record<string, unknown>).content);
  };
  const validTarget = (target: unknown): boolean => {
    if (!target || typeof target !== 'object') return false;
    const item = target as Record<string, unknown>;
    if (typeof item.documentId !== 'string' || typeof item.title !== 'string' || !validDocument(item.body)) return false;
    return item.before === null || validDocument(item.before);
  };
  return typeof candidate.id === 'string' && typeof candidate.version === 'string' && typeof candidate.digest === 'string'
    && typeof candidate.projectId === 'string' && typeof candidate.operationNamespace === 'string' && typeof candidate.conversationId === 'string'
    && targets.every(validTarget) ? value as ChatAdoptionPreview : null;
}

export function readChatViewPreferences(key: string): ChatViewPreferences {
  const store = storage();
  if (!store) return {};
  try {
    const raw = store.getItem(key);
    if (!raw) return {};
    const value = JSON.parse(raw) as Record<string, unknown>;
    return {
      rightMode: value.rightMode === 'document' || value.rightMode === 'review' ? value.rightMode : undefined,
      selectedDraftIds: Array.isArray(value.selectedDraftIds) ? value.selectedDraftIds.filter((id): id is string => typeof id === 'string') : undefined,
      editingDraftId: value.editingDraftId === null || typeof value.editingDraftId === 'string' ? value.editingDraftId : undefined,
      transcriptAnchor: value.transcriptAnchor === null || typeof value.transcriptAnchor === 'string' ? value.transcriptAnchor : undefined,
      preview: value.preview === null ? null : validPreview(value.preview) ?? undefined,
      applyOperationId: value.applyOperationId === null || typeof value.applyOperationId === 'string' ? value.applyOperationId : undefined,
    };
  } catch { return {}; }
}

export function writeChatViewPreferences(key: string, value: ChatViewPreferences): void {
  const store = storage();
  if (!store) return;
  try {
    const next = { ...readChatViewPreferences(key) } as ChatViewPreferences;
    for (const [field, fieldValue] of Object.entries(value) as Array<[keyof ChatViewPreferences, ChatViewPreferences[keyof ChatViewPreferences]]>) {
      if (fieldValue !== undefined) next[field] = fieldValue as never;
    }
    store.setItem(key, JSON.stringify(next));
  } catch { /* local preferences are optional */ }
}
