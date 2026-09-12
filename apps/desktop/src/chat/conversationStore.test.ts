// @vitest-environment node
import { describe, expect, it } from 'vitest';
import type { DiscussionRun, DiscussionStart } from '../ipc/discussions';
import type { ChatAdoptionPreview, ProjectConversationView, SaveProjectComposer } from '../ipc/projectChat';
import type { MockContextBudget } from '../ipc/context';
import type { OpenedProject, ProjectAccess } from '../ipc/projects';
import { ProjectConversationStore, type ProjectConversationTransport } from './conversationStore';

const access: ProjectAccess = { projectId: 'project-1', session: 'session-1', writerLease: 'lease-1', operationNamespace: 'namespace-1' };
const budget: MockContextBudget = { modelId: 'mock-story-context', contextWindowTokens: '32768', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' };
const head = (documentId = 'anchor', version = '0') => ({ documentId, version, bodyHash: `hash-${documentId}-${version}` });
const run = (operationId: string, status: DiscussionRun['status'] = 'running'): DiscussionRun => ({ id: `run-${operationId}`, intent: 'discuss', threadId: 'thread-1', owner: { projectId: 'project-1', operationNamespace: 'namespace-1', runId: `run-${operationId}` }, operationId, target: head(), packetId: 'packet-1', previousRunId: null, payloadHash: 'payload', status, dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null, createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z' });
const view = (activeRun: DiscussionRun | null = null): ProjectConversationView => ({ id: 'conversation-1', composer: { conversationId: 'conversation-1', version: '0', body: { text: '', sourceRefs: [], taskDraftRefs: [] } }, items: [], olderBefore: null, activeRun, drafts: [], sourceEpoch: '0', policyEpoch: '0', earlierWorkshop: false, workerIssues: [] });
const requestItem = (value: DiscussionRun): ProjectConversationView['items'][number] => ({ id: `item-${value.id}`, sequence: '1', kind: 'request', referenceId: value.id, payload: { instruction: 'Write this', run: value }, createdAt: '2026-01-01T00:00:00Z' });
const project = (): OpenedProject => ({ project: { projectId: 'project-1', operationNamespace: 'namespace-1', title: 'Test project', formatVersion: 1 }, access, documents: [], metadataVersion: '0', viewState: null, libraryWarning: null });
const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; };
const start = (operationId: string): DiscussionStart => ({ threadId: 'thread-1', run: run(operationId, 'queued'), userMessage: { id: 'message-1', threadId: 'thread-1', runId: `run-${operationId}`, role: 'user', content: 'Write this', scope: null, packetId: 'packet-1', createdAt: '2026-01-01T00:00:00Z' }, packet: { messages: [], options: { modelId: 'mock-story-context', tokenAccountingMethod: 'bytes' }, receipt: { packetId: 'packet-1', sessionId: 'session-1', snapshotId: 'snapshot-1', invocationOrdinal: '0', sourceHandles: [], coverage: [], omissions: [], inputHash: 'input', inputTokens: '1', tokenAccountingMethod: 'bytes' } } });

describe('project conversation store', () => {
  it('keeps newer composer text outside the immutable save capture', async () => {
    const first = deferred<{ conversationId: string; version: string; body: ReturnType<typeof view>['composer']['body'] }>();
    const requests: Array<{ operationId: string; expectedVersion: string; text: string }> = [];
    const api: ProjectConversationTransport = {
      read: async () => view(),
      save: async request => { requests.push({ operationId: request.operationId, expectedVersion: request.expectedVersion, text: request.body.text }); return requests.length === 1 ? first.promise : { conversationId: request.conversationId, version: '2', body: request.body }; },
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load();
    store.setText('First request'); const firstFlush = store.flush();
    store.setText('Newer request'); first.resolve({ conversationId: 'conversation-1', version: '1', body: { text: 'First request', sourceRefs: [], taskDraftRefs: [] } });
    await firstFlush;
    expect(requests.map(item => item.text)).toEqual(['First request', 'Newer request']);
    expect(requests[1].expectedVersion).toBe('1');
    expect(store.state.composer.text).toBe('Newer request');
    expect(store.state.composerDirty).toBe(false);
    store.dispose();
  });

  it('reconciles an uncertain start without submitting a second generation', async () => {
    const operationIds: string[] = []; let current = view();
    const api: ProjectConversationTransport = {
      read: async () => current,
      save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => { operationIds.push(request.operationId); throw new Error('start acknowledgment lost'); },
      retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }), disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load(); store.setText('Send once');
    await expect(store.send(null, budget)).rejects.toThrow('start acknowledgment lost');
    expect(store.state.status).toBe('uncertain');
    current = view(run(operationIds[0], 'running'));
    expect(await store.reconcileStart()).toBe(true);
    expect(operationIds).toHaveLength(1);
    expect(store.state.activeRun?.operationId).toBe(operationIds[0]);
    store.dispose();
  });

  it('discards a late refresh from an older read sequence', async () => {
    const oldRead = deferred<ProjectConversationView>(); const freshRead = deferred<ProjectConversationView>(); let reads = 0;
    const api: ProjectConversationTransport = {
      read: async () => { reads += 1; return reads === 1 ? oldRead.promise : freshRead.promise; }, save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }), disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }), prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); const firstLoad = store.load();
    const secondLoad = store.load(); freshRead.resolve(view(run('fresh', 'completed'))); await secondLoad; oldRead.resolve(view(run('old', 'completed'))); await firstLoad;
    expect(store.state.activeRun?.operationId).toBe('fresh'); store.dispose();
  });

  it('advances the accepted composer watermark and preserves typing plus source edits during start', async () => {
    const started = deferred<DiscussionStart>(); const startCalled = deferred<void>(); const saves: SaveProjectComposer[] = []; let current = view();
    const source = head('world', '2');
    const api: ProjectConversationTransport = {
      read: async () => current,
      save: async request => { saves.push(structuredClone(request)); return { conversationId: request.conversationId, version: String(Number(request.expectedVersion) + 1), body: request.body }; },
      start: async () => { startCalled.resolve(); return started.promise; },
      retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }), disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }), prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load(); store.setText('First request');
    const sending = store.send(null, budget); await startCalled.promise; await expect(store.send(null, budget)).rejects.toThrow('already being submitted'); store.setText('Newer text'); store.setSources([source]);
    expect(saves).toHaveLength(1); // The start fence suppresses autosave before acceptance.
    await expect(store.flush()).rejects.toThrow('accepted or reconciled');
    started.resolve(start('operation-accepted')); await sending;
    expect(store.state.composer.text).toBe('Newer text'); expect(store.state.composer.sourceRefs).toEqual([source]); expect(store.state.composerVersion).toBe('2');
    await store.flush(); expect(saves.at(-1)?.expectedVersion).toBe('2'); expect(saves.at(-1)?.body.sourceRefs).toEqual([source]); store.dispose();
  });

  it('permits a second send after the first terminal run advances the watermark', async () => {
    let current = view();
    const starts: Array<{ operationId: string; expectedComposerVersion: string }> = [];
    const api: ProjectConversationTransport = {
      read: async () => current,
      save: async request => {
        const version = String(Number(request.expectedVersion) + 1);
        current = { ...current, composer: { ...current.composer, version, body: request.body } };
        return { conversationId: request.conversationId, version, body: request.body };
      },
      start: async request => { starts.push({ operationId: request.operationId, expectedComposerVersion: request.expectedComposerVersion }); return start(request.operationId); },
      retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load();
    store.setText('First request'); await store.send(null, budget);
    const completed = run(starts[0].operationId, 'completed'); current = { ...current, composer: { ...current.composer, version: '1', body: { text: '', sourceRefs: [], taskDraftRefs: [] } }, items: [requestItem(completed)] };
    await store.refresh();
    store.setText('Second request'); await store.send(null, budget);
    expect(starts.map(item => item.expectedComposerVersion)).toEqual(['1', '3']);
    expect(store.state.composerVersion).toBe('4');
    store.dispose();
  });

  it('reconciles a terminal lost start acknowledgment and clears only the accepted buffer', async () => {
    const operationIds: string[] = []; let current = view();
    const api: ProjectConversationTransport = {
      read: async () => current,
      save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => { operationIds.push(request.operationId); throw new Error('ack lost'); },
      retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }), disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }), prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load(); store.setText('Accepted text');
    await expect(store.send(null, budget)).rejects.toThrow('ack lost');
    const terminal = run(operationIds[0], 'completed'); current = { ...view(), items: [requestItem(terminal)] };
    expect(await store.reconcileStart()).toBe(true); expect(store.state.composer.text).toBe(''); expect(store.state.composerVersion).toBe('2'); expect(store.state.activeRun?.status).toBe('completed'); store.dispose();
  });

  it('keeps an uncertain composer save exact and retries only that local operation', async () => {
    const requests: SaveProjectComposer[] = []; let fail = true;
    const api: ProjectConversationTransport = {
      read: async () => view(),
      save: async request => { requests.push(structuredClone(request)); if (fail) { fail = false; throw new Error('local save acknowledgment lost'); } return { conversationId: request.conversationId, version: '1', body: request.body }; },
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }), disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }), prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api); await store.load(); store.setText('Keep exact bytes');
    await expect(store.flush()).rejects.toThrow('local save acknowledgment lost'); expect(store.state.status).toBe('uncertain');
    expect(await store.reconcileStart()).toBe(true); expect(requests[1]).toEqual(requests[0]); expect(store.state.composerDirty).toBe(false); store.dispose();
  });

  it('requires an exact retained run before retrying local result saving', async () => {
    const retries: string[] = [];
    const older = run('old', 'completed');
    const newer = run('new', 'completed');
    const current: ProjectConversationView = {
      ...view(newer),
      items: [requestItem(older), { ...requestItem(newer), id: 'item-run-new', sequence: '2' }],
      workerIssues: [{ runId: newer.id, detail: 'The newer result needs local saving.' }],
    };
    const api: ProjectConversationTransport = {
      read: async () => current,
      save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => start(request.operationId),
      retrySave: async (_access, _conversationId, runId) => { retries.push(runId); },
      stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api);
    await store.load();
    await expect(store.retrySave()).rejects.toThrow('Choose the retained result');
    await store.retrySave(newer.id);
    expect(retries).toEqual([newer.id]);
    await expect(store.retrySave(older.id)).rejects.toThrow('no longer available');
    store.dispose();
  });

  it('keeps a pre-dispatch context refusal separate from uncertainty and permits only an explicit new Send', async () => {
    const starts: string[] = [];
    let savedVersion = 0;
    let retryCount = 0;
    const api: ProjectConversationTransport = {
      read: async () => ({ ...view(), items: [requestItem(run('older', 'completed'))] }),
      save: async request => ({ conversationId: request.conversationId, version: String(++savedVersion), body: request.body }),
      start: async request => {
        starts.push(request.operationId);
        if (starts.length === 1) throw { code: 'ContextPreparationFailed', detail: 'The approved scope changed.' };
        return start(request.operationId);
      },
      retrySave: async () => { retryCount += 1; }, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => { throw new Error('not used'); }, prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api);
    await store.load(); store.setText('Write from this approved brief.');
    await expect(store.send(null, budget)).rejects.toMatchObject({ code: 'ContextPreparationFailed' });
    expect(store.state.status).toBe('failed');
    expect(store.state.uncertainOperationId).toBeNull();
    expect(store.state.composer.text).toBe('Write from this approved brief.');
    await expect(store.retrySave()).rejects.toThrow('Choose the retained result');
    expect(starts).toHaveLength(1); expect(retryCount).toBe(0);
    await store.send(null, budget);
    expect(starts).toHaveLength(2); expect(starts[1]).not.toBe(starts[0]);
    store.dispose();
  });

  it('ignores late save acknowledgments and composer writes after disposal', async () => {
    const saveGate = deferred<{ conversationId: string; version: string; body: ReturnType<typeof view>['composer']['body'] }>();
    const api: ProjectConversationTransport = {
      read: async () => view(),
      save: async () => saveGate.promise,
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api);
    await store.load();
    store.setText('Keep this local buffer');
    const flushing = store.flush();
    store.dispose();
    store.setText('Must not mutate a disposed store');
    saveGate.resolve({ conversationId: 'conversation-1', version: '1', body: { text: 'Keep this local buffer', sourceRefs: [], taskDraftRefs: [] } });
    await flushing;
    expect(store.state.composer.text).toBe('Keep this local buffer');
    expect(store.state.status).toBe('saving');
  });

  it('does not enter mutation transports from disposed handlers', async () => {
    const calls = { disposition: 0, prepare: 0, adopt: 0 };
    const api: ProjectConversationTransport = {
      read: async () => view(),
      save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => { calls.disposition += 1; throw new Error('unexpected disposition IPC'); },
      prepareAdoption: async () => { calls.prepare += 1; throw new Error('unexpected preparation IPC'); },
      adopt: async () => { calls.adopt += 1; throw new Error('unexpected adoption IPC'); },
    };
    const store = new ProjectConversationStore(project(), api);
    store.dispose();
    await expect(store.setDisposition('response', '0', 'reconsider')).rejects.toThrow('conversation is closed');
    await expect(store.prepareAdoption([])).rejects.toThrow('conversation is closed');
    await expect(store.adopt({} as ChatAdoptionPreview)).rejects.toThrow('conversation is closed');
    expect(calls).toEqual({ disposition: 0, prepare: 0, adopt: 0 });
  });

  it('loads older timeline pages without dropping them during a later refresh', async () => {
    const item = (sequence: number) => ({ id: `item-${sequence}`, sequence: String(sequence), kind: 'event', referenceId: null, payload: {}, createdAt: '2026-01-01T00:00:00Z' });
    const latest = { ...view(run('live', 'running')), items: Array.from({ length: 40 }, (_, index) => item(index + 41)), olderBefore: '41' };
    const older = { ...view(), items: Array.from({ length: 40 }, (_, index) => item(index + 1)), olderBefore: null };
    const reads: Array<string | null | undefined> = [];
    const api: ProjectConversationTransport = {
      read: async (_access, before) => { reads.push(before); return before === '41' ? older : latest; },
      save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    };
    const store = new ProjectConversationStore(project(), api);
    await store.load();
    store.setText('Keep this newer local composer');
    expect(await store.loadOlder()).toBe(true);
    expect(store.state.view?.items).toHaveLength(80);
    await store.refresh();
    expect(store.state.view?.items).toHaveLength(80);
    expect(store.state.activeRun?.operationId).toBe('live');
    expect(store.state.composer.text).toBe('Keep this newer local composer');
    expect(reads).toEqual([null, '41', null]);
    store.dispose();
  });

  it('rotates the writer lease without remounting the composer or pending operation identity', async () => {
    const store = new ProjectConversationStore(project(), {
      read: async () => view(), save: async request => ({ conversationId: request.conversationId, version: '1', body: request.body }),
      start: async request => start(request.operationId), retrySave: async () => {}, stop: async () => ({ run: run('stop', 'stopped') }),
      disposition: async () => ({ id: 'item', sequence: '1', kind: 'disposition', referenceId: null, payload: {}, createdAt: '' }),
      prepareAdoption: async () => { throw new Error('not used'); }, adopt: async () => { throw new Error('not used'); },
    });
    await store.load();
    store.setText('Keep the composer while the lease rotates');
    store.updateAccess({ ...access, writerLease: 'lease-2' });
    expect(store.access.writerLease).toBe('lease-2');
    expect(store.state.composer.text).toBe('Keep the composer while the lease rotates');
    store.dispose();
  });
});
