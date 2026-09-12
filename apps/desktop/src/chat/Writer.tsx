import { memo, useCallback, useEffect, useRef, useState } from 'react';
import { Editor, Extension } from '@tiptap/core';
import { EditorContent } from '@tiptap/react';
import { EditorState, Plugin, Selection, TextSelection, type Transaction } from '@tiptap/pm/state';
import { closeHistory, redo, redoDepth, undo, undoDepth } from '@tiptap/pm/history';
import {
  bodyHash, canonicalJson, captureRevisionScope, captureSelection, confirmPreparation,
  DocumentSession, editorExtensions, HistoryPanel, prepareContinuation, prepareScopedReplacement,
  prepareStructuredReplacement, RecoveryCopy, ReviewPanel, SessionError, snapshotFromEditor,
  structuredRange, validateStructuredBlocks,
  type PreparedEditorChange, type Scope, type SessionState,
} from '../editor';
import { saveViewState, type DocumentRecord, type Endpoint, type Head, type Revision, type ViewState } from '../ipc/projects';
import { prepareContinuationProposal, prepareProposal, prepareStructuredProposal, readProposals, type PrepareContinuation, type PrepareStructured, type PreparedProposal, type Proposal } from '../ipc/proposals';
import { FeedbackPanel, ProposalPanel } from '../assistant';
import type { ProjectChapterComposer } from '../ipc/projectChat';
import { readProjectChapterFeedback, type ChapterDiscussionFeedback } from '../ipc/projectChat';
import { ChapterRangeReview, suggestedChapterRange } from './ChapterRangeReview';
import { confirmChapterRange } from './confirmChapterRange';
import { ChapterMemory } from '../story';
import { DocumentAliases } from '../story';
import type { SourceChoice } from '../ipc/sourcePins';

const Manuscript = memo(({ editor }: { editor: Editor }) => <EditorContent editor={editor} />);
export interface WriterConversation {
  stageChapter(task: ProjectChapterComposer): Promise<void>;
  attachSource(head: Head): Promise<void>;
  reviewRunId: string | null;
}
export function Writer({ active, sources, onError, onRename, navigation, conversation }: { active: { record: DocumentRecord; session: DocumentSession; viewState: ViewState | null }; sources: SourceChoice[]; onError: (message: string) => void; onRename: () => void; navigation?: { index: number; total: number; previous?: () => void; next?: () => void; disabled: boolean }; conversation?: WriterConversation }) {
  const { session, record } = active;
  const [state, setState] = useState(session.state);
  const [, redraw] = useState(0);
  const [pasteNotice, setPasteNotice] = useState('');
  const [discussionVisible, setDiscussionVisible] = useState(true);
  const [historyVisible, setHistoryVisible] = useState(false);
  const [reviewVisible, setReviewVisible] = useState(false);
  const [memoryVisible, setMemoryVisible] = useState(false);
  const [namesVisible, setNamesVisible] = useState(false);
  const [chatProposals, setChatProposals] = useState<Proposal[]>([]);
  const [chapterFeedback, setChapterFeedback] = useState<ChapterDiscussionFeedback | null>(null);
  const [rangeBusy, setRangeBusy] = useState(false);
  const [rangeError, setRangeError] = useState('');
  const [rangeConfirmed, setRangeConfirmed] = useState(false);
  const conversationRef = useRef(conversation);
  conversationRef.current = conversation;
  const namesButton = useRef<HTMLButtonElement>(null);
  const namesGuard = useRef<(() => Promise<void>) | null>(null);
  const registerNamesGuard = useCallback((guard: (() => Promise<void>) | null) => { namesGuard.current = guard; }, []);
  const flushBeforeNames = useCallback(() => session.flush(), [session]);
  useEffect(() => {
    session.setLeaveGuard(async () => {
      try { await namesGuard.current?.(); }
      catch (reason) { setNamesVisible(true); throw reason; }
    });
    return () => session.setLeaveGuard(null);
  }, [session]);
  const [assistantAction, setAssistantAction] = useState<{ kind: 'draft' | 'develop' | 'revise' | 'discuss'; nonce: number } | undefined>();
  function startAssistant(kind: 'draft' | 'develop' | 'revise' | 'discuss') {
    if (conversationRef.current) {
      void stageConversationTask(kind === 'draft' ? 'continue' : kind === 'revise' ? 'proposeEdits' : 'discuss').catch(reason => onError((reason as Error).message));
      return;
    }
    setHistoryVisible(false); setReviewVisible(false); setMemoryVisible(false); setDiscussionVisible(true);
    setAssistantAction(previous => ({ kind, nonce: (previous?.nonce ?? 0) + 1 }));
  }
  const reviewButton = useRef<HTMLButtonElement>(null);
  const memoryButton = useRef<HTMLButtonElement>(null);
  const historyButton = useRef<HTMLButtonElement>(null);
  const [discussionSelection, setDiscussionSelection] = useState<{ scope: Scope; nonce: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const discussionSaver = useRef<(() => Promise<void>) | null>(null);
  const registerDiscussionSaver = useCallback((save: (() => Promise<void>) | null) => { discussionSaver.current = save; }, []);
  const discuss = useRef<() => boolean>(() => false);
  // Keep the exact IDs/body for a preview whose native acknowledgment is lost.
  const continuationPreviews = useRef(new Map<string, PrepareContinuation>());
  const structuredPreviews = useRef(new Map<string, PrepareStructured>());
  const [editor] = useState(() => new Editor({
    extensions: [...editorExtensions, Extension.create({
      name: 'persistentEditing', priority: 1000,
      addProseMirrorPlugins: () => [new Plugin({ filterTransaction: tr => !tr.docChanged || session.state.editable || tr.getMeta('authorConflictChoice') === true || tr.getMeta('durableApply') === true })],
      addKeyboardShortcuts() { return {
        'Mod-z': () => undo(this.editor.state, tr => this.editor.view.dispatch(tr.setMeta('saveCause', 'undo'))),
        'Mod-Shift-z': () => redo(this.editor.state, tr => this.editor.view.dispatch(tr.setMeta('saveCause', 'redo'))),
        'Mod-y': () => redo(this.editor.state, tr => this.editor.view.dispatch(tr.setMeta('saveCause', 'redo'))),
        'Mod-s': () => { void session.checkpoint('manual').catch(error => onError(String(error.message))); return true; },
        'Mod-Shift-f': () => discuss.current(),
      }; },
    })],
    content: structuredClone(session.body.body),
    editorProps: { attributes: { 'aria-label': 'Manuscript', role: 'textbox', 'aria-multiline': 'true', spellcheck: 'true' }, handleClick: (_view, _pos, event) => { if ((event.target as HTMLElement).closest('a')) { event.preventDefault(); return true; } return false; } },
    onUpdate: ({ editor, transaction }) => {
      if (transaction.getMeta('authorConflictChoice') || transaction.getMeta('durableApply')) return;
      session.update(snapshotFromEditor(editor.getJSON()), transaction.getMeta('saveCause') ?? 'typing');
    },
  }));
  async function stageConversationTask(intent: 'discuss' | 'proposeEdits' | 'continue', selected?: Scope): Promise<void> {
    const bridge = conversationRef.current;
    if (!bridge) return;
    if (record.kind !== 'chapter') {
      await session.flush();
      await bridge.attachSource(session.state.head);
      return;
    }
    const captured = snapshotFromEditor((selected?.source ?? editor.state.doc).toJSON());
    const hash = await bodyHash(canonicalJson(captured));
    await session.flush();
    if (session.state.head.bodyHash !== hash) throw new SessionError('StaleSource', 'The chapter changed while its scope was being captured. Select it again.');
    const scope = selected
      ? { kind: 'passage' as const, start: selected.start, end: selected.end, quote: selected.quote, sourceBodyHash: hash }
      : intent === 'proposeEdits' ? captureRevisionScope(captured, hash, 'wholeDocument') : null;
    await bridge.stageChapter({ target: session.state.head, intent, basis: intent === 'continue' ? 'working' : null, scope });
    setMenu(null);
  }
  const refreshChatProposals = useCallback(async () => {
    const proposals = await readProposals(session.projectAccess, record.head.documentId);
    setChatProposals(proposals);
  }, [session, record.head.documentId]);
  useEffect(() => {
    setChapterFeedback(null); setRangeError(''); setRangeBusy(false); setRangeConfirmed(false);
    if (!conversation?.reviewRunId) return;
    let live = true;
    void readProposals(session.projectAccess, record.head.documentId).then(proposals => { if (live) setChatProposals(proposals); }).catch(reason => { if (live) onError((reason as Error).message); });
    void readProjectChapterFeedback(session.projectAccess, conversation.reviewRunId).then(feedback => { if (live) setChapterFeedback(feedback); }).catch(reason => { if (live) setRangeError(reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail) : (reason as Error).message); });
    return () => { live = false; };
  }, [conversation?.reviewRunId, record.head.documentId, session]);
  const proposedRange = chapterFeedback ? suggestedChapterRange(chapterFeedback, session.body) : null;
  async function confirmSuggestedRange(): Promise<void> {
    const runId = conversationRef.current?.reviewRunId;
    if (!proposedRange || !runId || chapterFeedback?.runId !== runId || rangeBusy) return;
    setRangeBusy(true); setRangeError('');
    try {
      await confirmChapterRange(session, proposedRange, async () => {
        const fresh = await readProjectChapterFeedback(session.projectAccess, runId);
        if (conversationRef.current?.reviewRunId !== runId || session.state.phase === 'disposed') throw new Error('The chapter review changed. Open the response again.');
        return fresh && fresh.runId === runId ? suggestedChapterRange(fresh, session.body) : null;
      }, async task => {
        if (conversationRef.current?.reviewRunId !== runId || !conversationRef.current) throw new Error('The chapter review changed. Open the response again.');
        await conversationRef.current.stageChapter(task);
      });
      if (conversationRef.current?.reviewRunId === runId) setRangeConfirmed(true);
    } catch (reason) {
      if (conversationRef.current?.reviewRunId === runId && session.state.phase !== 'disposed') setRangeError(reason instanceof Error ? reason.message : 'The passage could not be confirmed. Select it again in the chapter.');
    } finally { if (conversationRef.current?.reviewRunId === runId && session.state.phase !== 'disposed') setRangeBusy(false); }
  }
  const prepare = async (proposal: Proposal, text: string, operationId: string): Promise<PreparedProposal> => {
    if (proposal.kind === 'structured') {
      let request = structuredPreviews.current.get(operationId);
      try {
        const blocks: unknown = JSON.parse(text);
        validateStructuredBlocks(blocks);
        if (request && (request.proposalId !== proposal.id || canonicalJson(request.blocks) !== canonicalJson(blocks))) throw new Error('This preview request already belongs to different prose or formatting.');
        if (!request) {
          // quoteHash authenticates structural tokens in Rust, not plain text.
          if (await bodyHash(canonicalJson(proposal.sourceBody)) !== proposal.scope.sourceHash || proposal.source.bodyHash !== proposal.scope.sourceHash) throw new Error('The captured source does not match this suggestion.');
          const source = EditorState.create({ schema: editor.schema, doc: editor.schema.nodeFromJSON(proposal.sourceBody.body) });
          const tr = prepareStructuredReplacement(source, proposal.scope, blocks);
          request = { access: session.projectAccess, operationId, proposalId: proposal.id, expectedPreparedVersion: proposal.prepared?.version ?? '0', blocks, body: snapshotFromEditor(tr.doc.toJSON()) };
          structuredPreviews.current.set(operationId, structuredClone(request));
        }
      } catch (error) { throw new SessionError('InvalidProposal', (error as Error).message); }
      return confirmPreparation(request, await prepareStructuredProposal({ ...structuredClone(request), access: session.projectAccess }));
    }
    if (proposal.kind === 'continuation') {
      let request = continuationPreviews.current.get(operationId);
      const paragraphs = text.split('\n\n');
      if (request && (request.proposalId !== proposal.id || canonicalJson(request.paragraphs) !== canonicalJson(paragraphs))) {
        throw new SessionError('InvalidProposal', 'This preview request already belongs to different wording.');
      }
      if (!request) {
        try {
          if (await bodyHash(canonicalJson(proposal.sourceBody)) !== proposal.scope.sourceHash || proposal.source.bodyHash !== proposal.scope.sourceHash) throw new Error('The captured source does not match this suggestion.');
          const source = EditorState.create({ schema: editor.schema, doc: editor.schema.nodeFromJSON(proposal.sourceBody.body) });
          const tr = prepareContinuation(source, proposal.scope, paragraphs);
          request = { access: session.projectAccess, operationId, proposalId: proposal.id, expectedPreparedVersion: proposal.prepared?.version ?? '0', paragraphs, body: snapshotFromEditor(tr.doc.toJSON()) };
          continuationPreviews.current.set(operationId, structuredClone(request));
        } catch (error) { throw new SessionError('InvalidProposal', (error as Error).message); }
      }
      return confirmPreparation(request, await prepareContinuationProposal({ ...structuredClone(request), access: session.projectAccess }));
    }
    let source: EditorState; let tr;
    try {
      source = EditorState.create({ schema: editor.schema, doc: editor.schema.nodeFromJSON(proposal.sourceBody.body) });
      tr = prepareScopedReplacement(source, proposal.scope, text);
    } catch (error) { throw new SessionError('InvalidProposal', (error as Error).message); }
    const request = { access: session.projectAccess, operationId, proposalId: proposal.id, expectedPreparedVersion: proposal.prepared?.version ?? '0', replacementText: text, body: snapshotFromEditor(tr.doc.toJSON()) };
    return confirmPreparation(request, await prepareProposal(request));
  };
  function prepareChange(tr: Transaction): PreparedEditorChange {
      tr.setMeta('durableApply', true);
      const expected = editor.state.applyTransaction(tr).state.doc;
      return { body: snapshotFromEditor(expected.toJSON()), read: () => snapshotFromEditor(editor.getJSON()), commit: () => {
        // Reconciliation can revisit a callback after dispatch succeeded but
        // a later callback threw. The exact displayed result is already done.
        if (!editor.state.doc.eq(expected)) {
          if (!editor.state.doc.eq(tr.before)) throw new Error('The live editor changed before the saved suggestion could be displayed.');
          editor.view.dispatch(tr);
        }
        editor.view.dispatch(closeHistory(editor.state.tr));
        return snapshotFromEditor(editor.getJSON());
      } };
  }
  const apply = async (proposal: Proposal, prepared: PreparedProposal): Promise<void> => {
    await session.applyPrepared(proposal, prepared, () => {
      if (proposal.kind === 'structured') {
        if (!prepared.blocks) throw new SessionError('InvalidProposal', 'This suggestion has no prepared paragraph content.');
        const range = structuredRange(editor.state.doc, proposal.scope);
        const ids = prepared.body.body.content.slice(range.first, range.first + prepared.blocks.length).map(block => block.attrs.id);
        return prepareChange(prepareStructuredReplacement(editor.state, proposal.scope, prepared.blocks, ids));
      }
      if (proposal.kind === 'continuation') {
        if (!prepared.paragraphs?.length) throw new SessionError('InvalidProposal', 'This continuation has no prepared paragraphs.');
        const ids = prepared.body.body.content.slice(-prepared.paragraphs.length).map(block => block.attrs.id);
        return prepareChange(prepareContinuation(editor.state, proposal.scope, prepared.paragraphs, ids));
      }
      return prepareChange(prepareScopedReplacement(editor.state, proposal.scope, prepared.replacementText));
    });
  };
  const restore = async (revision: Revision): Promise<void> => {
    await session.restoreRevision(revision, () => {
      const content = editor.schema.nodeFromJSON(revision.body.body);
      return prepareChange(closeHistory(editor.state.tr).replaceWith(0, editor.state.doc.content.size, content.content));
    });
  };
  async function openHistory() {
    try { await session.checkpoint('manual'); setReviewVisible(false); setMemoryVisible(false); setHistoryVisible(true); }
    catch (error) { onError((error as Error).message); }
  }
  discuss.current = () => {
    const scope = captureSelection(editor);
    if (!scope || !session.state.editable) return false;
    if (conversationRef.current) {
      void stageConversationTask('discuss', scope).catch(reason => onError((reason as Error).message));
      return true;
    }
    setDiscussionSelection(previous => ({ scope, nonce: (previous?.nonce ?? 0) + 1 }));
    setHistoryVisible(false); setReviewVisible(false); setMemoryVisible(false); setDiscussionVisible(true); setMenu(null); return true;
  };
  const captureReviewSelection = useCallback((): Scope | null => {
    // Review evidence is tied to the last durable body. Do not capture an
    // unsaved editor selection that the review stage cannot authenticate.
    if (!session.state.editable || session.state.dirty) return null;
    return captureSelection(editor);
  }, [editor, session]);
  useEffect(() => {
    let disposed = false;
    let composing = false;
    let viewTimer: ReturnType<typeof setTimeout> | undefined;
    const scheduleViewSave = () => {
      if (disposed || composing) return;
      clearTimeout(viewTimer);
      viewTimer = setTimeout(() => {
        viewTimer = undefined;
        if (disposed) return;
        void session.persistViewBackground(saveReadingPosition).then(started => {
          // Dirty, composing, and lifecycle-busy states are normal while the
          // author works. Retry from a later idle turn without spinning.
          if (!started && !disposed && !composing && session.state.phase === 'editing' && !session.state.saving) scheduleViewSave();
        }).catch(() => { /* Session state retains owned background failures. */ });
      }, 1250);
    };
    const unsubscribe = session.subscribe(() => {
      const next = session.state;
      setState(next); editor.setEditable(next.editable, false);
      // A skipped attempt may have observed a lifecycle barrier or a body
      // flight. The state transition back to editing is the next idle arm.
      if (!disposed && next.phase === 'editing' && !next.saving) scheduleViewSave();
    });
    const saveLater = () => scheduleViewSave();
    const update = () => redraw(value => value + 1); editor.on('transaction', update);
    editor.on('selectionUpdate', saveLater); editor.on('update', saveLater);
    const endpoint = (position: number): Endpoint => {
      let found: Endpoint | null = null;
      editor.state.doc.forEach((node, offset) => {
        if (position >= offset && position < offset + node.nodeSize) found = { blockId: node.attrs.id as string, utf16Offset: node.isTextblock ? Math.max(0, Math.min(node.content.size, position - offset - 1)) : 0 };
      });
      return found ?? { blockId: editor.state.doc.lastChild!.attrs.id as string, utf16Offset: editor.state.doc.lastChild!.content.size };
    };
    const saveReadingPosition = async () => {
      if (disposed) return;
      // Capture every editor-derived value before crossing the async IPC
      // boundary. Cleanup may destroy the editor while the acknowledgment is
      // pending, so validation uses only these immutable values afterward.
      const access = session.projectAccess;
      const head = session.state.head;
      const selection = editor.state.selection;
      const anchor = endpoint(selection.anchor);
      const focus = endpoint(selection.head);
      if (disposed) return;
      const saved = await saveViewState(access, head, anchor, focus);
      if (disposed) return;
      if (saved.documentId !== head.documentId || saved.head.version !== head.version || saved.head.bodyHash !== head.bodyHash
        || saved.anchor.blockId !== anchor.blockId || saved.anchor.utf16Offset !== anchor.utf16Offset || saved.focus.blockId !== focus.blockId || saved.focus.utf16Offset !== focus.utf16Offset) {
        throw new SessionError('ProtocolError', 'The saved reading position did not match this document.');
      }
    };
    session.setViewSaver(async () => {
      await discussionSaver.current?.();
      await saveReadingPosition();
    });
    const saved = active.viewState;
    if (saved && saved.head.documentId === record.head.documentId && saved.head.version === record.head.version && saved.head.bodyHash === record.head.bodyHash) {
      const locate = (endpoint: Endpoint): number | null => {
        let result: number | null = null;
        editor.state.doc.forEach((node, offset) => { if (node.attrs.id === endpoint.blockId && endpoint.utf16Offset <= node.content.size) result = offset + (node.isTextblock ? 1 + endpoint.utf16Offset : 0); }); return result;
      };
      const anchor = locate(saved.anchor); const focus = locate(saved.focus);
      if (anchor !== null && focus !== null) {
        const a = editor.state.doc.resolve(anchor); const f = editor.state.doc.resolve(focus);
        editor.view.dispatch(editor.state.tr.setSelection(a.parent.isTextblock && f.parent.isTextblock ? TextSelection.create(editor.state.doc, anchor, focus) : Selection.near(f)).scrollIntoView());
      }
    }
    saveLater();
    const compositionStart = () => { composing = true; clearTimeout(viewTimer); viewTimer = undefined; session.setComposing(true); };
    let settling: ReturnType<typeof setTimeout> | undefined;
    const compositionEnd = () => { settling = setTimeout(() => { if (editor.view.composing) compositionEnd(); else { composing = false; session.setComposing(false); scheduleViewSave(); } }, 30); };
    editor.view.dom.addEventListener('compositionstart', compositionStart);
    editor.view.dom.addEventListener('compositionend', compositionEnd);
    return () => { disposed = true; unsubscribe(); clearTimeout(settling); clearTimeout(viewTimer); session.setViewSaver(null); editor.off('transaction', update); editor.off('selectionUpdate', saveLater); editor.off('update', saveLater); editor.view.dom.removeEventListener('compositionstart', compositionStart); editor.view.dom.removeEventListener('compositionend', compositionEnd); editor.destroy(); };
  }, [editor, session]);
  async function resolve(choice: 'keepLocal' | 'useSaved') {
    try {
      const body = await session.resolveConflict(choice);
      if (choice === 'useSaved') {
        const content = editor.schema.nodeFromJSON(body.body);
        editor.view.dispatch(editor.state.tr.replaceWith(0, editor.state.doc.content.size, content.content).setMeta('authorConflictChoice', true).setMeta('addToHistory', false));
      }
    } catch (error) { onError((error as Error).message); }
  }
  const saving = state.phase === 'applying' ? 'Applying change…' : state.phase === 'reconciling' ? 'Checking saved version…' : state.phase === 'conflict' ? 'Choose which version to keep' : state.phase === 'saveFailed' ? "Couldn't save" : state.dirty || state.saving ? 'Saving…' : 'Saved';
  const empty = !editor.getText().trim();
  const draftingLabel = record.kind === 'chapter' ? empty ? 'Draft chapter' : 'Continue chapter' : record.kind === 'character' ? 'Develop character' : record.kind === 'world' ? 'Develop world' : 'Develop idea';
  return <><main className="writing" aria-label="Writing desk">
    <div className="document-heading"><div className="document-title-group">{navigation && <div className="chapter-navigation"><button aria-label="Previous chapter" disabled={navigation.disabled || !navigation.previous} onClick={navigation.previous}>Previous</button><span>Chapter {navigation.index + 1} of {navigation.total}</span><button aria-label="Next chapter" disabled={navigation.disabled || !navigation.next} onClick={navigation.next}>Next</button></div>}<h1>{record.title}</h1></div><span className="save-status" role="status" aria-live="polite">{saving}</span></div>
    <div className="document-tools" role="toolbar" aria-label="Document actions"><button className="primary-button draft-action" aria-label="Start AI draft" disabled={!state.editable} onClick={() => startAssistant(record.kind === 'chapter' ? 'draft' : 'develop')}>{draftingLabel}</button><button disabled={!state.editable} onClick={onRename}>Rename document</button><button ref={historyButton} aria-pressed={historyVisible} disabled={!state.editable} onClick={() => { if (historyVisible) setHistoryVisible(false); else void openHistory(); }}>History</button>
      {record.kind === 'chapter' && <button ref={reviewButton} aria-pressed={reviewVisible} disabled={!state.editable} onClick={() => { setHistoryVisible(false); setMemoryVisible(false); setReviewVisible(value => !value); }}>Story review</button>}
      {record.kind === 'chapter' && <button ref={memoryButton} aria-pressed={memoryVisible} disabled={!state.editable} onClick={() => { setHistoryVisible(false); setReviewVisible(false); setDiscussionVisible(false); setMemoryVisible(value => !value); }}>Story memory</button>}
      {conversation ? <button disabled={!state.editable} onClick={() => startAssistant('discuss')}>Discuss in project chat</button> : <button aria-label="Discussion" aria-pressed={discussionVisible && !historyVisible && !reviewVisible && !memoryVisible} disabled={(historyVisible || reviewVisible || memoryVisible) && !state.editable} onClick={() => { setHistoryVisible(false); setReviewVisible(false); setMemoryVisible(false); setDiscussionVisible(value => historyVisible || reviewVisible || memoryVisible || !value); }}>Writing assistant</button>}
      {conversation && record.kind === 'chapter' && <button disabled={!state.editable} onClick={() => startAssistant('revise')}>Suggest chapter changes</button>}</div>
    {(record.kind === 'character' || record.kind === 'world') && <div className="document-names">
      <button ref={namesButton} aria-expanded={namesVisible} disabled={!state.editable} onClick={() => setNamesVisible(true)}>Names & aliases</button>
      <DocumentAliases access={session.projectAccess} documentId={state.head.documentId} title={record.title} visible={namesVisible} disabled={!state.editable} registerGuard={registerNamesGuard} beforeSave={flushBeforeNames} onClose={() => { setNamesVisible(false); namesButton.current?.focus(); }} />
    </div>}
    <div className="formatbar" role="toolbar" aria-label="Manuscript formatting">
      <select aria-label="Paragraph style" disabled={!state.editable} value={editor.isActive('heading') ? `h${editor.getAttributes('heading').level}` : 'p'} onChange={event => { if (event.target.value === 'p') editor.chain().focus().setNode('paragraph').run(); else editor.chain().focus().setNode('heading', { level: Number(event.target.value.slice(1)) }).run(); }}><option value="p">Paragraph</option><option value="h1">Heading 1</option><option value="h2">Heading 2</option><option value="h3">Heading 3</option></select>
      <button className="format-button bold" aria-label="Bold" aria-pressed={editor.isActive('bold')} disabled={!state.editable} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('bold').run()}>B</button>
      <button className="format-button italic" aria-label="Italic" aria-pressed={editor.isActive('italic')} disabled={!state.editable} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('italic').run()}>I</button>
      <button disabled={!state.editable} onClick={() => editor.chain().focus().insertContent({ type: 'sceneBreak', attrs: { id: crypto.randomUUID() } }).run()}>Scene break</button>
      <button disabled={!state.editable || !undoDepth(editor.state)} onClick={() => { undo(editor.state, tr => editor.view.dispatch(tr.setMeta('saveCause', 'undo'))); editor.commands.focus(); }}>Undo</button>
      <button disabled={!state.editable || !redoDepth(editor.state)} onClick={() => { redo(editor.state, tr => editor.view.dispatch(tr.setMeta('saveCause', 'redo'))); editor.commands.focus(); }}>Redo</button>
      <button className="selection-action" disabled={!state.editable || editor.state.selection.empty} onMouseDown={event => event.preventDefault()} onClick={() => discuss.current()}>Discuss selection</button>
    </div>
    {state.error && <section className="save-error" role="alert"><p>{state.error}</p><div className="header-actions">{state.phase === 'saveFailed' && <button onClick={() => void session.flush().catch(error => onError(error.message))}>Retry save</button>}{state.phase === 'reconciling' && <button onClick={() => void session.reconcile().catch(error => onError(error.message))}>Check saved version</button>}<button onClick={() => void navigator.clipboard.writeText(editor.getText()).catch(() => onError('Could not copy. Select and copy your text from the editor.'))}>Copy my text</button></div>
      {state.phase === 'conflict' && <><h2>Saved version</h2><div className="saved-version-preview">{session.savedConflict?.body.body.content.map(block => block.type === 'sceneBreak' ? <hr key={block.attrs.id} /> : <p key={block.attrs.id}>{block.content?.map((inline, index) => inline.type === 'hardBreak' ? <br key={index} /> : <span key={index} style={{ fontWeight: inline.marks?.some(mark => mark.type === 'bold') ? 700 : undefined, fontStyle: inline.marks?.some(mark => mark.type === 'italic') ? 'italic' : undefined }}>{inline.text}</span>)}</p>)}</div><p>Your local version is still in the editor.</p><button onClick={() => void resolve('keepLocal')}>Keep my local version</button><button onClick={() => void resolve('useSaved')}>Use the saved version</button></>}
      <RecoveryCopy capture={() => snapshotFromEditor(editor.getJSON())} />
    </section>}
    <div className="manuscript-scroll" onContextMenu={event => { if (!editor.state.selection.empty && state.editable) { event.preventDefault(); setMenu({ x: Math.min(event.clientX, window.innerWidth - 270), y: Math.min(event.clientY, window.innerHeight - 60) }); } }} onPaste={event => { if (/<(?:table|img|ul|ol|pre|video|iframe|script|blockquote|code|s|strike|del|u|sub|sup|h[4-6])\b/iu.test(event.clipboardData.getData('text/html'))) setPasteNotice('Pasted text with supported formatting. Other formatting or embedded content was omitted.'); }}><div className="manuscript-page">{empty && <div className="draft-start"><h2>{record.kind === 'chapter' ? 'What should happen in this chapter?' : 'What would you like to create?'}</h2><p>{record.kind === 'chapter' ? 'Describe the scene, the emotional turn, or the ending you have in mind. Your assistant will draft a version for you to review.' : 'Give your assistant a starting point. Develop a proposal together, then apply the version you want to keep.'}</p><button className="primary-button" disabled={!state.editable} onClick={() => startAssistant(record.kind === 'chapter' ? 'draft' : 'develop')}>{record.kind === 'chapter' ? 'Describe this chapter' : 'Describe your idea'}</button><span>Or write directly below.</span></div>}<Manuscript editor={editor} /></div></div>
    <footer className="writing-status"><span>{pasteNotice || 'Writing on this computer'}</span><span>Local manuscript</span></footer>
  </main>
  {!conversation && <FeedbackPanel session={session} state={state} title={record.title} documentKind={record.kind} sources={sources} selection={discussionSelection} assistantAction={assistantAction} visible={discussionVisible && !historyVisible && !reviewVisible && !memoryVisible} onClose={() => setDiscussionVisible(false)} registerSaver={registerDiscussionSaver} onPrepareProposal={prepare} onApplyProposal={apply} />}
  {conversation?.reviewRunId && <section className="chat-chapter-proposals" aria-label="Chapter suggestions"><h2>Chapter suggestions</h2>
    {proposedRange && !rangeConfirmed && <ChapterRangeReview range={proposedRange} busy={rangeBusy} stale={state.dirty || canonicalJson(state.head) !== canonicalJson(proposedRange.target)} error={(rangeError || chapterFeedback?.rangeError) ?? undefined} onConfirm={() => void confirmSuggestedRange()} />}
    {rangeConfirmed && <p role="status">Passage confirmed in the composer. Describe your change and send the edit request when ready.</p>}
    {!proposedRange && (rangeError || chapterFeedback?.rangeError) && <p role="status">{rangeError || chapterFeedback?.rangeError} The feedback remains in your conversation; select a current passage to request an edit.</p>}
    <ProposalPanel access={session.projectAccess} proposals={chatProposals.filter(proposal => proposal.runId === conversation.reviewRunId)} disabled={!state.editable} onPrepareProposal={prepare} onApplyProposal={async (proposal, prepared) => { await apply(proposal, prepared); await refreshChatProposals(); }} onRefresh={refreshChatProposals} />{!proposedRange && !chatProposals.some(proposal => proposal.runId === conversation.reviewRunId) && <p>This response has no saved chapter edit proposals.</p>}</section>}
  <HistoryPanel access={session.projectAccess} documentId={state.head.documentId} body={session.body} visible={historyVisible} disabled={!state.editable} onClose={() => { setHistoryVisible(false); historyButton.current?.focus(); }} onRestore={restore} />
  {record.kind === 'chapter' && <ReviewPanel session={session} state={state} visible={reviewVisible && !memoryVisible} captureSelection={captureReviewSelection} onClose={() => { setReviewVisible(false); reviewButton.current?.focus(); }} />}
  {record.kind === 'chapter' && <ChapterMemory session={session} state={state} title={record.title} visible={memoryVisible} onClose={() => { setMemoryVisible(false); memoryButton.current?.focus(); }} />}
  {menu && <><div className="menu-dismiss" onClick={() => setMenu(null)} /><div className="selection-menu" role="menu" aria-label="Selected passage" style={{ left: menu.x, top: menu.y }} onKeyDown={event => { if (event.key === 'Escape') { setMenu(null); editor.commands.focus(); } }}><button role="menuitem" autoFocus onMouseDown={event => event.preventDefault()} onClick={() => discuss.current()}>Discuss selection · Ctrl+Shift+F</button></div></>}
  </>;
}
