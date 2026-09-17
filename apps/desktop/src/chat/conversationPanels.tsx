/** Conversation transcript panels: disposition controls and the scrollable turn list. */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type UIEvent } from 'react';
import type { DocumentRecord } from '../ipc/projects';
import type { AssistantDraft, ChatDispositionOptions, ChatDispositionScope, ChatUnknownTo, ConversationItem } from '../ipc/projectChat';
import type { DiscussionRun } from '../ipc/discussions';
import { ContextInspector } from '../assistant';
import { conversationItems, type ProjectConversationStore } from './conversationStore';
import { DocumentSaveRecap } from './DocumentSaveRecap';
import { ChapterHandoff, type ChapterHandoffProposal } from './ChapterHandoff';
import { NewReplyAffordance, useTranscriptScroll } from './TranscriptScroll';
import { chapterOutputOf, dispositionLabel, dispositionOf, dispositionScopeLabel, dispositionUnknownToLabel, dispositionVersion, isLatestDisposition, latestDisposition, materializedDraftIds, outputOf, runPayload, type SavedDisposition } from './conversationHelpers';

export function ResponseDispositionControls({ referenceId, expectedVersion, runId, isAssumption, assumptionText, savedDisposition, documents, activeDocument, onDisposition, onStageAssumptionCorrection }: { referenceId: string; expectedVersion: string; runId: string; isAssumption?: boolean; assumptionText?: string; savedDisposition?: SavedDisposition | null; documents: DocumentRecord[]; activeDocument: DocumentRecord | null; onDisposition: (referenceId: string, version: string, value: string, options?: ChatDispositionOptions, rationale?: string) => void; onStageAssumptionCorrection?: (originalText: string, revisedText: string) => boolean | void }) {
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

export function Transcript({ store, documents, activeDocument, onDisposition, onStageAssumptionCorrection, onOpenDocument, onOpenDraft, onOpenChapterResult, onAdaptBrief, onPrepareHandoff, onBrowseDocuments, onCreateChapter, onCreateNote, anchor, onAnchorChange }: { store: ProjectConversationStore; documents: DocumentRecord[]; activeDocument: DocumentRecord | null; onDisposition: (referenceId: string, version: string, value: string, options?: ChatDispositionOptions, rationale?: string) => void; onStageAssumptionCorrection?: (originalText: string, revisedText: string) => boolean | void; onOpenDocument?: (document: DocumentRecord) => Promise<void> | void; onOpenDraft?: (draft: AssistantDraft) => Promise<void> | void; onOpenChapterResult?: (run: DiscussionRun) => Promise<void> | void; onAdaptBrief?: (messageId: string, text: string) => void; onPrepareHandoff?: (proposal: ChapterHandoffProposal, messageId: string, targetId: string | null, title: string) => Promise<void>; onBrowseDocuments?: () => void; onCreateChapter?: () => Promise<void> | void; onCreateNote?: () => Promise<void> | void; anchor?: string | null; onAnchorChange?: (itemId: string) => void }) {
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
