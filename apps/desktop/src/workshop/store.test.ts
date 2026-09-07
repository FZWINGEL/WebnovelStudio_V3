// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { emptyWorkshop, newSession, WorkshopStore, type WorkshopTransport } from './store';
import type { SaveWorkshop, WorkshopSnapshot, WorkshopView } from '../ipc/workshop';

const access = { projectId: 'p', session: 's', writerLease: 'l', operationNamespace: 'n' };
const base = (): WorkshopView => ({ version: '0', state: emptyWorkshop(), results: [] });
const deferred = <T,>() => { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; };

describe('Workshop draft persistence', () => {
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
