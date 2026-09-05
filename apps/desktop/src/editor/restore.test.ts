import { describe, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from './document';
import { DocumentSession } from './session';
import type { DocumentRecord, OperationReceipt, ProjectAccess, ProjectTransport, Revision } from '../ipc/projects';
import type { RestoreAck, RestoreRevision } from '../ipc/history';

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'renderer', writerLease: 'lease' };
const body = (text: string): WnsDocument => ({ schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text }] }] } });
async function fixture() {
  const original = body('Current writing'); const historical = body('Earlier writing');
  const document: DocumentRecord = { head: { documentId: 'chapter', version: '4', bodyHash: await bodyHash(canonicalJson(original)) }, body: original,
    title: 'Chapter', kind: 'chapter', metadataVersion: '0', lastCheckpointId: null };
  const revision: Revision = { id: 'old', head: { documentId: 'chapter', version: '1', bodyHash: await bodyHash(canonicalJson(historical)) }, body: historical, reason: 'manual', parentId: null };
  const ack = (request: RestoreRevision): RestoreAck => ({ access: request.access, operationId: request.operationId, alreadyApplied: false,
    result: { head: { ...document.head, version: '5', bodyHash: revision.head.bodyHash }, savedGeneration: request.localGeneration,
      restored: { revisionId: revision.id, beforeRevisionId: 'before', afterRevisionId: 'after' } },
    document: { ...document, head: { ...document.head, version: '5', bodyHash: revision.head.bodyHash }, body: historical, lastCheckpointId: 'after' } });
  const restore = vi.fn(async (request: RestoreRevision) => ack(request));
  const reconcile = vi.fn<ProjectTransport['reconcile']>(); const save = vi.fn<ProjectTransport['save']>();
  const transport: ProjectTransport = { restore, reconcile, save, validate: async () => {}, checkpoint: async request => ({ id: 'checkpoint', head: request.expected,
    body: request.expected.bodyHash === revision.head.bodyHash ? historical : original, reason: request.reason, parentId: null }) };
  const session = new DocumentSession(access, document, transport, { autosave: false, newId: () => 'restore-operation' });
  const commit = vi.fn(() => historical);
  const run = () => session.restoreRevision(revision, () => ({ body: historical, commit }));
  const receipt = async (request: RestoreRevision): Promise<OperationReceipt> => {
    const { session: _session, writerLease: _lease, ...logicalAccess } = request.access;
    return { operationId: request.operationId, operationKind: 'restore', payloadHash: await bodyHash(canonicalJson({ ...request, access: logicalAccess })), result: ack(request).result };
  };
  return { original, historical, document, revision, ack, restore, reconcile, save, transport, session, commit, run, receipt };
}

describe('explicit revision restore session', () => {
  it('refuses unavailable restore support before flushing local edits', async () => {
    const h = await fixture(); h.transport.restore = undefined; h.session.update(body('Still typing'));
    await expect(h.run()).rejects.toMatchObject({ code: 'RestoreUnavailable' });
    expect(h.save).not.toHaveBeenCalled(); expect(h.session.body).toEqual(body('Still typing'));
    expect(h.session.state.editable).toBe(true); expect(h.session.state.dirty).toBe(true);
  });
  it('keeps the live editor unchanged and gates typing and navigation until the exact durable result arrives', async () => {
    const h = await fixture(); let finish!: (value: RestoreAck) => void;
    h.restore.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const pending = h.run(); await vi.waitFor(() => expect(h.restore).toHaveBeenCalledOnce());
    expect(h.commit).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.original); expect(h.session.state.editable).toBe(false);
    expect(() => h.session.update(body('typed during restore'))).toThrow();
    const destination = vi.fn(async () => 'next'); const leaving = h.session.detachAfter(destination);
    await Promise.resolve(); expect(destination).not.toHaveBeenCalled();
    finish(h.ack(h.restore.mock.calls[0][0])); await pending; await leaving;
    expect(h.commit).toHaveBeenCalledOnce(); expect(h.save).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.historical);
    expect(h.session.state.savedGeneration).toBe('1'); expect(destination).toHaveBeenCalledOnce();
  });
  it('refuses a foreign, corrupt, identical or incorrectly preflighted saved version before dispatch', async () => {
    const h = await fixture();
    await expect(h.session.restoreRevision({ ...h.revision, head: { ...h.revision.head, documentId: 'other' } }, () => ({ body: h.historical, commit: h.commit }))).rejects.toMatchObject({ code: 'InvalidRevision' });
    await expect(h.session.restoreRevision({ ...h.revision, body: body('corrupt') }, () => ({ body: h.historical, commit: h.commit }))).rejects.toMatchObject({ code: 'InvalidRevision' });
    await expect(h.session.restoreRevision({ ...h.revision, head: h.document.head, body: h.original }, () => ({ body: h.original, commit: h.commit }))).rejects.toMatchObject({ code: 'NoChanges' });
    await expect(h.session.restoreRevision(h.revision, () => ({ body: h.original, commit: h.commit }))).rejects.toMatchObject({ code: 'InvalidRevision' });
    expect(h.restore).not.toHaveBeenCalled(); expect(h.commit).not.toHaveBeenCalled(); expect(h.session.state.editable).toBe(true);
  });
  it('reconciles a lost acknowledgment without dispatching the restore twice or autosaving it', async () => {
    const h = await fixture(); h.restore.mockRejectedValue(new Error('lost ACK'));
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.restore.mock.calls[0][0];
    expect(h.commit).not.toHaveBeenCalled();
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'new-lease' }, document: h.ack(request).document, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.restore).toHaveBeenCalledOnce(); expect(h.commit).toHaveBeenCalledOnce();
    expect(h.save).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.historical); expect(h.session.state.phase).toBe('editing');
  });
  it('retries only a fenced absent operation with its unchanged identity and a new lease', async () => {
    const h = await fixture(); h.restore.mockRejectedValueOnce(new Error('lost before dispatch'));
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.restore.mock.calls[0][0];
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'new-lease' }, document: h.document, receipts: [] });
    await h.session.reconcile(); expect(h.restore).toHaveBeenCalledTimes(2);
    expect(h.restore.mock.calls[1][0]).toEqual({ ...request, access: { ...access, writerLease: 'new-lease' } }); expect(h.commit).toHaveBeenCalledOnce();
  });
  it('preserves later saved prose for comparison instead of replaying an old restore receipt', async () => {
    const h = await fixture(); h.restore.mockRejectedValue(new Error('lost ACK'));
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.restore.mock.calls[0][0];
    const latestBody = body('Newer writing after restore');
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'new-lease' }, document: { ...h.document, body: latestBody,
      head: { ...h.document.head, version: '6', bodyHash: await bodyHash(canonicalJson(latestBody)) } }, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.session.state.phase).toBe('conflict'); expect(h.session.body).toEqual(h.original);
    expect(h.session.savedConflict?.body).toEqual(latestBody); expect(h.commit).not.toHaveBeenCalled(); expect(h.restore).toHaveBeenCalledOnce();
  });
  it('fences mismatched saved revision decisions even when the returned body matches', async () => {
    const h = await fixture(); h.restore.mockImplementation(async request => ({ ...h.ack(request), result: { ...h.ack(request).result,
      restored: { revisionId: 'another-revision', beforeRevisionId: 'before', afterRevisionId: 'after' } } }));
    await expect(h.run()).rejects.toMatchObject({ code: 'ProtocolError' }); expect(h.commit).not.toHaveBeenCalled(); expect(h.session.state.editable).toBe(false);
    const request = h.restore.mock.calls[0][0]; const wrong = await h.receipt(request); wrong.operationKind = 'apply';
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'new-lease' }, document: h.ack(request).document, receipts: [wrong] });
    await expect(h.session.reconcile()).rejects.toMatchObject({ code: 'ProtocolError' }); expect(h.commit).not.toHaveBeenCalled();
  });
  it('keeps post-commit display failure recoverable without another restore', async () => {
    const h = await fixture(); h.commit.mockImplementationOnce(() => { throw { code: 'PersistenceUnavailable', detail: 'Display failed after commit' }; });
    await expect(h.run()).rejects.toMatchObject({ code: 'PersistenceUnavailable' }); expect(h.session.state.phase).toBe('reconciling');
    const request = h.restore.mock.calls[0][0];
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'new-lease' }, document: h.ack(request).document, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.restore).toHaveBeenCalledOnce(); expect(h.commit).toHaveBeenCalledTimes(2); expect(h.save).not.toHaveBeenCalled();
  });
});
