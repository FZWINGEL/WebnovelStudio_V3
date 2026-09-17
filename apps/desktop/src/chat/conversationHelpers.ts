/** Pure transcript-model helpers for the project conversation view. */
import type { DocumentRecord } from '../ipc/projects';
import type { AssistantDraft, ChatDispositionScope, ChatUnknownTo, ConversationItem } from '../ipc/projectChat';
import type { DiscussionRun } from '../ipc/discussions';
import { parseChapterHandoff, type ChapterHandoffProposal } from './ChapterHandoff';
import { conversationItems } from './conversationStore';

export interface AssistantOutput { schemaVersion: string; answer: string; questions: Array<{ key: string; text: string }>; assumptions: Array<{ key: string; text: string }>; chapterHandoff: ChapterHandoffProposal | null }
export function runPayload(payload: Record<string, unknown>): Record<string, unknown> | null {
  return payload.run && typeof payload.run === 'object' ? payload.run as Record<string, unknown> : null;
}
export function outputOf(run: Record<string, unknown>): AssistantOutput | null {
  if (typeof run.outputText !== 'string' || !run.outputText.trim()) return null;
  try {
    const parsed: unknown = JSON.parse(run.outputText);
    if (!parsed || typeof parsed !== 'object') return null;
    const value = parsed as Record<string, unknown>;
    if (value.schemaVersion !== 'project-assistant-output.v1' || typeof value.answer !== 'string' || !Array.isArray(value.questions) || !Array.isArray(value.assumptions)) return null;
    return {
      schemaVersion: value.schemaVersion,
      answer: value.answer,
      chapterHandoff: parseChapterHandoff(value.chapterHandoff),
      questions: value.questions.filter((entry): entry is { key: string; text: string } => !!entry && typeof entry === 'object' && typeof (entry as Record<string, unknown>).key === 'string' && typeof (entry as Record<string, unknown>).text === 'string'),
      assumptions: value.assumptions.filter((entry): entry is { key: string; text: string } => !!entry && typeof entry === 'object' && typeof (entry as Record<string, unknown>).key === 'string' && typeof (entry as Record<string, unknown>).text === 'string'),
    };
  } catch { return null; }
}
export function chapterOutputOf(run: Record<string, unknown>): string | null {
  if (typeof run.outputText !== 'string' || !run.outputText.trim()) return null;
  const raw = run.outputText.trim();
  if (!raw.startsWith('{') && !raw.startsWith('[')) return raw;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed && typeof parsed === 'object') {
      const value = parsed as Record<string, unknown>;
      if (typeof value.answer === 'string') return value.answer;
      if (typeof value.text === 'string') return value.text;
      if (typeof value.replacementText === 'string') return value.replacementText;
      if (Array.isArray(value.paragraphs)) {
        const paragraphs = value.paragraphs.filter((item): item is string => typeof item === 'string');
        if (paragraphs.length) return paragraphs.join('\n\n');
      }
      const candidate = value.candidate;
      if (candidate && typeof candidate === 'object' && typeof (candidate as Record<string, unknown>).replacementText === 'string') return (candidate as Record<string, string>).replacementText;
    }
  } catch { return null; }
  return null;
}
export function dispositionVersion(items: ReturnType<typeof conversationItems>, referenceId: string): string {
  return latestDisposition(items, referenceId)?.version ?? '0';
}

export interface SavedDisposition {
  version: string;
  disposition: string;
  scope?: ChatDispositionScope;
  unknownTo?: ChatUnknownTo;
  rationale?: string;
}

export function dispositionScope(value: unknown): ChatDispositionScope | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const scope = value as Record<string, unknown>;
  if (scope.kind !== 'project' && scope.kind !== 'task' && scope.kind !== 'chapter' && scope.kind !== 'document') return undefined;
  if (scope.kind !== 'project' && typeof scope.referenceId !== 'string') return undefined;
  return scope.kind === 'project' ? { kind: 'project' } : { kind: scope.kind, referenceId: scope.referenceId as string };
}

export function dispositionUnknownTo(value: unknown): ChatUnknownTo | undefined {
  return value === 'author' || value === 'reader' || value === 'both' ? value : undefined;
}

export function latestDisposition(items: ReturnType<typeof conversationItems>, referenceId: string): SavedDisposition | null {
  const latest = items.filter(item => item.kind === 'chatDisposition' && (item.referenceId === referenceId || item.payload.referenceId === referenceId)).at(-1);
  return latest ? dispositionOf(latest) : null;
}

export function dispositionOf(item: ConversationItem): SavedDisposition {
  return {
    version: typeof item.payload.version === 'string' ? item.payload.version : '0',
    disposition: typeof item.payload.disposition === 'string' ? item.payload.disposition : 'updated',
    scope: dispositionScope(item.payload.scope),
    unknownTo: dispositionUnknownTo(item.payload.unknownTo),
    rationale: typeof item.payload.rationale === 'string' ? item.payload.rationale : undefined,
  };
}

export function isLatestDisposition(items: ReturnType<typeof conversationItems>, item: ConversationItem, referenceId: string): boolean {
  const latest = items.filter(candidate => candidate.kind === 'chatDisposition' && (candidate.referenceId === referenceId || candidate.payload.referenceId === referenceId)).at(-1);
  return latest?.id === item.id;
}

export function dispositionScopeLabel(scope: ChatDispositionScope | undefined): string {
  if (!scope) return 'Not recorded';
  if (scope.kind === 'project') return 'Project';
  if (scope.kind === 'task') return `This request · ${scope.referenceId}`;
  if (scope.kind === 'chapter') return `Chapter · ${scope.referenceId}`;
  return `Document · ${scope.referenceId}`;
}

export function dispositionUnknownToLabel(value: ChatUnknownTo | undefined): string | null {
  if (value === 'author') return 'Author';
  if (value === 'reader') return 'Reader';
  if (value === 'both') return 'Author and reader';
  return null;
}

export function dispositionLabel(value: string, draft: AssistantDraft | null | undefined): string {
  if (draft) {
    if (value === 'reconsider') return 'Draft marked for a fresh review.';
    if (value === 'rejected') return 'Draft rejected.';
    return `Draft decision saved: ${value}.`;
  }
  if (value === 'notNow') return 'Response deferred for this scope.';
  if (value === 'notRelevant') return 'Response marked not relevant for this scope.';
  if (value === 'keepMysterious') return 'Response kept mysterious.';
  if (value === 'assumptionReject') return 'Assumption rejected for this request.';
  if (value === 'reconsider') return 'Response reopened for a fresh answer.';
  return `Response decision saved: ${value}.`;
}

export function requestItemForRun(items: ConversationItem[], run: DiscussionRun | null): ConversationItem | null {
  if (!run) return null;
  return items.find(item => (item.kind === 'request' || item.kind === 'chapterRequest') && (item.referenceId === run.id || runPayload(item.payload)?.id === run.id)) ?? null;
}

export function materializedDraftIds(item: ConversationItem): string[] {
  if (item.kind !== 'materializeChatResult' || !Array.isArray(item.payload.draftRefs)) return [];
  return item.payload.draftRefs.flatMap(value => {
    if (!value || typeof value !== 'object') return [];
    const documentId = (value as Record<string, unknown>).documentId;
    return typeof documentId === 'string' && documentId.trim() ? [documentId] : [];
  });
}

export function frozenRequestContext(item: ConversationItem | null, documents: DocumentRecord[]): { surface: 'authorRoom' | 'chapterWriting'; targetLabel: string; scopeLabel: string } | undefined {
  if (!item) return undefined;
  if (item.kind === 'chapterRequest') {
    const target = item.payload.target && typeof item.payload.target === 'object' ? item.payload.target as Record<string, unknown> : null;
    const documentId = typeof target?.documentId === 'string' ? target.documentId : null;
    const title = documentId ? documents.find(document => document.head.documentId === documentId)?.title : null;
    const scope = item.payload.scope && typeof item.payload.scope === 'object' ? item.payload.scope as Record<string, unknown> : null;
    const quote = typeof scope?.quote === 'string' && scope.quote.trim() ? scope.quote.trim() : '';
    return {
      surface: 'chapterWriting',
      targetLabel: `Chapter · ${title ?? documentId ?? 'captured chapter'}`,
      scopeLabel: quote ? `Captured selection · “${quote.length > 96 ? `${quote.slice(0, 93)}…` : quote}”` : 'Captured chapter scope',
    };
  }
  const sources = Array.isArray(item.payload.sourceRefs) ? item.payload.sourceRefs.length : 0;
  const drafts = Array.isArray(item.payload.taskDraftRefs) ? item.payload.taskDraftRefs.length : 0;
  const scopeLabel = [sources ? `${sources} exact source${sources === 1 ? '' : 's'}` : '', drafts ? `${drafts} task draft${drafts === 1 ? '' : 's'}` : ''].filter(Boolean).join(' · ') || 'No additional sources attached';
  return { surface: 'authorRoom', targetLabel: 'Project conversation', scopeLabel };
}

export function reasonMessage(reason: unknown, fallback: string): string {
  if (reason && typeof reason === 'object') {
    const value = reason as Record<string, unknown>;
    if (typeof value.detail === 'string' && value.detail.trim()) return value.detail;
    if (typeof value.message === 'string' && value.message.trim()) return value.message;
  }
  return reason instanceof Error && reason.message ? reason.message : fallback;
}

export function reasonCode(reason: unknown): string | null {
  if (!reason || typeof reason !== 'object') return null;
  const code = (reason as Record<string, unknown>).code;
  return typeof code === 'string' ? code : null;
}

export function isStaleAdoptionError(reason: unknown): boolean {
  return ['DraftChanged', 'PreviewMismatch', 'ContextChanged', 'VersionConflict', 'Stale', 'PreviewStale'].includes(reasonCode(reason) ?? '');
}

