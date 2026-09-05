import { memo, useEffect, useRef, useState } from 'react';
import { Editor, Extension } from '@tiptap/core';
import { EditorContent } from '@tiptap/react';
import { Plugin, TextSelection } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { closeHistory, redo, undo, undoDepth, redoDepth } from '@tiptap/pm/history';
import { sample, snapshotFromEditor } from '../editor/document';
import { editorExtensions } from '../editor/schema';
import { captureSelection, generation, prepareReplacement, type Scope } from '../editor/selection';
import { runtimeInfo, validateSnapshot, type RuntimeInfo } from '../ipc/native';

const Manuscript = memo(({ editor }: { editor: Editor }) => <EditorContent editor={editor} />);
interface Note { id: number; text: string; quote?: string }
interface Preview { text: string; scope: Scope }

export function App() {
  const barrier = useRef(false);
  const scopeRef = useRef<Scope | null>(null);
  const feedbackRef = useRef<HTMLTextAreaElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const openSelectionRef = useRef<() => void>(() => {});
  const [editor] = useState(() => new Editor({
    extensions: [...editorExtensions, Extension.create({
      name: 'trialInteraction',
      addProseMirrorPlugins: () => [new Plugin({
        filterTransaction: tr => !barrier.current || !tr.docChanged || tr.getMeta('localTrialApply') === true,
        props: {
          decorations: state => {
            const selected = scopeRef.current;
            return selected && state.doc.eq(selected.source) ? DecorationSet.create(state.doc, [Decoration.inline(selected.from, selected.to, { class: 'feedback-selection' })]) : DecorationSet.empty;
          },
        },
      })],
      addKeyboardShortcuts: () => ({ 'Mod-Shift-f': () => { openSelectionRef.current(); return true; } }),
    })],
    content: structuredClone(sample),
    editorProps: {
      attributes: { 'aria-label': 'Chapter manuscript', role: 'textbox', 'aria-multiline': 'true', spellcheck: 'true' },
      handleClick: (_view, _pos, event) => {
        if ((event.target as HTMLElement).closest('a')) { event.preventDefault(); return true; }
        return false;
      },
    },
  }));
  const [revision, setRevision] = useState(0);
  const [selectionAvailable, setSelectionAvailable] = useState(false);
  const [scope, setScope] = useState<Scope | null>(null);
  const [note, setNote] = useState('');
  const [notes, setNotes] = useState<Note[]>([]);
  const [replacement, setReplacement] = useState('');
  const [replacementOpen, setReplacementOpen] = useState(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('Nothing is saved in this trial.');
  const [error, setError] = useState('');
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const stale = !!scope && scope.generation !== generation(editor);

  useEffect(() => {
    const update = () => { setRevision(value => value + 1); setSelectionAvailable(!editor.state.selection.empty); };
    editor.on('transaction', update);
    void runtimeInfo().then(setRuntime).catch(reason => setStatus(String(reason.message)));
    return () => { editor.off('transaction', update); editor.destroy(); };
  }, [editor]);

  useEffect(() => {
    if (menu) menuRef.current?.querySelector<HTMLButtonElement>('button')?.focus();
  }, [menu]);

  function chooseScope(next: Scope | null) {
    scopeRef.current = next;
    setScope(next);
    setPreview(null);
    setReplacement('');
    setReplacementOpen(false);
    setMenu(null);
    setError('');
    editor.view.dispatch(editor.state.tr.setMeta('scopeHighlight', true));
    requestAnimationFrame(() => feedbackRef.current?.focus());
  }

  function openSelection() {
    if (editor.view.composing || barrier.current) return;
    const selected = captureSelection(editor);
    if (!selected) { setError('Select words or paragraphs in the chapter first.'); return; }
    chooseScope(selected);
  }
  openSelectionRef.current = openSelection;

  async function checkSnapshot() {
    if (busy || editor.view.composing) return;
    setBusy(true); setError('');
    const capturedGeneration = generation(editor);
    try {
      const receipt = await validateSnapshot(snapshotFromEditor(editor.getJSON()));
      const newer = capturedGeneration !== generation(editor) ? ' You have newer edits.' : '';
      setStatus(`Rust checked ${receipt.blockCount} blocks. Fingerprints match. This is not a save.${newer}`);
    } catch (reason) { setError((reason as Error).message); }
    finally { setBusy(false); }
  }

  async function applyPreview() {
    if (!preview || busy || barrier.current) return;
    if (editor.view.composing) { setError('Finish composing your text, then apply.'); return; }
    barrier.current = true; setBusy(true); setError('');
    editor.setEditable(false, false);
    try {
      const tr = prepareReplacement(editor, preview.scope, preview.text);
      const prepared = editor.state.apply(tr);
      await validateSnapshot(snapshotFromEditor(prepared.doc.toJSON()));
      // W0 only: Rust validates but does not persist. Durable Apply belongs to W6.
      if (generation(editor) !== preview.scope.generation) throw new Error('The chapter changed while checking. Select the passage again.');
      editor.view.dispatch(tr);
      editor.view.dispatch(closeHistory(editor.state.tr));
      scopeRef.current = null; setScope(null); setPreview(null); setReplacementOpen(false); setReplacement('');
      setStatus('Replacement applied to this session. Undo restores the original.');
    } catch (reason) { setError((reason as Error).message); }
    finally {
      barrier.current = false; editor.setEditable(true, false); setBusy(false); editor.commands.focus();
    }
  }

  function keepFeedback(event: React.FormEvent) {
    event.preventDefault();
    if (!note.trim() || busy) return;
    setNotes(current => [...current, { id: Date.now(), text: note.trim(), quote: scope?.quote }]);
    setNote('');
    setStatus('Feedback kept for this session. No model request was made.');
  }

  function previewReplacement() {
    if (!scope) return;
    try { prepareReplacement(editor, scope, replacement); setPreview({ scope, text: replacement }); setError(''); }
    catch (reason) { setError((reason as Error).message); }
  }

  function showMenu(event: React.MouseEvent) {
    if (editor.state.selection.empty || editor.view.composing) return;
    event.preventDefault();
    setMenu({ x: Math.min(event.clientX, window.innerWidth - 268), y: Math.min(event.clientY, window.innerHeight - 120) });
  }

  return <div className="app" onKeyDown={event => {
    if (event.key === 'Escape') {
      setMenu(null);
      if (!barrier.current) {
        if (scope && !stale) editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, scope.from, scope.to)));
        editor.commands.focus();
      }
    }
  }}>
    <header className="app-header">
      <div className="brand"><svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 5h6l2 2 2-2h6v15h-6l-2 2-2-2H4V5Z M12 7v15 M7 9h2 M15 9h2 M7 13h2 M15 13h2" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" /></svg><strong>WebnovelStudio</strong><span className="trial-label">Editor trial</span></div>
      <span className="session-notice">Sample text · session only</span>
    </header>
    <div className="workspace">
      <main className="writing" aria-label="Writing desk">
        <div className="document-heading"><div><span className="chapter-label">Sample chapter</span><h1>The lantern keeper</h1></div></div>
        <div className="formatbar" role="toolbar" aria-label="Manuscript formatting">
          <label className="sr-only" htmlFor="block-style">Paragraph style</label>
          <select id="block-style" aria-label="Paragraph style" disabled={busy} value={editor.isActive('heading') ? `h${editor.getAttributes('heading').level}` : 'p'} onChange={event => {
            if (event.target.value === 'p') editor.chain().focus().setNode('paragraph').run();
            else editor.chain().focus().setNode('heading', { level: Number(event.target.value.slice(1)) }).run();
          }}><option value="p">Paragraph</option><option value="h1">Heading 1</option><option value="h2">Heading 2</option><option value="h3">Heading 3</option></select>
          <div className="format-group">
            <button className="format-button bold" aria-label="Bold" aria-pressed={editor.isActive('bold')} title="Bold (Ctrl+B)" disabled={busy} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('bold').run()}>B</button>
            <button className="format-button italic" aria-label="Italic" aria-pressed={editor.isActive('italic')} title="Italic (Ctrl+I)" disabled={busy} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('italic').run()}>I</button>
          </div>
          <button className="quiet-button" disabled={busy} onClick={() => editor.chain().focus().insertContent({ type: 'sceneBreak', attrs: { id: crypto.randomUUID() } }).run()}>Scene break</button>
          <div className="history-controls">
            <button className="quiet-button" aria-label="Undo" disabled={busy || !undoDepth(editor.state)} onClick={() => { undo(editor.state, editor.view.dispatch); editor.commands.focus(); }}>Undo</button>
            <button className="quiet-button" aria-label="Redo" disabled={busy || !redoDepth(editor.state)} onClick={() => { redo(editor.state, editor.view.dispatch); editor.commands.focus(); }}>Redo</button>
          </div>
          <button className="selection-action" disabled={busy || !selectionAvailable} title="Give feedback on selection (Ctrl+Shift+F)" onMouseDown={event => event.preventDefault()} onClick={openSelection}>Selection feedback</button>
        </div>
        <div className="manuscript-scroll" onContextMenu={showMenu} onPaste={event => {
          const html = event.clipboardData.getData('text/html');
          if (/<(?:table|img|ul|ol|pre|video|iframe|script|blockquote|code|s|strike|del|u|sub|sup|h[4-6])\b/iu.test(html)) setStatus('Pasted supported text and formatting. Unsupported formatting or embedded content was removed in this trial.');
        }}>
          <div className="manuscript-page"><Manuscript editor={editor} /></div>
        </div>
        <footer className="writing-status"><span>Manuscript · not saved</span><span>Local editing · no provider connected</span></footer>
      </main>
      <aside className="feedback" aria-labelledby="feedback-title">
        <div className="feedback-heading"><h2 id="feedback-title">Feedback</h2><span className="session-tag">This session</span></div>
        <p className="panel-intro">Keep a thought about the chapter, or select a passage to work on it.</p>
        <div className="scope-controls" aria-label="Feedback scope">
          <button className={!scope ? 'scope-button active' : 'scope-button'} aria-pressed={!scope} disabled={busy} onClick={() => chooseScope(null)}>Whole chapter</button>
          <button className={scope ? 'scope-button active' : 'scope-button'} aria-pressed={!!scope} disabled={busy || !selectionAvailable} onMouseDown={event => event.preventDefault()} onClick={openSelection}>Selected passage</button>
        </div>
        <div className="feedback-scroll">
          {scope && <section className="quoted-scope" aria-label="Captured selection"><div className="scope-title"><strong>Selected passage</strong><span>{scope.inlineOnly ? 'Within a paragraph' : 'Across paragraphs'}</span></div><blockquote>{scope.quote}</blockquote>{stale && <p className="stale-notice">The chapter changed. This quotation is kept as a reference. Select again to try a replacement.</p>}</section>}
          {!notes.length && !scope && <div className="feedback-empty"><p>What should feel different?</p><span>Voice, pacing, a missing detail — start with what you noticed.</span></div>}
          {notes.map(item => <article className="feedback-note" key={item.id}><div>You <span>{item.quote ? 'Selected passage' : 'Whole chapter'}</span></div>{item.quote && <blockquote>{item.quote}</blockquote>}<p>{item.text}</p></article>)}
          <form onSubmit={keepFeedback} className="feedback-form">
            <label htmlFor="feedback-input">{scope ? 'Your feedback on this passage' : 'Your feedback on the chapter'}</label>
            <textarea id="feedback-input" ref={feedbackRef} rows={4} value={note} disabled={busy} onChange={event => setNote(event.target.value)} placeholder={scope ? 'What would you change here?' : 'For example: bring Mei’s hesitation into the opening.'} />
            <div className="form-actions"><span>No AI response in this trial</span><button className="primary-button" disabled={!note.trim() || busy}>Keep feedback</button></div>
          </form>
          {scope && <section className="replacement-section" aria-label="Local replacement trial">
            {!replacementOpen ? <button className="text-button" disabled={busy || stale || !scope.replacementAllowed} onClick={() => setReplacementOpen(true)}>Try your own replacement</button> : <>
              <h3>Try a replacement</h3><p className="small-copy">Write the new text, then review it before applying. This exercises editing; no model writes it.</p>
              <label htmlFor="replacement-input">Replacement text</label>
              <textarea id="replacement-input" rows={4} value={replacement} disabled={busy || stale} onChange={event => { setReplacement(event.target.value); setPreview(null); }} />
              <p className="small-copy">{scope.formattingNote}{!scope.inlineOnly && ' Applying joins the selected paragraphs.'}</p>
              {!preview && <button className="secondary-button" disabled={busy || stale} onClick={previewReplacement}>Preview replacement</button>}
            </>}
            {!scope.replacementAllowed && <p className="small-copy">Feedback is available here. Replacement across scene breaks or different paragraph styles is outside this trial.</p>}
            {preview && <div className="replacement-preview"><h3>Review the change</h3><span className="preview-label">Before</span><blockquote>{preview.scope.quote}</blockquote><span className="preview-label">After</span><blockquote className="after-text">{preview.text || <em>Remove the selected text</em>}</blockquote><div className="preview-actions"><button className="secondary-button" disabled={busy} onClick={() => { setPreview(null); setStatus('Replacement rejected. The manuscript is unchanged.'); }}>Reject</button><button className="primary-button" disabled={busy || stale} onClick={() => void applyPreview()}>{busy ? 'Checking…' : 'Apply replacement'}</button></div></div>}
          </section>}
        </div>
        <div className="trial-footnote">Closing this window clears the manuscript and feedback. Use sample text only.</div>
      </aside>
    </div>
    <footer className="app-status"><div aria-live="polite" role={error ? 'alert' : 'status'} className={error ? 'error-status' : ''}>{error || status}</div><button className="quiet-button" disabled={busy} onClick={() => void checkSnapshot()}>{busy ? 'Checking…' : 'Check with Rust'}</button><span className="runtime-info" title={runtime ? `WebView2 ${runtime.webviewVersion}` : 'Native runtime unavailable'}>{runtime ? `Tauri · WebView2 ${runtime.webviewVersion}` : 'Desktop connection pending'}</span></footer>
    {menu && <><div className="menu-dismiss" onMouseDown={() => setMenu(null)} /><div ref={menuRef} className="selection-menu" style={{ left: menu.x, top: menu.y }} role="menu" aria-label="Selection actions"><button role="menuitem" onClick={openSelection}>Give feedback on selection</button><button role="menuitem" onClick={() => { setMenu(null); editor.commands.focus(); }}>Keep editing</button></div></>}
  </div>;
}
