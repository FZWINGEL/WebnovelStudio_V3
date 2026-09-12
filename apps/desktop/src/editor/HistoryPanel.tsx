import { errorTextFor } from '../kernel';
import { Fragment, useEffect, useRef, useState } from 'react';
import { bodyHash, canonicalJson, type WnsDocument } from '../kernel';
import { listDocumentHistory, readDocumentRevision, type RevisionSummary } from '../ipc/history';
import type { ProjectAccess, Revision } from '../ipc/projects';

function reasonLabel(reason: string): string {
  return ({ beforeApply: 'Before an applied edit', afterApply: 'Applied edit', beforeRestore: 'Before a restore', afterRestore: 'Restored version',
    beforeUndoRedo: 'Before undo or redo', afterUndoRedo: 'After undo or redo', manual: 'Saved version', switch: 'Left the document',
    close: 'Closed the project', source: 'Used in a request', export: 'Exported draft', interval: 'Writing checkpoint' } as Record<string, string>)[reason] ?? 'Saved version';
}
function label(item: RevisionSummary): string {
  const date = new Date(item.createdAt);
  const when = Number.isNaN(date.getTime()) ? '' : ` · ${date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}`;
  return `${reasonLabel(item.reason)}${when} · Version ${item.head.version}`;
}
const message = errorTextFor('Could not read the saved version. Try again.');

/** Inert text/formatting only. History never creates a second editor. */
export function SavedProse({ body }: { body: WnsDocument }) {
  return <div className="saved-prose">{body.body.content.map(block => {
    if (block.type === 'sceneBreak') return <hr key={block.attrs.id} />;
    const children = block.content?.map((inline, index) => inline.type === 'hardBreak' ? <br key={index} /> : <span key={index} style={{
      fontWeight: inline.marks?.some(mark => mark.type === 'bold') ? 700 : undefined,
      fontStyle: inline.marks?.some(mark => mark.type === 'italic') ? 'italic' : undefined,
      textDecoration: inline.marks?.some(mark => mark.type === 'link') ? 'underline' : undefined,
    }}>{inline.text}</span>);
    return <Fragment key={block.attrs.id}>{block.type === 'heading' ? <p className={`saved-heading saved-heading-${block.attrs.level}`}>{children}</p> : <p>{children || <br />}</p>}</Fragment>;
  })}</div>;
}

export function HistoryPanel({ access, documentId, body, visible, disabled, onClose, onRestore }: {
  access: ProjectAccess; documentId: string; body: WnsDocument; visible: boolean; disabled: boolean;
  onClose(): void; onRestore(revision: Revision): Promise<void>;
}) {
  const [items, setItems] = useState<RevisionSummary[]>([]);
  const [next, setNext] = useState<string | null>(null);
  const [selected, setSelected] = useState('');
  const [revision, setRevision] = useState<Revision | null>(null);
  const [loading, setLoading] = useState(false);
  const [reading, setReading] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [refresh, setRefresh] = useState(0);
  const listSequence = useRef(0); const readSequence = useRef(0); const restoreSequence = useRef(0);
  const owner = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}/${documentId}`;
  const liveOwner = useRef(owner); liveOwner.current = owner;
  const liveVisible = useRef(visible); liveVisible.current = visible;
  const ownsPanel = () => liveOwner.current === owner && liveVisible.current;
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!visible) return;
    heading.current?.focus();
  }, [visible]);
  useEffect(() => {
    const sequence = ++listSequence.current; ++readSequence.current;
    setItems([]); setNext(null); setSelected(''); setRevision(null); setReading(false); setRestoring(false); setError('');
    if (!visible) return;
    setLoading(true);
    void listDocumentHistory(access, documentId).then(page => {
      if (sequence !== listSequence.current || !ownsPanel()) return;
      setItems(page.items); setNext(page.nextBeforeVersion);
    }).catch(reason => { if (sequence === listSequence.current && ownsPanel()) setError(message(reason)); })
      .finally(() => { if (sequence === listSequence.current && ownsPanel()) setLoading(false); });
    return () => { ++listSequence.current; ++readSequence.current; ++restoreSequence.current; };
  }, [owner, visible, refresh]);
  useEffect(() => { setNotice(''); }, [owner]);
  async function choose(id: string) {
    if (disabled || restoring) return;
    const sequence = ++readSequence.current;
    setSelected(id); setRevision(null); setError(''); setNotice('');
    if (!id) { setReading(false); return; }
    setReading(true);
    try {
      const result = await readDocumentRevision(access, documentId, id);
      if (sequence !== readSequence.current || !ownsPanel()) return;
      const summary = items.find(item => item.id === id);
      if (result.id !== id || result.head.documentId !== documentId || !summary || canonicalJson(result.head) !== canonicalJson(summary.head)
        || await bodyHash(canonicalJson(result.body)) !== result.head.bodyHash) throw new Error('The saved version did not match the selected document and writing. Try reading it again.');
      if (sequence !== readSequence.current || !ownsPanel()) return;
      setRevision(result);
    } catch (reason) { if (sequence === readSequence.current && ownsPanel()) setError(message(reason)); }
    finally { if (sequence === readSequence.current && ownsPanel()) setReading(false); }
  }
  async function older() {
    if (!next || loading || disabled || restoring) return;
    const sequence = ++listSequence.current; setLoading(true); setError('');
    try {
      const page = await listDocumentHistory(access, documentId, next);
      if (sequence !== listSequence.current || !ownsPanel()) return;
      setItems(previous => [...previous, ...page.items.filter(item => !previous.some(existing => existing.id === item.id))]);
      setNext(page.nextBeforeVersion);
    } catch (reason) { if (sequence === listSequence.current && ownsPanel()) setError(message(reason)); }
    finally { if (sequence === listSequence.current && ownsPanel()) setLoading(false); }
  }
  async function restore() {
    if (!revision || disabled || restoring) return;
    const sequence = ++restoreSequence.current;
    setRestoring(true); setError('');
    try {
      await onRestore(revision);
      if (!ownsPanel() || sequence !== restoreSequence.current) return;
      setNotice('Restored. Your previous writing is kept in Saved versions.');
      setRefresh(value => value + 1);
    } catch (reason) { if (ownsPanel() && sequence === restoreSequence.current) setError(message(reason)); }
    finally { if (ownsPanel() && sequence === restoreSequence.current) setRestoring(false); }
  }
  if (!visible) return null;
  const identical = revision && canonicalJson(body) === canonicalJson(revision.body);
  return <aside className="history-panel" aria-labelledby="history-heading">
    <div className="feedback-heading"><h2 id="history-heading" tabIndex={-1} ref={heading}>Saved versions</h2><button disabled={restoring} onClick={onClose}>Back to writing</button></div>
    <p className="panel-intro">Choose a saved version to compare with your current writing.</p>
    <div className="history-chooser">
      <label htmlFor="saved-version">Saved version</label>
      <select id="saved-version" value={selected} disabled={disabled || restoring || (!items.length && loading)} onChange={event => void choose(event.target.value)}>
        <option value="">Choose a version…</option>{items.map(item => <option value={item.id} key={item.id}>{label(item)}</option>)}
      </select>
      <div className="history-list-actions"><button onClick={() => setRefresh(value => value + 1)} disabled={disabled || loading || restoring}>Refresh</button>{next && <button onClick={() => void older()} disabled={disabled || loading || restoring}>Load older versions</button>}</div>
    </div>
    {error && <div className="history-error" role="alert"><p>{error}</p>{selected && !revision && <button disabled={disabled || restoring} onClick={() => void choose(selected)}>Try reading again</button>}</div>}
    {notice && <p className="history-notice" role="status">{notice}</p>}
    <div className="history-preview" aria-label="Saved writing" aria-busy={reading}>
      {reading ? <p role="status">Reading saved version…</p> : revision ? <SavedProse body={revision.body} />
        : <p className="small-copy">{loading ? 'Reading saved versions…' : items.length ? 'Your selected version will appear here. Reading history leaves your manuscript unchanged.' : 'No saved versions yet. Keep writing; a version is kept when you leave the document or make a significant change.'}</p>}
    </div>
    {revision && <div className="history-restore"><p>{identical ? 'Your current writing matches this version.' : 'Restoring replaces the current writing in this document. The writing you have now will remain in Saved versions.'}</p>
      <button className="primary-button" disabled={disabled || restoring || !!identical} onClick={() => void restore()}>{restoring ? 'Restoring…' : 'Restore this version'}</button></div>}
  </aside>;
}
