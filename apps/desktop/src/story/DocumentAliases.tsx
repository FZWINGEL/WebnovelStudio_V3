import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { readDocumentAliases, setDocumentAliases, type DocumentAliasesRead } from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';
import './DocumentAliases.css';

export type DocumentAliasesGuard = () => Promise<void>;

export interface DocumentAliasesProps {
  access: ProjectAccess;
  documentId: string;
  title: string;
  visible: boolean;
  disabled?: boolean;
  onClose: () => void;
  registerGuard: (guard: DocumentAliasesGuard | null) => void;
  beforeSave?: () => Promise<void>;
  onSaved?: () => void;
}

interface AliasBaseline { aliases: string[]; sourceEpoch: string }
type Phase = 'loading' | 'idle' | 'saving' | 'reconciling' | 'conflict' | 'uncertain';
interface PendingWrite { operation: Promise<void>; ownerKey: string; writerLease: string; desired: string[]; dispatched: boolean }
interface PendingReconciliation { operation: Promise<void>; desired: string[] }

const MAX_ALIASES = 64;
const MAX_ALIAS_BYTES = 256;
const CONTROL_CHARACTER = /\p{Cc}/u;
const DECIMAL_EPOCH = /^(0|[1-9][0-9]*)$/u;

function errorMessage(reason: unknown): string {
  if (reason instanceof Error && reason.message) return reason.message;
  if (typeof reason === 'string' && reason) return reason;
  return 'The names could not be saved.';
}

function normalizeAliases(values: readonly string[]): string[] {
  const names: string[] = [];
  for (const value of values) {
    if (typeof value !== 'string') throw new Error('The saved names were not valid. Reload names and try again.');
    if (CONTROL_CHARACTER.test(value)) throw new Error('Names cannot contain control characters.');
    const name = value.trim();
    if (!name) continue;
    if (new TextEncoder().encode(name).length > MAX_ALIAS_BYTES) {
      throw new Error('This name is too long. Shorten it and try again.');
    }
    names.push(name);
  }
  const unique = [...new Set(names)].sort();
  if (unique.length > MAX_ALIASES) throw new Error(`Use at most ${MAX_ALIASES} names.`);
  return unique;
}

function aliasesFromText(text: string): string[] { return normalizeAliases(text.split(/\r\n|\r|\n/u)); }
function aliasesText(aliases: readonly string[]): string { return aliases.join('\n'); }
function sameAliases(left: readonly string[], right: readonly string[]): boolean {
  const a = [...left].sort(); const b = [...right].sort();
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

function readBaseline(value: DocumentAliasesRead, documentId: string): AliasBaseline {
  if (!value || value.documentId !== documentId || !Array.isArray(value.aliases) || typeof value.sourceEpoch !== 'string' || !DECIMAL_EPOCH.test(value.sourceEpoch)) {
    throw new Error('The saved names did not match this document. Reload names and try again.');
  }
  return { aliases: normalizeAliases(value.aliases), sourceEpoch: value.sourceEpoch };
}

function validEpochs(value: unknown): value is { source: string; policy: string } {
  return !!value && typeof value === 'object' && typeof (value as { source?: unknown }).source === 'string'
    && DECIMAL_EPOCH.test((value as { source: string }).source) && typeof (value as { policy?: unknown }).policy === 'string'
    && DECIMAL_EPOCH.test((value as { policy: string }).policy);
}

function isStrictlyNewerEpoch(previous: string, next: string): boolean {
  return DECIMAL_EPOCH.test(previous) && DECIMAL_EPOCH.test(next) && BigInt(next) > BigInt(previous);
}

/**
 * Edits the author-maintained names for one saved document. The panel remains
 * mounted while hidden so a caller can use its guard before changing documents.
 */
export function DocumentAliases({ access, documentId, title, visible, disabled = false, onClose, registerGuard, beforeSave, onSaved }: DocumentAliasesProps) {
  const ownerKey = `${access.projectId}\u0000${access.operationNamespace}\u0000${access.session}\u0000${documentId}`;
  const current = useRef({ access, documentId, ownerKey, beforeSave, onSaved });
  current.current = { access, documentId, ownerKey, beforeSave, onSaved };
  const mounted = useRef(true);
  const generation = useRef(0);
  const loadedOwner = useRef<string | null>(null);
  const inFlight = useRef<Promise<void> | null>(null);
  const pendingWrite = useRef<PendingWrite | null>(null);
  const pendingReconciliation = useRef<PendingReconciliation | null>(null);
  const previousLease = useRef(access.writerLease);
  const [reload, setReload] = useState(0);
  const [baseline, setBaseline] = useState<AliasBaseline | null>(null);
  const [draftText, setDraftText] = useState('');
  const [phase, setPhase] = useState<Phase>('loading');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [uncertainDesired, setUncertainDesired] = useState<string[] | null>(null);
  const inputId = useId();
  const hintId = `${inputId}-hint`;

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; generation.current += 1; inFlight.current = null; };
  }, []);

  useEffect(() => {
    generation.current += 1;
    loadedOwner.current = null;
    inFlight.current = null;
    setBaseline(null);
    setDraftText('');
    setPhase('loading');
    setError('');
    setNotice('');
    setUncertainDesired(null);
    pendingWrite.current = null;
    pendingReconciliation.current = null;
    guardState.current = { dirty: false, uncertain: false };
  }, [ownerKey]);

  useEffect(() => {
    if (previousLease.current === access.writerLease) return;
    previousLease.current = access.writerLease;
    if (baseline === null && loadedOwner.current === ownerKey) {
      loadedOwner.current = null;
      setPhase('loading');
      setError('');
      setNotice('');
      setReload(value => value + 1);
    }
  }, [access.writerLease, baseline, ownerKey]);

  const owns = useCallback((key: string, requestGeneration: number, writerLease?: string) => mounted.current
    && current.current.ownerKey === key && generation.current === requestGeneration
    && (writerLease === undefined || current.current.access.writerLease === writerLease), []);

  useEffect(() => {
    if (!visible || loadedOwner.current === ownerKey) return;
    const requestGeneration = generation.current;
    const capturedAccess = current.current.access;
    const capturedDocumentId = current.current.documentId;
    loadedOwner.current = ownerKey;
    void readDocumentAliases(capturedAccess, capturedDocumentId).then(value => {
      if (!owns(ownerKey, requestGeneration, capturedAccess.writerLease)) return;
      const next = readBaseline(value, capturedDocumentId);
      setBaseline(next);
      setDraftText(aliasesText(next.aliases));
      setPhase('idle');
      setError('');
      setNotice('');
      guardState.current = { dirty: false, uncertain: false };
    }).catch(reason => {
      if (!owns(ownerKey, requestGeneration, capturedAccess.writerLease)) return;
      loadedOwner.current = null;
      setPhase('loading');
      setError(errorMessage(reason));
    });
  }, [ownerKey, owns, reload, visible]);

  const parsed = (() => {
    try { return { aliases: aliasesFromText(draftText), error: '' }; }
    catch (reason) { return { aliases: [] as string[], error: errorMessage(reason) }; }
  })();
  const dirty = baseline !== null && (parsed.error.length > 0 || !sameAliases(parsed.aliases, baseline.aliases));
  const busy = phase === 'saving' || phase === 'reconciling';
  const guardState = useRef({ dirty: false, uncertain: false });
  guardState.current = { dirty, uncertain: phase === 'uncertain' || uncertainDesired !== null };
  const guard = useCallback(async () => {
    const operation = inFlight.current;
    if (operation) await operation;
    if (guardState.current.dirty || guardState.current.uncertain) throw new Error('Save or discard names before leaving.');
  }, []);

  useEffect(() => {
    registerGuard(guard);
    return () => registerGuard(null);
  }, [guard, registerGuard]);

  function track(operation: Promise<void>, capturedOwner?: { ownerKey: string; access: { writerLease: string } }) {
    inFlight.current = operation;
    void operation.finally(() => {
      if (inFlight.current !== operation) return;
      inFlight.current = null;
      const pending = pendingWrite.current;
      if (pending?.operation === operation) pendingWrite.current = null;
      const reconciliation = pendingReconciliation.current;
      if (reconciliation?.operation === operation) pendingReconciliation.current = null;
      if (capturedOwner && mounted.current && current.current.ownerKey === capturedOwner.ownerKey
        && current.current.access.writerLease !== capturedOwner.access.writerLease) {
        if (pending?.dispatched) {
          setUncertainDesired(pending.desired);
          setPhase('uncertain');
          setError('The writer session changed after the names request was sent. Check saved names before trying again.');
          setNotice('The original names request is retained; it will not be replayed automatically.');
          guardState.current = { dirty: true, uncertain: true };
        } else if (reconciliation?.operation === operation) {
          setUncertainDesired(reconciliation.desired);
          setPhase('uncertain');
          setError('The writer session changed while checking saved names. Check saved names again before leaving.');
          setNotice('No names request was replayed. The check can be repeated with the current writer session.');
          guardState.current = { dirty: true, uncertain: true };
        } else {
          setPhase(previous => previous === 'saving' || previous === 'reconciling' ? 'idle' : previous);
          setError(previous => previous || 'The writer session changed. Review names and save again.');
          setNotice('');
        }
      }
    }).catch(() => {});
  }

  function discard() {
    if (!baseline || busy || phase === 'uncertain') return;
    setDraftText(aliasesText(baseline.aliases));
    setPhase('idle');
    setError('');
    setNotice('Names draft discarded.');
    setUncertainDesired(null);
    guardState.current = { dirty: false, uncertain: false };
  }

  function keepDraft() {
    if (phase !== 'conflict' || busy) return;
    setPhase('idle');
    setError('');
    setNotice('Your names are kept as a draft. Save again to apply them to the latest list.');
    setUncertainDesired(null);
  }

  function save() {
    if (!baseline || !dirty || busy || phase === 'conflict' || phase === 'uncertain') return;
    let desired: string[];
    try { desired = aliasesFromText(draftText); }
    catch (reason) { setError(errorMessage(reason)); setNotice(''); return; }
    const capturedOwner = current.current;
    const requestGeneration = generation.current;
    const capturedBaseline = { aliases: [...baseline.aliases], sourceEpoch: baseline.sourceEpoch };
    const operation = (async () => {
      let writeDispatched = false;
      setPhase('saving'); setError(''); setNotice('Checking the latest saved names…');
      try {
        await capturedOwner.beforeSave?.();
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        const latest = readBaseline(await readDocumentAliases(capturedOwner.access, capturedOwner.documentId), capturedOwner.documentId);
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        if (!sameAliases(latest.aliases, capturedBaseline.aliases)) {
          pendingWrite.current = null;
          setBaseline(latest);
          setPhase('conflict');
          setError('These names changed elsewhere. Review the latest list before saving.');
          setNotice('Your draft is still shown below.');
          return;
        }
        setNotice('Saving names…');
        writeDispatched = true;
        if (pendingWrite.current) pendingWrite.current.dispatched = true;
        const acknowledgement = await setDocumentAliases(capturedOwner.access, capturedOwner.documentId, latest.sourceEpoch, desired);
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        if (!validEpochs(acknowledgement) || !isStrictlyNewerEpoch(latest.sourceEpoch, acknowledgement.source)) {
          throw new Error('The names save acknowledgment did not advance the saved story.');
        }
        const saved = { aliases: desired, sourceEpoch: acknowledgement.source };
        setBaseline(saved);
        setDraftText(aliasesText(desired));
        setPhase('idle'); setError(''); setNotice('Names saved.'); setUncertainDesired(null);
        guardState.current = { dirty: false, uncertain: false };
        pendingWrite.current = null;
        try { capturedOwner.onSaved?.(); } catch { /* A refresh callback cannot change a confirmed durable write. */ }
      } catch (reason) {
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        if (writeDispatched) {
          setUncertainDesired(desired);
          setPhase('uncertain');
          setError(`The names save may have completed. Check saved names before trying again. ${errorMessage(reason)}`);
          setNotice('The original names request is retained; it will not be replayed automatically.');
          guardState.current = { dirty: true, uncertain: true };
        } else {
          pendingWrite.current = null;
          setPhase('idle'); setError(errorMessage(reason)); setNotice('');
        }
      }
    })();
    pendingWrite.current = { operation, ownerKey: capturedOwner.ownerKey, writerLease: capturedOwner.access.writerLease, desired: [...desired], dispatched: false };
    track(operation, capturedOwner);
  }

  function checkSaved() {
    const desired = uncertainDesired;
    if (!desired || busy) return;
    const capturedOwner = current.current;
    const requestGeneration = generation.current;
    const operation = (async () => {
      setPhase('reconciling'); setError(''); setNotice('Checking saved names…');
      try {
        const latest = readBaseline(await readDocumentAliases(capturedOwner.access, capturedOwner.documentId), capturedOwner.documentId);
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        setBaseline(latest);
        if (sameAliases(latest.aliases, desired)) {
          setDraftText(aliasesText(latest.aliases));
          setPhase('idle'); setUncertainDesired(null); setError(''); setNotice('Names saved. The current list matches your request.');
          guardState.current = { dirty: false, uncertain: false };
          try { capturedOwner.onSaved?.(); } catch { /* A refresh callback cannot change a confirmed durable write. */ }
        } else {
          setPhase('conflict'); setUncertainDesired(null);
          setError('The saved names differ from your request. Review the current list before saving again.');
          setNotice('Keep your draft or discard it explicitly.');
        }
      } catch (reason) {
        if (!owns(capturedOwner.ownerKey, requestGeneration, capturedOwner.access.writerLease)) return;
        setPhase('uncertain'); setError(`The saved names could not be checked. ${errorMessage(reason)}`); setNotice('No retry was sent. Check saved names again when the project is available.');
        guardState.current = { dirty: true, uncertain: true };
      }
    })();
    pendingReconciliation.current = { operation, desired: [...desired] };
    track(operation, capturedOwner);
  }

  const canEdit = !!baseline && !busy && phase !== 'uncertain' && phase !== 'conflict';
  const close = async () => {
    try { await guard(); onClose(); }
    catch (reason) { setError(errorMessage(reason)); setNotice('Save or discard names before closing.'); }
  };
  const status = phase === 'loading' && !error ? 'Reading saved names…' : phase === 'saving' ? 'Saving names…' : phase === 'reconciling' ? 'Checking saved names…' : '';
  return <section className="document-aliases" aria-label="Names and aliases" hidden={!visible} aria-busy={busy}>
    <header className="document-aliases-heading">
      <div><h2>{title}</h2><p className="document-aliases-copy">Keep alternate names and transliterations with this saved story material.</p></div>
      <button type="button" className="document-aliases-close" onClick={() => { void close(); }} disabled={disabled || busy}>Close names</button>
    </header>
    {status && <p className="document-aliases-status" role="status">{status}</p>}
    {error && <p className="document-aliases-error" role="alert">{error}</p>}
    {!baseline && error && <button type="button" className="secondary-button" disabled={busy || disabled} onClick={() => { loadedOwner.current = null; setReload(value => value + 1); }}>Reload names</button>}
    <label htmlFor={inputId}>Alternate names and transliterations</label><textarea id={inputId} aria-describedby={hintId} value={draftText} disabled={!canEdit || disabled} onChange={event => { setDraftText(event.currentTarget.value); setError(''); setNotice(''); }} placeholder="One name per line" />
    <p id={hintId} className="document-aliases-copy">One name per line, up to {MAX_ALIASES} names. Your original spellings are preserved.</p>
    {phase === 'conflict' && baseline && <div className="document-aliases-conflict" aria-label="Latest saved names"><strong>Latest saved names</strong><p>{baseline.aliases.length ? baseline.aliases.join(' · ') : 'No names are currently saved.'}</p></div>}
    {parsed.error && dirty && <p className="document-aliases-error" role="alert">{parsed.error}</p>}
    {notice && <p className="document-aliases-notice" role="status">{notice}</p>}
    <div className="document-aliases-actions">
      <button type="button" className="primary-button" disabled={disabled || !dirty || !!parsed.error || busy || phase === 'conflict' || phase === 'uncertain'} onClick={save}>Save names</button>
      <button type="button" className="secondary-button" disabled={disabled || !dirty || busy || phase === 'uncertain' || !baseline} onClick={discard}>Discard changes</button>
      {phase === 'uncertain' && <button type="button" className="secondary-button" disabled={disabled || busy} onClick={checkSaved}>Check saved names</button>}
      {phase === 'conflict' && <button type="button" className="secondary-button" disabled={disabled || busy} onClick={keepDraft}>Keep my names</button>}
    </div>
  </section>;
}
