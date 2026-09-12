import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState, type ComponentProps, type ReactNode, type UIEvent } from 'react';
import { useSyncExternalStore } from 'react';
import { useProviders } from '../providers/ProviderContext';
import type { MockContextBudget } from '../ipc/context';
import type { DocumentRecord, Head, OpenedProject, ProjectAccess } from '../ipc/projects';
import type { AssistantDraft, ChatDispositionOptions, ChatDispositionScope, ChatUnknownTo, ChatAdoptionTarget, ConversationItem, ProjectChapterComposer } from '../ipc/projectChat';
import { readChatAdoptionPreview } from '../ipc/projectChat';
import type { DiscussionRun, SafeBriefInput } from '../ipc/discussions';
import { SafeBriefEditor } from '../assistant/SafeBriefEditor';
import { ContextInspector } from '../assistant/ContextInspector';
import { approveChapterBrief } from './brief';
import { conversationItems, draftRefs, ProjectConversationStore } from './conversationStore';
import { DraftReviewPanel, type DraftReviewPanelHandle } from './DraftReviewPanel';
import { ProjectDocumentsPanel } from './ProjectDocumentsPanel';
import { DocumentSaveRecap } from './DocumentSaveRecap';
import { Writer } from '../editor/Writer';
import { RequestStatus } from './RequestStatus';
import { useStoryFreshness } from './useStoryFreshness';
import { ChapterHandoff, parseChapterHandoff, type ChapterHandoffProposal } from './ChapterHandoff';
import { useDraftReviewContext } from './useDraftReviewContext';
import { readStalePreviewComparison } from './stalePreviewComparison';
import type { StalePreviewComparison } from './DraftReviewPanel';
import { chatViewPreferenceKey, readChatViewPreferences, writeChatViewPreferences } from './viewPreferences';
import './chat.css';
import './CoauthorConversation.css';
import { ChatSplitPane } from './ChatSplitPane';
import { NewReplyAffordance, useTranscriptScroll } from './TranscriptScroll';

export interface ProjectConversationHandle { flush(): Promise<void>; refresh?(): Promise<void>; stageChapter(chapter: ProjectChapterComposer): Promise<void>; attachSource(head: Head): Promise<void> }
export interface ProjectConversationProps {
  project: OpenedProject;
  activeDocument?: DocumentRecord | null;
  onOpenDocument(document: DocumentRecord): Promise<void> | void;
  onPrepareSource?(document: DocumentRecord): Promise<Head>;
  onPrepareChapter?(targetId: string | null, title: string): Promise<DocumentRecord>;
  onDocumentsChanged(documents: DocumentRecord[]): Promise<void> | void;
  onEarlierWorkshop(): void;
  onBeforeAdoption?(targets: ChatAdoptionTarget[]): Promise<void>;
  onAdoptionFailure?(reason: unknown): Promise<void>;
  onCreateChapter?(): Promise<void> | void;
  onCreateNote?(): Promise<void> | void;
  onOpenChapterResult?(run: DiscussionRun): Promise<void> | void;
  onAccessChanged?(access: ProjectAccess): Promise<void> | void;
  /// The document surface's inputs. The chat renders `<Writer>` itself rather
  /// than receiving a built node, so the shell no longer assembles a feature's
  /// JSX and the conversation wiring below is the chat's own, not a round trip
  /// through the shell's ref.
  editor?: Omit<ComponentProps<typeof Writer>, 'conversation'>;
  /// The chapter run whose review the editor should surface, if any. Set by the
  /// shell, which owns the navigation that produces it.
  reviewRunId?: string | null;
  budget?: MockContextBudget;
}

const defaultBudget: MockContextBudget = { modelId: 'mock-story-context', contextWindowTokens: '32768', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' };
interface AssistantOutput { schemaVersion: string; answer: string; questions: Array<{ key: string; text: string }>; assumptions: Array<{ key: string; text: string }>; chapterHandoff: ChapterHandoffProposal | null }
function runPayload(payload: Record<string, unknown>): Record<string, unknown> | null {
  return payload.run && typeof payload.run === 'object' ? payload.run as Record<string, unknown> : null;
}
function outputOf(run: Record<string, unknown>): AssistantOutput | null {
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
function chapterOutputOf(run: Record<string, unknown>): string | null {
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
function dispositionVersion(items: ReturnType<typeof conversationItems>, referenceId: string): string {
  return latestDisposition(items, referenceId)?.version ?? '0';
}

interface SavedDisposition {
  version: string;
  disposition: string;
  scope?: ChatDispositionScope;
  unknownTo?: ChatUnknownTo;
  rationale?: string;
}

function dispositionScope(value: unknown): ChatDispositionScope | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const scope = value as Record<string, unknown>;
  if (scope.kind !== 'project' && scope.kind !== 'task' && scope.kind !== 'chapter' && scope.kind !== 'document') return undefined;
  if (scope.kind !== 'project' && typeof scope.referenceId !== 'string') return undefined;
  return scope.kind === 'project' ? { kind: 'project' } : { kind: scope.kind, referenceId: scope.referenceId as string };
}

function dispositionUnknownTo(value: unknown): ChatUnknownTo | undefined {
  return value === 'author' || value === 'reader' || value === 'both' ? value : undefined;
}

function latestDisposition(items: ReturnType<typeof conversationItems>, referenceId: string): SavedDisposition | null {
  const latest = items.filter(item => item.kind === 'chatDisposition' && (item.referenceId === referenceId || item.payload.referenceId === referenceId)).at(-1);
  return latest ? dispositionOf(latest) : null;
}

function dispositionOf(item: ConversationItem): SavedDisposition {
  return {
    version: typeof item.payload.version === 'string' ? item.payload.version : '0',
    disposition: typeof item.payload.disposition === 'string' ? item.payload.disposition : 'updated',
    scope: dispositionScope(item.payload.scope),
    unknownTo: dispositionUnknownTo(item.payload.unknownTo),
    rationale: typeof item.payload.rationale === 'string' ? item.payload.rationale : undefined,
  };
}

function isLatestDisposition(items: ReturnType<typeof conversationItems>, item: ConversationItem, referenceId: string): boolean {
  const latest = items.filter(candidate => candidate.kind === 'chatDisposition' && (candidate.referenceId === referenceId || candidate.payload.referenceId === referenceId)).at(-1);
  return latest?.id === item.id;
}

function dispositionScopeLabel(scope: ChatDispositionScope | undefined): string {
  if (!scope) return 'Not recorded';
  if (scope.kind === 'project') return 'Project';
  if (scope.kind === 'task') return `This request · ${scope.referenceId}`;
  if (scope.kind === 'chapter') return `Chapter · ${scope.referenceId}`;
  return `Document · ${scope.referenceId}`;
}

function dispositionUnknownToLabel(value: ChatUnknownTo | undefined): string | null {
  if (value === 'author') return 'Author';
  if (value === 'reader') return 'Reader';
  if (value === 'both') return 'Author and reader';
  return null;
}

function dispositionLabel(value: string, draft: AssistantDraft | null | undefined): string {
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

function requestItemForRun(items: ConversationItem[], run: DiscussionRun | null): ConversationItem | null {
  if (!run) return null;
  return items.find(item => (item.kind === 'request' || item.kind === 'chapterRequest') && (item.referenceId === run.id || runPayload(item.payload)?.id === run.id)) ?? null;
}

function materializedDraftIds(item: ConversationItem): string[] {
  if (item.kind !== 'materializeChatResult' || !Array.isArray(item.payload.draftRefs)) return [];
  return item.payload.draftRefs.flatMap(value => {
    if (!value || typeof value !== 'object') return [];
    const documentId = (value as Record<string, unknown>).documentId;
    return typeof documentId === 'string' && documentId.trim() ? [documentId] : [];
  });
}

function frozenRequestContext(item: ConversationItem | null, documents: DocumentRecord[]): { surface: 'authorRoom' | 'chapterWriting'; targetLabel: string; scopeLabel: string } | undefined {
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

function reasonMessage(reason: unknown, fallback: string): string {
  if (reason && typeof reason === 'object') {
    const value = reason as Record<string, unknown>;
    if (typeof value.detail === 'string' && value.detail.trim()) return value.detail;
    if (typeof value.message === 'string' && value.message.trim()) return value.message;
  }
  return reason instanceof Error && reason.message ? reason.message : fallback;
}

function reasonCode(reason: unknown): string | null {
  if (!reason || typeof reason !== 'object') return null;
  const code = (reason as Record<string, unknown>).code;
  return typeof code === 'string' ? code : null;
}

function isStaleAdoptionError(reason: unknown): boolean {
  return ['DraftChanged', 'PreviewMismatch', 'ContextChanged', 'VersionConflict', 'Stale', 'PreviewStale'].includes(reasonCode(reason) ?? '');
}

function ResponseDispositionControls({ referenceId, expectedVersion, runId, isAssumption, assumptionText, savedDisposition, documents, activeDocument, onDisposition, onStageAssumptionCorrection }: { referenceId: string; expectedVersion: string; runId: string; isAssumption?: boolean; assumptionText?: string; savedDisposition?: SavedDisposition | null; documents: DocumentRecord[]; activeDocument: DocumentRecord | null; onDisposition: (referenceId: string, version: string, value: string, options?: ChatDispositionOptions, rationale?: string) => void; onStageAssumptionCorrection?: (originalText: string, revisedText: string) => boolean | void }) {
  const chapter = activeDocument?.kind === 'chapter' ? activeDocument : documents.find(document => document.kind === 'chapter' && (document.role ?? 'ordinary') === 'ordinary');
  const document = activeDocument ?? documents.find(item => (item.role ?? 'ordinary') === 'ordinary');
  const availableScopeKinds = new Set<ChatDispositionScope['kind']>(['project', 'task', ...(chapter ? ['chapter' as const] : []), ...(document ? ['document' as const] : [])]);
  const savedScopeKind = savedDisposition?.scope?.kind;
  const initialScopeKind = savedScopeKind && availableScopeKinds.has(savedScopeKind) ? savedScopeKind : 'project';
  const [scopeKind, setScopeKind] = useState<ChatDispositionScope['kind']>(initialScopeKind);
  const [scopeEdited, setScopeEdited] = useState(false);
  const [unknownTo, setUnknownTo] = useState<ChatUnknownTo>(savedDisposition?.unknownTo ?? 'reader');
  const [editingAssumption, setEditingAssumption] = useState(false);
  const [editedAssumption, setEditedAssumption] = useState(assumptionText ?? '');
  useEffect(() => {
    const nextKind = savedDisposition?.scope?.kind;
    setScopeKind(nextKind && availableScopeKinds.has(nextKind) ? nextKind : 'project');
    setScopeEdited(false);
    setUnknownTo(savedDisposition?.unknownTo ?? 'reader');
  }, [savedDisposition?.scope?.kind, savedDisposition?.scope?.referenceId, savedDisposition?.unknownTo, chapter?.head.documentId, document?.head.documentId]);
  useEffect(() => {
    if (!editingAssumption) setEditedAssumption(assumptionText ?? '');
  }, [assumptionText, editingAssumption]);
  const scope: ChatDispositionScope = !scopeEdited && savedDisposition?.scope && savedDisposition.scope.kind === scopeKind
    ? savedDisposition.scope
    : scopeKind === 'project' ? { kind: 'project' } : scopeKind === 'task' ? { kind: 'task', referenceId: runId } : scopeKind === 'chapter' && chapter ? { kind: 'chapter', referenceId: chapter.head.documentId } : document ? { kind: 'document', referenceId: document.head.documentId } : { kind: 'project' };
  const submit = (value: string) => onDisposition(referenceId, expectedVersion, value, { scope, unknownTo: value === 'keepMysterious' && !isAssumption ? unknownTo : undefined });
  const beginAssumptionEdit = () => { setEditedAssumption(assumptionText ?? ''); setEditingAssumption(true); };
  const cancelAssumptionEdit = () => { setEditedAssumption(assumptionText ?? ''); setEditingAssumption(false); };
  const stageAssumptionCorrection = () => {
    if (!assumptionText?.trim() || !editedAssumption.trim() || !onStageAssumptionCorrection) return;
    const staged = onStageAssumptionCorrection(assumptionText, editedAssumption);
    if (staged !== false) setEditingAssumption(false);
  };
  return <div className="chat-question-actions">
    <label className="chat-disposition-scope">Scope<select value={scopeKind} disabled={!!isAssumption} onChange={event => { setScopeKind(event.target.value as ChatDispositionScope['kind']); setScopeEdited(true); }}><option value="project">Project</option><option value="task">This request</option>{chapter && <option value="chapter">Chapter · {chapter.title}</option>}{document && <option value="document">Open document · {document.title}</option>}</select></label>
    {!isAssumption && <label className="chat-disposition-scope">Keep unknown to<select value={unknownTo} onChange={event => setUnknownTo(event.target.value as ChatUnknownTo)}><option value="author">Author</option><option value="reader">Reader</option><option value="both">Author and reader</option></select></label>}
    {isAssumption && onStageAssumptionCorrection && <button type="button" onClick={beginAssumptionEdit}>Edit assumption</button>}
    {isAssumption && editingAssumption && <div className="chat-assumption-correction"><label>Correction for the next draft<textarea aria-label="Correct proposed assumption" value={editedAssumption} onChange={event => setEditedAssumption(event.target.value)} /></label><button type="button" disabled={!editedAssumption.trim()} onClick={stageAssumptionCorrection}>Use correction for next draft</button><button type="button" onClick={cancelAssumptionEdit}>Cancel</button></div>}
    {isAssumption ? <><button type="button" onClick={() => submit('assumptionReject')}>Reject assumption</button><button type="button" onClick={() => submit('reconsider')}>Reconsider</button></> : <><button type="button" onClick={() => submit('notNow')}>Not now</button><button type="button" onClick={() => submit('notRelevant')}>Not relevant</button><button type="button" onClick={() => submit('keepMysterious')}>Keep mysterious</button><button type="button" onClick={() => submit('reconsider')}>Reconsider</button></>}
  </div>;
}

function Transcript({ store, documents, activeDocument, onDisposition, onStageAssumptionCorrection, onOpenDocument, onOpenDraft, onOpenChapterResult, onAdaptBrief, onPrepareHandoff, onBrowseDocuments, onCreateChapter, onCreateNote, anchor, onAnchorChange }: { store: ProjectConversationStore; documents: DocumentRecord[]; activeDocument: DocumentRecord | null; onDisposition: (referenceId: string, version: string, value: string, options?: ChatDispositionOptions, rationale?: string) => void; onStageAssumptionCorrection?: (originalText: string, revisedText: string) => boolean | void; onOpenDocument?: (document: DocumentRecord) => Promise<void> | void; onOpenDraft?: (draft: AssistantDraft) => Promise<void> | void; onOpenChapterResult?: (run: DiscussionRun) => Promise<void> | void; onAdaptBrief?: (messageId: string, text: string) => void; onPrepareHandoff?: (proposal: ChapterHandoffProposal, messageId: string, targetId: string | null, title: string) => Promise<void>; onBrowseDocuments?: () => void; onCreateChapter?: () => Promise<void> | void; onCreateNote?: () => Promise<void> | void; anchor?: string | null; onAnchorChange?: (itemId: string) => void }) {
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const items = conversationItems(state.view);
  const hasItems = items.length > 0 || !!state.activeRun;
  const transcriptRef = useRef<HTMLDivElement>(null);
  const replyEntries = useMemo(() => items.flatMap(item => {
    if (item.kind === 'request' || item.kind === 'chapterRequest') {
      const run = runPayload(item.payload);
      if (run && ['completed', 'failed', 'interrupted'].includes(String(run.status))) {
        const replyId = `${item.id}:assistant`;
        return [{ id: replyId, key: `${replyId}:${String(run.status)}:${String(run.updatedAt ?? run.id ?? '')}` }];
      }
    }
    return [];
  }), [items]);
  const { unseenReplyCount, jumpToLatest } = useTranscriptScroll({ containerRef: transcriptRef, itemCount: items.length, replyKeys: replyEntries.map(entry => entry.key), latestReplyId: replyEntries.at(-1)?.id ?? null, olderLoading: state.olderLoading });
  useEffect(() => {
    if (!anchor) return;
    const timer = setTimeout(() => {
      const element = [...(transcriptRef.current?.querySelectorAll<HTMLElement>('[data-conversation-item-id]') ?? [])].find(item => item.dataset.conversationItemId === anchor);
      element?.scrollIntoView?.({ block: 'start' });
    }, 0);
    return () => clearTimeout(timer);
  }, [anchor]);
  const rememberAnchor = (event: UIEvent<HTMLDivElement>) => {
    if (!onAnchorChange) return;
    const top = event.currentTarget.getBoundingClientRect().top;
    const visible = [...event.currentTarget.querySelectorAll<HTMLElement>('[data-conversation-item-id]')].find(item => item.getBoundingClientRect().bottom >= top + 6);
    if (visible?.dataset.conversationItemId) onAnchorChange(visible.dataset.conversationItemId);
  };
  return <div className="chat-transcript-shell">
    <div ref={transcriptRef} className="chat-transcript" aria-label="Project conversation" onScroll={rememberAnchor}>
      <DocumentSaveRecap access={store.access} saves={state.view?.documentSaves ?? []} documents={documents} onOpenDocument={onOpenDocument} />
      {(state.view?.olderBefore || state.olderLoading || state.olderError) && <div className="chat-older-timeline"><button type="button" disabled={state.olderLoading || !state.view?.olderBefore} onClick={() => void store.loadOlder()}>{state.olderLoading ? 'Loading earlier messages…' : 'Load earlier messages'}</button>{state.olderError && <span role="alert">{state.olderError}</span>}</div>}
    {!hasItems && <div className="chat-empty chat-welcome"><h1>What are you writing?</h1><p>Start with an idea, a character, a scene, or a question. I’ll help shape it into material you can review.</p><p className="chat-muted">You can answer one question, keep something mysterious, or ask me to draft a first version without completing a form.</p><div className="chat-empty-actions">{onCreateNote && <button type="button" onClick={() => void onCreateNote()}>Bring a note</button>}{onBrowseDocuments && <button type="button" onClick={onBrowseDocuments}>Browse documents</button>}{onCreateChapter && <button type="button" onClick={() => void onCreateChapter()}>Blank chapter</button>}</div></div>}
    {items.map(item => {
      const run = runPayload(item.payload);
      const output = run ? outputOf(run) : null;
      if ((item.kind === 'request' || item.kind === 'chapterRequest') && run) {
        const instruction = typeof item.payload.instruction === 'string' ? item.payload.instruction : 'Request sent.';
        const runId = typeof run.id === 'string' ? run.id : item.referenceId ?? item.id;
        const chapterText = item.kind === 'chapterRequest' ? chapterOutputOf(run) : null;
        const typedRun = run as unknown as DiscussionRun;
        const userMessageId = typeof item.payload.userMessageId === 'string' ? item.payload.userMessageId : null;
        const assistantMessageId = typeof item.payload.assistantMessageId === 'string' ? item.payload.assistantMessageId : null;
        const canAdapt = item.kind === 'request' && run.status === 'completed' && !!onAdaptBrief;
        const answerText = output?.answer ?? chapterText;
        return <div key={item.id} data-conversation-item-id={item.id} className="chat-turn">
          <article className="chat-message chat-message-user"><div className="chat-message-meta">You · {item.kind === 'chapterRequest' ? 'Chapter task' : 'Project conversation'} <span>{item.createdAt ? new Date(item.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : ''}</span></div><p>{instruction}</p>{canAdapt && userMessageId && <button type="button" className="chat-adapt-brief" onClick={() => onAdaptBrief?.(userMessageId, instruction)}>Adapt as writing brief</button>}</article>
          <article className="chat-message chat-message-assistant" data-assistant-reply-id={`${item.id}:assistant`}><div className="chat-message-meta">Assistant <span>{typeof run.status === 'string' ? run.status : ''}</span></div>
            {output ? <><p>{output.answer}</p>{output.assumptions.length > 0 && <div className="chat-assumptions"><strong>Proposed assumptions · this request</strong>{output.assumptions.map(assumption => { const reference = `${runId}:${assumption.key}`; return <div className="chat-assumption" key={assumption.key}><p>{assumption.text}</p><ResponseDispositionControls referenceId={reference} expectedVersion={dispositionVersion(items, reference)} runId={runId} isAssumption assumptionText={assumption.text} savedDisposition={latestDisposition(items, reference)} documents={documents} activeDocument={activeDocument} onDisposition={onDisposition} onStageAssumptionCorrection={onStageAssumptionCorrection} /></div>; })}</div>}{output.questions.map(question => { const reference = `${runId}:${question.key}`; return <div className="chat-question" key={question.key}><p><strong>Question</strong> {question.text}</p><ResponseDispositionControls referenceId={reference} expectedVersion={dispositionVersion(items, reference)} runId={runId} savedDisposition={latestDisposition(items, reference)} documents={documents} activeDocument={activeDocument} onDisposition={onDisposition} /></div>; })}</> : chapterText ? <p className="chat-prose">{chapterText}</p> : run.status === 'queued' || run.status === 'running' || run.status === 'stopping' ? <p className="chat-muted">The assistant is preparing a response…</p> : <p className="chat-warning" role="alert">The response was saved but did not match the project response format. No draft was created.</p>}
            {item.kind === 'chapterRequest' && onOpenChapterResult && run.status === 'completed' && <button type="button" className="chat-open-chapter-result" onClick={() => void onOpenChapterResult(typedRun)}>Open chapter result</button>}
            {canAdapt && assistantMessageId && answerText && <button type="button" className="chat-adapt-brief" onClick={() => onAdaptBrief?.(assistantMessageId, answerText)}>Adapt answer as writing brief</button>}
            {item.kind === 'request' && typedRun.status === 'completed' && output?.chapterHandoff && assistantMessageId && onPrepareHandoff && <ChapterHandoff proposal={output.chapterHandoff} documents={documents} onPrepare={(targetId, title) => onPrepareHandoff(output.chapterHandoff!, assistantMessageId, targetId, title)} />}
            {typedRun.packetId && <details className="chat-context-inspection"><summary>Inspect supplied context</summary><ContextInspector access={store.access} packetId={typedRun.packetId} delivered={typedRun.dispatchState === 'delivered'} appServerDelivery={typedRun.providerResult?.appServer ?? undefined} refreshKey={`${typedRun.id}:${typedRun.updatedAt}`} showVersionLinks /></details>}
          </article>
        </div>;
      }
      if (item.kind === 'materializeChatResult') {
        const valid = item.payload.outputValid === true;
        const drafts = Array.isArray(item.payload.draftRefs) ? item.payload.draftRefs.length : 0;
        const reviewDrafts = materializedDraftIds(item).flatMap(documentId => {
          const draft = state.view?.drafts.find(candidate => candidate.document.head.documentId === documentId);
          return draft ? [draft] : [];
        });
        return <article className="chat-message chat-message-assistant" data-conversation-item-id={item.id} key={item.id}>
          <div className="chat-message-meta">Assistant result</div>
          <p>{valid ? drafts ? `${drafts} isolated draft${drafts === 1 ? '' : 's'} saved for review.` : 'Answer saved. No draft was requested.' : 'The response was retained, but no reviewable draft was created.'}</p>
          {reviewDrafts.length > 0 && <div className="chat-inline-drafts" aria-label="Drafts ready to review">
            <span className="chat-inline-drafts-label">Drafts ready to review</span>
            {reviewDrafts.map(draft => <button type="button" className="chat-inline-draft" key={draft.document.head.documentId} onClick={() => void onOpenDraft?.(draft)}>
              <span>{draft.document.title}</span>
              <small>{draft.document.kind} · Draft v{draft.document.head.version} · {draft.disposition === 'pending' ? 'Not adopted' : draft.disposition}</small>
            </button>)}
          </div>}
        </article>;
      }
      if (item.kind === 'chatDisposition') {
        const value = typeof item.payload.disposition === 'string' ? item.payload.disposition : 'updated';
        const reference = item.referenceId ?? (typeof item.payload.referenceId === 'string' ? item.payload.referenceId : '');
        const draft = reference && !reference.includes(':') ? state.view?.drafts.find(candidate => candidate.document.head.documentId === reference) : null;
        const saved = dispositionOf(item);
        const unknownTo = dispositionUnknownToLabel(saved?.unknownTo);
        const canReconsider = !!reference && reference.includes(':') && value !== 'reconsider' && isLatestDisposition(items, item, reference) && !!onDisposition;
        return <article className="chat-message chat-message-event" data-conversation-item-id={item.id} key={item.id}><p className="chat-muted">{dispositionLabel(value, draft)}</p><div className="chat-disposition-details"><span>Response version {saved?.version ?? '0'}</span><span>Scope: {dispositionScopeLabel(saved?.scope)}</span>{unknownTo && <span>Unknown to: {unknownTo}</span>}{saved?.rationale && <span>Rationale: {saved.rationale}</span>}</div>{draft && onOpenDraft && <button type="button" onClick={() => void onOpenDraft(draft)}>Open affected draft</button>}{canReconsider && <button type="button" onClick={() => onDisposition(reference, saved?.version ?? '0', 'reconsider', saved?.scope ? { scope: saved.scope } : undefined)}>Reconsider this response</button>}</article>;
      }
      if (item.kind === 'adoptionDecision') {
        const ids = Array.isArray(item.payload.documentIds) ? item.payload.documentIds.filter((id): id is string => typeof id === 'string') : [];
        return <article className="chat-message chat-message-event chat-adoption-decision" data-conversation-item-id={item.id} key={item.id}><p><strong>Drafts adopted</strong> · {ids.length || 'The reviewed'} document{ids.length === 1 ? '' : 's'} were applied to the working story.</p>{onOpenDocument && ids.map(id => { const document = documents.find(candidate => candidate.head.documentId === id); return document ? <button type="button" key={id} onClick={() => void onOpenDocument(document)}>Open {document.title}</button> : null; })}</article>;
      }
      return null;
    })}
    </div>
    <NewReplyAffordance count={unseenReplyCount} onJump={jumpToLatest} />
  </div>;
}

export const ProjectConversation = forwardRef<ProjectConversationHandle, ProjectConversationProps>(function ProjectConversation({ project, activeDocument = null, onOpenDocument, onPrepareSource, onPrepareChapter, onDocumentsChanged, onEarlierWorkshop, onBeforeAdoption, onAdoptionFailure, onCreateChapter, onCreateNote, onOpenChapterResult, onAccessChanged, editor, reviewRunId = null, budget = defaultBudget }, ref) {
  const provider = useProviders();
  const selection = provider.state?.settings.active ?? null;
  const modelReady = !provider.busy && !!provider.state && provider.state.dispatch.kind !== 'blocked' && !!selection;
  const storeRef = useRef<ProjectConversationStore | null>(null);
  const projectRef = useRef(`${project.project.projectId}:${project.access.operationNamespace}:${project.access.session}`);
  const accessKey = `${project.project.projectId}:${project.access.operationNamespace}:${project.access.session}`;
  if (!storeRef.current || projectRef.current !== accessKey) {
    storeRef.current?.dispose();
    storeRef.current = new ProjectConversationStore(project);
    projectRef.current = accessKey;
  }
  const store = storeRef.current;
  const draftReviewRef = useRef<DraftReviewPanelHandle>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const stageChapter = useCallback(async (chapter: ProjectChapterComposer) => {
    await store.stageChapter(chapter);
    if (!mounted.current || storeRef.current !== store) return;
    setMobileSurface('chat');
    setTimeout(() => { if (mounted.current && storeRef.current === store) composerRef.current?.focus(); }, 0);
  }, [store]);
  const attachSource = useCallback(async (head: Head) => {
    await store.attachSource(head);
    composerRef.current?.focus();
  }, [store]);

  useImperativeHandle(ref, () => ({
    refresh: async () => { await store.refresh(); },
    flush: async () => { await draftReviewRef.current?.flush(); await store.flush(); },
    stageChapter,
    attachSource,
  }), [store, stageChapter, attachSource]);
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const storyFreshness = useStoryFreshness({
    access: project.access,
    run: state.activeRun,
    currentSourceEpoch: state.view?.sourceEpoch ?? null,
    currentPolicyEpoch: state.view?.policyEpoch ?? null,
  });
  const [mobileSurface, setMobileSurface] = useState<'chat' | 'documents' | 'chapter' | 'review'>('chat');
  const [error, setError] = useState('');
  const [adoption, setAdoption] = useState('');
  const [preview, setPreview] = useState<import('../ipc/projectChat').ChatAdoptionPreview | null>(null);
  const [previewVerified, setPreviewVerified] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const applyOperationId = useRef<ReturnType<typeof crypto.randomUUID> | null>(null);
  const [rightMode, setRightMode] = useState<'document' | 'review'>('document');
  const [composerDetailsOpen, setComposerDetailsOpen] = useState(false);
  const [briefOpen, setBriefOpen] = useState(false);
  const [briefApprovalBusy, setBriefApprovalBusy] = useState(false);
  const [briefFocus, setBriefFocus] = useState(0);
  const [transcriptAnchor, setTranscriptAnchor] = useState<string | null>(null);
  const preferencesLoaded = useRef<string | null>(null);
  const skipPreferenceWrite = useRef(false);
  const mounted = useRef(true);
  const briefApprovalSequence = useRef(0);
  const isCurrentConversation = () => mounted.current && storeRef.current === store;
  useEffect(() => { mounted.current = true; void store.load().catch(reason => mounted.current && setError(reasonMessage(reason, 'Could not open the project conversation.'))); return () => { mounted.current = false; store.dispose(); }; }, [store]);
  useEffect(() => {
    try { store.updateAccess(project.access); }
    catch (reason) { if (mounted.current) setError(reasonMessage(reason, 'The project conversation lease changed unexpectedly.')); }
  }, [project.access.operationNamespace, project.access.projectId, project.access.session, project.access.writerLease, store]);
  useEffect(() => {
    if (!['queued', 'running', 'stopping', 'uncertain'].includes(state.status)) return;
    const timer = setInterval(() => { void store.refresh().catch(() => {}); }, 900);
    return () => clearInterval(timer);
  }, [state.status, store]);
  const preferenceKey = state.view ? chatViewPreferenceKey(store.access, state.view.id) : null;
  useEffect(() => {
    if (!preferenceKey || preferencesLoaded.current === preferenceKey) return;
    const preferences = readChatViewPreferences(preferenceKey);
    skipPreferenceWrite.current = true;
    setRightMode(preferences.rightMode ?? 'document');
    setMobileSurface(preferences.rightMode === 'review' ? 'review' : 'chat');
    setTranscriptAnchor(preferences.transcriptAnchor ?? null);
    setPreview(preferences.preview ?? null);
    setPreviewVerified(!preferences.preview);
    applyOperationId.current = preferences.applyOperationId && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(preferences.applyOperationId)
      ? preferences.applyOperationId as ReturnType<typeof crypto.randomUUID> : null;
    preferencesLoaded.current = preferenceKey;
    if (preferences.preview) {
      void readChatAdoptionPreview(store.access, state.view!.id, preferences.preview.id).then(restored => {
        if (preferencesLoaded.current === preferenceKey) { setPreview(restored); setPreviewVerified(true); }
      }).catch(reason => {
        if (preferencesLoaded.current === preferenceKey) { setPreviewVerified(false); setPreviewError(`The saved adoption preview could not be reloaded: ${reasonMessage(reason, 'The saved preview is unavailable.')}`); }
      });
    }
  }, [preferenceKey]);
  useEffect(() => {
    if (!preferenceKey || preferencesLoaded.current !== preferenceKey) return;
    if (skipPreferenceWrite.current) { skipPreferenceWrite.current = false; return; }
    writeChatViewPreferences(preferenceKey, { rightMode, transcriptAnchor, preview, applyOperationId: applyOperationId.current });
  }, [preferenceKey, preview, rightMode, transcriptAnchor]);
  const drafts = state.view?.drafts ?? [];
  const reviewableDrafts = drafts.filter(draft => draft.disposition === 'pending');
  const activeOrdinaryDocument = activeDocument && (activeDocument.role ?? 'ordinary') === 'ordinary' ? activeDocument : null;
  const autoReviewKey = `${accessKey}:${activeOrdinaryDocument?.head.documentId ?? 'none'}:${reviewableDrafts.map(draft => `${draft.document.head.documentId}:${draft.dispositionVersion}:${draft.stale}`).join('|')}`;
  const lastAutoReviewKey = useRef<string | null>(null);
  useEffect(() => {
    if (lastAutoReviewKey.current === autoReviewKey) return;
    lastAutoReviewKey.current = autoReviewKey;
    if (!activeOrdinaryDocument && reviewableDrafts.length > 0) {
      setRightMode('review');
    }
  }, [autoReviewKey]);
  const send = async () => {
    setError('');
    const currentBrief = store.state.composer.chapter?.safeBrief;
    if (briefApprovalBusy || (currentBrief && !currentBrief.confirmed)) {
      setError('Approve or remove the writing brief before sending this chapter request.');
      return;
    }
    if (!modelReady) {
      setError(provider.busy ? 'Checking your saved model choice…' : provider.state?.dispatch.detail || 'Check Settings before sending. Choose a connected model first.');
      return;
    }
    try { await store.send(selection, budget); }
    catch (reason) { if (mounted.current) setError(reasonMessage(reason, 'The request could not be sent.')); }
  };
  const prepareAdoption = async (selected: AssistantDraft[]) => {
    setAdoption('Preparing an exact adoption preview…'); setPreviewError('');
    try {
      await draftReviewRef.current?.flush();
      const refs = draftReviewRef.current?.currentRefs(selected.map(draft => draft.document.head.documentId)) ?? draftRefs(selected);
      const nextPreview = await store.prepareAdoption(refs);
      applyOperationId.current = crypto.randomUUID();
      setPreview(nextPreview); setPreviewVerified(true); setAdoption('Exact preview ready. Review the before and after versions, then apply it explicitly.'); setRightMode('review');
    } catch (reason) { setAdoption(`The adoption preview could not be prepared: ${reasonMessage(reason, 'The preview request failed.')}`); }
  };
  const applyPreview = async (selectedPreview: import('../ipc/projectChat').ChatAdoptionPreview) => {
    setPreviewError(''); setAdoption('Applying the reviewed preview…');
    try {
      // The preview carries exact draft heads. Flush any edits made after the
      // preview was prepared so the core can reject a stale preview safely.
      await draftReviewRef.current?.flush();
      if (!isCurrentConversation()) return;
      await onBeforeAdoption?.(selectedPreview.targets);
      if (!isCurrentConversation()) return;
      const ack = await store.adopt(selectedPreview, applyOperationId.current ?? (applyOperationId.current = crypto.randomUUID()));
      if (!isCurrentConversation()) return;
      await onDocumentsChanged(ack.documents);
      if (!isCurrentConversation()) return;
      setPreview(null); setPreviewVerified(false); applyOperationId.current = null; setAdoption(`${ack.documents.length} document${ack.documents.length === 1 ? '' : 's'} adopted.`); await switchRightMode('document');
    } catch (reason) {
      if (!isCurrentConversation()) return;
      let recoveryDetail = '';
      try { await onAdoptionFailure?.(reason); }
      catch (recoveryError) { recoveryDetail = ` Editor recovery still needs attention: ${reasonMessage(recoveryError, 'The current saved document could not be restored.')}`; }
      if (!isCurrentConversation()) return;
      const detail = reasonMessage(reason, 'The acknowledgement was lost.') + recoveryDetail;
      setPreviewError(isStaleAdoptionError(reason) ? `The preview is stale: ${detail} Prepare a fresh preview explicitly. The old preview remains available for comparison.` : `The adoption result is not confirmed: ${detail} Check saved documents before retrying; the exact preview is retained.`);
      setAdoption(isStaleAdoptionError(reason) ? 'Preview needs refresh.' : 'Adoption needs confirmation.');
    }
  };
  const compareStalePreview = async (selectedPreview: import('../ipc/projectChat').ChatAdoptionPreview) => {
    const comparison = await readStalePreviewComparison(store.access, selectedPreview);
    if (!isCurrentConversation()) throw new Error('The project changed while reading the comparison.');
    await store.refresh();
    return comparison;
  };
  const prepareAgainstCurrent = async (selected: AssistantDraft[], comparison: StalePreviewComparison) => {
    if (!preview || comparison.previewId !== preview.id) return;
    try {
      await store.refresh();
      if (!isCurrentConversation()) return;
      const current = selected.map(draft => store.draft(draft.document.head.documentId));
      if (current.some(draft => !draft || draft.stale || draft.disposition !== 'pending')) {
        throw new Error('This draft needs a fresh assistant response based on the changed story. Use Refresh with assistant, then review its new draft.');
      }
      await prepareAdoption(current as AssistantDraft[]);
    } catch (reason) {
      if (isCurrentConversation()) setPreviewError(`The preview is stale: ${reasonMessage(reason, 'Current versions could not be prepared.')}`);
    }
  };
  const switchRightMode = async (mode: 'document' | 'review') => {
    if (rightMode === 'review' && mode === 'document') {
      try { await draftReviewRef.current?.flush(); }
      catch (reason) { if (isCurrentConversation()) setPreviewError(reasonMessage(reason, 'Save the active draft before leaving review.')); return false; }
    }
    if (!isCurrentConversation()) return false;
    setRightMode(mode);
    return true;
  };
  const switchMobileSurface = async (surface: 'chat' | 'documents' | 'chapter' | 'review') => {
    if (surface === 'documents' || surface === 'chapter') {
      if (!await switchRightMode('document')) return;
    } else if (surface === 'review') {
      if (!await switchRightMode('review')) return;
    }
    if (isCurrentConversation()) setMobileSurface(surface);
  };
  const openDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      setRightMode('review');
      await draftReviewRef.current?.openDraft(draft.document.head.documentId);
      setMobileSurface('review');
    } catch (reason) { setError(reasonMessage(reason, 'Save the active draft before opening another review.')); }
  };
  const openChapterResult = async (run: DiscussionRun) => {
    try {
      if (!await switchRightMode('document')) return;
      await onOpenChapterResult?.(run);
      if (isCurrentConversation()) setMobileSurface('chapter');
    } catch (reason) { if (isCurrentConversation()) setError(reasonMessage(reason, 'The chapter response could not be opened.')); }
  };
  const attachDocument = async (document: DocumentRecord) => {
    try {
      await draftReviewRef.current?.flush();
      const head = onPrepareSource ? await onPrepareSource(document) : document.head;
      if (!isCurrentConversation()) return;
      await store.attachSource(head);
      if (!isCurrentConversation()) return;
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'Save the source before attaching it to the project conversation.')); }
  };
  const backToChat = async () => {
    try {
      await draftReviewRef.current?.flush();
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setPreviewError(reasonMessage(reason, 'Save the active draft before returning to chat.')); }
  };
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      if (mobileSurface === 'review' || mobileSurface === 'documents' || mobileSurface === 'chapter') {
        event.preventDefault();
        void backToChat();
      } else if (rightMode === 'review') {
        event.preventDefault();
        void switchRightMode('document');
      }
    };
    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [mobileSurface, rightMode]);
  const reject = async (draft: AssistantDraft) => {
    try { await draftReviewRef.current?.flush(); await store.setDisposition(draft.document.head.documentId, draft.dispositionVersion, 'rejected'); }
    catch (reason) { setError(reasonMessage(reason, 'The draft could not be rejected.')); }
  };
  const reviseDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      const refs = (state.composer.taskDraftRefs ?? []).filter(reference => reference.head.documentId !== draft.document.head.documentId);
      const exactRef = draftReviewRef.current?.currentRefs([draft.document.head.documentId])[0] ?? { head: draft.document.head, dispositionVersion: draft.dispositionVersion };
      store.setTaskDrafts([...refs, exactRef]);
      if (!state.composer.text.trim()) store.setText('Revise this draft with the project assistant.');
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'The draft could not be attached for revision.')); }
  };
  const reconsiderDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      await store.setDisposition(draft.document.head.documentId, draft.dispositionVersion, 'reconsider');
      const replacement = store.state.view?.drafts.find(candidate => candidate.document.head.documentId === draft.document.head.documentId && candidate.disposition === 'pending');
      if (replacement) store.setTaskDrafts([...(store.state.composer.taskDraftRefs ?? []).filter(reference => reference.head.documentId !== draft.document.head.documentId), { head: replacement.document.head, dispositionVersion: replacement.dispositionVersion }]);
      store.setText(`Reconsider the rejected draft “${draft.document.title}” and propose a fresh direction.`);
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'The draft could not be reconsidered.')); }
  };
  const clearChapter = async () => {
    try { await store.clearChapter(); composerRef.current?.focus(); }
    catch (reason) { setError(reasonMessage(reason, 'The chapter task could not be cleared.')); }
  };
  const openBrief = () => {
    if (!state.composer.chapter) return;
    if (!state.composer.chapter.safeBrief) store.setChapterBrief({ text: '', originMessageId: null, confirmed: false });
    setBriefOpen(true); setBriefFocus(value => value + 1);
  };
  const updateBrief = (brief: SafeBriefInput) => {
    const attempt = ++briefApprovalSequence.current;
    if (!brief.confirmed) { setBriefApprovalBusy(false); store.setChapterBrief(brief); return; }
    const chapter = store.state.composer.chapter;
    if (!chapter) return;
    const conversationId = store.state.view?.id;
    const exactChapter = JSON.stringify(chapter);
    let unchanged = true;
    // Observe every chapter mutation, including a change and later reversal.
    // Typing in the independent request field does not cancel an approval.
    const unsubscribe = store.subscribe(() => {
      if (JSON.stringify(store.state.composer.chapter) !== exactChapter) unchanged = false;
    });
    const stillCurrent = () => isCurrentConversation() && attempt === briefApprovalSequence.current && unchanged && store.state.view?.id === conversationId;
    setBriefApprovalBusy(true);
    void approveChapterBrief(brief, chapter.scope ?? null).then(approved => {
      if (stillCurrent()) store.setChapterBrief(approved);
    }).catch(reason => {
      if (stillCurrent()) setError(reasonMessage(reason, 'The writing brief could not be approved.'));
    }).finally(() => {
      unsubscribe();
      if (isCurrentConversation() && attempt === briefApprovalSequence.current) setBriefApprovalBusy(false);
    });
  };
  const removeBrief = () => { store.setChapterBrief(null); setBriefOpen(false); };
  const adaptBrief = (messageId: string, text: string) => {
    const chapter = store.state.composer.chapter;
    if (!chapter || !state.view) return;
    store.setChapterBrief({
      text,
      originMessageId: messageId,
      confirmed: false,
      projectOrigin: {
        version: 'project-conversation-brief.v1',
        projectId: project.access.projectId,
        operationNamespace: project.access.operationNamespace,
        conversationId: state.view.id,
        messageId,
        target: chapter.target,
        // The provenance is intentionally unapproved until the author edits
        // or accepts the brief. Approval replaces these placeholders with
        // hashes of the exact scope and text.
        scopeHash: '0'.repeat(64),
        textHash: '0'.repeat(64),
      },
    });
    setBriefOpen(true); setBriefFocus(value => value + 1);
  };
  const prepareHandoff = async (proposal: ChapterHandoffProposal, messageId: string, targetId: string | null, title: string) => {
    if (!onPrepareChapter || !state.view) throw new Error('Open the project before preparing chapter writing.');
    await draftReviewRef.current?.flush();
    const target = await onPrepareChapter(targetId, title);
    if (!isCurrentConversation()) return;
    if (target.kind !== 'chapter' || (target.role ?? 'ordinary') !== 'ordinary') throw new Error('Choose an ordinary chapter for this writing task.');
    const text = [proposal.instruction.trim(), proposal.brief.trim()].filter(Boolean).join('\n\n');
    await store.stageChapter({ target: target.head, intent: 'continue', basis: 'working', scope: null, safeBrief: {
      text, originMessageId: messageId, confirmed: false,
      projectOrigin: { version: 'project-conversation-brief.v1', projectId: project.access.projectId, operationNamespace: project.access.operationNamespace,
        conversationId: state.view.id, messageId, target: target.head, scopeHash: '0'.repeat(64), textHash: '0'.repeat(64) },
    } });
    if (!isCurrentConversation()) return;
    // A proposed transition never replaces an independently typed request.
    if (!store.state.composer.text.trim()) store.setText(proposal.instruction);
    setBriefOpen(true); setBriefFocus(value => value + 1);
    setMobileSurface('chat');
    composerRef.current?.focus();
  };
  const stageAssumptionCorrection = (originalText: string, revisedText: string): boolean => {
    if (store.state.composer.chapter) {
      setError('Return to the project conversation before staging an author-room assumption correction. Use “Return to project conversation” first.');
      return false;
    }
    const correction = ['Correction for the next draft.', `Original assumption: “${originalText}”`, `Author correction: ${revisedText}`].join('\n');
    const existing = store.state.composer.text;
    const separator = existing && !existing.endsWith('\n') ? '\n\n' : '';
    store.setText(`${existing}${separator}${correction}`);
    setMobileSurface('chat');
    composerRef.current?.focus();
    return true;
  };
  const disposition = (referenceId: string, version: string, value: string, options: ChatDispositionOptions = {}, rationale = '') => { void store.setDisposition(referenceId, version, value, rationale, options).catch(reason => setError(reasonMessage(reason, 'The response decision could not be saved.'))); };
  const composerText = state.composer.text;
  const chapter = state.composer.chapter;
  const exactSourceCount = new Set([...(state.composer.sourceRefs ?? []), ...(state.composer.focusedDocumentRef ? [state.composer.focusedDocumentRef] : [])].map(head => `${head.documentId}:${head.version}:${head.bodyHash}`)).size;
  const taskDraftCount = new Set((state.composer.taskDraftRefs ?? []).map(reference => `${reference.head.documentId}:${reference.head.version}:${reference.head.bodyHash}:${reference.dispositionVersion}`)).size;
  const reviewOrigins = useDraftReviewContext({ access: project.access, view: state.view, enabled: rightMode === 'review' || mobileSurface === 'review' });
  const reviewContext = reviewOrigins.context;
  const previewOriginsReady = !preview || preview.targets.every(target => {
    const draft = drafts.find(draft => draft.document.head.documentId === target.draft.head.documentId);
    return draft && !!reviewContext[draft.originRunId];
  });
  const scopeSummary = [exactSourceCount ? `${exactSourceCount} exact source${exactSourceCount === 1 ? '' : 's'}` : '', taskDraftCount ? `${taskDraftCount} unadopted draft${taskDraftCount === 1 ? '' : 's'}` : '', chapter ? 'chapter task' : ''].filter(Boolean).join(' · ') || 'No sources attached';
  const composerStatus = state.status === 'saving' ? 'Saving request…' : state.status === 'queued' || state.status === 'running' || state.status === 'stopping' ? 'Newer edits stay local until this request is accepted' : state.composerDirty ? 'Unsaved request is kept locally' : 'Composer saved';
  const chapterTarget = chapter ? project.documents.find(document => document.head.documentId === chapter.target.documentId) : null;
  const chapterHasScope = !!chapter?.scope;
  const runContext = frozenRequestContext(requestItemForRun(state.view?.items ?? [], state.activeRun), project.documents);
  const requestContext = {
    surface: chapter ? 'chapterWriting' as const : 'authorRoom' as const,
    targetLabel: chapter ? `Chapter · ${chapterTarget?.title ?? chapter.target.documentId}` : activeDocument ? (state.composer.focusedDocumentRef?.documentId === activeDocument.head.documentId ? `Discussing · ${activeDocument.title}` : `Open document · ${activeDocument.title}`) : 'Project conversation',
    scopeLabel: scopeSummary,
    selection,
    run: runContext,
  };
  const composing = useRef(false);
  return <section className="chat-project-conversation" aria-label={`Conversation for ${project.project.title}`}>
    <RequestStatus status={state.status} run={state.activeRun} error={error || state.error} uncertainOperationId={state.uncertainOperationId} workerIssues={state.view?.workerIssues} requestContext={requestContext} freshness={storyFreshness} onStop={() => void store.stop().catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The request could not be stopped.')); })} onRetrySave={runId => void store.retrySave(runId).catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The retained response could not be saved.')); })} onReconcile={() => void store.reconcileStart().catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The operation could not be reconciled.')); })} />
    <nav className="chat-mobile-tabs" aria-label="Project workspace surfaces"><button type="button" aria-selected={mobileSurface === 'chat'} onClick={() => void switchMobileSurface('chat')}>Chat</button><button type="button" aria-selected={mobileSurface === 'documents'} onClick={() => void switchMobileSurface('documents')}>Documents</button><button type="button" aria-selected={mobileSurface === 'chapter'} onClick={() => void switchMobileSurface('chapter')}>Chapter</button>{rightMode === 'review' && <button type="button" aria-selected={mobileSurface === 'review'} onClick={() => void switchMobileSurface('review')}>Review</button>}</nav>
    <ChatSplitPane initialWidthPercent={47} projectKey={`${project.access.projectId}:${project.access.operationNamespace}`} left={
      <section className={`chat-conversation-surface ${mobileSurface === 'chat' ? 'mobile-visible' : ''}`}>
        <header className="coauthor-conversation-header">
          <div>
            <span className="coauthor-conversation-kicker">Project conversation</span>
            <h1>Let's build your story</h1>
          </div>
          {reviewableDrafts.length > 0 && <span className="coauthor-conversation-draft-count">{reviewableDrafts.length} draft{reviewableDrafts.length === 1 ? '' : 's'} to review</span>}
        </header>
        <Transcript store={store} documents={project.documents} activeDocument={activeDocument} anchor={transcriptAnchor} onAnchorChange={setTranscriptAnchor} onDisposition={disposition} onStageAssumptionCorrection={stageAssumptionCorrection} onOpenDocument={onOpenDocument} onOpenDraft={openDraft} onOpenChapterResult={onOpenChapterResult ? openChapterResult : undefined} onAdaptBrief={chapter && chapter.intent !== 'discuss' ? adaptBrief : undefined} onPrepareHandoff={onPrepareChapter ? prepareHandoff : undefined} onBrowseDocuments={() => void switchMobileSurface('documents')} onCreateChapter={onCreateChapter} onCreateNote={onCreateNote} />
        <div className="chat-composer-wrap">
          <div className="chat-composer-reference">{chapter ? <>Chapter task · <strong>{chapterTarget?.title ?? chapter.target.documentId}</strong></> : activeDocument ? state.composer.focusedDocumentRef?.documentId === activeDocument.head.documentId ? <>Discussing <strong>{activeDocument.title}</strong></> : <>Open document · <strong>{activeDocument.title}</strong> <small>(attach from Writer to include its exact text)</small></> : 'Project conversation'}<span>{composerStatus} · {scopeSummary}</span></div>
          {chapter && <section className="chat-chapter-context" aria-label="Captured chapter task"><div className="chat-chapter-context-heading"><strong>Captured chapter task</strong><button type="button" onClick={() => void clearChapter()}>Return to project conversation</button></div>{chapter.scope && <p className="chat-chapter-scope"><strong>Captured scope:</strong> “{chapter.scope.quote || 'Whole chapter'}”</p>}<div className="chat-chapter-controls"><label>Intent<select value={chapter.intent} onChange={event => store.setChapterIntent(event.target.value as ProjectChapterComposer['intent'])}><option value="discuss">Discuss this chapter</option>{chapterHasScope && <option value="proposeEdits">Suggest edits to the captured scope</option>}<option value="continue">Continue this chapter</option></select></label>{chapter.intent === 'continue' && <label>Continuation basis<select value={chapter.basis ?? 'working'} onChange={event => store.setChapterBasis(event.target.value as NonNullable<ProjectChapterComposer['basis']>)}><option value="working">Current working story</option><option value="reviewed">Reviewed story</option></select></label>}</div>{chapter.intent !== 'discuss' && <div className="chat-brief-launcher"><span>{chapter.safeBrief?.confirmed ? 'Writing brief approved' : chapter.safeBrief ? 'Writing brief needs approval' : 'Writing brief · optional'}</span><button type="button" onClick={openBrief} disabled={briefApprovalBusy}>{chapter.safeBrief ? 'Edit brief' : 'Add writing brief'}</button></div>}{briefOpen && chapter.intent !== 'discuss' && chapter.safeBrief && <SafeBriefEditor value={chapter.safeBrief} disabled={briefApprovalBusy || ['saving', 'queued', 'running', 'stopping', 'uncertain'].includes(state.status)} focusKey={briefFocus} onChange={updateBrief} onRemove={removeBrief} />}</section>}
          <details className="coauthor-composer-details" open={composerDetailsOpen} onToggle={event => setComposerDetailsOpen(event.currentTarget.open)}>
            <summary><span>Sources &amp; details</span><small>{scopeSummary}</small></summary>
            <div className="chat-composer-context" tabIndex={0} aria-label="Request context and scope">
            {((state.composer.sourceRefs ?? []).length > 0 || (state.composer.taskDraftRefs ?? []).length > 0 || state.composer.focusedDocumentRef) && <div className="chat-composer-chips" aria-label="Attached context"><span className="chat-composer-chips-label">Attached context</span>{(state.composer.sourceRefs ?? []).map(head => <span className="chat-composer-chip" key={`source:${head.documentId}`}><span>{project.documents.find(document => document.head.documentId === head.documentId)?.title ?? head.documentId} · v{head.version} · {head.bodyHash.slice(0, 10)}…</span><button type="button" aria-label={`Remove ${head.documentId} source`} onClick={() => store.setSources((state.composer.sourceRefs ?? []).filter(item => item.documentId !== head.documentId))}>×</button></span>)}{state.composer.focusedDocumentRef && <span className="chat-composer-chip"><span>{project.documents.find(document => document.head.documentId === state.composer.focusedDocumentRef?.documentId)?.title ?? state.composer.focusedDocumentRef.documentId} · focused v{state.composer.focusedDocumentRef.version}</span><button type="button" aria-label="Remove focused document" onClick={() => store.setFocus(undefined)}>×</button></span>}{(state.composer.taskDraftRefs ?? []).map(reference => <span className="chat-composer-chip chat-composer-chip-draft" key={`draft:${reference.head.documentId}`}><span>{drafts.find(draft => draft.document.head.documentId === reference.head.documentId)?.document.title ?? reference.head.documentId} · draft v{reference.head.version} · decision {reference.dispositionVersion}</span><button type="button" aria-label={`Remove ${reference.head.documentId} draft`} onClick={() => store.setTaskDrafts((state.composer.taskDraftRefs ?? []).filter(item => item.head.documentId !== reference.head.documentId))}>×</button></span>)}</div>}
            </div>
          </details>
          <textarea ref={composerRef} disabled={!state.view} aria-busy={!state.view} aria-label="Message the project assistant" value={composerText} onChange={event => store.setText(event.target.value)} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }} onKeyDown={event => { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && !composing.current && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); } }} placeholder={chapter ? 'Describe what should happen in this chapter…' : 'Tell me what you want to explore or write next…'} rows={4} />
          <div className="chat-composer-actions"><button type="button" onClick={() => void send()} disabled={!state.view || !modelReady || !composerText.trim() || briefApprovalBusy || !!(chapter?.safeBrief && !chapter.safeBrief.confirmed) || ['queued', 'running', 'stopping', 'saving', 'uncertain'].includes(state.status)} className="primary-button">Send <span>Ctrl+Enter</span></button><button type="button" onClick={onEarlierWorkshop}>Earlier Workshop</button><span className="chat-composer-help">Enter makes a new line · the assistant may ask one useful question</span></div>
        </div>
      </section>
      } right={<section className={`chat-document-surface ${mobileSurface !== 'chat' ? 'mobile-visible' : ''} chat-mobile-${mobileSurface}`}>
        <nav className="chat-right-mode-tabs" aria-label="Document workspace mode"><button type="button" aria-selected={rightMode === 'document'} onClick={() => void switchRightMode('document')}>Document</button><button type="button" aria-selected={rightMode === 'review'} onClick={() => void switchRightMode('review')}>Review drafts{drafts.length ? ` (${drafts.length})` : ''}</button></nav>
        <div className={`chat-document-view ${rightMode === 'review' ? 'is-hidden' : ''}`}>
          <div className="chat-document-editor">{editor ? <Writer key={`${project.project.projectId}:${editor.active.record.head.documentId}`} {...editor} conversation={{ stageChapter, attachSource, reviewRunId: reviewRunId ?? null }} /> : <div className="chat-empty"><h2>Open a document</h2><p>Select a chapter, world note, character, or draft from the document panel.</p></div>}</div>
          <ProjectDocumentsPanel project={project} activeDocument={activeDocument} drafts={drafts} relatedDocumentIds={(state.composer.sourceRefs ?? []).map(head => head.documentId)} onOpenDocument={onOpenDocument} onOpenDraft={openDraft} onAttachSource={attachDocument} chapterTaskActive={!!chapter} onCreateChapter={onCreateChapter} />
        </div>
        <div className={`chat-review-view ${rightMode === 'document' ? 'is-hidden' : ''}`}>
          <button type="button" className="chat-back-to-chat" onClick={() => void backToChat()}>Back to chat</button>
          {reviewOrigins.loading && <p role="status">Loading the drafts’ original requests and assumptions…</p>}
          {reviewOrigins.error && <p role="alert">{reviewOrigins.error} <button type="button" onClick={reviewOrigins.retry}>Retry reading original requests</button></p>}
          <DraftReviewPanel ref={draftReviewRef} reviewContext={reviewContext} project={project} drafts={drafts} preview={preview} previewVerified={previewVerified && previewOriginsReady} previewError={previewError} viewKey={preferenceKey ?? undefined} onPrepareAdoption={prepareAdoption} onApplyPreview={applyPreview} onCompareStalePreview={compareStalePreview} onPrepareAgainstCurrent={prepareAgainstCurrent} onReject={reject} onRevise={reviseDraft} onReconsider={reconsiderDraft} onOpenDraft={openDraft} onOpenDocument={onOpenDocument} onAccessChanged={onAccessChanged} onDraftSaved={async (_draft, _head) => { await store.refresh(); }} />
        </div>
        {adoption && <p className="chat-adoption-status" role="status">{adoption}</p>}
        {previewError && <p className="chat-draft-warning chat-adoption-warning" role="alert">{previewError} The exact preview is retained; review it before retrying.</p>}
      </section>} />
  </section>;
});
