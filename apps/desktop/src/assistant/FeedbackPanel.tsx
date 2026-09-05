import { useCallback, useEffect, useRef, useState } from 'react';
import type { Scope } from '../editor/selection';
import { bodyHash, canonicalJson, snapshotFromEditor } from '../editor/document';
import type { DocumentSession, SessionState } from '../editor/session';
import { discussionRetry, readDiscussion, saveDiscussionDraft, startDiscussion, stopDiscussion, type ComposerBody, type DiscussionRun, type DiscussionView, type StartDiscussion } from '../ipc/discussions';
import { readProposals, type PreparedProposal, type Proposal } from '../ipc/proposals';
import { ComposerSession, composerIntent, emptyComposer } from './composer';
import { ContextInspector } from './ContextInspector';
import { GuidancePanel } from './GuidancePanel';
import { ProposalPanel } from './ProposalPanel';

function detail(reason: unknown): string { return reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail) : reason instanceof Error ? reason.message : 'The discussion could not be updated. Your text is retained.'; }
function uncertain(reason: unknown): boolean { return !reason || typeof reason !== 'object' || !('code' in reason) || ['UncertainOutcome', 'ReconciliationRequired', 'StaleWriterLease'].includes(String(reason.code)); }
function sameAccess(left: { projectId: string; operationNamespace: string; session: string; writerLease: string }, right: { projectId: string; operationNamespace: string; session: string; writerLease: string }): boolean {
  return left.projectId === right.projectId && left.operationNamespace === right.operationNamespace && left.session === right.session && left.writerLease === right.writerLease;
}
const activeRun = (run: DiscussionRun) => ['queued', 'running', 'stopping'].includes(run.status);

export function FeedbackPanel({ session, state, title, documentKind, selection, visible, onClose, registerSaver, onPrepareProposal, onApplyProposal }: {
  session: DocumentSession; state: SessionState; title: string; documentKind?: string; selection: { scope: Scope; nonce: number } | null; visible: boolean;
  onClose: () => void; registerSaver: (save: (() => Promise<void>) | null) => void;
  onPrepareProposal?: (proposal: Proposal, text: string, operationId: string) => Promise<PreparedProposal>;
  onApplyProposal?: (proposal: Proposal, prepared: PreparedProposal) => Promise<void>;
}) {
  const [view, setView] = useState<DiscussionView | null>(null);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [body, setBody] = useState<ComposerBody>(emptyComposer);
  const [error, setError] = useState('');
  const [sending, setSending] = useState(false);
  const [pending, setPending] = useState<StartDiscussion | null>(null);
  const [scopeBusy, setScopeBusy] = useState(false);
  const [scopeStale, setScopeStale] = useState(false);
  const [reload, setReload] = useState(0);
  const [guidanceEpoch, setGuidanceEpoch] = useState(0);
  const [guidanceAdoption, setGuidanceAdoption] = useState<{ text: string; originMessageId: string; nonce: number } | null>(null);
  const guidanceChanged = useCallback(() => setGuidanceEpoch(value => value + 1), []);
  const sendingRef = useRef(false);
  const controller = useRef<ComposerSession | null>(null);
  const mounted = useRef(true);
  const owner = useRef(session); owner.current = session;
  const isCurrent = () => mounted.current && owner.current === session;
  const composer = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false);
  const compositionWaiters = useRef<Array<() => void>>([]);
  const polling = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const documentId = state.head.documentId;
  const currentIntent = composerIntent(body);
  const canSuggestEdits = documentKind === 'chapter' && body.scope?.kind === 'passage';
  const save = useRef(async () => {});
  save.current = async () => {
    if (composing.current) await new Promise<void>(resolve => compositionWaiters.current.push(resolve));
    await controller.current?.save();
  };
  function update(next: ComposerBody, keepRetry = false) {
    if (!keepRetry) next = { ...next, previousRunId: null };
    controller.current?.update(next); setBody(structuredClone(next));
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => { void save.current().catch(reason => { if (isCurrent()) setError(detail(reason)); }); }, 750);
  }
  async function refresh() {
    if (polling.current) return;
    polling.current = true;
    const access = session.projectAccess;
    try {
      const [result, retained] = await Promise.all([readDiscussion(access, documentId), readProposals(access, documentId)]);
      if (isCurrent() && sameAccess(access, session.projectAccess) && result.documentId === documentId) {
        setView(result); setProposals(retained);
      }
    } catch (reason) { if (isCurrent() && sameAccess(access, session.projectAccess)) setError(detail(reason)); }
    finally { if (isCurrent()) polling.current = false; }
  }
  useEffect(() => {
    mounted.current = true;
    controller.current = null; polling.current = false; sendingRef.current = false;
    setView(null); setProposals([]); setBody(emptyComposer()); setPending(null); setSending(false); setScopeBusy(false); setScopeStale(false); setGuidanceAdoption(null);
    registerSaver(() => save.current());
    let cancelled = false;
    const access = session.projectAccess;
    void Promise.all([readDiscussion(access, documentId), readProposals(access, documentId)]).then(([result, retained]) => {
      if (cancelled) return;
      if (result.documentId !== documentId) throw new Error('This discussion belongs to another document.');
      if (!isCurrent() || !sameAccess(access, session.projectAccess)) return;
      const restored = new ComposerSession(documentId, result.draft, () => session.projectAccess, saveDiscussionDraft);
      controller.current = restored; setBody(structuredClone(restored.body)); setView(result); setProposals(retained); setError('');
    }).catch(reason => { if (!cancelled) setError(detail(reason)); });
    return () => { cancelled = true; mounted.current = false; registerSaver(null); if (timer.current) clearTimeout(timer.current); };
  }, [session, reload]);
  useEffect(() => {
    if (!view?.runs.some(activeRun)) return;
    const interval = setInterval(() => { void refresh(); }, 500);
    return () => clearInterval(interval);
  }, [view?.runs.some(activeRun), session]);
  useEffect(() => {
    if (!selection || !controller.current) return;
    let cancelled = false; setScopeBusy(true);
    const selected = selection.scope;
    const selectedBody = snapshotFromEditor(selected.source.toJSON());
    void bodyHash(canonicalJson(selectedBody)).then(hash => {
      if (cancelled) return;
      update({ ...controller.current!.body, scope: { kind: 'passage', start: selected.start, end: selected.end, quote: selected.quote, sourceBodyHash: hash } });
      composer.current?.focus();
    }).catch(reason => { if (!cancelled) setError(detail(reason)); }).finally(() => { if (!cancelled) setScopeBusy(false); });
    return () => { cancelled = true; };
  }, [selection, !!view]);
  useEffect(() => {
    let cancelled = false;
    if (!body.scope) { setScopeStale(false); return; }
    void bodyHash(canonicalJson(session.body)).then(hash => { if (!cancelled) setScopeStale(hash !== body.scope!.sourceBodyHash); }).catch(() => { if (!cancelled) setScopeStale(true); });
    return () => { cancelled = true; };
  }, [body.scope, state.generation, session]);
  async function send(checkPending = false) {
    if (!controller.current || sendingRef.current || composing.current || scopeBusy) return;
    const submittedController = controller.current;
    const submitted = structuredClone(submittedController.body);
    if (!submitted.text.trim() && !pending) return;
    const submittedIntent = composerIntent(submitted);
    if (!checkPending && submittedIntent === 'proposeEdits' && !canSuggestEdits) {
      setError(documentKind === 'chapter' ? 'Select a passage before requesting suggested edits.' : 'Suggested edits are available for chapter passages only.');
      return;
    }
    sendingRef.current = true; setSending(true); setError('');
    let request: StartDiscussion | null = checkPending ? pending : null;
    let confirmed = false;
    try {
      if (checkPending) await session.reconcile();
      const result = await session.withLifecycleGuard(async () => {
        await session.flush(); await submittedController.save();
        if (!isCurrent()) throw { code: 'RetiredDiscussion', detail: 'This discussion is no longer open.' };
        if (!request) {
          if (submitted.scope && await bodyHash(canonicalJson(session.body)) !== submitted.scope.sourceBodyHash) throw { code: 'StaleScope', detail: 'The text changed. Select the passage again, or discuss the whole document.' };
          request = { access: session.projectAccess, operationId: crypto.randomUUID(), expected: session.state.head, instruction: submitted.text, intent: submittedIntent === 'discuss' ? undefined : submittedIntent, scope: submitted.scope, pinnedDocumentIds: submitted.pinnedDocumentIds,
            budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' }, previousRunId: submitted.previousRunId ?? null };
        }
        setPending(request);
        const result = await startDiscussion({ ...request, access: session.projectAccess });
        if (result.run.owner.projectId !== request.access.projectId || result.run.owner.operationNamespace !== request.access.operationNamespace || result.run.target.documentId !== documentId || result.run.operationId !== request.operationId) throw new Error('The discussion response did not match this request.');
        return result;
      });
      if (!isCurrent()) return;
      confirmed = true;
      setPending(null);
      setView(previous => previous ? { ...previous, threadId: result.threadId, messages: [...previous.messages.filter(item => item.id !== result.userMessage.id), result.userMessage], runs: [...previous.runs.filter(item => item.id !== result.run.id), result.run] } : previous);
      const sentBody = request ? { text: request.instruction, intent: request.intent, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, previousRunId: request.previousRunId } : submitted;
      if (submittedController.clearIfUnchanged(sentBody)) { setBody(structuredClone(submittedController.body)); await submittedController.save(); }
      if (!isCurrent()) return;
      await refresh();
    } catch (reason) {
      if (!isCurrent()) return;
      if (!uncertain(reason)) setPending(null);
      setError(`${detail(reason)}${!confirmed && uncertain(reason) && request ? ' Check the request before sending another.' : ''}`);
    } finally { if (isCurrent()) { sendingRef.current = false; setSending(false); } }
  }
  async function prepareRetry(run: DiscussionRun) {
    if (!controller.current || scopeBusy || sendingRef.current || pending) return;
    const currentController = controller.current;
    const access = session.projectAccess;
    setScopeBusy(true); setError('');
    try {
      const draft = await discussionRetry(access, run.id);
      if (!isCurrent() || controller.current !== currentController) return;
      if (session.projectAccess.writerLease !== access.writerLease || draft.previousRunId !== run.id) throw new Error('The discussion changed while preparing another attempt. Try again.');
      update({ ...draft, intent: draft.intent ?? run.intent ?? 'discuss' }, true);
      await currentController.save();
      if (isCurrent()) composer.current?.focus();
    } catch (reason) { if (isCurrent()) setError(detail(reason)); }
    finally { if (isCurrent()) setScopeBusy(false); }
  }
  async function stop(run: DiscussionRun) {
    try { await stopDiscussion(session.projectAccess, run.id); if (isCurrent()) await refresh(); }
    catch (reason) { if (isCurrent()) setError(detail(reason)); }
  }
  const locked = sending || !!pending || scopeBusy;
  const latest = view?.runs.at(-1);
  const latestIsCurrentProject = latest?.owner.projectId === session.projectAccess.projectId && latest?.owner.operationNamespace === session.projectAccess.operationNamespace;
  const pin = (id: string) => { if (!locked && controller.current && !controller.current.body.pinnedDocumentIds.includes(id)) update({ ...controller.current.body, pinnedDocumentIds: [...controller.current.body.pinnedDocumentIds, id] }); };
  return <aside className="feedback persistent-feedback" aria-label="Document discussion" style={visible ? undefined : { display: 'none' }}>
    <div className="feedback-heading"><h2>Discussion</h2><button onClick={onClose} aria-label="Hide discussion">Hide</button></div>
    <p className="panel-intro">Talk through {title}. Select text to focus on a passage.</p>
    <div className="scope-controls"><span>Local test model</span><span className="session-tag">No live AI connected</span></div>
    <div className="feedback-scroll">
      <GuidancePanel key={`${session.projectAccess.projectId}/${documentId}`} session={session} documentId={documentId} adoption={guidanceAdoption} refreshKey={`${guidanceEpoch}/${view?.runs.at(-1)?.id ?? ''}`} onChanged={guidanceChanged} />
      {!view && !error && <p role="status">Opening discussion…</p>}
      {view && !view.messages.length && <div className="feedback-empty"><p>What would you like to improve?</p><span>Ask about pacing, a character’s choices, or an earlier detail. Your discussion is saved with this document.</span></div>}
      {view?.messages.map(item => {
        const run = view.runs.find(run => run.id === item.runId);
        const isProposalOutput = item.role === 'assistant' && run?.intent === 'proposeEdits';
        return <article className="feedback-note" key={item.id}><div>{item.role === 'user' ? 'You' : 'Test assistant'}{item.role === 'assistant' && run && run.status !== 'completed' && <span>{run.status} · incomplete</span>}</div>{item.scope && <blockquote>{item.scope.quote}</blockquote>}{isProposalOutput ? <p className="discussion-state">Suggestions are ready to review below.</p> : <p>{item.content}</p>}{!isProposalOutput && <button className="text-button" disabled={locked} onClick={() => setGuidanceAdoption(previous => ({ text: item.content, originMessageId: item.id, nonce: (previous?.nonce ?? 0) + 1 }))}>Keep as guidance</button>}</article>;
      })}
      {view?.runs.filter(run => activeRun(run)).map(run => <section key={run.id} className="feedback-note"><div>Test assistant <span>{run.status === 'queued' ? 'Preparing…' : 'Responding…'}</span></div>{run.intent === 'proposeEdits' ? <p className="discussion-state">Preparing suggestions for the selected passage…</p> : run.outputText && <p>{run.outputText}</p>}<button onClick={() => void stop(run)}>Stop response</button></section>)}
      {latest?.intent === 'proposeEdits' && !activeRun(latest) && proposals.length === 0 && <p className="proposal-empty" role="status">No valid suggestions were retained. The response remains available as discussion text.</p>}
      {proposals.length > 0 && <ProposalPanel key={`${session.projectAccess.projectId}/${session.projectAccess.operationNamespace}/${documentId}`} access={session.projectAccess} proposals={proposals} disabled={!session.state.editable} onPrepareProposal={onPrepareProposal} onApplyProposal={onApplyProposal} onRefresh={refresh} />}
      {latest && !activeRun(latest) && latest.status !== 'completed' && <p className="discussion-state">This response is {latest.status}. {latest.stopReason === 'context_stale' ? 'The story changed before it could start.' : ''}<button disabled={locked || !latestIsCurrentProject} onClick={() => void prepareRetry(latest)}>Prepare another attempt</button></p>}
      {latest && (latestIsCurrentProject ? <ContextInspector access={session.projectAccess} packetId={latest.packetId} delivered={latest.dispatchState === 'delivered'} refreshKey={`${state.head.version}/${guidanceEpoch}`} onPin={pin} /> : <p className="small-copy">Discussion retained from the original project. A new request will use this copy’s story context.</p>)}
    </div>
    <form className="feedback-form" onSubmit={event => { event.preventDefault(); void send(); }}>
      {body.previousRunId && <div className="retry-notice"><p>Another attempt at the same feedback. Uses current story sources and retains the original one-use guidance if it is still active. Editing the feedback, selection, or included sources starts a new request.</p><button type="button" className="text-button" disabled={locked} onClick={() => update({ ...body, previousRunId: null })}>Use as a new request</button></div>}
      {body.scope && <div className="quoted-scope"><div className="scope-title"><strong>Selected passage</strong><button type="button" disabled={locked} className="text-button" onClick={() => update({ ...body, scope: null })}>Use whole document</button></div><blockquote>{body.scope.quote}</blockquote>{scopeStale && <p className="stale-notice">The manuscript changed. Select the passage again before sending.</p>}</div>}
      {body.pinnedDocumentIds.length > 0 && <div className="source-pins">{body.pinnedDocumentIds.map((id, index) => <button type="button" key={id} disabled={locked} onClick={() => update({ ...body, pinnedDocumentIds: body.pinnedDocumentIds.filter(pin => pin !== id) })}>Included source {index + 1} · remove</button>)}</div>}
      <div className="intent-controls" role="group" aria-label="Feedback action"><span className="intent-label">Work with the manuscript</span><button type="button" className={currentIntent === 'discuss' ? 'intent-button active' : 'intent-button'} aria-pressed={currentIntent === 'discuss'} disabled={locked} onClick={() => update({ ...body, intent: 'discuss' })}>Discuss</button><button type="button" className={currentIntent === 'proposeEdits' ? 'intent-button active' : 'intent-button'} aria-pressed={currentIntent === 'proposeEdits'} disabled={locked} onClick={() => update({ ...body, intent: 'proposeEdits' })}>Suggest edits</button></div>
      {currentIntent === 'proposeEdits' && !canSuggestEdits && <p className="small-copy proposal-requirement">Select a passage in a chapter to request suggested edits. Discussion remains available for this document.</p>}
      <label htmlFor="discussion-composer">{currentIntent === 'proposeEdits' ? 'Request edits for this passage' : body.scope ? 'Discuss this passage' : 'Discuss this document'}</label>
      <textarea id="discussion-composer" ref={composer} value={body.text} disabled={!view || locked} maxLength={16000} placeholder="Make this moment more emotional, but keep the ending…" onChange={event => update({ ...body, text: event.target.value })} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; compositionWaiters.current.splice(0).forEach(resolve => resolve()); }} />
      {error && <div className="discussion-error" role="alert">{error}{!view && <button type="button" onClick={() => setReload(value => value + 1)}>Retry loading discussion</button>}{view && !pending && <button type="button" onClick={() => void save.current().then(() => setError('')).catch(reason => setError(detail(reason)))}>Retry saving discussion</button>}</div>}
      <div className="form-actions"><span>{currentIntent === 'proposeEdits' ? 'Review a suggestion before applying it.' : 'Discussion never changes the manuscript.'}</span>{pending && !sending ? <button type="button" onClick={() => void send(true)}>Check request</button> : <button className="primary-button" disabled={!view || locked || scopeStale || !body.text.trim() || view.runs.some(activeRun) || (currentIntent === 'proposeEdits' && !canSuggestEdits)}>Send</button>}</div>
    </form>
  </aside>;
}
