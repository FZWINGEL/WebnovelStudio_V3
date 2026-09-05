import { useEffect, useRef, useState } from 'react';
import type { DocumentSession } from '../editor/session';
import {
  readGuidance,
  saveGuidance,
  type GuidanceScope,
  type GuidanceVersion,
  type SaveGuidance,
} from '../ipc/guidance';

const MAX_UI_LENGTH = 4096;
const MAX_BACKEND_BYTES = 16 * 1024;

type Adoption = { text: string; originMessageId: string; nonce: number };
type GuidanceDraft = {
  guidanceId: string;
  expectedVersion: string;
  text: string;
  scope: GuidanceScope;
  originMessageId: string | null;
};
type PendingWrite = { request: SaveGuidance; action: 'save' | 'remove' };
type ContextIdentity = {
  session: DocumentSession;
  documentId: string;
  projectId: string;
  operationNamespace: string;
  sessionId: string;
};

const scopeLabels: Record<GuidanceScope, string> = {
  request: 'Next request',
  document: 'This document',
  project: 'This project',
};

function clone<T>(value: T): T {
  return structuredClone(value);
}

function detail(reason: unknown): string {
  if (reason && typeof reason === 'object' && 'detail' in reason) return String(reason.detail);
  if (reason instanceof Error) return reason.message;
  return 'The writing guidance could not be saved. Your text is retained.';
}

function errorCode(reason: unknown): string | null {
  return reason && typeof reason === 'object' && 'code' in reason ? String(reason.code) : null;
}

function uncertain(reason: unknown): boolean {
  const code = errorCode(reason);
  return !code || ['UncertainOutcome', 'ReconciliationRequired', 'StaleWriterLease'].includes(code);
}

function unconfirmed(message: string): never {
  // A malformed acknowledgment may arrive after the database committed. Keep
  // the exact operation available for reconciliation instead of issuing a new
  // operation ID.
  throw { code: 'UncertainOutcome', detail: message };
}

function utf8Bytes(value: string): number {
  return typeof TextEncoder === 'undefined' ? value.length : new TextEncoder().encode(value).length;
}

function numericVersion(value: string): bigint | null {
  if (!/^(0|[1-9][0-9]*)$/u.test(value)) return null;
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}

function contextOf(session: DocumentSession, documentId: string): ContextIdentity {
  const access = session.projectAccess;
  return {
    session,
    documentId,
    projectId: access.projectId,
    operationNamespace: access.operationNamespace,
    sessionId: access.session,
  };
}

function sameContext(current: ContextIdentity | null, captured: ContextIdentity): boolean {
  if (!current || current.session !== captured.session || current.documentId !== captured.documentId
    || current.projectId !== captured.projectId || current.operationNamespace !== captured.operationNamespace
    || current.sessionId !== captured.sessionId) return false;
  const access = captured.session.projectAccess;
  return access.projectId === captured.projectId
    && access.operationNamespace === captured.operationNamespace
    && access.session === captured.sessionId;
}

/** The durable author-room instruction editor. */
export function GuidancePanel({ session, documentId, adoption, refreshKey, onChanged }: {
  session: DocumentSession;
  documentId: string;
  adoption: Adoption | null;
  refreshKey: string;
  onChanged: () => void;
}) {
  const [rows, setRows] = useState<GuidanceVersion[]>([]);
  const [draft, setDraft] = useState<GuidanceDraft | null>(null);
  const [pending, setPending] = useState<PendingWrite | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const mounted = useRef(false);
  const currentContext = useRef<ContextIdentity | null>(null);
  const contextKey = `${session.projectAccess.projectId}/${session.projectAccess.operationNamespace}/${session.projectAccess.session}/${documentId}`;
  const previousContextKey = useRef<string | null>(null);
  const readGeneration = useRef(0);
  const writeGeneration = useRef(0);
  const savingRef = useRef(false);
  const pendingRef = useRef<PendingWrite | null>(null);
  const focusForm = useRef(false);
  const directionInput = useRef<HTMLTextAreaElement>(null);
  const form = useRef<HTMLDivElement>(null);
  const previousSession = useRef<DocumentSession | null>(null);

  currentContext.current = contextOf(session, documentId);

  function isCurrent(captured: ContextIdentity): boolean {
    return mounted.current && sameContext(currentContext.current, captured);
  }

  async function reload(captured: ContextIdentity, clearError = false): Promise<boolean> {
    const generation = ++readGeneration.current;
    const access = captured.session.projectAccess;
    if (clearError && isCurrent(captured)) setError('');
    try {
      const result = await readGuidance(access, captured.documentId);
      if (!isCurrent(captured) || readGeneration.current !== generation) return false;
      setRows(result);
      setLoading(false);
      return true;
    } catch (reason) {
      if (!isCurrent(captured) || readGeneration.current !== generation) return false;
      setLoading(false);
      setError(detail(reason));
      return false;
    }
  }

  useEffect(() => {
    mounted.current = true;
    const captured = contextOf(session, documentId);
    const changedContext = previousContextKey.current !== null && (previousSession.current !== session || previousContextKey.current !== contextKey);
    previousSession.current = session;
    previousContextKey.current = contextKey;
    if (changedContext) {
      // A late write from the previous project/document must not hold the new
      // editor hostage or populate its form.
      writeGeneration.current += 1;
      savingRef.current = false;
      pendingRef.current = null;
      setPending(null);
      setSaving(false);
      setDraft(null);
      setDetailsOpen(false);
      focusForm.current = false;
    }
    setLoading(true);
    setNotice('');
    void reload(captured, true);
    return () => {
      mounted.current = false;
      readGeneration.current += 1;
    };
  }, [session, documentId, refreshKey, contextKey]);

  useEffect(() => {
    if (!draft || !focusForm.current) return;
    focusForm.current = false;
    const timer = setTimeout(() => {
      if (!mounted.current) return;
      form.current?.scrollIntoView?.({ block: 'nearest' });
      directionInput.current?.focus();
    }, 0);
    return () => clearTimeout(timer);
  }, [draft]);

  useEffect(() => {
    if (!adoption) return;
    if (!sameContext(currentContext.current, contextOf(session, documentId))) return;
    if (savingRef.current || pendingRef.current) {
      setError('Finish saving this direction before keeping another message.');
      return;
    }
    setError('');
    setNotice('');
    setDetailsOpen(true);
    focusForm.current = true;
    // Opening this editor is intentionally the only effect of adoption. The
    // author must press Save guidance before it becomes durable.
    setDraft({
      guidanceId: crypto.randomUUID(),
      expectedVersion: '0',
      text: adoption.text,
      scope: 'document',
      originMessageId: adoption.originMessageId,
    });
  }, [adoption?.nonce, session, documentId]);

  function beginNew() {
    if (savingRef.current) return;
    setError('');
    setNotice('');
    setDetailsOpen(true);
    focusForm.current = true;
    setDraft({ guidanceId: crypto.randomUUID(), expectedVersion: '0', text: '', scope: 'document', originMessageId: null });
  }

  function edit(item: GuidanceVersion) {
    if (savingRef.current) return;
    setError('');
    setNotice('');
    setDetailsOpen(true);
    focusForm.current = true;
    setDraft({
      guidanceId: item.guidanceId,
      expectedVersion: item.version,
      text: item.text,
      scope: item.scope,
      originMessageId: item.originMessageId,
    });
  }

  function cancelDraft() {
    if (savingRef.current) return;
    setDraft(null);
    setError('');
    setNotice('');
  }

  function validateResponse(result: GuidanceVersion, request: SaveGuidance): void {
    if (!result || typeof result !== 'object') unconfirmed('The guidance response was empty.');
    if (!result.guidanceId || (request.guidanceId && result.guidanceId !== request.guidanceId)) {
      unconfirmed('The guidance response belongs to another instruction.');
    }
    if (result.text !== request.text || result.scope !== request.scope || result.documentId !== request.documentId
      || result.active !== request.active || result.originMessageId !== request.originMessageId) {
      unconfirmed('The guidance response does not match the saved instruction.');
    }
    const expected = numericVersion(request.expectedVersion);
    const actual = numericVersion(result.version);
    if (expected === null || actual === null) unconfirmed('The guidance response has an invalid version.');
    const expectedNext = expected + 1n;
    const validNext = actual === expectedNext;
    const validNoopHead = expected > 0n && actual === expected && result.guidanceId === request.guidanceId;
    if (!validNext && !validNoopHead) {
      unconfirmed('The guidance response advanced to an unexpected version.');
    }
  }

  async function write(request: SaveGuidance, action: PendingWrite['action'], checking = false) {
    if (savingRef.current) return;
    const captured = contextOf(session, documentId);
    const immutable = clone(request);
    const generation = ++writeGeneration.current;
    pendingRef.current = { request: immutable, action };
    setPending({ request: immutable, action });
    savingRef.current = true;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      if (checking) await captured.session.reconcile();
      let result: GuidanceVersion | undefined;
      await captured.session.withLifecycleGuard(async () => {
        await captured.session.flush();
        if (!isCurrent(captured)) throw { code: 'RetiredGuidance', detail: 'This guidance editor is no longer open.' };
        result = await saveGuidance({ ...immutable, access: captured.session.projectAccess });
      });
      if (!isCurrent(captured) || generation !== writeGeneration.current || !result) return;
      validateResponse(result, immutable);
      pendingRef.current = null;
      setPending(null);
      if (action === 'save') setDraft(null);
      const refreshed = await reload(captured);
      if (!isCurrent(captured) || generation !== writeGeneration.current) return;
      setNotice(refreshed ? (action === 'remove' ? 'Writing guidance removed.' : 'Writing guidance saved.') : 'Writing guidance saved. The list will refresh when available.');
      // The parent refreshes the packet inspector only after the durable
      // guidance response has passed identity and protocol validation.
      onChanged();
    } catch (reason) {
      if (!isCurrent(captured) || generation !== writeGeneration.current) return;
      if (uncertain(reason)) {
        pendingRef.current = { request: immutable, action };
        setPending({ request: immutable, action });
        setError(`${detail(reason)} Check guidance before trying again.`);
      } else {
        pendingRef.current = null;
        setPending(null);
        // Definite errors leave the exact draft text in place so the author
        // can correct it or retry with a fresh operation ID.
        setError(detail(reason));
      }
    } finally {
      if (isCurrent(captured) && generation === writeGeneration.current) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  }

  async function saveDraft() {
    if (!draft || savingRef.current) return;
    if (!draft.text.trim()) {
      setError('Write a direction before saving guidance.');
      return;
    }
    const bytes = utf8Bytes(draft.text);
    if (draft.text.length > MAX_UI_LENGTH || bytes > MAX_BACKEND_BYTES) {
      setError(bytes > MAX_BACKEND_BYTES
        ? 'This direction is too large to save. Shorten it before saving.'
        : 'This adopted message is longer than the 4,096-character editor limit. Shorten it before saving.');
      return;
    }
    const request: SaveGuidance = {
      access: session.projectAccess,
      operationId: crypto.randomUUID(),
      guidanceId: draft.guidanceId,
      expectedVersion: draft.expectedVersion,
      text: draft.text,
      scope: draft.scope,
      documentId: draft.scope === 'project' ? null : documentId,
      active: true,
      originMessageId: draft.originMessageId,
    };
    await write(request, 'save');
  }

  async function remove(item: GuidanceVersion) {
    if (savingRef.current) return;
    const request: SaveGuidance = {
      access: session.projectAccess,
      operationId: crypto.randomUUID(),
      guidanceId: item.guidanceId,
      expectedVersion: item.version,
      text: item.text,
      scope: item.scope,
      documentId: item.documentId,
      active: false,
      originMessageId: item.originMessageId,
    };
    await write(request, 'remove');
  }

  const locked = saving || !!pending;
  const tooLong = draft ? draft.text.length > MAX_UI_LENGTH || utf8Bytes(draft.text) > MAX_BACKEND_BYTES : false;
  return <section className="writing-guidance" aria-labelledby="writing-guidance-heading">
    <div className="guidance-heading">
      <details className="writing-guidance-details" open={detailsOpen} onToggle={event => setDetailsOpen(event.currentTarget.open)}>
        <summary><h3 id="writing-guidance-heading">Writing guidance <span aria-label={`${rows.length} active guidance instructions`}>{rows.length}</span></h3></summary>
        <div className="guidance-detail-body">
          <p className="small-copy guidance-explainer">Directions you keep for future discussions. Save one when you want the assistant to remember an author choice.</p>
          {loading && !rows.length && <p className="small-copy" role="status">Loading guidance…</p>}
          {!loading && !rows.length && !error && <p className="small-copy guidance-empty">No writing guidance yet. Add a direction you want the assistant to remember.</p>}
          {!!rows.length && <ul className="guidance-list">
            {rows.map(item => <li key={item.guidanceId} className="guidance-item">
              <div className="guidance-meta"><strong>{scopeLabels[item.scope]}</strong><span>Version {item.version}</span></div>
              <p>{item.text}</p>
              {item.originMessageId && <span className="guidance-origin">Kept from a discussion</span>}
              <div className="guidance-actions">
                <button type="button" className="text-button" onClick={() => edit(item)} disabled={locked}>Edit</button>
                <button type="button" className="text-button guidance-remove" onClick={() => void remove(item)} disabled={locked}>Remove</button>
              </div>
            </li>)}
          </ul>}
        </div>
      </details>
      <button type="button" className="quiet-button" onClick={beginNew} disabled={locked}>Add guidance</button>
    </div>
    {draft && <div className="guidance-editor" ref={form} aria-label={draft.expectedVersion === '0' ? 'Add writing guidance' : 'Edit writing guidance'}>
      <div className="guidance-editor-heading"><strong>{draft.expectedVersion === '0' ? 'Add guidance' : 'Edit guidance'}</strong><span>Not saved yet</span></div>
      <label htmlFor="writing-guidance-text">Direction</label>
      <textarea id="writing-guidance-text" ref={directionInput} value={draft.text} maxLength={MAX_UI_LENGTH} disabled={locked} onChange={event => setDraft(current => current ? { ...current, text: event.target.value } : current)} placeholder="Keep the ending intact and make the emotional turn quieter…" />
      <div className="guidance-options">
        <label htmlFor="writing-guidance-scope">Apply to</label>
        <select id="writing-guidance-scope" value={draft.scope} disabled={locked} onChange={event => setDraft(current => current ? { ...current, scope: event.target.value as GuidanceScope } : current)}>
          <option value="request">{scopeLabels.request}</option>
          <option value="document">{scopeLabels.document}</option>
          <option value="project">{scopeLabels.project}</option>
        </select>
        <span className="guidance-count">{draft.text.length}/{MAX_UI_LENGTH}</span>
      </div>
      <p className="small-copy guidance-scope-help">Next request applies to one discussion and its unchanged retries. This document applies to this chapter or note. This project applies across the project.</p>
      {tooLong && <p className="guidance-error" role="alert">{utf8Bytes(draft.text) > MAX_BACKEND_BYTES ? 'This direction is too large to save. Shorten it before saving.' : 'This adopted message is longer than the 4,096-character editor limit. Shorten it before saving.'}</p>}
      <div className="guidance-editor-actions"><button type="button" className="secondary-button" onClick={cancelDraft} disabled={locked}>Cancel</button><button type="button" className="primary-button" onClick={() => void saveDraft()} disabled={locked || tooLong || !draft.text.trim()}>Save guidance</button></div>
    </div>}
    {pending && !saving && <div className="guidance-retry" role="alert"><span>Save could not be confirmed. The exact request is retained.</span><button type="button" className="secondary-button" onClick={() => void write(pending.request, pending.action, true)}>Check guidance</button></div>}
    {saving && <p className="guidance-status" role="status">Saving guidance…</p>}
    {error && !tooLong && <div className="guidance-error" role="alert"><span>{error}</span>{!loading && !pending && <button type="button" className="text-button" onClick={() => void reload(contextOf(session, documentId), true)}>Retry loading</button>}</div>}
    {notice && <p className="guidance-status" role="status">{notice}</p>}
  </section>;
}
