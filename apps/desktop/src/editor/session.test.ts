import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DocumentRecord, Head, OperationReceipt, ProjectAccess, ProjectTransport, ReconciledDocument, SaveAck, SaveSnapshot } from '../ipc/projects';
import { bodyHash, canonicalJson, type WnsDocument } from './document';
import { DocumentSession, SessionError, logicalSaveJson } from './session';
import receiptFixture from '../../../../contracts/fixtures/w2_save_receipt.json';

const access: ProjectAccess = { projectId: 'project-1', session: 'session-1', writerLease: 'lease-1', operationNamespace: 'namespace-1' };
function makeBody(text: string, id = 'paragraph-1'): WnsDocument {
  return { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id }, content: text ? [{ type: 'text', text }] : undefined }] } };
}
function defer<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (reason?: unknown) => void } {
  let resolve!: (value: T) => void; let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => { resolve = resolvePromise; reject = rejectPromise; });
  return { promise, resolve, reject };
}
class FakeTransport implements ProjectTransport {
  bodies = new Map<string, WnsDocument>();
  validateImpl: (body: WnsDocument) => Promise<void> = async () => {};
  validate(body: WnsDocument): Promise<void> { return this.validateImpl(body); }
  saves: SaveSnapshot[] = [];
  checkpoints: Array<{ access: ProjectAccess; expected: Head; reason: string }> = [];
  saveImpl: (request: SaveSnapshot) => Promise<SaveAck> = request => this.ack(request);
  reconcileImpl: (request: Parameters<ProjectTransport['reconcile']>[0]) => Promise<ReconciledDocument> = async request => { throw new Error(`unexpected reconcile for ${request.documentId}`); };
  checkpointImpl: (request: Parameters<ProjectTransport['checkpoint']>[0]) => Promise<{ id: string; head: Head; body: WnsDocument; reason: string; parentId: string | null }> = async request => ({ id: 'checkpoint-1', head: request.expected, body: this.bodies.get(request.expected.bodyHash)!, reason: request.reason, parentId: null });
  async save(request: SaveSnapshot): Promise<SaveAck> { this.saves.push(structuredClone(request)); return this.saveImpl(request); }
  async reconcile(request: Parameters<ProjectTransport['reconcile']>[0]): Promise<ReconciledDocument> { return this.reconcileImpl(request); }
  async checkpoint(request: Parameters<ProjectTransport['checkpoint']>[0]) { this.checkpoints.push(structuredClone(request)); return this.checkpointImpl(request); }
  async ack(request: SaveSnapshot, overrides: Partial<SaveAck> = {}): Promise<SaveAck> {
    const hash = await bodyHash(canonicalJson(request.body));
    this.bodies.set(hash, structuredClone(request.body));
    const changed = hash === request.expected.bodyHash ? 0n : 1n;
    return { projectId: request.access.projectId, documentId: request.expected.documentId, session: request.access.session, operationNamespace: request.access.operationNamespace, operationId: request.operationId,
      head: { ...request.expected, version: (BigInt(request.expected.version) + changed).toString(), bodyHash: hash }, savedGeneration: request.localGeneration, ...overrides };
  }
}
async function makeHarness(options: ConstructorParameters<typeof DocumentSession>[3] = {}) {
  const transport = new FakeTransport(); const body = makeBody('initial'); const hash = await bodyHash(canonicalJson(body));
  transport.bodies.set(hash, body);
  const document: DocumentRecord = { head: { documentId: 'document-1', version: '0', bodyHash: hash }, title: 'Draft', kind: 'chapter', metadataVersion: '0', body, lastCheckpointId: null };
  return { session: new DocumentSession(access, document, transport, options), transport, body, document };
}
async function waitForSaves(transport: FakeTransport, count: number): Promise<void> { await vi.waitFor(() => expect(transport.saves).toHaveLength(count)); }
async function receiptFor(request: SaveSnapshot, transport: FakeTransport): Promise<OperationReceipt> {
  const ack = await transport.ack(request);
  const payloadHash = await logicalPayloadHash(request);
  return { operationId: request.operationId, operationKind: 'save', payloadHash, result: { head: ack.head, savedGeneration: ack.savedGeneration } };
}
async function logicalPayloadHash(request: SaveSnapshot): Promise<string> {
  return bodyHash(canonicalJson({
    access: { projectId: request.access.projectId, operationNamespace: request.access.operationNamespace },
    operationId: request.operationId,
    expected: { documentId: request.expected.documentId, version: request.expected.version, bodyHash: request.expected.bodyHash },
    localGeneration: request.localGeneration,
    body: request.body,
    cause: request.cause,
  }));
}
function expectSessionError(error: unknown, code: string): void { expect(error).toBeInstanceOf(SessionError); expect((error as SessionError).code).toBe(code); }
afterEach(() => { vi.useRealTimers(); });

describe('DocumentSession', () => {
  it('keeps an immutable in-flight capture while later typing updates the live body', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); const gate = defer<SaveAck>(); transport.saveImpl = () => gate.promise;
    const firstBody = makeBody('first'); session.update(firstBody); const flush = session.flush(); await waitForSaves(transport, 1);
    const firstBlock = firstBody.body.content[0];
    if (firstBlock.type === 'sceneBreak') throw new Error('test body unexpectedly has a scene break');
    firstBlock.content![0] = { type: 'text', text: 'mutated outside session' }; session.update(makeBody('second'));
    expect(session.body).toEqual(makeBody('second')); expect(transport.saves[0].body).toEqual(makeBody('first'));
    gate.resolve(await transport.ack(transport.saves[0])); transport.saveImpl = request => transport.ack(request); await flush;
    expect(transport.saves).toHaveLength(2); expect(transport.saves[1].body).toEqual(makeBody('second'));
  });
  it('coalesces all edits during one save into the latest follow-up capture', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); const gate = defer<SaveAck>();
    transport.saveImpl = () => transport.saves.length === 1 ? gate.promise : transport.ack(transport.saves.at(-1)!);
    session.update(makeBody('one')); const flush = session.flush(); await waitForSaves(transport, 1); session.update(makeBody('two')); session.update(makeBody('three'));
    gate.resolve(await transport.ack(transport.saves[0])); await flush; expect(transport.saves).toHaveLength(2); expect(transport.saves[1].body).toEqual(makeBody('three')); expect(session.state.dirty).toBe(false);
  });
  it.each([
    ['project', (ack: SaveAck) => ({ ...ack, projectId: 'other' })], ['session', (ack: SaveAck) => ({ ...ack, session: 'other' })], ['namespace', (ack: SaveAck) => ({ ...ack, operationNamespace: 'other' })],
    ['operation', (ack: SaveAck) => ({ ...ack, operationId: 'other' })], ['generation', (ack: SaveAck) => ({ ...ack, savedGeneration: '99' })],
    ['hash', (ack: SaveAck) => ({ ...ack, head: { ...ack.head, bodyHash: '0'.repeat(64) } })], ['version', (ack: SaveAck) => ({ ...ack, head: { ...ack.head, version: '9' } })],
  ] as const)('fences a save when the %s acknowledgment identity is wrong', async (_label, mutate) => {
    const { session, transport } = await makeHarness({ autosave: false }); transport.saveImpl = async request => mutate(await transport.ack(request)); session.update(makeBody('retain me'));
    await expect(session.flush()).rejects.toSatisfy(error => { expectSessionError(error, 'ProtocolError'); return true; }); expect(session.state.phase).toBe('reconciling'); expect(session.body).toEqual(makeBody('retain me')); expect(session.state.editable).toBe(false);
  });
  it('does not send a no-op snapshot', async () => { const { session, transport, body } = await makeHarness({ autosave: false }); session.update(structuredClone(body)); await session.flush(); expect(transport.saves).toHaveLength(0); expect(session.state.dirty).toBe(false); });
  it('keeps the body and allows retry after a definite save failure', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); transport.saveImpl = async () => { throw { code: 'PersistenceUnavailable', detail: 'disk is full' }; }; const body = makeBody('unsaved text'); session.update(body);
    await expect(session.flush()).rejects.toSatisfy(error => { expectSessionError(error, 'PersistenceUnavailable'); return true; }); expect(session.state.phase).toBe('saveFailed'); expect(session.state.editable).toBe(true); expect(session.body).toEqual(body);
    transport.saveImpl = request => transport.ack(request); await session.flush(); expect(session.state.dirty).toBe(false);
  });
  it('fences an uncertain result and reconciles before allowing a retry', async () => {
    const { session, transport, document } = await makeHarness({ autosave: false }); transport.saveImpl = async () => { throw { code: 'UncertainOutcome', detail: 'connection dropped' }; }; session.update(makeBody('pending operation'));
    await expect(session.flush()).rejects.toSatisfy(error => { expectSessionError(error, 'UncertainOutcome'); return true; }); const first = transport.saves[0]; expect(() => session.update(makeBody('blocked edit'))).toThrow(/pending document operation/i);
    transport.reconcileImpl = async () => ({ access: { ...access, writerLease: 'lease-2' }, document, receipts: [] }); transport.saveImpl = request => transport.ack(request); await session.reconcile();
    expect(transport.saves).toHaveLength(2); expect(transport.saves[1].operationId).toBe(first.operationId); expect(transport.saves[1].localGeneration).toBe(first.localGeneration); expect(transport.saves[1].body).toEqual(first.body); expect(transport.saves[1].access.writerLease).toBe('lease-2'); expect(session.state.phase).toBe('editing');
  });
  it('never lets a receipt acknowledgment overwrite a newer local body', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); const gate = defer<SaveAck>(); transport.saveImpl = () => transport.saves.length === 1 ? gate.promise : transport.ack(transport.saves.at(-1)!);
    session.update(makeBody('first')); const flush = session.flush(); await waitForSaves(transport, 1); session.update(makeBody('newer local body')); gate.resolve(await transport.ack(transport.saves[0])); await flush;
    expect(session.body).toEqual(makeBody('newer local body')); expect(transport.saves).toHaveLength(2); expect(session.state.dirty).toBe(false);
  });
  it('resubmits the same absent-receipt operation with a fresh lease', async () => {
    const { session, transport, document } = await makeHarness({ autosave: false }); transport.saveImpl = async () => { throw { code: 'WriterLeaseExpired', detail: 'lease expired' }; }; session.update(makeBody('retry me'));
    await expect(session.flush()).rejects.toSatisfy(error => { expectSessionError(error, 'WriterLeaseExpired'); return true; }); const first = transport.saves[0]; transport.reconcileImpl = async () => ({ access: { ...access, writerLease: 'lease-3' }, document, receipts: [] }); transport.saveImpl = request => transport.ack(request); await session.reconcile();
    expect(transport.saves).toHaveLength(2); expect(transport.saves[1].operationId).toBe(first.operationId); expect(transport.saves[1].expected).toEqual(first.expected); expect(transport.saves[1].localGeneration).toBe(first.localGeneration); expect(transport.saves[1].body).toEqual(first.body); expect(transport.saves[1].access.writerLease).toBe('lease-3'); expect(await logicalPayloadHash(transport.saves[1])).toBe(await logicalPayloadHash(first));
  });
  it('reports conflict when the latest head differs from a matching receipt', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); transport.saveImpl = async () => { throw { code: 'UncertainOutcome', detail: 'connection dropped' }; }; session.update(makeBody('local replacement')); await expect(session.flush()).rejects.toBeInstanceOf(SessionError);
    const receipt = await receiptFor(transport.saves[0], transport); const latestBody = makeBody('newer saved body'); const latestHash = await bodyHash(canonicalJson(latestBody)); const latest: DocumentRecord = { head: { documentId: 'document-1', version: '2', bodyHash: latestHash }, title: 'Draft', kind: 'chapter', metadataVersion: '0', body: latestBody, lastCheckpointId: null };
    transport.reconcileImpl = async () => ({ access: { ...access, writerLease: 'lease-4' }, document: latest, receipts: [receipt] }); await session.reconcile(); expect(session.state.phase).toBe('conflict'); expect(session.savedConflict?.body).toEqual(latestBody); expect(session.body).toEqual(makeBody('local replacement'));
  });
  it('waits for composition to end before starting autosave', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); session.setComposing(true); session.update(makeBody('composing text')); await expect(session.flush()).rejects.toSatisfy(error => { expectSessionError(error, 'CompositionPending'); return true; }); expect(transport.saves).toHaveLength(0); session.setComposing(false); await session.flush(); expect(transport.saves).toHaveLength(1);
  });
  it('serializes detach and waits for a second navigation guard', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); const saveGate = defer<SaveAck>(); const checkpointGate = defer<{ id: string; head: Head; body: WnsDocument; reason: string; parentId: string | null }>(); const events: string[] = [];
    transport.saveImpl = () => saveGate.promise; transport.checkpointImpl = () => checkpointGate.promise; session.update(makeBody('before detach')); const detaching = session.detach('switch'); await waitForSaves(transport, 1); events.push('save-started'); const second = session.withLifecycleGuard(async () => { events.push('second-entered'); return 'second'; }); await Promise.resolve(); expect(events).toEqual(['save-started']);
    saveGate.resolve(await transport.ack(transport.saves[0])); await vi.waitFor(() => expect(transport.checkpoints).toHaveLength(1)); expect(events).toEqual(['save-started']); checkpointGate.resolve({ id: 'checkpoint-1', head: session.state.head, body: session.body, reason: 'switch', parentId: null }); await detaching;
    await expect(second).rejects.toSatisfy(error => { expectSessionError(error, 'Disposed'); return true; }); expect(session.state.phase).toBe('disposed'); expect(events).toEqual(['save-started']);
  });
  it('flushes, checkpoints, saves the view, and only then prepares the destination', async () => {
    const { session, transport } = await makeHarness({ autosave: false });
    const events: string[] = [];
    transport.saveImpl = async request => { events.push('flush'); return transport.ack(request); };
    transport.checkpointImpl = async request => {
      events.push('checkpoint');
      return { id: 'checkpoint-1', head: request.expected, body: transport.bodies.get(request.expected.bodyHash)!, reason: request.reason, parentId: null };
    };
    session.setViewSaver(async () => { events.push('view'); });
    session.update(makeBody('before destination'));

    const destination = await session.detachAfter(async () => { events.push('destination'); return 'project-b'; }, 'switch');

    expect(destination).toBe('project-b');
    expect(events).toEqual(['flush', 'checkpoint', 'view', 'destination']);
    expect(session.state.phase).toBe('disposed');
  });
  it.each([
    ['definite', 'PersistenceUnavailable', 'saveFailed', true],
    ['uncertain', 'UncertainOutcome', 'reconciling', false],
  ] as const)('retains the editor buffer when a %s view save fails', async (_kind, code, phase, editable) => {
    const { session, transport, document } = await makeHarness({ autosave: false });
    const retained = makeBody('view buffer retained');
    const viewSave = vi.fn(async () => { throw { code, detail: 'view acknowledgment unavailable' }; });
    session.setViewSaver(viewSave);
    session.update(retained);

    await expect(session.persistView()).rejects.toSatisfy(error => { expectSessionError(error, code); return true; });

    expect(viewSave).toHaveBeenCalledOnce();
    expect(session.body).toEqual(retained);
    expect(session.state.phase).toBe(phase);
    expect(session.state.editable).toBe(editable);
    expect(session.state.dirty).toBe(false);
    if (code === 'UncertainOutcome') {
      const restored: DocumentRecord = { ...document, head: session.state.head, body: retained };
      let reconcileRequest: Parameters<ProjectTransport['reconcile']>[0] | undefined;
      transport.reconcileImpl = async request => {
        reconcileRequest = request;
        return { access: { ...access, writerLease: 'lease-view-2' }, document: restored, receipts: [] };
      };
      await session.reconcile();
      expect(reconcileRequest?.pendingOperationIds).toEqual([]);
      expect(session.state.phase).toBe('editing');
      expect(session.body).toEqual(retained);
    }
  });
  it('does not run a queued background view save after detach disposes the session', async () => {
    const { session, transport } = await makeHarness({ autosave: false });
    const checkpointGate = defer<{ id: string; head: Head; body: WnsDocument; reason: string; parentId: string | null }>();
    const events: string[] = [];
    let viewSaves = 0;
    session.setViewSaver(async () => { viewSaves += 1; events.push(`view-${viewSaves}`); });
    transport.checkpointImpl = () => checkpointGate.promise;
    session.update(makeBody('queued view')); const detaching = session.detachAfter(async () => { events.push('destination'); return 'done'; }, 'switch');
    await waitForSaves(transport, 1); await vi.waitFor(() => expect(transport.checkpoints).toHaveLength(1));
    const background = session.persistView();
    await Promise.resolve();
    expect(viewSaves).toBe(0);
    checkpointGate.resolve({ id: 'checkpoint-1', head: session.state.head, body: session.body, reason: 'switch', parentId: null });
    await expect(detaching).resolves.toBe('done');
    await expect(background).rejects.toSatisfy(error => { expectSessionError(error, 'Disposed'); return true; });
    expect(viewSaves).toBe(1);
    expect(events).toEqual(['view-1', 'destination']);
  });
  it('fences an uncertain project metadata write and reconciles before retrying', async () => {
    const { session, transport, document } = await makeHarness({ autosave: false });
    const retained = makeBody('metadata write keeps this buffer');
    session.update(retained);
    let writes = 0;
    await expect(session.projectWrite(async () => { writes += 1; throw { code: 'UncertainOutcome', detail: 'metadata acknowledgment lost' }; })).rejects.toSatisfy(error => { expectSessionError(error, 'UncertainOutcome'); return true; });
    expect(writes).toBe(1);
    expect(session.body).toEqual(retained);
    expect(session.state.phase).toBe('reconciling');
    expect(session.state.editable).toBe(false);
    const restored: DocumentRecord = { ...document, head: session.state.head, body: retained };
    let reconcileRequest: Parameters<ProjectTransport['reconcile']>[0] | undefined;
    transport.reconcileImpl = async request => {
      reconcileRequest = request;
      return { access: { ...access, writerLease: 'lease-metadata-2' }, document: restored, receipts: [] };
    };
    await session.reconcile();
    expect(reconcileRequest?.pendingOperationIds).toEqual([]);
    expect(session.state.phase).toBe('editing');
    expect(session.state.editable).toBe(true);
    await expect(session.projectWrite(async () => 'metadata-retried')).resolves.toBe('metadata-retried');
    expect(session.body).toEqual(retained);
  });
  it('debounces autosave while typing but fires by the two second maximum', async () => {
    vi.useFakeTimers(); const { session, transport } = await makeHarness(); session.update(makeBody('one')); await vi.advanceTimersByTimeAsync(500); session.update(makeBody('two')); await vi.advanceTimersByTimeAsync(500); session.update(makeBody('three')); await vi.advanceTimersByTimeAsync(500); session.update(makeBody('four')); await vi.advanceTimersByTimeAsync(499); expect(transport.saves).toHaveLength(0); await vi.advanceTimersByTimeAsync(1); await waitForSaves(transport, 1); expect(transport.saves[0].body).toEqual(makeBody('four'));
  });
  it('keeps the body and local state when checkpoint fails', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); const body = makeBody('checkpoint remains nondestructive'); session.update(body); transport.checkpointImpl = async () => { throw { code: 'PersistenceUnavailable', detail: 'checkpoint unavailable' }; };
    await expect(session.checkpoint('manual')).rejects.toMatchObject({ code: 'PersistenceUnavailable' }); expect(session.body).toEqual(body); expect(session.state.phase).toBe('saveFailed'); expect(session.state.dirty).toBe(false); expect(transport.saves).toHaveLength(1);
  });
  it('matches the literal receipt shared with the real Rust implementation', async () => {
    expect(logicalSaveJson(receiptFixture.request as SaveSnapshot)).toBe(receiptFixture.logicalJson);
    expect(await bodyHash(logicalSaveJson(receiptFixture.request as SaveSnapshot))).toBe(receiptFixture.payloadHash);
  });
  it.each(['checkpoint', 'detach'] as const)('fences an uncertain %s until reconciliation', async operation => {
    const { session, transport, document } = await makeHarness({ autosave: false });
    transport.checkpointImpl = async () => { throw { code: 'UncertainOutcome', detail: 'lost checkpoint acknowledgment' }; };
    await expect(operation === 'detach' ? session.detach() : session.checkpoint('manual')).rejects.toMatchObject({ code: 'UncertainOutcome' });
    expect(session.state.phase).toBe('reconciling'); expect(session.state.editable).toBe(false);
    transport.reconcileImpl = async () => ({ access: { ...access, writerLease: 'lease-2' }, document, receipts: [] });
    await session.reconcile(); expect(session.state.phase).toBe('editing'); expect(session.body).toEqual(document.body);
  });
  it('keeps an unrecognized server outcome fenced', async () => {
    const { session, transport } = await makeHarness({ autosave: false }); session.update(makeBody('retained'));
    transport.saveImpl = async () => { throw { code: 'NewServerFailure', detail: 'unknown outcome' }; };
    await expect(session.flush()).rejects.toMatchObject({ code: 'NewServerFailure' }); expect(session.state.phase).toBe('reconciling');
  });
  it('validates recovered snapshot structure before exposing a conflict', async () => {
    const { session, transport, document } = await makeHarness({ autosave: false });
    transport.validateImpl = async () => { throw new Error('Invalid recovered document'); };
    transport.reconcileImpl = async () => ({ access: { ...access, writerLease: 'lease-2' }, document, receipts: [] });
    await expect(session.reconcile()).rejects.toBeInstanceOf(SessionError);
    expect(session.savedConflict).toBe(null); expect(session.state.editable).toBe(false);
  });
  it('keeps the current editor usable when opening a destination fails', async () => {
    const { session } = await makeHarness({ autosave: false }); session.update(makeBody('current project text'));
    await expect(session.detachAfter(async () => { throw new Error('Folder unavailable'); })).rejects.toThrow('Folder unavailable');
    expect(session.state.editable).toBe(true); expect(session.state.dirty).toBe(false); expect(session.body).toEqual(makeBody('current project text'));
  });
});
