// @vitest-environment node
import { describe, expect, it, vi } from 'vitest';
import { emptyWorkshop, newSession, WorkshopStore, type WorkshopTransport } from './store';
import type { SaveWorkshop, WorkshopResult, WorkshopSnapshot, WorkshopView } from '../ipc/workshop';

const access = { projectId: 'p', session: 's', writerLease: 'l', operationNamespace: 'n' };
const base = (): WorkshopView => ({ version: '0', state: emptyWorkshop(), results: [] });
const deferred = <T,>() => { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; };
const provisional = (id: string, sessionId: string): WorkshopResult => ({
  run: {
    id, threadId: id, owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, runId: id },
    operationId: id, intent: 'discuss', payloadHash: 'payload', target: { documentId: 'anchor', version: '1', bodyHash: 'hash' },
    packetId: `packet-${id}`, previousRunId: null, status: 'queued', dispatchState: 'notDispatched', sequence: '0',
    outputText: '', stopReason: null, createdAt: '2026-09-12T00:00:00Z', updatedAt: '2026-09-12T00:00:00Z',
  },
  sessionId, workingGeneration: '0', action: 'directions', output: null, validationError: null, stale: false,
});

describe('Workshop draft persistence', () => {
  it('records and deduplicates provisional results without changing draft or save watermarks', async () => {
    const first = deferred<WorkshopSnapshot>(); const requests: SaveWorkshop[] = [];
    const store = new WorkshopStore(access, { read: async () => base(), save: request => {
      requests.push(structuredClone(request)); return first.promise;
    } });
    try {
      await store.load();
      const session = newSession();
      store.edit(state => ({ ...state, currentSessionId: session.id, sessions: [session] }));
      const save = store.flush();
      const state = structuredClone(store.state);
      const notify = vi.fn(); store.subscribe(notify);
      store.recordProvisionalResult(provisional('first', session.id));
      store.recordProvisionalResult(provisional('second', session.id));
      const replacement = provisional('first', session.id); replacement.run.status = 'running';
      store.recordProvisionalResult(replacement);
      replacement.run.status = 'completed';
      expect(notify).toHaveBeenCalledTimes(3);
      expect(store.results.map(result => [result.run.id, result.run.status])).toEqual([['second', 'queued'], ['first', 'running']]);
      expect(store.state).toEqual(state);
      expect([store.version, store.generation, store.savedGeneration, store.dirty, store.saving]).toEqual(['0', 1, 0, true, true]);
      expect(requests).toHaveLength(1);
      first.resolve({ version: '1', state: requests[0].state }); await save;
      expect([store.version, store.generation, store.savedGeneration, store.dirty]).toEqual(['1', 1, 1, false]);
      expect(store.results).toHaveLength(2);
      expect(requests).toHaveLength(1);
    } finally { store.dispose(); }
  });
  it('synchronizes lock admission without notifications and refuses edits until unlocked', async () => {
    const session = newSession();
    const save = vi.fn(async (request: SaveWorkshop) => ({ version: '1', state: request.state }));
    const store = new WorkshopStore(access, { read: async () => ({ ...base(), state: { ...emptyWorkshop(), sessions: [session] } }), save });
    try {
      await store.load();
      const notify = vi.fn(); store.subscribe(notify);
      store.setInteractionLocked(true);
      const change = vi.fn(state => state);
      store.edit(change);
      store.editSession(session.id, value => ({ ...value, workingText: 'Blocked edit' }), true);
      await store.flush();
      expect(store.locked).toBe(true);
      expect(change).not.toHaveBeenCalled();
      expect(notify).not.toHaveBeenCalled();
      expect(save).not.toHaveBeenCalled();
      expect([store.generation, store.savedGeneration, store.state.sessions[0].workingGeneration]).toEqual([0, 0, '0']);
      expect(store.state.sessions[0].workingText).toBe('');
      store.setInteractionLocked(false);
      expect(notify).not.toHaveBeenCalled();
      store.editSession(session.id, value => ({ ...value, workingText: 'Allowed edit' }), true);
      expect(notify).toHaveBeenCalledTimes(1);
      expect(store.state.sessions[0].workingGeneration).toBe('1');
      await store.flush();
      expect(save).toHaveBeenCalledTimes(1);
      expect(store.state.sessions[0].workingText).toBe('Allowed edit');
      expect(store.dirty).toBe(false);
    } finally { store.dispose(); }
  });
  it('continues typing across a save acknowledgment and persists the newer generation', async () => {
    const first = deferred<WorkshopSnapshot>(); const requests: SaveWorkshop[] = [];
    const api: WorkshopTransport = { read: async () => base(), save: async request => { requests.push(structuredClone(request)); return requests.length === 1 ? first.promise : { version: '2', state: request.state }; } };
    const store = new WorkshopStore(access, api); await store.load();
    store.edit(state => ({ ...state, sessions: [newSession('world', 'First idea')] }));
    const saved = store.flush();
    store.editSession(store.state.sessions[0].id, session => ({ ...session, brief: 'Newer idea' }));
    first.resolve({ version: '1', state: requests[0].state }); await saved;
    expect(store.state.sessions[0].brief).toBe('Newer idea');
    expect(requests[1].expectedVersion).toBe('1'); expect(store.dirty).toBe(false); store.dispose();
  });
  it('flushes an edit made while the previous drain is settling after its last acknowledgment', async () => {
    vi.useFakeTimers();
    const first = deferred<WorkshopSnapshot>(); const requests: SaveWorkshop[] = [];
    const store = new WorkshopStore(access, { read: async () => base(), save: request => {
      requests.push(structuredClone(request));
      return requests.length === 1 ? first.promise : Promise.resolve({ version: '2', state: request.state });
    } });
    try {
      await store.load();
      const session = newSession('world', 'First idea');
      store.edit(state => ({ ...state, currentSessionId: session.id, sessions: [session] }));
      const initialFlush = store.flush();
      first.resolve({ version: '1', state: requests[0].state });
      // Let send() and drain() settle, before flush() clears its shared flight.
      await Promise.resolve();
      await Promise.resolve();
      store.editSession(session.id, value => ({ ...value, brief: 'Newer idea' }));
      await Promise.all([initialFlush, store.flush()]);
      // The explicit flush cancelled this edit's 250ms autosave timer.
      expect(vi.getTimerCount()).toBe(0);
      await vi.advanceTimersByTimeAsync(300);
      expect(requests).toHaveLength(2);
      expect(requests[1].expectedVersion).toBe('1');
      expect(requests[1].state.sessions[0].brief).toBe('Newer idea');
      expect(store.dirty).toBe(false);
    } finally {
      store.dispose();
      vi.useRealTimers();
    }
  });
  it('retries immutable uncertain bytes before saving later author edits', async () => {
    const requests: SaveWorkshop[] = []; let fail = true;
    const store = new WorkshopStore(access, { read: async () => base(), save: async request => {
      requests.push(structuredClone(request)); if (fail) { fail = false; throw new Error('Acknowledgment lost'); }
      return { version: requests.length === 2 ? '1' : '2', state: request.state };
    } });
    await store.load(); store.edit(state => ({ ...state, sessions: [newSession()] }));
    await expect(store.flush()).rejects.toThrow('Acknowledgment lost');
    store.editSession(store.state.sessions[0].id, session => ({ ...session, workingText: 'Still editable offline' }), true);
    await store.flush();
    expect(requests[1]).toEqual(requests[0]);
    expect(requests[2].operationId).not.toBe(requests[0].operationId);
    expect(store.state.sessions[0].workingText).toBe('Still editable offline'); store.dispose();
  });
  it('never replaces a dirty manual version when an older provider poll arrives', async () => {
    const store = new WorkshopStore(access, { read: async () => base(), save: async request => ({ version: '1', state: request.state }) });
    await store.load(); store.edit(state => ({ ...state, sessions: [newSession('world', 'Local notes')] }));
    await store.refreshResults(); expect(store.state.sessions[0].brief).toBe('Local notes'); expect(store.dirty).toBe(true); store.dispose();
  });
  it('can save a corrected draft after a definitive validation refusal', async () => {
    const requests: SaveWorkshop[] = [];
    const store = new WorkshopStore(access, { read: async () => base(), save: async request => {
      requests.push(structuredClone(request));
      if (requests.length === 1) throw { code: 'InvalidRequest', detail: 'The title is invalid.' };
      return { version: '1', state: request.state };
    } });
    await store.load(); store.edit(state => ({ ...state, sessions: [newSession('world', 'Invalid title')] }));
    await expect(store.flush()).rejects.toMatchObject({ code: 'InvalidRequest' });
    store.editSession(store.state.sessions[0].id, session => ({ ...session, title: 'Corrected title' }));
    await store.flush();
    expect(requests[1].operationId).not.toBe(requests[0].operationId);
    expect(requests[1].state.sessions[0].title).toBe('Corrected title');
    expect(store.dirty).toBe(false); store.dispose();
  });
});
