import { createElement, forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { Editor, Extension } from '@tiptap/core';
import { EditorContent } from '@tiptap/react';
import { Plugin } from '@tiptap/pm/state';
import { editorExtensions } from '../editor';
import { snapshotFromEditor, type Inline, type WnsDocument } from '../editor';
import { DocumentSession, type SessionState } from '../editor';
import { validateSnapshot } from '../ipc/native';
import { checkpointAssistantDraft, reconcileAssistantDraft, saveAssistantDraft, type AssistantDraft } from '../ipc/projectChat';
import type { DocumentRecord, Head, OpenedProject, ProjectTransport } from '../ipc/projects';
import type { ChatAdoptionPreview, ProjectChatDraftRef } from '../ipc/projectChat';
import { ContextInspector } from '../assistant';
import { chatViewPreferenceKey, readChatViewPreferences, writeChatViewPreferences } from './viewPreferences';
import { DraftReviewDiff } from './DraftReviewDiff';
import { ChatAdoptionEffects } from './ChatAdoptionEffects';
import type { DraftReviewContext } from './useDraftReviewContext';
import './CoauthorReview.css';
export type { DraftReviewContext } from './useDraftReviewContext';

function inlineText(inline: Inline): string { return inline.type === 'hardBreak' ? '\n' : inline.text; }
export function plainText(document: WnsDocument): string {
  return document.body.content.map(block => block.type === 'sceneBreak' ? '—' : (block.content ?? []).map(inlineText).join('')).join('\n\n');
}
export interface DraftReviewPanelProps {
  project: OpenedProject;
  drafts: AssistantDraft[];
  preview?: ChatAdoptionPreview | null;
  previewVerified?: boolean;
  previewError?: string;
  onPrepareAdoption(drafts: AssistantDraft[]): Promise<void> | void;
  onApplyPreview(preview: ChatAdoptionPreview): Promise<void> | void;
  onReject(draft: AssistantDraft): Promise<void> | void;
  onRevise?(draft: AssistantDraft): Promise<void> | void;
  onReconsider?(draft: AssistantDraft): Promise<void> | void;
  onOpenDocument?(document: DocumentRecord): Promise<void> | void;
  onOpenDraft?(draft: AssistantDraft): Promise<void> | void;
  onDraftSaved?(draft: AssistantDraft, head: Head): Promise<void> | void;
  viewKey?: string;
  onAccessChanged?(access: import('../ipc/projects').ProjectAccess): Promise<void> | void;
  /** Exact request material persisted with the originating conversation run. */
  reviewContext?: Readonly<Record<string, DraftReviewContext>>;
  /** Current target documents loaded by the parent for a stale immutable preview. */
  staleComparison?: StalePreviewComparison | null;
  /** Load current target documents/heads for the retained stale preview. */
  onCompareStalePreview?(preview: ChatAdoptionPreview): Promise<StalePreviewComparison>;
  /** Explicitly prepare a new preview against the loaded current target versions. */
  onPrepareAgainstCurrent?(drafts: AssistantDraft[], comparison: StalePreviewComparison): Promise<void> | void;
}

export interface StalePreviewComparison {
  previewId: string;
  targets: Array<{ documentId: string; current: DocumentRecord | null }>;
  /** Optional broad invalidation markers supplied by the parent reader. */
  sourceEpoch?: string;
  currentSourceEpoch?: string;
  policyEpoch?: string;
  currentPolicyEpoch?: string;
}

export interface DraftReviewPanelHandle { flush(): Promise<void>; currentRefs(documentIds?: string[]): ProjectChatDraftRef[]; openDraft(documentId: string): Promise<void> }

function DraftEditor({ draft, project, onSaved, onSession, onAccessChanged, autoFocus = false }: { draft: AssistantDraft; project: OpenedProject; onSaved?: (draft: AssistantDraft, head: Head) => Promise<void> | void; onSession(session: DocumentSession | null): void; onAccessChanged?: (access: import('../ipc/projects').ProjectAccess) => Promise<void> | void; autoFocus?: boolean }) {
  const transport = useMemo<ProjectTransport>(() => ({
    validate: async body => { await validateSnapshot(body); },
    save: request => saveAssistantDraft(draft.conversationId, draft.dispositionVersion, request),
    reconcile: request => reconcileAssistantDraft(draft.conversationId, request),
    checkpoint: request => checkpointAssistantDraft(draft.conversationId, request),
  }), [draft.conversationId, draft.dispositionVersion]);
  const [session] = useState(() => new DocumentSession(project.access, draft.document, transport));
  const onSessionRef = useRef(onSession);
  useEffect(() => { onSessionRef.current = onSession; }, [onSession]);
  const [editor] = useState(() => new Editor({
    extensions: [...editorExtensions, Extension.create({
      name: 'draftEditingGuard', priority: 1000,
      addProseMirrorPlugins: () => [new Plugin({ filterTransaction: transaction => !transaction.docChanged || session.state.editable || transaction.getMeta('durableApply') === true })],
    })],
    content: draft.document.body.body,
    editorProps: { attributes: { role: 'textbox', 'aria-label': `Edit ${draft.document.title}`, 'aria-multiline': 'true', spellcheck: 'true' } },
    onUpdate: ({ editor: current }) => { session.update(snapshotFromEditor(current.getJSON())); },
  }));
  const [state, setState] = useState<SessionState>(session.state);
  const [editorError, setEditorError] = useState('');
  const lastSavedVersion = useRef(draft.document.head.version);
  useEffect(() => {
    onSessionRef.current(session);
    const off = session.subscribe(() => setState(session.state));
    let compositionTimer: ReturnType<typeof setTimeout> | undefined;
    const compositionStart = () => { if (compositionTimer) clearTimeout(compositionTimer); session.setComposing(true); };
    const compositionEnd = () => {
      const settle = () => {
        if (editor.view.composing) { compositionTimer = setTimeout(settle, 30); return; }
        session.setComposing(false);
      };
      compositionTimer = setTimeout(settle, 30);
    };
    editor.view.dom.addEventListener('compositionstart', compositionStart);
    editor.view.dom.addEventListener('compositionend', compositionEnd);
    return () => { off(); onSessionRef.current(null); if (compositionTimer) clearTimeout(compositionTimer); editor.view.dom.removeEventListener('compositionstart', compositionStart); editor.view.dom.removeEventListener('compositionend', compositionEnd); editor.destroy(); };
  }, [editor, session]);
  useEffect(() => { editor.setEditable(state.editable); }, [editor, state.editable]);
  useEffect(() => { if (autoFocus) { editor.commands.focus('start'); editor.view.dom.focus(); } }, [autoFocus, editor]);
  useEffect(() => {
    if (!state.dirty && state.head.version !== lastSavedVersion.current) {
      lastSavedVersion.current = state.head.version;
      void Promise.resolve(onSaved?.(draft, state.head)).catch(reason => setEditorError(reason instanceof Error ? reason.message : 'The saved draft changed, but the review list could not refresh.'));
    }
  }, [draft, onSaved, state]);
  const save = async () => { setEditorError(''); try { await session.flush(); await onSaved?.(draft, session.state.head); } catch (reason) { setEditorError(reason instanceof Error ? reason.message : 'The draft could not be saved.'); } };
  const checkpoint = async () => { setEditorError(''); try { await session.checkpoint('manual'); await onSaved?.(draft, session.state.head); } catch (reason) { setEditorError(reason instanceof Error ? reason.message : 'The draft checkpoint could not be saved.'); } };
  const reconcile = async () => {
    setEditorError('');
    const previousAccess = session.projectAccess;
    const notifyLease = async () => {
      const nextAccess = session.projectAccess;
      if (JSON.stringify(previousAccess) !== JSON.stringify(nextAccess)) await onAccessChanged?.(nextAccess);
    };
    try {
      await session.reconcile();
    } catch (reason) {
      // Reconcile can rotate the writer lease before a later consistency check
      // fails. Propagate that new access even when the visible recovery action
      // still reports the error and keeps the local buffer untouched.
      try { await notifyLease(); } catch { /* preserve the original recovery error */ }
      setEditorError(reason instanceof Error ? reason.message : 'The saved draft could not be reconciled.');
      return;
    }
    try { await notifyLease(); await onSaved?.(draft, session.state.head); }
    catch (reason) { setEditorError(reason instanceof Error ? reason.message : 'The reconciled draft could not be refreshed.'); }
  };
  const copyLocal = async () => {
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard access is unavailable.');
      await navigator.clipboard.writeText(plainText(session.body));
      setEditorError('Local draft text copied.');
    } catch (reason) { setEditorError(reason instanceof Error ? reason.message : 'The local draft could not be copied.'); }
  };
  const resolveConflict = async (choice: 'keepLocal' | 'useSaved') => {
    setEditorError('');
    try {
      const body = await session.resolveConflict(choice);
      if (choice === 'useSaved') editor.commands.setContent(body.body, { emitUpdate: false });
      await onSaved?.(draft, session.state.head);
    } catch (reason) { setEditorError(reason instanceof Error ? reason.message : 'The draft conflict could not be resolved.'); }
  };
  return <div className="chat-draft-editor">
    <div className="chat-draft-editor-toolbar" role="toolbar" aria-label="Draft formatting">
      <button type="button" onClick={() => editor.chain().focus().toggleMark('bold').run()} disabled={!state.editable} aria-pressed={editor.isActive('bold')}>Bold</button>
      <button type="button" onClick={() => editor.chain().focus().toggleMark('italic').run()} disabled={!state.editable} aria-pressed={editor.isActive('italic')}>Italic</button>
      <button type="button" onClick={() => void save()} disabled={!state.editable || !state.dirty || state.saving}>{state.saving ? 'Saving…' : 'Save draft'}</button>
      <button type="button" onClick={() => void checkpoint()} disabled={!state.editable || state.saving}>Checkpoint</button>
      {state.phase === 'reconciling' && <button type="button" onClick={() => void reconcile()}>Check saved draft</button>}
      {(state.phase === 'reconciling' || state.phase === 'conflict') && <button type="button" onClick={() => void copyLocal()}>Copy local draft</button>}
      {state.phase === 'conflict' && <><button type="button" onClick={() => void resolveConflict('keepLocal')}>Keep my draft</button><button type="button" onClick={() => void resolveConflict('useSaved')}>Use saved draft</button></>}
      <span className="chat-draft-editor-status">{editorError || state.error || (state.dirty ? 'Saving draft changes…' : `Draft version ${state.head.version}`)}</span>
    </div>
    <EditorContent editor={editor} />
  </div>;
}

function RenderedDocument({ document }: { document: WnsDocument }) {
  return <div className="chat-rendered-document">{document.body.content.map((block, blockIndex) => {
    if (block.type === 'sceneBreak') return <div className="chat-rendered-scene-break" key={block.attrs.id || blockIndex}>* * *</div>;
    const content = (block.content ?? []).map((inline, inlineIndex) => {
      if (inline.type === 'hardBreak') return <br key={inlineIndex} />;
      let value: React.ReactNode = inline.text;
      for (const mark of inline.marks ?? []) {
        if (mark.type === 'bold') value = <strong>{value}</strong>;
        else if (mark.type === 'italic') value = <em>{value}</em>;
        else if (mark.type === 'link') value = <a href={mark.attrs.href}>{value}</a>;
      }
      return <span key={inlineIndex}>{value}</span>;
    });
    if (block.type === 'heading') {
      const level = Math.min(6, Math.max(1, block.attrs.level));
      return createElement(`h${level}`, { key: block.attrs.id || blockIndex }, content);
    }
    return <p key={block.attrs.id || blockIndex}>{content}</p>;
  })}</div>;
}

function BeforeAfter({ before, after, afterBody, title }: { before: DocumentRecord | null | undefined; after: AssistantDraft | null; afterBody?: WnsDocument; title?: string }) {
  return <details className="chat-draft-before-after" open>
    <summary>Review the full proposed document</summary>
    <div className="chat-draft-comparison">
      <section><h4>Before</h4>{before ? <RenderedDocument document={before.body} /> : before === null ? <p className="chat-prose">New document: no existing working body.</p> : <p className="chat-prose">The exact before version is captured when you prepare the adoption preview.</p>}</section>
      <section><h4>After · {title ?? after?.document.title ?? 'Draft'}</h4>{afterBody ? <RenderedDocument document={afterBody} /> : after ? <RenderedDocument document={after.document.body} /> : <p className="chat-prose">The proposed draft is empty.</p>}</section>
    </div>
  </details>;
}

function ReviewContext({ context }: { context?: DraftReviewContext }) {
  if (!context) return null;
  return <section className="chat-draft-review-context" aria-label="Originating request and assumptions">
    <h4>Author request</h4>
    <p className="chat-prose">{context.instruction}</p>
    {context.assumptions.length > 0 && <><h4>Working assumptions</h4><ul>{context.assumptions.map((assumption, index) => <li key={`${index}:${assumption}`}>{assumption}</li>)}</ul></>}
  </section>;
}

function DraftChangeSummary({ draft }: { draft: AssistantDraft }) {
  return <section className="chat-draft-change-summary" aria-label="What changed">
    <h4>What changed</h4>
    <p className="chat-prose">{draft.target ? `Proposed revision of the supplied document at version ${draft.target.version}.` : 'Proposed new document from the project conversation.'}</p>
  </section>;
}

function AffectedDocuments({ target, draft }: { target?: ChatAdoptionPreview['targets'][number]; draft?: AssistantDraft }) {
  if (!target && !draft) return null;
  const title = target?.title ?? draft?.document.title ?? 'Untitled document';
  const kind = target?.kind ?? draft?.document.kind ?? 'document';
  const draftHead = target?.draft.head ?? draft?.document.head;
  const source = target?.before ?? null;
  return <section className="chat-draft-affected" aria-label="Affected documents">
    <h4>Affected documents</h4>
    <ul>
      <li><strong>{title}</strong> · {kind} · {source ? `target v${source.head.version}` : 'new target'}</li>
      <li>Editable scope: Whole document</li>
      {source ? <li>Protected existing metadata/order · metadata v{source.metadataVersion} · role {source.role}</li> : <li>New target: no existing metadata/order to replace.</li>}
      {draftHead && <li>Assistant draft version {draftHead.version}</li>}
    </ul>
  </section>;
}

function stalePreviewError(value: string): boolean {
  return value.trim().toLocaleLowerCase().startsWith('the preview is stale:');
}

function StalePreviewSources({ preview, comparison, drafts, onPrepareAgainstCurrent }: { preview: ChatAdoptionPreview; comparison: StalePreviewComparison; drafts: AssistantDraft[]; onPrepareAgainstCurrent?: (draft: AssistantDraft, comparison: StalePreviewComparison) => Promise<void> | void }) {
  return <section className="chat-stale-preview-sources" aria-label="Current source heads">
    <h4>Sources and current heads</h4>
    <p className="chat-prose">The retained preview remains unchanged. Compare each captured target with its current saved version before preparing a new preview.</p>
    <ol>
      {preview.targets.map(target => {
        const current = comparison.targets.find(item => item.documentId === target.documentId)?.current ?? null;
        const targetDraft = drafts.find(draft => draft.document.head.documentId === target.draft.head.documentId);
        return <li key={target.documentId}>
          <p><strong>{target.title}</strong> · preview target {target.before ? `v${target.before.head.version}` : 'new'} · current {current ? `v${current.head.version}` : target.before === null ? 'not created' : 'missing'}</p>
          {current ? <DraftReviewDiff before={target.before} after={current.body} title={`${target.title} current source`} mode="details" /> : target.before === null ? <p className="chat-prose">This preview proposes a new document. No working document exists at its reserved destination.</p> : <p className="chat-draft-warning">No current target was returned. Choose or create a target explicitly before preparing again.</p>}
          {targetDraft && !targetDraft.stale && onPrepareAgainstCurrent && <button type="button" disabled={target.before !== null && !current} onClick={() => void onPrepareAgainstCurrent(targetDraft, comparison)}>Prepare against current versions</button>}
        </li>;
      })}
    </ol>
  </section>;
}

export const DraftReviewPanel = forwardRef<DraftReviewPanelHandle, DraftReviewPanelProps>(function DraftReviewPanel({ project, drafts, preview = null, previewVerified = true, previewError = '', onPrepareAdoption, onApplyPreview, onReject, onRevise, onReconsider, onOpenDocument, onOpenDraft, onDraftSaved, viewKey, onAccessChanged, reviewContext, staleComparison = null, onCompareStalePreview, onPrepareAgainstCurrent }, ref) {
  const reviewable = drafts.filter(draft => draft.disposition === 'pending');
  const [selected, setSelected] = useState<string[]>(reviewable.filter(draft => !draft.stale).map(draft => draft.document.head.documentId));
  const [editingId, setEditingId] = useState<string | null>(null);
  const [focusEditingId, setFocusEditingId] = useState<string | null>(null);
  const [openedDraft, setOpenedDraft] = useState<{ id: string; readOnly: boolean } | null>(null);
  const [activeDraftId, setActiveDraftId] = useState<string | null>(() => reviewable[0]?.document.head.documentId ?? drafts[0]?.document.head.documentId ?? null);
  const [activePresentation, setActivePresentation] = useState<'read' | 'changes'>(preview ? 'changes' : 'read');
  const [loadedStaleComparison, setLoadedStaleComparison] = useState<StalePreviewComparison | null>(null);
  const [comparisonBusy, setComparisonBusy] = useState(false);
  const [comparisonError, setComparisonError] = useState('');
  const draftList = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (preview) setActivePresentation('changes');
  }, [preview?.id]);
  useEffect(() => {
    if (!openedDraft) return;
    const card = [...(draftList.current?.querySelectorAll<HTMLElement>('[data-draft-id]') ?? [])]
      .find(element => element.dataset.draftId === openedDraft.id);
    card?.scrollIntoView?.({ block: 'nearest' });
    if (openedDraft.readOnly) card?.focus();
  }, [openedDraft]);
  useEffect(() => {
    if (activeDraftId && drafts.some(draft => draft.document.head.documentId === activeDraftId)) return;
    setActiveDraftId(reviewable[0]?.document.head.documentId ?? drafts[0]?.document.head.documentId ?? null);
  }, [activeDraftId, drafts, reviewable]);
  const activeSession = useRef<DocumentSession | null>(null);
  const restoredViewKey = useRef<string | null>(null);
  const preferenceKey = viewKey ?? (drafts[0] ? chatViewPreferenceKey(project.access, drafts[0].conversationId) : null);
  useEffect(() => {
    if (!preferenceKey || restoredViewKey.current === preferenceKey) return;
    const preferences = readChatViewPreferences(preferenceKey);
    if (preferences.selectedDraftIds) setSelected(preferences.selectedDraftIds);
    if (preferences.editingDraftId !== undefined) setEditingId(preferences.editingDraftId);
    restoredViewKey.current = preferenceKey;
  }, [preferenceKey]);
  useEffect(() => {
    if (!preferenceKey || restoredViewKey.current !== preferenceKey) return;
    writeChatViewPreferences(preferenceKey, { selectedDraftIds: selected, editingDraftId: editingId });
  }, [editingId, preferenceKey, selected]);
  useEffect(() => { setSelected(previous => previous.filter(id => reviewable.some(draft => draft.document.head.documentId === id && !draft.stale))); }, [drafts]);
  useEffect(() => {
    if (loadedStaleComparison && loadedStaleComparison.previewId !== preview?.id) setLoadedStaleComparison(null);
    if (staleComparison && staleComparison.previewId !== preview?.id) setComparisonError('');
  }, [loadedStaleComparison, preview?.id, staleComparison]);
  useImperativeHandle(ref, () => ({
    flush: async () => { if (activeSession.current) await activeSession.current.flush(); },
    currentRefs: (documentIds?: string[]) => drafts.filter(draft => (documentIds ?? selected).includes(draft.document.head.documentId) && draft.disposition === 'pending').map(draft => ({ head: activeSession.current && draft.document.head.documentId === editingId ? activeSession.current.state.head : draft.document.head, dispositionVersion: draft.dispositionVersion })),
    openDraft: async (documentId: string) => {
      const target = drafts.find(draft => draft.document.head.documentId === documentId);
      if (!target) return;
      if (activeSession.current) await activeSession.current.flush();
      setSelected(previous => previous.includes(documentId) ? previous : [...previous, documentId]);
      setActiveDraftId(documentId);
      setActivePresentation('read');
      setEditingId(target.disposition === 'pending' && !target.stale ? documentId : null);
      setFocusEditingId(target.disposition === 'pending' && !target.stale ? documentId : null);
      setOpenedDraft({ id: documentId, readOnly: target.disposition !== 'pending' || target.stale });
    },
  }), [drafts, editingId, selected]);
  const beginEdit = async (id: string) => {
    try {
      if (activeSession.current) await activeSession.current.flush();
      setActiveDraftId(id);
      setActivePresentation('read');
      setEditingId(id);
    }
    catch { /* The editor keeps its error and remains mounted for an explicit retry. */ }
  };
  const switchDraft = async (id: string) => {
    if (id === activeDraftId) return;
    try {
      if (activeSession.current) await activeSession.current.flush();
      setEditingId(null);
      setFocusEditingId(null);
      setActiveDraftId(id);
      setActivePresentation('read');
    } catch {
      // Keep the current draft mounted when its local buffer cannot be flushed.
      // The editor exposes the actionable save/reconciliation state in place.
    }
  };
  if (!drafts.length) return <section className="chat-draft-review chat-empty coauthor-review-panel"><h2>Drafts to review</h2><p>No assistant drafts are waiting for a decision.</p></section>;
  const selectedDrafts = reviewable.filter(draft => selected.includes(draft.document.head.documentId) && !draft.stale);
  const activeDraft = drafts.find(draft => draft.document.head.documentId === activeDraftId) ?? drafts[0];
  const adoptionLabel = preview ? preview.targets.length === 1 ? `Adopt ${preview.targets[0].title} draft v${preview.targets[0].draft.head.version}` : `Adopt all ${preview.targets.length} documents` : '';
  const adoptionAccessibleLabel = preview ? preview.targets.length === 1 ? adoptionLabel : `Adopt all ${preview.targets.length} documents: ${preview.targets.map(target => `${target.title} draft v${target.draft.head.version}`).join(', ')}` : '';
  const previewIsStale = !!preview && stalePreviewError(previewError);
  const comparison = preview && (staleComparison?.previewId === preview.id ? staleComparison : loadedStaleComparison?.previewId === preview.id ? loadedStaleComparison : null);
  const compareStalePreview = async () => {
    if (!preview || !onCompareStalePreview) return;
    setComparisonBusy(true); setComparisonError('');
    try {
      const next = await onCompareStalePreview(preview);
      if (next.previewId !== preview.id) throw new Error('The current-head comparison belongs to a different preview.');
      setLoadedStaleComparison(next);
    } catch (reason) {
      setComparisonError(reason instanceof Error ? reason.message : 'The current source heads could not be loaded.');
    } finally { setComparisonBusy(false); }
  };
  const prepareAgainstCurrent = async (draft: AssistantDraft) => {
    if (!comparison || !onPrepareAgainstCurrent) return;
    setComparisonError('');
    try { await onPrepareAgainstCurrent([draft], comparison); }
    catch (reason) { setComparisonError(reason instanceof Error ? reason.message : 'A fresh preview could not be prepared against the current versions.'); }
  };
  const prepareReview = async (items: AssistantDraft[]) => {
    try {
      if (activeSession.current) await activeSession.current.flush();
      await onPrepareAdoption(items);
    } catch {
      // DocumentSession keeps its local error/reconciliation state visible.
      // A failed flush must not dispatch a preparation for an unsaved buffer.
    }
  };
  const activePreviewTarget = preview?.targets.find(target => target.draft.head.documentId === activeDraft.document.head.documentId);
  const activeStatus = activeDraft.disposition === 'adopted' ? 'Adopted' : activeDraft.disposition === 'rejected' || activeDraft.disposition === 'superseded' ? 'Closed' : activeDraft.stale ? 'Stale' : 'Not adopted';
  const prepareFooterLabel = selectedDrafts.length > 1 ? `Prepare grouped review · ${selectedDrafts.length} drafts` : selectedDrafts.length === 1 ? `Prepare review · ${selectedDrafts[0].document.title} draft v${selectedDrafts[0].document.head.version}` : 'Select a draft to prepare review';
  return <section className="chat-draft-review coauthor-review-panel" aria-labelledby="chat-draft-review-title">
    <header className="coauthor-review-header">
      <div><h2 id="chat-draft-review-title">Drafts to review</h2></div>
      <span className="coauthor-review-count">{reviewable.length} pending</span>
    </header>
    <nav className="coauthor-review-draft-tabs" role="tablist" aria-label="Available drafts">
      {drafts.map(draft => {
        const id = draft.document.head.documentId;
        const status = draft.disposition === 'adopted' ? 'Adopted' : draft.disposition === 'rejected' || draft.disposition === 'superseded' ? 'Closed' : draft.stale ? 'Stale' : 'Not adopted';
        return <button key={`${id}:${draft.initialRevisionId}`} type="button" role="tab" data-draft-id={id} aria-selected={activeDraft.document.head.documentId === id} aria-controls={`coauthor-read-${id}`} onClick={() => void switchDraft(id)}>
          <span className="coauthor-review-tab-title">{draft.document.title}</span><span className="coauthor-review-tab-meta">{draft.document.kind} · v{draft.document.head.version} · {status}</span>
        </button>;
      })}
    </nav>
    <fieldset className="coauthor-review-selection" aria-label="Drafts included in this review">
      <legend>Review together</legend>
      {reviewable.map(draft => <label key={`selection:${draft.document.head.documentId}`}><input type="checkbox" data-draft-id={draft.document.head.documentId} aria-label={`Include ${draft.document.title} in this adoption`} checked={selected.includes(draft.document.head.documentId)} disabled={draft.stale} onChange={event => setSelected(value => event.target.checked ? [...value, draft.document.head.documentId] : value.filter(item => item !== draft.document.head.documentId))} /><span>{draft.document.title}</span>{draft.stale && <small>stale</small>}</label>)}
    </fieldset>
    <div className="coauthor-review-document-header">
      <div><h3>{activeDraft.document.title}</h3><p>{activeDraft.document.kind} · Draft {activeDraft.document.head.version} · <strong>{activeStatus}</strong></p></div>
      {(onOpenDraft || onOpenDocument) && <button type="button" onClick={() => onOpenDraft ? void onOpenDraft(activeDraft) : void onOpenDocument?.(activeDraft.document)}>Open draft</button>}
    </div>
    <nav className="coauthor-review-presentation-tabs" role="tablist" aria-label={`Review ${activeDraft.document.title}`}>
      <button type="button" role="tab" aria-selected={activePresentation === 'read'} aria-controls={`coauthor-read-${activeDraft.document.head.documentId}`} onClick={() => setActivePresentation('read')}>Read</button>
      <button type="button" role="tab" aria-selected={activePresentation === 'changes'} aria-controls={`coauthor-changes-${activeDraft.document.head.documentId}`} onClick={() => setActivePresentation('changes')}>Changes</button>
    </nav>
    <div className="coauthor-review-document-surface" ref={draftList}>
      <div className="coauthor-review-tabpanel coauthor-review-changes" role="tabpanel" id={`coauthor-changes-${activeDraft.document.head.documentId}`} hidden={activePresentation !== 'changes'}>
        {preview && <section className="chat-adoption-preview" aria-label="Exact adoption preview"><div className="chat-panel-heading"><div><h3>Exact adoption preview</h3><p>{preview.targets.length} document{preview.targets.length === 1 ? '' : 's'} ready for your review</p></div></div><details className="chat-draft-provenance"><summary>Preview version details</summary><dl><div><dt>Preview</dt><dd>{preview.id}</dd></div><div><dt>Digest</dt><dd>{preview.digest}</dd></div></dl></details><details className="coauthor-review-effects" open={!!preview.effects?.proposedRelationships.length}><summary>Relationships, protected content and other effects</summary><ChatAdoptionEffects effects={preview.effects} targets={preview.targets} /></details>{preview.targets.map(target => {
          // The preview target is the ordinary destination document. The draft
          // identity lives in the immutable draft reference, so never resolve it
          // from target.documentId (which is intentionally a different ID).
          const targetDraft = drafts.find(draft => draft.document.head.documentId === target.draft.head.documentId);
          const context = targetDraft ? reviewContext?.[targetDraft.originRunId] : undefined;
          return <article key={target.documentId}><h4>{target.title}</h4><DraftReviewDiff before={target.before} after={target.body} title={target.title} mode="summary" /><ReviewContext context={context} /><AffectedDocuments target={target} /><BeforeAfter before={target.before} after={null} afterBody={target.body} title={target.title} /><DraftReviewDiff before={target.before} after={target.body} title={target.title} mode="details" /></article>;
        })}{previewIsStale && <section className="chat-stale-preview-actions" aria-label="Stale preview recovery"><h4>Source changed since this preview</h4><p className="chat-prose">The immutable preview remains available. Compare current saved heads before preparing a new preview.</p>{onCompareStalePreview && <button type="button" disabled={comparisonBusy} onClick={() => void compareStalePreview()}>{comparisonBusy ? 'Loading current heads…' : 'Compare sources/current heads'}</button>}{!onCompareStalePreview && !comparison && <p className="chat-draft-warning">Current heads are not loaded. Ask the project surface to compare them before preparing again.</p>}{comparison && <StalePreviewSources preview={preview} comparison={comparison} drafts={drafts} onPrepareAgainstCurrent={onPrepareAgainstCurrent ? (draft) => prepareAgainstCurrent(draft) : undefined} />}{comparison && comparison.sourceEpoch && comparison.currentSourceEpoch && comparison.sourceEpoch !== comparison.currentSourceEpoch && <p className="chat-draft-warning">The story source epoch changed ({comparison.sourceEpoch} → {comparison.currentSourceEpoch}); a draft marked stale must be refreshed with the assistant.</p>}{comparison && comparison.policyEpoch && comparison.currentPolicyEpoch && comparison.policyEpoch !== comparison.currentPolicyEpoch && <p className="chat-draft-warning">The project policy changed ({comparison.policyEpoch} → {comparison.currentPolicyEpoch}); review the retained preview before preparing again.</p>}{comparisonError && <p className="chat-draft-warning" role="alert">{comparisonError}</p>}</section>}{previewError && <p className="chat-draft-warning" role="alert">{previewError} The old preview remains available; compare current sources and prepare a fresh preview explicitly.</p>}</section>}
        {!preview && <><DraftChangeSummary draft={activeDraft} /><BeforeAfter before={undefined} after={activeDraft} /><DraftReviewDiff before={(activeDraft.target ? project.documents.find(document => document.head.documentId === activeDraft.target?.documentId) : undefined) ?? null} after={activeDraft.document.body} title={activeDraft.document.title} mode="all" /></>}
        {activePreviewTarget && !preview && <p className="coauthor-review-note">This draft is bound to the prepared preview target above.</p>}
      </div>
      <div className="coauthor-review-tabpanel coauthor-review-read" role="tabpanel" id={`coauthor-read-${activeDraft.document.head.documentId}`} data-draft-id={activeDraft.document.head.documentId} tabIndex={-1} hidden={activePresentation !== 'read'}>
        {editingId === activeDraft.document.head.documentId ? <DraftEditor key={activeDraft.document.head.documentId} draft={activeDraft} project={project} autoFocus={focusEditingId === activeDraft.document.head.documentId} onSession={session => { activeSession.current = session; }} onAccessChanged={onAccessChanged} onSaved={(savedDraft, head) => onDraftSaved?.(savedDraft, head)} /> : <RenderedDocument document={activeDraft.document.body} />}
        <details className="coauthor-review-disclosure">
          <summary>Request, assumptions and provenance</summary>
          <ReviewContext context={reviewContext?.[activeDraft.originRunId]} />
          <AffectedDocuments draft={activeDraft} />
          {activeDraft.predecessorDocumentId && <p className="chat-muted">Revision of an earlier assistant draft{drafts.some(item => item.document.head.documentId === activeDraft.predecessorDocumentId) && onOpenDraft ? <> · <button type="button" onClick={() => { const previous = drafts.find(item => item.document.head.documentId === activeDraft.predecessorDocumentId); if (previous) void onOpenDraft(previous); }}>Open earlier draft</button></> : ` · ${activeDraft.predecessorDocumentId}`}. Both drafts retain their own review decisions.</p>}
          <details className="chat-draft-provenance" open><summary>Source and generation details</summary><dl><div><dt>Source target</dt><dd>{activeDraft.target ? `${activeDraft.target.documentId} · v${activeDraft.target.version}` : 'Project context'}</dd></div><div><dt>Source body hash</dt><dd>{activeDraft.target?.bodyHash ?? 'Not target-bound'}</dd></div><div><dt>Context packet</dt><dd>{activeDraft.packetId}</dd></div><div><dt>Origin run</dt><dd>{activeDraft.originRunId}</dd></div><div><dt>Initial revision</dt><dd>{activeDraft.initialRevisionId}</dd></div></dl><details className="chat-draft-context"><summary>Inspect supplied context</summary><ContextInspector access={project.access} packetId={activeDraft.packetId} delivered refreshKey={`${activeDraft.packetId}:${activeDraft.initialRevisionId}`} /></details></details>
        </details>
        {!activeDraft.disposition || activeDraft.disposition === 'pending' ? <>
          {editingId !== activeDraft.document.head.documentId && <button type="button" className="chat-draft-edit coauthor-review-edit" onClick={() => void beginEdit(activeDraft.document.head.documentId)}>Edit this draft</button>}
          <div className="chat-draft-actions coauthor-review-actions">
            <button type="button" disabled={activeDraft.stale || !selectedDrafts.some(item => item.document.head.documentId === activeDraft.document.head.documentId)} onClick={() => void prepareReview([activeDraft])}>Prepare adoption preview</button>
            {onRevise && <button type="button" onClick={() => void onRevise(activeDraft)}>{activeDraft.stale ? 'Refresh with assistant' : 'Revise with assistant'}</button>}
            <button type="button" onClick={() => void onReject(activeDraft)}>Reject</button>
          </div>
          {activeDraft.stale && <p className="chat-draft-warning" role="alert">This draft was based on an older source. Refresh it in the conversation before adopting.</p>}
        </> : activeDraft.disposition === 'rejected' || activeDraft.disposition === 'superseded' ? onReconsider && <button type="button" onClick={() => void onReconsider(activeDraft)}>Reconsider in a new review</button> : null}
      </div>
    </div>
    <footer className="coauthor-review-footer" aria-label="Draft review action">
      <div><strong>{preview ? `${preview.targets.length} draft${preview.targets.length === 1 ? '' : 's'} prepared` : `${selectedDrafts.length} draft${selectedDrafts.length === 1 ? '' : 's'} selected`}</strong><span>{preview ? 'Exact versions are ready for your decision.' : 'Choose drafts to prepare an exact review.'}</span></div>
      {preview ? <button type="button" className="coauthor-review-primary" aria-label={previewIsStale ? 'Preview is stale; compare current sources before preparing again' : adoptionAccessibleLabel} disabled={!previewVerified || previewIsStale} onClick={() => void onApplyPreview(preview)}>{previewIsStale ? 'Preview stale — compare sources' : previewVerified ? adoptionLabel : 'Rechecking preview…'}</button> : <button type="button" className="coauthor-review-primary" disabled={!selectedDrafts.length} onClick={() => void prepareReview(selectedDrafts)}>{prepareFooterLabel}</button>}
    </footer>
  </section>;
});
