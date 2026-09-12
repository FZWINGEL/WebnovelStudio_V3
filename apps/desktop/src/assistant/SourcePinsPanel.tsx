import { useEffect, useId, useRef, useState } from 'react';
import type { DocumentSession } from '../editor';
import { readSourcePins, saveSourcePins, type SaveSourcePins, type SourceChoice, type SourcePinScope, type SourcePinSet, type SourcePinsView } from '../ipc/sourcePins';

const labels: Record<SourcePinScope, string> = { document: 'This document', project: 'This project' };
function detail(reason: unknown): string { return reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail) : reason instanceof Error ? reason.message : 'Could not save these sources.'; }
function uncertain(reason: unknown): boolean { return !reason || typeof reason !== 'object' || !('code' in reason) || ['UncertainOutcome', 'ReconciliationRequired', 'StaleWriterLease'].includes(String(reason.code)); }
function validSet(set: SourcePinSet, scope: SourcePinScope, documentId: string): boolean {
  return !!set && set.scope === scope && set.targetDocumentId === (scope === 'document' ? documentId : null)
    && set.audience === 'authorRoom' && /^(0|[1-9][0-9]{0,18})$/u.test(set.version)
    && Array.isArray(set.sourceDocumentIds) && set.sourceDocumentIds.length <= 64
    && set.sourceDocumentIds.every(id => typeof id === 'string' && id.length > 0)
    && JSON.stringify(set.sourceDocumentIds) === JSON.stringify([...new Set(set.sourceDocumentIds)].sort());
}

/** Persistent source preferences affect future discussions, never old packets or prose. */
export function SourcePinsPanel({ session, documentId, sources, adoption, restricted, disabled, onChanged, onPendingChange }: {
  session: DocumentSession; documentId: string; sources: SourceChoice[];
  adoption: { documentId: string; nonce: number } | null;
  restricted: boolean; disabled: boolean; onChanged(): void;
  onPendingChange?: (pending: boolean) => void;
}) {
  const [view, setView] = useState<SourcePinsView | null>(null);
  const [scope, setScope] = useState<SourcePinScope>('document');
  const [selected, setSelected] = useState('');
  const [open, setOpen] = useState(false);
  const [error, setError] = useState(''); const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false); const [pending, setPending] = useState<SaveSourcePins | null>(null);
  const [reload, setReload] = useState(0);
  const controlsId = useId(); const selector = useRef<HTMLSelectElement>(null); const summary = useRef<HTMLElement>(null);
  const returnFocus = useRef<Element | null>(null);
  const flight = useRef(false); const pendingRef = useRef<SaveSourcePins | null>(null);
  const identity = `${session.projectAccess.projectId}/${session.projectAccess.operationNamespace}/${session.projectAccess.session}/${documentId}`;
  const current = useRef({ session, identity }); current.current = { session, identity };
  const generation = useRef(0);
  const pendingCallback = useRef(onPendingChange); pendingCallback.current = onPendingChange;
  useEffect(() => () => pendingCallback.current?.(false), [session, identity]);
  useEffect(() => {
    const request = ++generation.current;
    setView(null); setError(''); setNotice(''); setPending(null); pendingRef.current = null;
    setBusy(false); flight.current = false; setSelected('');
    void readSourcePins(session.projectAccess, documentId).then(result => {
      if (generation.current !== request) return;
      if (!validSet(result?.project, 'project', documentId) || !validSet(result?.document, 'document', documentId)) throw new Error('The saved source list did not match this document. Reload sources.');
      setView(result);
    }).catch(reason => { if (generation.current === request) setError(detail(reason)); });
    return () => { ++generation.current; };
  }, [session, identity, reload]);
  useEffect(() => {
    if (!adoption || flight.current || pendingRef.current) return;
    setSelected(adoption.documentId); setOpen(true); setNotice('');
  }, [adoption?.nonce]);
  useEffect(() => { if (open && selected) selector.current?.focus(); }, [open, adoption?.nonce]);
  useEffect(() => {
    if (busy || pending || !returnFocus.current) return;
    const previous = returnFocus.current; returnFocus.current = null;
    if (document.activeElement !== previous && document.activeElement !== document.body) return;
    const destination = open && selector.current && !selector.current.disabled ? selector.current : summary.current;
    destination?.focus();
  }, [busy, pending, view, open]);

  async function write(request: SaveSourcePins, checking = false) {
    if (flight.current) return;
    const captured = current.current; const requestGeneration = generation.current;
    const owns = () => current.current.session === captured.session && current.current.identity === captured.identity && generation.current === requestGeneration;
    const immutable = structuredClone(request);
    const focused = document.activeElement;
    flight.current = true; setBusy(true); setError(''); setNotice('');
    pendingRef.current = immutable; setPending(immutable);
    pendingCallback.current?.(true);
    try {
      if (checking) await session.reconcile();
      let result: SourcePinSet | undefined;
      await session.withLifecycleGuard(async () => {
        await session.flush();
        if (!owns()) return;
        result = await saveSourcePins({ ...immutable, access: session.projectAccess });
      });
      if (!owns() || !result) return;
      if (!validSet(result, immutable.scope, documentId)
        || JSON.stringify(result.sourceDocumentIds) !== JSON.stringify(immutable.sourceDocumentIds)
        || ![BigInt(immutable.expectedVersion), BigInt(immutable.expectedVersion) + 1n].includes(BigInt(result.version))) {
        throw { code: 'UncertainOutcome', detail: 'The save acknowledgment did not match these sources. Check the saved sources.' };
      }
      const saved = result;
      if (checking) {
        // A replay confirms the original write; another confirmed write may be newer.
        try {
          const latest = await readSourcePins(session.projectAccess, documentId);
          if (!owns()) return;
          if (!validSet(latest?.project, 'project', documentId) || !validSet(latest?.document, 'document', documentId)) throw new Error('The current source list did not match this document.');
          setView(latest); setNotice('Save confirmed. Showing the current sources.');
        } catch (reason) {
          if (!owns()) return;
          setView(null); setError(`Your source changes were saved, but the current list could not be read. ${detail(reason)}`);
        }
      } else {
        setView(previous => previous ? { ...previous, [saved.scope]: saved } : previous);
        setNotice('Sources saved for future discussions.');
      }
      if (focused && summary.current?.parentElement?.contains(focused)) returnFocus.current = focused;
      // Removed buttons no longer belong to the panel after React commits.
      else if (focused && !focused.isConnected) returnFocus.current = focused;
      pendingRef.current = null; setPending(null); pendingCallback.current?.(false); onChanged();
    } catch (reason) {
      if (!owns()) return;
      if (!uncertain(reason)) { pendingRef.current = null; setPending(null); pendingCallback.current?.(false); }
      setError(detail(reason));
    } finally { if (owns()) { flight.current = false; setBusy(false); } }
  }
  function change(scope: SourcePinScope, ids: string[]) {
    if (!view || disabled || busy || pendingRef.current) return;
    const set = view[scope];
    void write({ access: session.projectAccess, operationId: crypto.randomUUID(), scope,
      targetDocumentId: set.targetDocumentId, expectedVersion: set.version, sourceDocumentIds: [...new Set(ids)].sort() });
  }
  const locked = disabled || busy || pending !== null;
  const count = (view?.document.sourceDocumentIds.length ?? 0) + (view?.project.sourceDocumentIds.length ?? 0);
  const title = (id: string) => sources.find(source => source.id === id)?.title ?? 'Unavailable source';
  return <details className="persistent-source-pins" open={open} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary ref={summary}>Story sources{count > 0 ? ` · ${count}` : ''}</summary>
    <p className="small-copy">Keep important story material in future discussions. Each request uses its latest saved text.</p>
    <p className="small-copy">{restricted ? 'These saved sources are for discussion. They will not be added to this edit request.' : 'Saved sources apply to discussion, not to suggested edits. They do not override private-source rules.'}</p>
    {!view && !error && <p role="status">Reading saved sources…</p>}
    {view && <>
      {count === 0 && <p className="small-copy">No sources kept yet. You can still discuss your writing.</p>}
      <ul className="persistent-source-list">{(['document', 'project'] as const).flatMap(scope => view[scope].sourceDocumentIds.map(id => <li key={`${scope}/${id}`}><span>{title(id)}<small>{labels[scope]}</small></span><button className="text-button" disabled={locked} aria-label={`Remove ${title(id)} from ${labels[scope].toLowerCase()} sources`} onClick={() => change(scope, view[scope].sourceDocumentIds.filter(source => source !== id))}>Remove</button></li>))}</ul>
      <form className="source-pin-form" onSubmit={event => { event.preventDefault(); if (selected) change(scope, [...view[scope].sourceDocumentIds, selected]); }}>
        <label htmlFor={`${controlsId}-source`}>Story source</label><select ref={selector} id={`${controlsId}-source`} value={selected} disabled={locked} onChange={event => setSelected(event.target.value)}><option value="">Choose a story source…</option>{selected && !sources.some(source => source.id === selected) && <option value={selected}>Unavailable source</option>}{sources.map(source => <option key={source.id} value={source.id}>{source.title}</option>)}</select>
        {selected && !sources.some(source => source.id === selected) && <p className="small-copy">This source is no longer available in this project. Choose another source.</p>}
        <label htmlFor={`${controlsId}-scope`}>Use in discussions for</label><select id={`${controlsId}-scope`} value={scope} disabled={locked} onChange={event => setScope(event.target.value as SourcePinScope)}><option value="document">This document</option><option value="project">This project</option></select>
        <button className="secondary-button" disabled={locked || !selected || !sources.some(source => source.id === selected) || view[scope].sourceDocumentIds.includes(selected)}>Keep source</button>
      </form>
    </>}
    {error && <p className="error-status" role="alert">{error} {pending ? <button disabled={busy} onClick={() => void write(pending, true)}>Check saved sources</button> : <button disabled={busy} onClick={() => setReload(value => value + 1)}>Reload sources</button>}</p>}
    {notice && <p className="small-copy" role="status">{notice}</p>}
  </details>;
}
