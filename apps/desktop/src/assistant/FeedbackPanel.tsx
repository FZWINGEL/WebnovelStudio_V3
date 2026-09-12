import { useCallback, useEffect, useRef, useState } from 'react';
import type { Scope } from '../editor';
import { bodyHash, canonicalJson, snapshotFromEditor } from '../editor';
import { captureRevisionScope } from '../editor';
import type { DocumentSession, SessionState } from '../editor';
import { DEFAULT_LOOKUP_ALLOWANCE, discussionRetry, readDiscussion, retryDiscussionSave, saveDiscussionDraft, startDiscussion, stopDiscussion, type ComposerBody, type DiscussionRun, type DiscussionView, type LookupAllowance, type StartDiscussion } from '../ipc/discussions';
import { readProposals, type PreparedProposal, type Proposal } from '../ipc/proposals';
import { ComposerSession, composerIntent, emptyComposer } from './composer';
import { ContextInspector } from './ContextInspector';
import type { LookupDeliveryState } from './LookupContextView';
import { GuidancePanel } from './GuidancePanel';
import { ProposalPanel } from './ProposalPanel';
import { SourcePinsPanel } from './SourcePinsPanel';
import { SafeBriefEditor, validBriefText } from './SafeBriefEditor';
import type { SourceChoice } from '../ipc/sourcePins';
import { useProviders } from '../providers';
import { sameModel } from '../ipc/providers';

function detail(reason: unknown): string { return reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail) : reason instanceof Error ? reason.message : 'The discussion could not be updated. Your text is retained.'; }
function uncertain(reason: unknown): boolean { return !reason || typeof reason !== 'object' || !('code' in reason) || ['UncertainOutcome', 'ReconciliationRequired', 'StaleWriterLease'].includes(String(reason.code)); }
function sameAccess(left: { projectId: string; operationNamespace: string; session: string; writerLease: string }, right: { projectId: string; operationNamespace: string; session: string; writerLease: string }): boolean {
  return left.projectId === right.projectId && left.operationNamespace === right.operationNamespace && left.session === right.session && left.writerLease === right.writerLease;
}
const activeRun = (run: DiscussionRun) => ['queued', 'running', 'stopping'].includes(run.status);
function assistantName(run?: DiscussionRun): string { return run?.providerBinding?.modelId === 'gpt-5.6-luna' ? 'GPT-5.6-Luna' : run?.providerBinding ? run.providerBinding.modelId : 'Test assistant'; }
function sameLookupAllowance(left: LookupAllowance | undefined, right: LookupAllowance | undefined): boolean {
  return !!left && !!right && left.maxAdditionalInvocations === right.maxAdditionalInvocations
    && left.totalInputBytes === right.totalInputBytes && left.totalOutputBytes === right.totalOutputBytes;
}
/** The run points at the initial packet; a completed lookup answer records its final packet on the assistant message. */
export function finalContextPacketIdForRun(view: Pick<DiscussionView, 'messages'> | null, run: Pick<DiscussionRun, 'id' | 'packetId'> | undefined): string | undefined {
  if (!run) return undefined;
  return [...(view?.messages ?? [])].reverse().find(item => item.role === 'assistant' && item.runId === run.id && item.packetId)?.packetId ?? run.packetId;
}
function lookupStateLabel(state: NonNullable<DiscussionRun['lookup']>['invocations'][number]['state']): string {
  switch (state) {
    case 'prepared': return 'prepared · not sent';
    case 'claimed': return 'in progress · delivery not confirmed';
    case 'needsContext': return 'read complete · more context requested';
    case 'completed': return 'answer received';
    case 'failed': return 'failed · answer not confirmed';
    case 'stopped': return 'stopped';
    default: return 'delivery unknown';
  }
}
function lookupPacketDelivered(inputDelivered: boolean): boolean {
  return inputDelivered;
}
function lookupDeliveryState(invocation: NonNullable<DiscussionRun['lookup']>['invocations'][number]): LookupDeliveryState {
  if (invocation.state === 'prepared') return 'prepared';
  return invocation.inputDelivered ? 'delivered' : 'unconfirmed';
}

/** A shell action selects a writing mode; it never submits a provider request by itself. */
export type AssistantAction = {
  kind: 'draft' | 'develop' | 'revise' | 'discuss';
  nonce: number;
};

function hasDocumentText(document: ReturnType<typeof snapshotFromEditor>): boolean {
  return document.body.content.some(block => block.type !== 'sceneBreak'
    && (block.content ?? []).some(inline => inline.type === 'text' && inline.text.trim().length > 0));
}

function appServerDeliveryLabels(delivery: NonNullable<DiscussionRun['providerResult']>['appServer']): string[] {
  if (!delivery || delivery.submission === 'notSent') return [
    'App-server acknowledgment: the turn was not sent.',
    'App-server terminal: not applicable.',
    'Request settlement: closed before dispatch; no automatic retry was made.',
  ];
  const acknowledgment = delivery.submission === 'uncertain'
    ? 'App-server acknowledgment: uncertain; it may have accepted this turn. It was not automatically retried.'
    : 'App-server acknowledgment: the owned turn was acknowledged.';
  const terminal = delivery.terminal === 'completed'
    ? 'App-server terminal: completed.'
    : delivery.terminal === 'interrupted'
      ? 'App-server terminal: interrupted. A local stop does not confirm that upstream processing has stopped.'
      : delivery.terminal === 'failed'
        ? 'App-server terminal: failed. The saved result was not automatically retried.'
        : 'App-server terminal: not confirmed.';
  const settlement = delivery.requestSettled && delivery.connection === 'reusable'
    ? 'Request settlement: settled; the shared app-server is reusable for another request.'
    : delivery.requestSettled
      ? 'Request settlement: settled; the app-server connection was closed.'
      : 'Request settlement: unresolved. It was not automatically retried.';
  return [acknowledgment, terminal, settlement];
}

function ResponseDetails({ run }: { run: DiscussionRun }) {
  const result = run.providerResult;
  if (!result) return null;
  const binding = result.binding;
  const requested = [assistantName(run), binding.reasoning ? `${binding.reasoning === 'xhigh' ? 'Extra high' : binding.reasoning} reasoning` : null, binding.serviceTier === 'priority' ? 'Fast' : binding.serviceTier].filter(Boolean).join(' · ');
  const usage = result.delivery?.usage;
  const appServerLabels = appServerDeliveryLabels(result.appServer);
  return <details className="response-details"><summary>Response details</summary>
    <p className="small-copy">Requested {requested}. {result.effectiveIdentity === null ? 'The provider did not confirm its effective model settings.' : result.effectiveIdentity}</p>
    {result.reportedModel && <p className="small-copy">Provider-reported model: {result.reportedModel}. This is recorded separately from the requested model.</p>}
    <p className="small-copy">{usage ? `Provider-reported usage: ${usage.inputTokens?.toLocaleString() ?? 'unknown'} input tokens; ${usage.outputTokens?.toLocaleString() ?? 'unknown'} output tokens.` : result.usage ? `Provider-reported usage: ${result.usage.inputTokens.toLocaleString()} input tokens and ${result.usage.outputTokens.toLocaleString()} output tokens, including ${result.usage.reasoningOutputTokens.toLocaleString()} reasoning tokens.` : 'Usage is unavailable for this response.'}</p>
    {result.delivery && <p className="small-copy">{result.delivery.submission === 'responseReceived' ? result.status === 'completed' ? 'The API returned a complete response to the saved request.' : 'Response headers were received; the saved result may be partial.' : result.delivery.submission === 'uncertain' ? 'The request may have reached the API, but delivery could not be confirmed. It was not automatically retried.' : 'The API request was not sent.'}</p>}
    {result.appServer ? appServerLabels.map(label => <p className="small-copy" key={label}>{label}</p>) : <p className="small-copy">{result.cleanup === 'settled' ? result.delivery ? 'The local HTTP request has finished.' : 'The local provider process has finished.' : 'Local process cleanup could not be confirmed.'} {(result.status === 'stopped' || (result.delivery && result.delivery.submission !== 'notSent' && result.status !== 'completed')) && 'Stopping locally does not confirm that the upstream service stopped processing or charging.'}</p>}
  </details>;
}

function ProposalResponse({ run, content, hasCandidates }: { run: DiscussionRun; content: string; hasCandidates: boolean }) {
  const [expanded, setExpanded] = useState(false);
  return <>
    {run.intent === 'continue' && <p className="small-copy">Continue chapter · {run.basis === 'reviewed' ? 'Reviewed story' : 'Working draft'}</p>}
    <p className="discussion-state">{run.status === 'completed' && hasCandidates ? 'Suggestions are ready to review below.' : run.status === 'completed' ? 'The assistant did not return a usable edit. Your writing is unchanged.' : `The suggestion response is ${run.status} and cannot be applied.`}</p>
    <details onToggle={event => setExpanded(event.currentTarget.open)}>
      <summary>View saved response</summary>
      {expanded && <p>{content}</p>}
    </details>
  </>;
}

export function FeedbackPanel({ session, state, title, documentKind, sources = [], selection, visible, onClose, registerSaver, onPrepareProposal, onApplyProposal, assistantAction }: {
  session: DocumentSession; state: SessionState; title: string; documentKind?: string; selection: { scope: Scope; nonce: number } | null; visible: boolean;
  sources?: SourceChoice[];
  onClose: () => void; registerSaver: (save: (() => Promise<void>) | null) => void;
  onPrepareProposal?: (proposal: Proposal, text: string, operationId: string) => Promise<PreparedProposal>;
  onApplyProposal?: (proposal: Proposal, prepared: PreparedProposal) => Promise<void>;
  assistantAction?: AssistantAction;
}) {
  const providers = useProviders();
  const model = providers.state?.catalog.models.find(model => sameModel(model.key, providers.state!.settings.active));
  const activeProviderId = providers.state?.settings.active.providerId;
  const lookupSupported = activeProviderId === 'codex' || activeProviderId === 'mock';
  const modelReady = !providers.busy && !!providers.state && providers.state.dispatch.kind !== 'blocked';
  const [view, setView] = useState<DiscussionView | null>(null);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [body, setBody] = useState<ComposerBody>(emptyComposer);
  const [error, setError] = useState('');
  const [pinNotice, setPinNotice] = useState('');
  const [sending, setSending] = useState(false);
  const [savingResponse, setSavingResponse] = useState(false);
  const [pending, setPending] = useState<StartDiscussion | null>(null);
  const [scopeBusy, setScopeBusy] = useState(false);
  const [scopeStale, setScopeStale] = useState(false);
  const [reload, setReload] = useState(0);
  const [guidanceEpoch, setGuidanceEpoch] = useState(0);
  const [sourceAdoption, setSourceAdoption] = useState<{ documentId: string; nonce: number } | null>(null);
  const [sourcesPending, setSourcesPending] = useState(false);
  const [briefFocus, setBriefFocus] = useState(0);
  const briefLauncher = useRef<HTMLButtonElement>(null);
  const [guidanceAdoption, setGuidanceAdoption] = useState<{ text: string; originMessageId: string; nonce: number } | null>(null);
  const guidanceChanged = useCallback(() => setGuidanceEpoch(value => value + 1), []);
  const sendingRef = useRef(false);
  const controller = useRef<ComposerSession | null>(null);
  const lastAssistantAction = useRef<number | null>(null);
  const mounted = useRef(true);
  const owner = useRef(session); owner.current = session;
  const isCurrent = () => mounted.current && owner.current === session;
  const composer = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false);
  const compositionWaiters = useRef<Array<() => void>>([]);
  const polling = useRef(false);
  useEffect(() => {
    let cancelled = false; const access = session.projectAccess;
    void readProposals(access, state.head.documentId).then(result => {
      if (!cancelled && sameAccess(access, session.projectAccess)) setProposals(result);
    }).catch(reason => { if (!cancelled) setError(detail(reason)); });
    return () => { cancelled = true; };
  }, [guidanceEpoch, session, state.head.documentId, state.head.version]);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const documentId = state.head.documentId;
  const currentIntent = composerIntent(body);
  const canSuggestEdits = !!body.scope && (documentKind === 'chapter'
    ? ['passage', 'blocks', 'wholeDocument'].includes(body.scope.kind)
    : ['blocks', 'wholeDocument'].includes(body.scope.kind));
  const save = useRef(async () => {});
  save.current = async () => {
    if (composing.current) await new Promise<void>(resolve => compositionWaiters.current.push(resolve));
    await controller.current?.save();
  };
  function update(next: ComposerBody, keepRetry = false) {
    const previousBody = controller.current?.body ?? body;
    const previousIntent = composerIntent(previousBody);
    const nextIntent = composerIntent(next);
    const leavingDiscussionForRestrictedChapter = documentKind === 'chapter'
      && previousIntent === 'discuss' && nextIntent !== 'discuss';
    const removedTransientPins = leavingDiscussionForRestrictedChapter ? next.pinnedDocumentIds.length : 0;
    if (removedTransientPins > 0) {
      next = { ...next, pinnedDocumentIds: [] };
      setPinNotice(`Removed discussion-only source${removedTransientPins === 1 ? '' : 's'} from this writing request. Saved sources are unchanged and remain available in Discuss. You can carry approved directions into chapter writing through an approved writing brief.`);
    } else if (previousIntent !== nextIntent && nextIntent === 'discuss') {
      setPinNotice('');
    }
    if (!keepRetry) next = { ...next, previousRunId: undefined };
    if (composerIntent(next) !== 'continue') next = { ...next, basis: undefined };
    if (composerIntent(next) === 'discuss') next = { ...next, safeBrief: undefined };
    else {
      next = { ...next, lookup: undefined };
      if (!keepRetry && next.safeBrief && canonicalJson(next.scope) !== canonicalJson(controller.current?.body.scope ?? null)) {
        next = { ...next, safeBrief: { ...next.safeBrief, confirmed: false } };
      }
    }
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
    lastAssistantAction.current = null;
    controller.current = null; polling.current = false; sendingRef.current = false;
    setView(null); setProposals([]); setBody(emptyComposer()); setPinNotice(''); setPending(null); setSending(false); setSavingResponse(false); setScopeBusy(false); setScopeStale(false); setGuidanceAdoption(null);
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
      update({ ...controller.current!.body, intent: composerIntent(controller.current!.body) === 'continue' ? 'proposeEdits' : controller.current!.body.intent, basis: undefined, scope: { kind: 'passage', start: selected.start, end: selected.end, quote: selected.quote, sourceBodyHash: hash } });
      composer.current?.focus();
    }).catch(reason => { if (!cancelled) setError(detail(reason)); }).finally(() => { if (!cancelled) setScopeBusy(false); });
    return () => { cancelled = true; };
  }, [selection, !!view]);
  async function wholeDocumentScope(currentController: ComposerSession): Promise<ComposerBody['scope']> {
    const source = structuredClone(session.body);
    const hash = await bodyHash(canonicalJson(source));
    if (!isCurrent() || controller.current !== currentController) return null;
    if (canonicalJson(source) !== canonicalJson(session.body)) throw new Error('The document changed. Capture the editing scope again.');
    return captureRevisionScope(source, hash, 'wholeDocument');
  }
  async function activateAssistantAction(action: AssistantAction) {
    const currentController = controller.current;
    if (!currentController || sendingRef.current || scopeBusy || pending) return;
    const current = currentController.body;
    try {
      if (action.kind === 'discuss') {
        update({ ...current, intent: 'discuss' });
      } else if (action.kind === 'draft' && documentKind === 'chapter') {
        update({ ...current, intent: 'continue', basis: current.basis ?? 'working', scope: null });
      } else if (action.kind === 'develop' || action.kind === 'revise' || action.kind === 'draft') {
        const scope = action.kind === 'revise' && current.scope && !scopeStale ? current.scope : await wholeDocumentScope(currentController);
        if (!scope) return;
        // The hash can resolve after the author starts typing. Merge the mode
        // into the latest body so an explicit action never replaces new text.
        const latest = controller.current?.body ?? current;
        update({ ...latest, intent: 'proposeEdits', basis: undefined, scope });
      }
      composer.current?.focus();
    } catch (reason) { if (isCurrent()) setError(detail(reason)); }
  }
  useEffect(() => {
    if (!assistantAction || !view || !controller.current || scopeBusy || sendingRef.current || pending || lastAssistantAction.current === assistantAction.nonce) return;
    lastAssistantAction.current = assistantAction.nonce;
    void activateAssistantAction(assistantAction);
  }, [assistantAction?.kind, assistantAction?.nonce, !!view, scopeBusy, !!pending]);
  useEffect(() => {
    let cancelled = false;
    if (!body.scope) { setScopeStale(false); return; }
    void bodyHash(canonicalJson(session.body)).then(hash => { if (!cancelled) setScopeStale(hash !== body.scope!.sourceBodyHash); }).catch(() => { if (!cancelled) setScopeStale(true); });
    return () => { cancelled = true; };
  }, [body.scope, state.generation, session]);
  async function chooseRevisionScope(kind: 'blocks' | 'wholeDocument') {
    if (!controller.current || scopeBusy || sendingRef.current || pending || !session.state.editable) return;
    const currentController = controller.current;
    const source = structuredClone(session.body);
    const selected = structuredClone(currentController.body.scope);
    setScopeBusy(true); setError('');
    try {
      const hash = await bodyHash(canonicalJson(source));
      if (!isCurrent() || controller.current !== currentController) return;
      if (canonicalJson(source) !== canonicalJson(session.body)) throw new Error('The chapter changed. Choose the editing scope again.');
      update({ ...currentController.body, intent: 'proposeEdits', scope: captureRevisionScope(source, hash, kind, selected) });
      composer.current?.focus();
    } catch (reason) { if (isCurrent()) setError(detail(reason)); }
    finally { if (isCurrent()) setScopeBusy(false); }
  }
  async function send(checkPending = false) {
    if (!checkPending && !modelReady) return;
    const selectedModel = providers.state?.settings.active;
    if (!controller.current || sendingRef.current || composing.current || scopeBusy || (sourcesPending && !checkPending)) return;
    const submittedController = controller.current;
    const submitted = structuredClone(submittedController.body);
    if (!submitted.text.trim() && !pending) return;
    const submittedIntent = composerIntent(submitted);
    if (!checkPending && documentKind === 'chapter' && submittedIntent !== 'discuss' && submitted.pinnedDocumentIds.length > 0) {
      setError('Remove discussion-only sources below or switch to Discuss before sending this chapter request.');
      return;
    }
    if (!checkPending && submittedIntent === 'proposeEdits' && !canSuggestEdits) {
      setError(documentKind === 'chapter' ? 'Select a passage or choose Whole chapter before requesting suggested edits.' : 'Choose Whole document before requesting a reviewable development draft.');
      return;
    }
    if (!checkPending && submittedIntent === 'continue' && (documentKind !== 'chapter' || !submitted.basis || submitted.scope)) {
      setError('Choose a story basis before continuing a chapter.'); return;
    }
    if (!checkPending && submitted.safeBrief && (!submitted.safeBrief.confirmed || !validBriefText(submitted.safeBrief.text))) {
      setError('Approve the writing brief or remove it before sending.'); return;
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
          if (submitted.scope && await bodyHash(canonicalJson(session.body)) !== submitted.scope.sourceBodyHash) throw { code: 'StaleScope', detail: 'The text changed. Capture the editing scope again, or discuss the whole document.' };
          request = { access: session.projectAccess, operationId: crypto.randomUUID(), expected: session.state.head, instruction: submitted.text, intent: submittedIntent === 'discuss' ? undefined : submittedIntent, scope: submitted.scope, pinnedDocumentIds: submitted.pinnedDocumentIds,
            modelSelection: selectedModel ? { ...selectedModel } : undefined,
            basis: submittedIntent === 'continue' ? submitted.basis : undefined,
            safeBrief: submittedIntent !== 'discuss' && documentKind === 'chapter' ? submitted.safeBrief : undefined,
            budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' }, previousRunId: submitted.previousRunId ?? null,
            lookup: submittedIntent === 'discuss' ? submitted.lookup : undefined };
        }
        setPending(request);
        const result = await startDiscussion({ ...request, access: session.projectAccess });
        if (result.run.owner.projectId !== request.access.projectId || result.run.owner.operationNamespace !== request.access.operationNamespace || result.run.target.documentId !== documentId || result.run.operationId !== request.operationId) throw new Error('The discussion response did not match this request.');
        if (request.intent === 'continue' && (result.run.intent !== 'continue' || result.run.basis !== request.basis)) throw new Error('The response did not confirm the story basis for this continuation.');
        const binding = result.packet.options.providerBinding;
        if (request.modelSelection?.providerId === 'codex' || request.modelSelection?.providerId.startsWith('openai-compatible:')) {
          const resolvedDefault = binding?.profileVersion === 'codex-stdin.author.v1';
          if (!binding || binding.providerId !== request.modelSelection.providerId || binding.modelId !== request.modelSelection.modelId || (!(resolvedDefault && request.modelSelection.reasoning == null) && binding.reasoning !== request.modelSelection.reasoning) || (!(resolvedDefault && request.modelSelection.serviceTier == null) && binding.serviceTier !== request.modelSelection.serviceTier)
            || JSON.stringify(result.run.providerBinding) !== JSON.stringify(binding)) throw new Error('The response did not confirm the model settings for this request.');
          if (request.modelSelection.providerId.startsWith('openai-compatible:') && (!binding.http || binding.profileVersion !== 'openai-chat-completions.v1')) throw new Error('The response did not confirm the API connection for this request.');
        } else if (binding || result.run.providerBinding) throw new Error('The local test request unexpectedly returned a live provider binding.');
        const brief = result.packet.receipt.safeBrief;
        if (request.safeBrief ? !brief || brief.text !== request.safeBrief.text || brief.textHash !== await bodyHash(request.safeBrief.text)
          || (brief.originMessageId ?? null) !== (request.safeBrief.originMessageId ?? null) : !!brief) throw new Error('The response did not confirm the approved writing brief.');
        const packetLookup = result.packet.receipt.lookup;
        if (request.lookup ? !sameLookupAllowance(request.lookup, packetLookup?.allowance) || !sameLookupAllowance(request.lookup, result.run.lookup?.allowance)
          || packetLookup?.completedInvocations !== 0 : packetLookup || result.run.lookup) throw new Error('The response did not confirm the bounded story lookup allowance.');
        return result;
      });
      if (!isCurrent()) return;
      confirmed = true;
      setPending(null);
      setView(previous => previous ? { ...previous, threadId: result.threadId, messages: [...previous.messages.filter(item => item.id !== result.userMessage.id), result.userMessage], runs: [...previous.runs.filter(item => item.id !== result.run.id), result.run] } : previous);
      const sentBody = request ? { text: request.instruction, intent: request.intent, basis: request.basis, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, previousRunId: request.previousRunId ?? undefined, safeBrief: request.safeBrief, lookup: request.lookup } : submitted;
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
  async function checkSavedResponse(runId?: string) {
    if (sendingRef.current || savingResponse) return;
    setSavingResponse(true); setError('');
    try {
      await session.reconcile();
      if (!isCurrent()) return;
      if (runId) {
        const access = session.projectAccess;
        const result = await retryDiscussionSave(access, documentId, runId);
        if (!isCurrent() || !sameAccess(access, session.projectAccess)) return;
        if (result.documentId !== documentId) throw new Error('This saved response belongs to another document.');
        setView(result);
      }
      await refresh();
      if (isCurrent() && !controller.current) setReload(value => value + 1);
    } catch (reason) { if (isCurrent()) setError(detail(reason)); }
    finally { if (isCurrent()) setSavingResponse(false); }
  }
  const locked = sending || !!pending || scopeBusy || savingResponse;
  const latest = view?.runs.at(-1);
  const latestPacketId = finalContextPacketIdForRun(view, latest);
  const lookupInvocations = latest?.lookup?.invocations ?? [];
  const [selectedLookupPacketId, setSelectedLookupPacketId] = useState<string | null>(null);
  useEffect(() => {
    setSelectedLookupPacketId(null);
  }, [latest?.id, session.projectAccess.projectId, session.projectAccess.operationNamespace]);
  const selectedLookup = selectedLookupPacketId ? lookupInvocations.find(item => item.packetId === selectedLookupPacketId) : undefined;
  const contextPacketId = selectedLookup?.packetId ?? latestPacketId;
  const contextLookupInvocation = selectedLookup ?? lookupInvocations.find(item => item.packetId === latestPacketId);
  const contextLookupDelivery = contextLookupInvocation ? lookupDeliveryState(contextLookupInvocation) : undefined;
  const contextDelivered = selectedLookup ? lookupPacketDelivered(selectedLookup.inputDelivered) : latest?.lookup && lookupInvocations.length > 0
    ? lookupPacketDelivered(lookupInvocations.find(item => item.packetId === latestPacketId)?.inputDelivered ?? false)
    : latest?.dispatchState === 'delivered';
  // The wire carries `null` for an absent delivery; the prop takes `undefined`.
  const contextAppServerDelivery = (contextPacketId === latestPacketId ? latest?.providerResult?.appServer : undefined) ?? undefined;
  const latestIsCurrentProject = latest?.owner.projectId === session.projectAccess.projectId && latest?.owner.operationNamespace === session.projectAccess.operationNamespace;
  const pin = (id: string) => { if (!locked && controller.current && !controller.current.body.pinnedDocumentIds.includes(id)) update({ ...controller.current.body, pinnedDocumentIds: [...controller.current.body.pinnedDocumentIds, id] }); };
  const canIncludeTransientSource = documentKind !== 'chapter' || currentIntent === 'discuss';
  const hasRestrictedChapterPins = documentKind === 'chapter' && currentIntent !== 'discuss' && body.pinnedDocumentIds.length > 0;
  const pinActionNotice = hasRestrictedChapterPins
    ? 'This chapter writing request still includes discussion-only sources. Remove them below before sending, or switch to Discuss.'
    : pinNotice;
  const openBrief = (origin?: { id: string; content: string }) => {
    if (locked || !controller.current) return;
    update({ ...controller.current.body, intent: composerIntent(controller.current.body) === 'continue' ? 'continue' : 'proposeEdits', safeBrief: origin ? { text: origin.content, originMessageId: origin.id, confirmed: false } : controller.current.body.safeBrief ?? { text: '', originMessageId: null, confirmed: false } });
    setBriefFocus(value => value + 1);
  };
  const draftLabel = documentKind === 'chapter' ? (hasDocumentText(session.body) ? 'Continue chapter' : 'Draft chapter') : `Develop ${title}`;
  return <aside className="feedback persistent-feedback" aria-label="Document discussion" style={visible ? undefined : { display: 'none' }}>
    <div className="feedback-heading"><h2>Writing assistant</h2><button onClick={onClose} aria-label="Hide discussion">Hide</button></div>
    <p className="panel-intro">Draft, revise, or discuss {title}. Select text to focus on a passage.</p>
    <div className="scope-controls"><span>{model?.label ?? 'Model unavailable'}</span><span className="session-tag">{modelReady ? providers.state?.dispatch.kind === 'codexCli' || providers.state?.dispatch.kind === 'claudeCli' ? 'Live AI connected' : providers.state?.dispatch.kind === 'openAiCompatible' ? 'API endpoint configured' : providers.state?.dispatch.kind === 'localMock' ? 'Local test model · no AI calls' : 'No live AI connected' : providers.busy ? 'Checking model…' : 'Not connected'}</span></div>
    {!modelReady && <p className="discussion-state">{providers.busy ? 'Checking your saved model choice…' : providers.state?.dispatch.detail || 'Check Settings before sending. Your writing and feedback stay saved.'}</p>}
    {view?.workerIssues?.map(issue => <div key={issue.runId} className="discussion-error response-save-notice" role="alert"><p>{issue.detail}</p><button disabled={locked} onClick={() => void checkSavedResponse(issue.runId)}>Retry saving response</button></div>)}
    <div className="feedback-scroll">
      {currentIntent !== 'discuss' && body.safeBrief && <SafeBriefEditor value={body.safeBrief} disabled={locked} focusKey={briefFocus}
        onChange={safeBrief => update({ ...body, safeBrief })} onRemove={() => { update({ ...body, safeBrief: undefined }); briefLauncher.current?.focus(); }} />}
      <details className="request-options"><summary>Request options</summary>
        <GuidancePanel key={`guidance/${session.projectAccess.projectId}/${documentId}`} session={session} documentId={documentId} adoption={guidanceAdoption} refreshKey={`${guidanceEpoch}/${view?.runs.at(-1)?.id ?? ''}`} onChanged={guidanceChanged} />
        <SourcePinsPanel key={`sources/${session.projectAccess.projectId}/${documentId}`} session={session} documentId={documentId} sources={sources} adoption={sourceAdoption} restricted={currentIntent !== 'discuss'} disabled={locked} onChanged={guidanceChanged} onPendingChange={setSourcesPending} />
      </details>
      {!view && !error && <p role="status">Opening discussion…</p>}
       {view && !view.messages.length && <div className="feedback-empty"><p>What would you like to write or improve?</p><span>Draft new material, revise the current document, or ask about pacing, choices, and earlier details. Your discussion is saved with this document.</span></div>}
      {view?.messages.map(item => {
        const run = view.runs.find(run => run.id === item.runId);
        const isProposalOutput = item.role === 'assistant' && (run?.intent === 'proposeEdits' || run?.intent === 'continue');
        const canAdaptBrief = documentKind === 'chapter' && run && (run.intent ?? 'discuss') === 'discuss' && run.owner.projectId === session.projectAccess.projectId && run.owner.operationNamespace === session.projectAccess.operationNamespace;
        return <article className="feedback-note" key={item.id}><div>{item.role === 'user' ? 'You' : assistantName(run)}{item.role === 'assistant' && run && run.status !== 'completed' && <span>{run.status} · incomplete</span>}</div>{item.scope && <blockquote>{item.scope.quote}</blockquote>}{isProposalOutput && run ? <ProposalResponse run={run} content={item.content} hasCandidates={proposals.some(proposal => proposal.runId === run.id)} /> : <p>{item.content}</p>}{!isProposalOutput && <button className="text-button" disabled={locked} onClick={() => setGuidanceAdoption(previous => ({ text: item.content, originMessageId: item.id, nonce: (previous?.nonce ?? 0) + 1 }))}>Keep as guidance</button>}{canAdaptBrief && <button className="quiet-button" disabled={locked} onClick={() => openBrief(item)}>Adapt as writing brief</button>}{item.role === 'assistant' && run && <ResponseDetails run={run} />}</article>;
      })}
      {view?.runs.filter(run => activeRun(run)).map(run => {
        const issue = view.workerIssues?.find(issue => issue.runId === run.id);
        return <section key={run.id} className="feedback-note"><div>{assistantName(run)} <span>{issue ? 'Response needs saving' : run.status === 'stopping' ? 'Stopping…' : run.status === 'queued' ? 'Preparing…' : 'Responding…'}</span></div>{!issue && run.status === 'stopping' && <p className="discussion-state" role="status">Finishing the stop request. Your partial response stays saved.</p>}{run.intent === 'proposeEdits' || run.intent === 'continue' ? !issue && run.status !== 'stopping' && <p className="discussion-state">{run.intent === 'continue' ? 'Preparing a chapter continuation…' : 'Preparing suggested edits…'}</p> : run.outputText && <p>{run.outputText}</p>}{!issue && <button disabled={run.status === 'stopping' || savingResponse} onClick={() => void stop(run)}>Stop response</button>}</section>;
      })}
      {proposals.length > 0 && <ProposalPanel key={`${session.projectAccess.projectId}/${session.projectAccess.operationNamespace}/${documentId}`} access={session.projectAccess} proposals={proposals} disabled={!session.state.editable} onPrepareProposal={onPrepareProposal} onApplyProposal={onApplyProposal} onRefresh={refresh} />}
      {latest && !activeRun(latest) && latest.status !== 'completed' && <p className="discussion-state">This response is {latest.status}. {latest.stopReason === 'context_stale' ? 'The story changed before it could start.' : ''}<button disabled={locked || !latestIsCurrentProject} onClick={() => void prepareRetry(latest)}>Prepare another attempt</button></p>}
      {latest && lookupInvocations.length > 1 && <label className="context-call-selector" htmlFor="discussion-context-call"><span id="discussion-context-call-label">Context for model call</span><select aria-labelledby="discussion-context-call-label" id="discussion-context-call" value={selectedLookupPacketId ?? contextPacketId ?? latest.packetId} disabled={locked} onChange={event => setSelectedLookupPacketId(event.target.value)}>{lookupInvocations.map((invocation, index) => <option key={invocation.packetId} value={invocation.packetId}>Call {index + 1} · {lookupStateLabel(invocation.state)}</option>)}</select></label>}
      {latest && contextPacketId && (latestIsCurrentProject ? <ContextInspector access={session.projectAccess} packetId={contextPacketId} delivered={contextDelivered} appServerDelivery={contextAppServerDelivery} lookupDelivery={contextLookupDelivery} refreshKey={`${state.head.version}/${guidanceEpoch}`} {...(canIncludeTransientSource ? { onPin: pin } : {})} pinDisabled={locked || sourcesPending} onKeepSource={id => { if (!locked && !sourcesPending) setSourceAdoption(previous => ({ documentId: id, nonce: (previous?.nonce ?? 0) + 1 })); }} /> : <p className="small-copy">Discussion retained from the original project. A new request will use this copy’s story context.</p>)}
    </div>
    <form className="feedback-form" onSubmit={event => { event.preventDefault(); void send(); }}>
      {body.previousRunId && <div className="retry-notice"><p>Another attempt at the same feedback. Uses current story sources and retains the original one-use guidance if it is still active. Editing the feedback, selection, or included sources starts a new request.</p><button type="button" className="text-button" disabled={locked} onClick={() => update({ ...body, previousRunId: undefined })}>Use as a new request</button></div>}
      {body.scope && <div className="quoted-scope"><div className="scope-title"><strong>{body.scope.kind === 'wholeDocument' ? documentKind === 'chapter' ? 'Whole chapter' : 'Whole document' : body.scope.kind === 'blocks' ? 'Selected paragraphs' : 'Selected passage'}</strong><button type="button" disabled={locked} className="text-button" onClick={() => update({ ...body, intent: 'discuss', scope: null })}>{currentIntent === 'proposeEdits' ? 'Discuss instead' : 'Use whole document'}</button></div><blockquote>{body.scope.quote || <em>Empty document</em>}</blockquote>{scopeStale && <p className="stale-notice">The manuscript changed. Capture the editing scope again before sending.</p>}</div>}
      {pinActionNotice && <p className="discussion-state" role="status">{pinActionNotice}</p>}
      {body.pinnedDocumentIds.length > 0 && <div className="source-pins">{body.pinnedDocumentIds.map(id => <button type="button" key={id} disabled={locked} onClick={() => update({ ...body, pinnedDocumentIds: body.pinnedDocumentIds.filter(pin => pin !== id) })}>{sources.find(source => source.id === id)?.title ?? 'Unavailable source'} · remove</button>)}</div>}
      <div className="intent-controls" role="group" aria-label="Feedback action"><span className="intent-label">Writing mode</span><button type="button" className={currentIntent === 'continue' ? 'intent-button active' : 'intent-button'} aria-label={draftLabel} aria-pressed={currentIntent === 'continue'} disabled={locked} onClick={() => void activateAssistantAction({ kind: documentKind === 'chapter' ? 'draft' : 'develop', nonce: Date.now() })}>{draftLabel}</button><button type="button" className={currentIntent === 'proposeEdits' ? 'intent-button active' : 'intent-button'} aria-pressed={currentIntent === 'proposeEdits'} disabled={locked} onClick={() => update({ ...body, intent: 'proposeEdits' })}>Suggest edits</button><button type="button" className={currentIntent === 'discuss' ? 'intent-button active' : 'intent-button'} aria-pressed={currentIntent === 'discuss'} disabled={locked} onClick={() => update({ ...body, intent: 'discuss' })}>Discuss</button></div>
      {currentIntent === 'proposeEdits' && !!documentKind && <div className="revision-scope-controls" role="group" aria-label="Editing scope">
        <span>Change the editing scope</span>
        {documentKind === 'chapter' && <button type="button" className="secondary-button" disabled={locked || scopeStale || !body.scope?.start || !body.scope.end} aria-pressed={body.scope?.kind === 'blocks'} onClick={() => void chooseRevisionScope('blocks')}>Selected paragraphs</button>}
        <button type="button" className="secondary-button" disabled={locked} aria-pressed={body.scope?.kind === 'wholeDocument'} onClick={() => void chooseRevisionScope('wholeDocument')}>{documentKind === 'chapter' ? 'Whole chapter' : 'Whole document'}</button>
        {body.scope?.kind === 'blocks' && <p className="small-copy">The complete selected paragraphs, including their formatting and scene breaks, may change.</p>}
        {body.scope?.kind === 'wholeDocument' && <p className="small-copy">Every part of this {documentKind === 'chapter' ? 'chapter' : 'document'} may change. Review the complete suggestion before applying it.</p>}
      </div>}
      {currentIntent === 'proposeEdits' && !canSuggestEdits && <p className="small-copy proposal-requirement">{documentKind === 'chapter' ? 'Select a passage or choose Whole chapter to request suggested edits.' : 'Choose Whole document to request a reviewable development draft.'}</p>}
      {currentIntent === 'continue' && <div className="continuation-basis"><label htmlFor="continuation-basis">Story basis</label><select id="continuation-basis" value={body.basis ?? 'working'} disabled={locked} onChange={event => update({ ...body, basis: event.target.value as 'working' | 'reviewed' })}><option value="working">Working draft</option><option value="reviewed">Reviewed story</option></select><p className="small-copy">{body.basis === 'reviewed' ? 'Uses the reviewed chapters before this one. Every earlier chapter needs a current review.' : 'Uses the current draft, including unreviewed chapters.'} New paragraphs will be added at the end only after you review and apply them.</p></div>}
      {currentIntent !== 'discuss' && <div className="safe-brief-status"><span>{body.safeBrief ? body.safeBrief.confirmed ? 'Writing brief approved' : 'Writing brief needs approval' : 'Writing brief · optional'}</span><button ref={briefLauncher} className="text-button" type="button" disabled={locked} onClick={() => openBrief()}>{body.safeBrief ? 'Edit brief' : 'Add writing brief'}</button></div>}
      {currentIntent === 'discuss' && <label className="lookup-opt-in" htmlFor="discussion-lookup"><input id="discussion-lookup" type="checkbox" aria-label="Look up story details when needed" checked={!!body.lookup} disabled={locked || (!lookupSupported && !body.lookup)} onChange={event => update({ ...body, lookup: event.target.checked ? { ...DEFAULT_LOOKUP_ALLOWANCE } : undefined })} /><span><strong>Look up story details when needed</strong><small>{lookupSupported ? 'Up to 3 model calls. Each call may use additional credits.' : body.lookup ? 'Story lookup currently uses Codex. Turn this off to send one API response.' : 'Story lookups are available through Codex.'}</small></span></label>}
      <label htmlFor="discussion-composer">{currentIntent === 'continue' ? 'What should happen next?' : currentIntent === 'proposeEdits' ? body.scope?.kind === 'wholeDocument' ? `Request edits for this ${documentKind === 'chapter' ? 'chapter' : 'document'}` : body.scope?.kind === 'blocks' ? 'Request edits for these paragraphs' : 'Request edits for this passage' : body.scope && body.scope.kind !== 'wholeDocument' ? 'Discuss this passage' : 'Discuss this document'}</label>
      <textarea id="discussion-composer" ref={composer} value={body.text} disabled={!view || locked} maxLength={16000} placeholder="Make this moment more emotional, but keep the ending…" onChange={event => update({ ...body, text: event.target.value })} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; compositionWaiters.current.splice(0).forEach(resolve => resolve()); }} />
      {error && <div className="discussion-error" role="alert">{error}{!pending && <button type="button" disabled={locked} onClick={() => void checkSavedResponse()}>Check saved discussion</button>}{!view && <button type="button" onClick={() => setReload(value => value + 1)}>Retry loading discussion</button>}{view && !pending && <button type="button" onClick={() => void save.current().then(() => setError('')).catch(reason => setError(detail(reason)))}>Retry saving discussion</button>}</div>}
      <div className="form-actions"><span>{sourcesPending ? 'Check saved sources before sending a new request.' : hasRestrictedChapterPins ? 'Remove discussion-only sources before sending.' : currentIntent !== 'discuss' ? 'Review a suggestion before applying it.' : 'Discussion never changes the manuscript.'}</span>{pending && !sending ? <button type="button" onClick={() => void send(true)}>Check request</button> : <button className="primary-button" disabled={!modelReady || (!!body.lookup && !lookupSupported) || !view || locked || sourcesPending || hasRestrictedChapterPins || scopeStale || !body.text.trim() || view.runs.some(activeRun) || (currentIntent === 'proposeEdits' && !canSuggestEdits) || (currentIntent === 'continue' && (documentKind !== 'chapter' || !body.basis)) || (currentIntent !== 'discuss' && !!body.safeBrief && (!body.safeBrief.confirmed || !validBriefText(body.safeBrief.text)))}>Send</button>}</div>
    </form>
  </aside>;
}
