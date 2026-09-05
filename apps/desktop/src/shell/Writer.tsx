import { memo, useCallback, useEffect, useRef, useState } from 'react';
import { Editor, Extension } from '@tiptap/core';
import { EditorContent } from '@tiptap/react';
import { EditorState, Plugin, Selection, TextSelection, type Transaction } from '@tiptap/pm/state';
import { closeHistory, redo, redoDepth, undo, undoDepth } from '@tiptap/pm/history';
import { editorExtensions } from '../editor/schema';
import { snapshotFromEditor } from '../editor/document';
import { DocumentSession, SessionError, type PreparedEditorChange } from '../editor/session';
import { saveViewState, type DocumentRecord, type Endpoint, type Revision, type ViewState } from '../ipc/projects';
import { captureSelection, prepareScopedReplacement, type Scope } from '../editor/selection';
import { prepareProposal, type PreparedProposal, type Proposal } from '../ipc/proposals';
import { FeedbackPanel } from '../assistant/FeedbackPanel';
import { HistoryPanel } from './HistoryPanel';
import type { SourceChoice } from '../ipc/sourcePins';

const Manuscript = memo(({ editor }: { editor: Editor }) => <EditorContent editor={editor} />);
export function Writer({ active, sources, onError, onRename }: { active: { record: DocumentRecord; session: DocumentSession; viewState: ViewState | null }; sources: SourceChoice[]; onError: (message: string) => void; onRename: () => void }) {
  const { session, record } = active;
  const [state, setState] = useState(session.state);
  const [, redraw] = useState(0);
  const [pasteNotice, setPasteNotice] = useState('');
  const [discussionVisible, setDiscussionVisible] = useState(true);
  const [historyVisible, setHistoryVisible] = useState(false);
  const historyButton = useRef<HTMLButtonElement>(null);
  const [discussionSelection, setDiscussionSelection] = useState<{ scope: Scope; nonce: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const discussionSaver = useRef<(() => Promise<void>) | null>(null);
  const registerDiscussionSaver = useCallback((save: (() => Promise<void>) | null) => { discussionSaver.current = save; }, []);
  const discuss = useRef<() => boolean>(() => false);
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
  const prepare = async (proposal: Proposal, text: string, operationId: string): Promise<PreparedProposal> => {
    let source: EditorState; let tr;
    try {
      source = EditorState.create({ schema: editor.schema, doc: editor.schema.nodeFromJSON(proposal.sourceBody.body) });
      tr = prepareScopedReplacement(source, proposal.scope, text);
    } catch (error) { throw new SessionError('InvalidProposal', (error as Error).message); }
    return prepareProposal({ access: session.projectAccess, operationId, proposalId: proposal.id, expectedPreparedVersion: proposal.prepared?.version ?? '0',
      replacementText: text, body: snapshotFromEditor(tr.doc.toJSON()) });
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
    await session.applyPrepared(proposal, prepared, () => prepareChange(prepareScopedReplacement(editor.state, proposal.scope, prepared.replacementText)));
  };
  const restore = async (revision: Revision): Promise<void> => {
    await session.restoreRevision(revision, () => {
      const content = editor.schema.nodeFromJSON(revision.body.body);
      return prepareChange(closeHistory(editor.state.tr).replaceWith(0, editor.state.doc.content.size, content.content));
    });
  };
  async function openHistory() {
    try { await session.checkpoint('manual'); setHistoryVisible(true); }
    catch (error) { onError((error as Error).message); }
  }
  discuss.current = () => {
    const scope = captureSelection(editor);
    if (!scope || !session.state.editable) return false;
    setDiscussionSelection(previous => ({ scope, nonce: (previous?.nonce ?? 0) + 1 }));
    setHistoryVisible(false); setDiscussionVisible(true); setMenu(null); return true;
  };
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
  return <><main className="writing" aria-label="Writing desk">
    <div className="document-heading"><h1>{record.title}</h1><button disabled={!state.editable} onClick={onRename}>Rename document</button><button ref={historyButton} aria-pressed={historyVisible} disabled={!state.editable} onClick={() => { if (historyVisible) setHistoryVisible(false); else void openHistory(); }}>History</button><button aria-pressed={discussionVisible && !historyVisible} disabled={historyVisible && !state.editable} onClick={() => { setHistoryVisible(false); setDiscussionVisible(value => historyVisible || !value); }}>Discussion</button><span className="save-status" role="status" aria-live="polite">{saving}</span></div>
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
    </section>}
    <div className="manuscript-scroll" onContextMenu={event => { if (!editor.state.selection.empty && state.editable) { event.preventDefault(); setMenu({ x: Math.min(event.clientX, window.innerWidth - 270), y: Math.min(event.clientY, window.innerHeight - 60) }); } }} onPaste={event => { if (/<(?:table|img|ul|ol|pre|video|iframe|script|blockquote|code|s|strike|del|u|sub|sup|h[4-6])\b/iu.test(event.clipboardData.getData('text/html'))) setPasteNotice('Pasted text with supported formatting. Other formatting or embedded content was omitted.'); }}><div className="manuscript-page"><Manuscript editor={editor} /></div></div>
    <footer className="writing-status"><span>{pasteNotice || 'Writing on this computer'}</span><span>Offline writing</span></footer>
  </main>
  <FeedbackPanel session={session} state={state} title={record.title} documentKind={record.kind} sources={sources} selection={discussionSelection} visible={discussionVisible && !historyVisible} onClose={() => setDiscussionVisible(false)} registerSaver={registerDiscussionSaver} onPrepareProposal={prepare} onApplyProposal={apply} />
  <HistoryPanel access={session.projectAccess} documentId={state.head.documentId} body={session.body} visible={historyVisible} disabled={!state.editable} onClose={() => { setHistoryVisible(false); historyButton.current?.focus(); }} onRestore={restore} />
  {menu && <><div className="menu-dismiss" onClick={() => setMenu(null)} /><div className="selection-menu" role="menu" aria-label="Selected passage" style={{ left: menu.x, top: menu.y }} onKeyDown={event => { if (event.key === 'Escape') { setMenu(null); editor.commands.focus(); } }}><button role="menuitem" autoFocus onMouseDown={event => event.preventDefault()} onClick={() => discuss.current()}>Discuss selection · Ctrl+Shift+F</button></div></>}
  </>;
}
