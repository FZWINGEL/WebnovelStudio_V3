// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { canonicalJson, bodyHash, type WnsDocument } from '../kernel';
import type { DocumentSession, SessionState } from '../editor/session';
import type { ProjectAccess, Head } from '../ipc/projects';
import type { DigestCandidate, MemoryJob, MemoryRead, MemoryViewRecord } from '../ipc/memory';
import * as memory from '../ipc/memory';
import * as context from '../ipc/context';
import { ChapterMemory } from '../story/ChapterMemory';
import * as providerIpc from '../ipc/providers';
import { ProviderSettingsProvider } from '../providers/ProviderContext';

vi.mock('../ipc/providers', async original => ({ ...await original<typeof import('../ipc/providers')>(), readProviderState: vi.fn() }));

vi.mock('../ipc/memory', () => ({ readMemory: vi.fn(), readMemorySource: vi.fn(), retryMemorySave: vi.fn(), startMemory: vi.fn(), stopMemory: vi.fn() }));
vi.mock('../ipc/context', () => ({
  readStoryContextSource: vi.fn(), preparedStoryContext: vi.fn(), preparedStoryContextIsCurrent: vi.fn(), storyContextSnapshot: vi.fn(), searchStoryContext: vi.fn(),
}));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'memory', session: 'session', writerLease: 'lease' };
const head: Head = { documentId: 'chapter', version: '1', bodyHash: 'head-hash' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'opening' }, content: [{ type: 'text', text: 'The lantern waited.' }] }] } };
const source = { projectId: access.projectId, documentId: head.documentId, revisionId: 'revision', bodyHash: 'source-hash' };
const state: SessionState = { phase: 'editing', generation: '0', savedGeneration: '0', head, saving: false, dirty: false, editable: true, error: null };

function job(overrides: Partial<MemoryJob> = {}): MemoryJob {
  return { id: 'job-1', owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, jobId: 'job-1' }, operationId: 'operation-1', payloadHash: 'payload', target: head, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1', disclosurePolicyVersion: '1', providerBinding: null, status: 'queued', dispatchState: 'pending', stopReason: null, result: null, view: null, historical: false, createdAt: '2026-09-06T12:00:00Z', updatedAt: '2026-09-06T12:00:00Z', ...overrides };
}
function read(jobs: MemoryJob[] = [], views: MemoryViewRecord[] = [], extra: Partial<MemoryRead> = {}): MemoryRead { return { documentId: head.documentId, jobs, views, ...extra }; }
function candidate(): DigestCandidate { return { schemaVersion: 'navigation-digest.v1', source, items: [{ text: 'The lantern is waiting.', uncertainty: null, evidence: [{ blockId: 'opening', fromUtf16: 0, toUtf16: 20, quote: 'The lantern waited.' }] }] }; }
function view(overrides: Partial<MemoryViewRecord> = {}): MemoryViewRecord {
  return { id: 'view-1', jobId: 'job-1', projectId: access.projectId, operationNamespace: access.operationNamespace, documentId: head.documentId, target: head, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1', disclosurePolicyVersion: '1', candidate: candidate(), current: true, sourceChanged: false, policyAvailable: true, historical: false, createdAt: '2026-09-06T12:00:00Z', ...overrides };
}
function createSession() {
  let currentAccess = access;
  const session = {
    get projectAccess() { return currentAccess; },
    get state() { return state; },
    body,
    flush: vi.fn(async () => {}),
    reconcile: vi.fn(async () => { currentAccess = { ...currentAccess, writerLease: 'lease-after-reconcile' }; }),
    withLifecycleGuard: vi.fn(async (work: () => Promise<unknown>) => work()),
  } as unknown as DocumentSession;
  return { session, rotateLease: (lease: string) => { currentAccess = { ...currentAccess, writerLease: lease }; } };
}

let host: HTMLDivElement;
let root: Root;
let session: DocumentSession;
async function render(visible = true) { await act(async () => root.render(<ChapterMemory session={session} state={state} title="The return" visible={visible} onClose={vi.fn()} />)); }
function button(name: string): HTMLButtonElement { return [...host.querySelectorAll('button')].find(item => item.textContent === name) as HTMLButtonElement; }
async function click(name: string) { await act(async () => button(name).click()); }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  vi.mocked(memory.readMemory).mockResolvedValue(read());
  vi.mocked(memory.startMemory).mockResolvedValue(job());
  vi.mocked(memory.retryMemorySave).mockResolvedValue(job({ status: 'completed', view: view() }));
  const made = createSession(); session = made.session;
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('chapter story memory controller', () => {
  it('blocks memory when the checked catalog lacks Astra maintenance traits even if another model is ready', async () => {
    vi.mocked(providerIpc.readProviderState).mockResolvedValue({
      settings: { revision: '3', active: { providerId: 'codex', modelId: 'gpt-6-astra', reasoning: 'high', serviceTier: null }, favorites: [] },
      catalog: { models: [{ key: providerIpc.storyMemoryModel, label: 'GPT-6 Astra', providerLabel: 'Codex', reasoningLevels: ['xhigh'], serviceTiers: [{id:'priority',label:'Fast'}], contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: true, statusDetail: 'Historical reference' }] },
      dispatch: { kind: 'codexCli', detail: 'Connected' }, codexConnection: { ready: true, memoryReady: false, detail: 'Connected' },
      storyMemory: { revision: '0', providerId: 'codex', providerLabel: 'Codex', modelId: 'gpt-6-astra', reasoning: 'low', serviceTier: 'priority', ready: false, detail: 'Connected but Astra maintenance traits are unavailable' },
    });
    await act(async () => root.render(<ProviderSettingsProvider><ChapterMemory session={session} state={state} title="The return" visible onClose={vi.fn()} /></ProviderSettingsProvider>));
    expect([...host.querySelectorAll('button')].find(button=>button.textContent==='Refresh story memory')!.disabled).toBe(true);
    expect(memory.startMemory).not.toHaveBeenCalled();
  });
  it('fails closed when native maintenance state is absent instead of selecting a live fallback', async () => {
    vi.mocked(providerIpc.readProviderState).mockResolvedValue({
      settings: { revision: '0', active: providerIpc.localModel, favorites: [] },
      catalog: { models: [{ key: providerIpc.localModel, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' }] },
      dispatch: { kind: 'localMock', detail: 'No live AI connected' },
    });
    await act(async () => root.render(<ProviderSettingsProvider><ChapterMemory session={session} state={state} title="The return" visible onClose={vi.fn()} /></ProviderSettingsProvider>));
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Refresh story memory')!.disabled).toBe(true);
    expect(memory.startMemory).not.toHaveBeenCalled();
  });
  it('uses Astra low for memory even when the writing picker has another model', async () => {
    vi.mocked(providerIpc.readProviderState).mockResolvedValue({
      settings: { revision: '3', active: { providerId: 'claude', modelId: 'claude-sonnet', reasoning: null, serviceTier: null }, favorites: [] },
      catalog: { models: [{ key: providerIpc.storyMemoryModel, label: 'GPT-6 Astra', providerLabel: 'Codex', reasoningLevels: ['low'], serviceTiers: [{ id: 'priority', label: 'Fast' }], contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: true, statusDetail: 'Connected' }] },
      dispatch: { kind: 'blocked', detail: 'Drafting model is unavailable' },
      codexConnection: { ready: true, detail: 'Connected' },
      storyMemory: { revision: '4', providerId: 'codex', providerLabel: 'Codex', modelId: 'gpt-6-astra', reasoning: 'low', serviceTier: 'priority', ready: true, detail: 'Connected' },
    });
    await act(async () => root.render(<ProviderSettingsProvider><ChapterMemory session={session} state={state} title="The return" visible onClose={vi.fn()} /></ProviderSettingsProvider>));
    expect(host.textContent).toContain('gpt-6-astra · low');
    await click('Refresh story memory');
    expect(memory.startMemory).toHaveBeenCalledOnce();
    expect(vi.mocked(memory.startMemory).mock.calls[0][0].modelSelection).toEqual(providerIpc.storyMemoryModel);
  });
  it('reads when shown without starting a generation, and starts only after Refresh', async () => {
    await render(false); expect(memory.readMemory).not.toHaveBeenCalled(); expect(memory.startMemory).not.toHaveBeenCalled();
    await render(true); expect(memory.readMemory).toHaveBeenCalledOnce(); expect(memory.startMemory).not.toHaveBeenCalled();
    await click('Refresh story memory');
    expect(memory.startMemory).toHaveBeenCalledOnce(); expect(session.flush).toHaveBeenCalledOnce();
    const request = vi.mocked(memory.startMemory).mock.calls[0][0]; expect(request.operationId).toBeTruthy(); expect(request.expected).toEqual(head); expect(request.modelSelection).toEqual(expect.objectContaining({ modelId: 'mock-story-context' }));
  });

  it('keeps an uncertain start on Check saved result with the same operation id', async () => {
    vi.mocked(memory.startMemory).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The start acknowledgment was lost.' }).mockResolvedValueOnce(job({ status: 'queued' }));
    await render(); await click('Refresh story memory');
    expect(host.textContent).toContain('Check the saved result');
    await click('Check saved result');
    expect(memory.startMemory).toHaveBeenCalledTimes(2);
    expect(vi.mocked(memory.startMemory).mock.calls[1][0].operationId).toBe(vi.mocked(memory.startMemory).mock.calls[0][0].operationId);
  });

  it('retains unresolved app-server cleanup as an unresolved request resource', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue(read([job({ status: 'interrupted', result: {
      jobId: 'job-1', eventId: 'event', rawOutput: null, outcome: 'failed', confirmedStdinBytes: null, usage: null,
      cleanup: 'unresolved', error: null, validationError: null, candidate: null, effectiveIdentity: null, createdAt: '2026-09-06T12:00:00Z',
      appServer: { dispatch: { serverGeneration: 'server-1', threadId: 'thread-1', rpcId: 'rpc-1', packetHash: 'a'.repeat(64), requestHash: 'b'.repeat(64) }, submission: 'acknowledged', turnId: 'turn-1', terminal: null, requestSettled: false, connection: 'unresolved' },
    } })]));
    await render();
    expect(host.textContent).toContain('The Codex app-server request resource could not be settled.');
    expect(host.textContent).toContain('This refresh remains unresolved');
    expect(host.textContent).not.toContain('Local process cleanup could not be confirmed');
  });

  it('uses local persistence retry for a completed candidate instead of generating again', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue(read([job({ status: 'completed', result: { jobId: 'job-1', eventId: 'event', rawOutput: '{}', outcome: 'completed', confirmedStdinBytes: null, usage: null, cleanup: 'settled', error: null, validationError: null, candidate: candidate(), effectiveIdentity: null, createdAt: '2026-09-06T12:00:00Z' } })]));
    await render();
    expect(host.textContent).toContain('Check the saved result'); expect(host.textContent).toContain('The lantern is waiting.');
    await click('Check saved result');
    expect(session.reconcile).toHaveBeenCalledOnce();
    expect(memory.retryMemorySave).toHaveBeenCalledWith({ ...access, writerLease: 'lease-after-reconcile' }, 'job-1'); expect(memory.startMemory).not.toHaveBeenCalled();
  });

  it('offers local recovery when native reports a result-save fault without a result payload', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue(read([job({ status: 'completed' })], [], { pendingSave: true, pendingJobIds: ['job-1'] }));
    await render();
    expect(host.textContent).toContain('Check the saved result');
    await click('Check saved result');
    expect(memory.retryMemorySave).toHaveBeenCalledWith({ ...access, writerLease: 'lease-after-reconcile' }, 'job-1'); expect(memory.startMemory).not.toHaveBeenCalled();
  });

  it('recovers an actor fenced by an uncertain background commit before retrying its retained result', async () => {
    vi.mocked(memory.readMemory).mockRejectedValueOnce({ code: 'WriterLeaseExpired', detail: 'The background commit fenced the writer lease.' })
      .mockResolvedValue(read([job({ status: 'running', dispatchState: 'dispatched' })], [], { pendingSave: true, pendingJobIds: ['job-1'] }));
    await render();
    expect(button('Check saved result')).toBeTruthy();
    await click('Check saved result');
    expect(session.reconcile).toHaveBeenCalledOnce();
    expect(memory.retryMemorySave).toHaveBeenCalledWith({ ...access, writerLease: 'lease-after-reconcile' }, 'job-1');
    expect(memory.startMemory).not.toHaveBeenCalled();
    expect(memory.stopMemory).not.toHaveBeenCalled();
  });

  it('labels changed and revoked views while keeping revoked content and evidence hidden', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue(read([job({ status: 'completed', view: view() })], [view({ id: 'changed', current: false, sourceChanged: true }), view({ id: 'old', current: false, sourceChanged: false }), view({ id: 'revoked', policyAvailable: false, candidate: candidate() })]));
    await render();
    expect(host.textContent).toContain('Changed source'); expect(host.textContent).toContain('Recovered historical memory'); expect(host.textContent).toContain('Unavailable memory');
    expect(host.textContent).toContain('Content and evidence are hidden.');
    expect(host.querySelector('.memory-view-revoked')?.textContent).not.toContain('The lantern is waiting.');
    expect([...host.querySelectorAll('button')].filter(item => item.textContent === 'Inspect source')).toHaveLength(2);
  });

  it('only reads exact source text after Inspect source and rejects an unvalidated projection', async () => {
    const exactHash = await bodyHash(canonicalJson(body));
    const exactSource = { ...source, bodyHash: exactHash };
    const exactView = view({ source: exactSource, candidate: { ...candidate(), source: exactSource } });
    vi.mocked(memory.readMemory).mockResolvedValue(read([job({ status: 'completed', view: exactView, source: exactSource })], [exactView]));
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: { handle: exactSource.revisionId, displayName: 'The return', source: exactSource, kind: 'currentDraft', current: true, coverage: 'verbatim', disclosure: { readerPosition: null, visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] }, passages: [{ handle: exactSource.revisionId, source: exactSource, blockId: 'opening', blockOrder: 0, text: 'The lantern waited.' }], body, usedValidatedProjection: false });
    await render(); expect(context.readStoryContextSource).not.toHaveBeenCalled();
    await click('Inspect source'); await vi.waitFor(() => expect(context.readStoryContextSource).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(host.textContent).toContain('Exact source text retained with this memory.'));
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: { handle: exactSource.revisionId, displayName: 'The return', source: exactSource, kind: 'currentDraft', current: true, coverage: 'verbatim', disclosure: { readerPosition: null, visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] }, passages: [], body, usedValidatedProjection: false });
    await click('Inspect source'); await vi.waitFor(() => expect(host.textContent).toContain('The inspected source did not match this story memory.'));
  });

  it('reads recovered historical source through its view receipt', async () => {
    const exactHash = await bodyHash(canonicalJson(body));
    const historicalSource = { ...source, projectId: 'original-project', bodyHash: exactHash };
    const historicalView = view({ id: 'historical-view', jobId: 'historical-job', projectId: 'original-project', operationNamespace: 'old-namespace', historical: true, current: false, source: historicalSource, candidate: { ...candidate(), source: historicalSource } });
    const historicalJob = job({ id: 'historical-job', owner: { projectId: 'original-project', operationNamespace: 'old-namespace', jobId: 'historical-job' }, operationId: 'old-operation', historical: true, source: historicalSource, view: historicalView, status: 'completed' });
    vi.mocked(memory.readMemory).mockResolvedValue(read([historicalJob], [historicalView]));
    vi.mocked(memory.readMemorySource).mockResolvedValue({ descriptor: { handle: historicalSource.revisionId, displayName: 'Earlier chapter', source: historicalSource, kind: 'historical', current: false, coverage: 'verbatim', disclosure: { readerPosition: null, visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] }, passages: [{ handle: historicalSource.revisionId, source: historicalSource, blockId: 'opening', blockOrder: 0, text: 'The lantern waited.' }], body, usedValidatedProjection: true });
    await render();
    expect(host.textContent).toContain('Recovered historical memory');
    await click('Inspect source'); await vi.waitFor(() => expect(memory.readMemorySource).toHaveBeenCalledWith(access, 'historical-view'));
    await vi.waitFor(() => expect(host.textContent).toContain('Exact source text retained with this memory.'));
    expect(memory.startMemory).not.toHaveBeenCalled();
    expect(memory.retryMemorySave).not.toHaveBeenCalled();
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
  });
});
