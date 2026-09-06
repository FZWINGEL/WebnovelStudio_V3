import { useEffect, useRef, useState, type ReactNode } from 'react';
import { readStoryContextSource, type SourceRead } from '../ipc/context';
import { readMemory, readMemorySource, retryMemorySave, startMemory, stopMemory, type DigestCandidate, type MemoryJob, type MemoryRead, type MemoryViewRecord, type StartMemory } from '../ipc/memory';
import type { DocumentSession, SessionState } from '../editor/session';
import { bodyHash, canonicalJson } from '../editor/document';
import { localModel, sameModel, storyMemoryModel } from '../ipc/providers';
import { useProviders } from '../providers/ProviderContext';
import { ContextInspector } from '../assistant/ContextInspector';
import { MemoryPanel, type MemoryInspectedSource, type MemoryItem, type MemoryPanelState, type MemoryView } from './MemoryPanel';

const MEMORY_BUDGET = { modelId: 'mock-story-context' as const, contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' };
const activeStatuses = new Set<MemoryJob['status']>(['queued', 'running', 'stopping']);

function detail(reason: unknown): string {
  return reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail)
    : reason instanceof Error ? reason.message : 'Story memory could not be updated. Your writing is unchanged.';
}
function uncertain(reason: unknown): boolean {
  return !reason || typeof reason !== 'object' || !('code' in reason)
    || ['UncertainOutcome', 'ReconciliationRequired', 'PersistenceUnavailable', 'ProtocolError', 'ProviderInputUnknown'].includes(String(reason.code));
}
function sameHead(left: { documentId: string; version: string; bodyHash: string }, right: { documentId: string; version: string; bodyHash: string }): boolean {
  return left.documentId === right.documentId && left.version === right.version && left.bodyHash === right.bodyHash;
}
function sameSource(left: { projectId: string; documentId: string; revisionId: string; bodyHash: string }, right: { projectId: string; documentId: string; revisionId: string; bodyHash: string }): boolean {
  return left.projectId === right.projectId && left.documentId === right.documentId && left.revisionId === right.revisionId && left.bodyHash === right.bodyHash;
}
function validHead(head: { documentId: string; version: string; bodyHash: string }, documentId: string): boolean {
  return head.documentId === documentId && !!head.version && !!head.bodyHash;
}
function validSource(source: { projectId: string; documentId: string; revisionId: string; bodyHash: string }, access: { projectId: string }, documentId: string): boolean {
  return source.projectId === access.projectId && source.documentId === documentId && !!source.revisionId && !!source.bodyHash;
}
function sameOwner(owner: { projectId: string; operationNamespace: string; jobId: string }, access: { projectId: string; operationNamespace: string }, jobId: string): boolean {
  return owner.projectId === access.projectId && owner.operationNamespace === access.operationNamespace && owner.jobId === jobId;
}
function readableJobOwner(job: MemoryJob, access: { projectId: string; operationNamespace: string }): boolean {
  return sameOwner(job.owner, access, job.id) || (job.historical === true && !!job.owner.projectId && !!job.owner.operationNamespace && job.owner.jobId === job.id && job.source.projectId === job.owner.projectId);
}
function validateJob(job: MemoryJob, access: { projectId: string; operationNamespace: string }, documentId: string, expected?: { documentId: string; version: string; bodyHash: string }): void {
  if (!job.id || job.historical === true || !sameOwner(job.owner, access, job.id) || !validHead(job.target, documentId) || !validSource(job.source, access, documentId)
    || (expected && !sameHead(job.target, expected)) || (job.result && job.result.jobId !== job.id) || (job.view && job.view.jobId !== job.id)) {
    throw new Error('The saved story memory did not match this chapter. Check the saved result before continuing.');
  }
  if (job.result?.candidate && !sameSource(job.result.candidate.source, job.source)) throw new Error('The story memory candidate cited a different source. Check the saved result.');
  if (job.view && !validView(job.view, access, documentId, job.id)) throw new Error('The saved story memory view did not match this chapter.');
}
function validView(view: MemoryViewRecord, access: { projectId: string; operationNamespace: string }, documentId: string, jobId?: string): boolean {
  const ownerValid = view.historical === true
    ? !!view.projectId && !!view.operationNamespace && !view.current
    : view.projectId === access.projectId && view.operationNamespace === access.operationNamespace;
  return !!view.id && ownerValid && view.documentId === documentId
    && (!jobId || view.jobId === jobId) && validHead(view.target, documentId)
    && (validSource(view.source, access, documentId) || (view.historical === true && !view.current && view.source.projectId === view.projectId && view.source.documentId === documentId && !!view.source.revisionId && !!view.source.bodyHash))
    && (!view.candidate || sameSource(view.candidate.source, view.source));
}
function validateRead(read: MemoryRead, access: { projectId: string; operationNamespace: string }, documentId: string): void {
  if (read.documentId !== documentId) throw new Error('The saved story memory belongs to another document.');
  read.jobs.forEach(job => {
    if (job.historical === true) {
      if (!readableJobOwner(job, access) || !validHead(job.target, documentId) || job.source.projectId !== job.owner.projectId || job.source.documentId !== documentId) throw new Error('The recovered story memory belongs to another project or document.');
      if (job.result?.candidate && !sameSource(job.result.candidate.source, job.source)) throw new Error('The recovered story memory candidate cited a different source.');
      if (job.view && (job.view.jobId !== job.id || job.view.documentId !== documentId)) throw new Error('The recovered story memory view did not match this chapter.');
    } else validateJob(job, access, documentId);
  });
  read.views.forEach(view => {
    const job = read.jobs.find(job => job.id === view.jobId);
    if (!validView(view, access, documentId) || !job || view.projectId !== job.owner.projectId
      || view.operationNamespace !== job.owner.operationNamespace || !sameSource(view.source, job.source)) throw new Error('The saved story memory view belongs to another project or document.');
  });
}
function itemRows(candidate: DigestCandidate | null): MemoryItem[] {
  return candidate?.items.map(item => ({ text: item.text, uncertainty: item.uncertainty ?? undefined,
    evidence: item.evidence.map(evidence => ({ quote: evidence.quote })) })) ?? [];
}
function viewKind(view: MemoryViewRecord): MemoryView['kind'] {
  if (!view.policyAvailable) return 'revoked';
  if (view.current) return 'current';
  if (view.sourceChanged) return 'changedSource';
  return 'recoveredHistorical';
}
function displayView(view: MemoryViewRecord): MemoryView {
  return { id: view.id, kind: viewKind(view), createdAt: view.createdAt, items: view.policyAvailable ? itemRows(view.candidate) : [] };
}
function candidateDisplay(job: MemoryJob, currentHead: SessionState['head']): MemoryView | null {
  const candidate = job.result?.candidate;
  if (!candidate || job.view || job.status !== 'completed' || job.result?.outcome !== 'completed') return null;
  return { id: `candidate/${job.id}`, kind: sameHead(job.target, currentHead) ? 'current' : 'changedSource', createdAt: job.result?.createdAt ?? job.updatedAt, items: itemRows(candidate) };
}
function errorForJob(job: MemoryJob): string | undefined {
  if (job.result?.cleanup === 'unresolved') return 'Local process cleanup could not be confirmed. This refresh is interrupted and its result cannot become current memory.';
  if (job.stopReason === 'dispatch_outcome_unknown') return 'The refresh could not confirm its local dispatch. No automatic retry was made. You can request a new refresh.';
  if (job.stopReason === 'recovered_unknown_external_outcome') return job.result
    ? 'The refresh was interrupted when the project closed. Its late result was retained as history and was not installed. You can request a new refresh.'
    : 'The refresh was interrupted when the project closed. Its previous external outcome is unknown. No automatic retry was made.';
  if (job.stopReason === 'author_stopped') return 'You stopped this refresh.';
  return job.result?.error || job.result?.validationError || undefined;
}
type MemorySourceTarget = { viewId: string; snapshotId: string; packetId: string; source: MemoryViewRecord['source']; policyAvailable: boolean; delivered: boolean; historical: boolean };
function stateFromRead(read: MemoryRead, currentHead: SessionState['head']): { state: MemoryPanelState; latest: MemoryJob | null; sources: Map<string, MemorySourceTarget> } {
  const latest = read.jobs.filter(job => job.historical !== true).at(-1) ?? null;
  const sources = new Map<string, MemorySourceTarget>();
  const views = read.views.map(view => {
    const job = read.jobs.find(candidate => candidate.id === view.jobId);
    sources.set(view.id, { viewId: view.id, snapshotId: view.snapshotId, packetId: view.packetId, source: view.source, policyAvailable: view.policyAvailable, delivered: !!job?.result, historical: view.historical === true });
    return displayView(view);
  });
  const pending = latest ? candidateDisplay(latest, currentHead) : null;
  if (pending && latest) {
    views.push(pending);
    if (latest.result?.candidate) sources.set(pending.id, { viewId: pending.id, snapshotId: latest.snapshotId, packetId: latest.packetId, source: latest.source, policyAvailable: true, delivered: !!latest.result, historical: false });
  }
  if (!latest) return { state: views.length ? { kind: 'completed', disposition: 'candidate', views } : { kind: 'empty', views: [] }, latest, sources };
  const pendingSave = read.pendingSave === true || read.pendingJobIds?.includes(latest.id) === true;
  if (pendingSave) {
    return { state: { kind: 'completed', disposition: 'needsReconciliation', message: 'This refresh needs a local check. Check the saved result before refreshing again.', views }, latest, sources };
  }
  if (activeStatuses.has(latest.status)) {
    const phase = latest.status as 'queued' | 'running' | 'stopping';
    return { state: { kind: 'active', phase, views }, latest, sources };
  }
  if (latest.status === 'completed') {
    if (pending) return { state: { kind: 'completed', disposition: 'needsReconciliation', views }, latest, sources };
    if (latest.view || views.length) return { state: { kind: 'completed', disposition: 'candidate', views }, latest, sources };
    return { state: { kind: 'failed', message: errorForJob(latest) || 'Story memory finished without a readable result.', views }, latest, sources };
  }
  if (latest.status === 'stopped' || latest.status === 'interrupted') {
    return { state: { kind: 'interrupted', message: errorForJob(latest), views }, latest, sources };
  }
  return { state: { kind: 'failed', message: errorForJob(latest), views }, latest, sources };
}
async function sourceMatches(read: SourceRead, target: { source: MemoryViewRecord['source']; viewId: string }): Promise<boolean> {
  if (read.descriptor.handle !== target.source.revisionId || !sameSource(read.descriptor.source, target.source)) return false;
  const hash = await bodyHash(canonicalJson(read.body));
  if (hash !== target.source.bodyHash || hash !== read.descriptor.source.bodyHash) return false;
  const blocks = read.body.body.content;
  return read.passages.length === blocks.length && read.passages.every((passage, index) => {
    const block = blocks[index];
    const text = block.type === 'sceneBreak' ? '' : (block.content ?? []).map(node => node.type === 'hardBreak' ? '\n' : node.text).join('');
    return passage.handle === read.descriptor.handle && sameSource(passage.source, read.descriptor.source)
      && passage.blockOrder === index && passage.blockId === block.attrs.id && passage.text === text;
  });
}

function allowanceLabel(providerId: string | undefined): string {
  return providerId === 'codex' ? 'One request · 24 KiB input · 64 KiB retained response' : 'One request · local offline';
}

export function ChapterMemory({ session, state, title, visible, onClose }: {
  session: DocumentSession; state: SessionState; title: string; visible: boolean; onClose(): void;
}) {
  const providers = useProviders();
  const memorySelection = providers.state?.settings.active.providerId === 'mock' ? localModel : storyMemoryModel;
  const model = providers.state?.catalog.models.find(item => sameModel(item.key, memorySelection));
  const modelAvailable = !providers.busy && !!providers.state && !!model && (memorySelection.providerId === 'mock'
    ? providers.state.dispatch.kind === 'localMock'
    : providers.state.codexConnection?.memoryReady ?? (providers.state.codexConnection?.ready === true && model.ready && model.reasoningLevels.includes('xhigh') && model.serviceTiers.some(tier => tier.id === 'priority')));
  const access = session.projectAccess;
  const ownerIdentity = `${access.projectId}/${access.operationNamespace}/${state.head.documentId}`;
  const identity = `${ownerIdentity}/${access.session}/${access.writerLease}`;
  const runtime = useRef({ session, state, identity, ownerIdentity, visible }); runtime.current = { session, state, identity, ownerIdentity, visible };
  const sequence = useRef(0); const readFlight = useRef<Promise<void> | null>(null); const busyRef = useRef(false);
  const readToken = useRef(0);
  const busyToken = useRef(0); const busyOwner = useRef<string | null>(null);
  const latestJob = useRef<MemoryJob | null>(null); const pendingStart = useRef<StartMemory | null>(null);
  const pendingSaveJobIds = useRef(new Set<string>());
  const sourceTargets = useRef(new Map<string, MemorySourceTarget>());
  const [packetViewId, setPacketViewId] = useState<string>();
  const [memoryState, setMemoryState] = useState<MemoryPanelState>({ kind: 'loading', views: [] });
  const [error, setError] = useState(''); const [busy, setBusy] = useState(false); const [inspected, setInspected] = useState<MemoryInspectedSource>();
  function owns(capturedIdentity = identity, capturedSequence = sequence.current): boolean {
    return runtime.current.visible && runtime.current.identity === capturedIdentity && sequence.current === capturedSequence;
  }
  function beginBusy(owner = ownerIdentity): number {
    const token = busyToken.current + 1; busyToken.current = token; busyOwner.current = owner; busyRef.current = true; setBusy(true); return token;
  }
  function endBusy(token: number, owner = ownerIdentity): void {
    if (busyToken.current === token && busyOwner.current === owner) { busyRef.current = false; busyOwner.current = null; setBusy(false); }
  }
  async function readLatest(): Promise<void> {
    if (!runtime.current.visible) return;
    if (readFlight.current) return readFlight.current;
    const capturedIdentity = runtime.current.identity; const capturedSequence = sequence.current; const capturedReadToken = ++readToken.current;
    const access = runtime.current.session.projectAccess; const documentId = runtime.current.state.head.documentId; const head = runtime.current.state.head;
    let flight!: Promise<void>;
    flight = (async () => {
      try {
        const result = await readMemory(access, documentId);
        if (!owns(capturedIdentity, capturedSequence) || readToken.current !== capturedReadToken) return;
        validateRead(result, access, documentId);
        const derived = stateFromRead(result, head);
        latestJob.current = derived.latest; sourceTargets.current = derived.sources;
        pendingSaveJobIds.current = new Set(result.pendingJobIds ?? (result.pendingSave && derived.latest ? [derived.latest.id] : []));
        setMemoryState(derived.state); setError('');
        const inspectedTarget = inspected && derived.sources.get(inspected.viewId);
        if (inspected && (!inspectedTarget || !inspectedTarget.policyAvailable)) setInspected(undefined);
        if (packetViewId && !derived.sources.has(packetViewId)) setPacketViewId(undefined);
      } catch (reason) {
        if (owns(capturedIdentity, capturedSequence) && readToken.current === capturedReadToken) {
          setError(detail(reason));
          if (uncertain(reason) || (reason && typeof reason === 'object' && 'code' in reason && reason.code === 'WriterLeaseExpired')) {
            setInspected(undefined); setPacketViewId(undefined);
            setMemoryState(previous => ({ kind: 'completed', disposition: 'needsReconciliation', message: 'Story memory needs a local connection check before it can be read again.', views: previous.views }));
          }
        }
      }
      finally { if (readFlight.current === flight) readFlight.current = null; }
    })();
    readFlight.current = flight; return flight;
  }
  useEffect(() => {
    const request = ++sequence.current; latestJob.current = null; sourceTargets.current = new Map(); setInspected(undefined); setError('');
    setPacketViewId(undefined); readToken.current += 1; readFlight.current = null;
    if (!visible) return;
    setMemoryState(previous => ({ kind: 'loading', views: previous.views }));
    void readLatest();
    return () => { if (sequence.current === request) sequence.current += 1; };
  }, [identity, visible, state.head.version, state.head.bodyHash]);
  const activeJobId = latestJob.current && activeStatuses.has(latestJob.current.status) ? latestJob.current.id : null;
  useEffect(() => {
    if (!visible || !activeJobId) return;
    const timer = setInterval(() => { void readLatest(); }, 500);
    return () => clearInterval(timer);
  }, [visible, activeJobId, identity]);
  async function refresh() {
    if (!visible || busyRef.current || !modelAvailable || memoryState.kind === 'active') return;
    const selectedModel = providers.state ? structuredClone(memorySelection) : null;
    if (!selectedModel) return;
    const token = beginBusy(); setError(''); setMemoryState(previous => ({ kind: 'active', phase: 'queued', views: previous.views }));
    const capturedOwner = ownerIdentity;
    try {
      let request!: StartMemory;
      await session.withLifecycleGuard(async () => {
        await session.flush();
        if (!runtime.current.visible || runtime.current.ownerIdentity !== capturedOwner) throw new Error('This chapter is no longer open.');
        request = { access: session.projectAccess, operationId: crypto.randomUUID(), expected: session.state.head, budget: MEMORY_BUDGET, modelSelection: selectedModel };
        pendingStart.current = structuredClone(request);
        const job = await startMemory(request);
        validateJob(job, request.access, request.expected.documentId, request.expected);
        latestJob.current = job; pendingStart.current = null;
      });
      if (runtime.current.visible && runtime.current.ownerIdentity === capturedOwner) { readToken.current += 1; readFlight.current = null; await readLatest(); }
    } catch (reason) {
      if (runtime.current.visible && runtime.current.ownerIdentity === capturedOwner) {
        if (uncertain(reason)) setMemoryState(previous => ({ kind: 'completed', disposition: 'needsReconciliation', message: 'The refresh request may have been saved. Check the saved result before refreshing again.', views: previous.views }));
        else { pendingStart.current = null; setMemoryState(previous => ({ kind: 'failed', message: detail(reason), views: previous.views })); }
        setError(detail(reason));
      }
    } finally { endBusy(token); }
  }
  async function stop() {
    const job = latestJob.current; if (!visible || !job || !activeStatuses.has(job.status)) return;
    setError(''); setMemoryState(previous => ({ kind: 'active', phase: 'stopping', views: previous.views }));
    const capturedIdentity = identity;
    try {
      const stopped = await stopMemory(session.projectAccess, job.id);
      if (!owns(capturedIdentity)) return;
      validateJob(stopped, session.projectAccess, state.head.documentId); latestJob.current = stopped; await readLatest();
    } catch (reason) { if (owns(capturedIdentity)) setError(detail(reason)); }
  }
  async function reconcile() {
    if (!visible || busyRef.current) return;
    const pending = pendingStart.current; const candidate = latestJob.current;
    const recoveryJobId = candidate && pendingSaveJobIds.current.has(candidate.id) ? candidate.id : null;
    const needsConnectionCheck = memoryState.kind === 'completed' && memoryState.disposition === 'needsReconciliation';
    if (!pending && !recoveryJobId && !needsConnectionCheck && (!candidate || candidate.status !== 'completed' || !!candidate.view || !candidate.result?.candidate)) return;
    const capturedOwner = ownerIdentity; const token = beginBusy(); setError('');
    try {
      await session.reconcile();
      if (!runtime.current.visible || runtime.current.ownerIdentity !== capturedOwner) return;
      if (pending) {
        const request = { ...pending, access: session.projectAccess };
        const job = await startMemory(request);
        validateJob(job, request.access, request.expected.documentId, request.expected);
        pendingStart.current = null; latestJob.current = job;
      } else {
        // An uncertain background commit can fence the entire actor before
        // read_memory returns its pending IDs. Re-read under the fresh lease
        // before deciding which local terminal or installation to reconcile.
        const access = session.projectAccess;
        const read = await readMemory(access, state.head.documentId);
        if (!runtime.current.visible || runtime.current.ownerIdentity !== capturedOwner) return;
        validateRead(read, access, state.head.documentId);
        const latest = read.jobs.filter(job => !job.historical).at(-1);
        const retryId = recoveryJobId ?? read.pendingJobIds?.at(-1)
          ?? (latest?.status === 'completed' && !latest.view && latest.result?.candidate ? latest.id : null);
        if (retryId) {
          const job = await retryMemorySave(access, retryId);
          validateJob(job, access, state.head.documentId); latestJob.current = job;
        }
      }
      if (runtime.current.visible && runtime.current.ownerIdentity === capturedOwner) { readToken.current += 1; readFlight.current = null; await readLatest(); }
    } catch (reason) { if (runtime.current.visible && runtime.current.ownerIdentity === capturedOwner) setError(detail(reason)); }
    finally { endBusy(token); }
  }
  async function inspect(viewId: string) {
    const target = sourceTargets.current.get(viewId); if (!visible || !target || !target.policyAvailable) return;
    const capturedIdentity = identity; const capturedSequence = sequence.current; setError(''); setInspected(undefined);
    try {
      const result = target.historical ? await readMemorySource(session.projectAccess, target.viewId) : await readStoryContextSource(session.projectAccess, target.snapshotId, target.source.revisionId);
      if (!owns(capturedIdentity, capturedSequence)) return;
      const matches = await sourceMatches(result, { source: target.source, viewId });
      if (!owns(capturedIdentity, capturedSequence)) return;
      if (!matches) throw new Error('The inspected source did not match this story memory.');
      setInspected({ viewId, label: result.descriptor.displayName, passages: result.passages.map(passage => ({ blockId: passage.blockId, text: passage.text })) });
    } catch (reason) { if (owns(capturedIdentity, capturedSequence)) setError(detail(reason)); }
  }
  const packetTarget = packetViewId ? sourceTargets.current.get(packetViewId) : undefined;
  const inspectedPacket: ReactNode = packetTarget && packetViewId ? <ContextInspector access={session.projectAccess} packetId={packetTarget.packetId} delivered={packetTarget.delivered} refreshKey={`${identity}/${packetViewId}`} /> : undefined;
  const frozenJob = latestJob.current;
  const showFrozenProfile = !!frozenJob && (activeStatuses.has(frozenJob.status)
    || (memoryState.kind === 'completed' && memoryState.disposition === 'needsReconciliation'));
  const frozenModelId = showFrozenProfile ? frozenJob.providerBinding?.modelId : undefined;
  const frozenModel = frozenModelId ? providers.state?.catalog.models.find(item => item.key.modelId === frozenModelId) : undefined;
  const currentModelId = memorySelection.modelId;
  const currentProviderId = memorySelection.providerId;
  const modelLabel = frozenJob && showFrozenProfile ? (frozenModelId ? (frozenModel?.label ?? frozenModelId) : 'Local test model')
    : (memorySelection.providerId === 'codex' ? 'GPT-5.6-Luna · Extra high' : model?.label ?? currentModelId ?? (providers.busy ? 'Loading model…' : ''));
  const profileProviderId = showFrozenProfile ? frozenJob?.providerBinding?.providerId : currentProviderId;
  if (!visible) return null;
  return <MemoryPanel documentTitle={title} modelLabel={modelLabel} modelAvailable={modelAvailable}
    allowanceLabel={allowanceLabel(profileProviderId)}
    state={memoryState} busy={busy} error={error} inspectedSource={inspected} onRefresh={() => void refresh()} onStop={() => void stop()}
    inspectedPacket={inspectedPacket} onReconcile={() => void reconcile()} onInspect={viewId => void inspect(viewId)} onInspectPacket={viewId => setPacketViewId(viewId)}
    onCloseInspectedSource={() => setInspected(undefined)} onCloseInspectedPacket={() => setPacketViewId(undefined)} onClose={onClose} />;
}
