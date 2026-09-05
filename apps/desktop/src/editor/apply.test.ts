import { describe, expect, it, vi } from 'vitest';
import { DocumentSession } from './session';
import { bodyHash, canonicalJson, type WnsDocument } from './document';
import type { ApplyAck, ApplyProposal, PreparedProposal, Proposal } from '../ipc/proposals';
import type { DocumentRecord, OperationReceipt, ProjectAccess, ProjectTransport, ReconciledDocument } from '../ipc/projects';

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'renderer', writerLease: 'lease' };
function body(text: string): WnsDocument { return { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text }] }] } }; }
function gate<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; }
async function setup() {
  const original = body('source'); const result = body('replacement');
  const document: DocumentRecord = { head: { documentId: 'chapter', version: '0', bodyHash: await bodyHash(canonicalJson(original)) }, title: 'Chapter', kind: 'chapter', metadataVersion: '0', body: original, lastCheckpointId: 'before' };
  const prepared: PreparedProposal = { id: 'prepared', proposalId: 'proposal', version: '1', replacementText: 'replacement', body: result, bodyHash: await bodyHash(canonicalJson(result)) };
  const proposal: Proposal = { id: 'proposal', runId: 'run', candidate: { title: 'Clearer', replacementText: 'replacement', explanation: 'Test replacement.' }, source: document.head, sourceBody: original,
    scope: { kind: 'passage', start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: 'p', utf16Offset: 6 }, sourceHash: document.head.bodyHash, quote: 'source', quoteHash: 'quote', prefix: null, suffix: null },
    packetId: 'packet', snapshotId: 'snapshot', current: true, historicalCopy: false, prepared, decision: null };
  const ack = (request: ApplyProposal): ApplyAck => ({ access: request.access, operationId: request.operationId, alreadyApplied: false,
    result: { head: { ...request.expected, version: '1', bodyHash: prepared.bodyHash }, savedGeneration: request.localGeneration, applied: { decisionId: 'decision', proposalId: proposal.id, preparedId: prepared.id, beforeRevisionId: 'before', afterRevisionId: 'after' } },
    document: { ...document, head: { ...request.expected, version: '1', bodyHash: prepared.bodyHash }, body: result, lastCheckpointId: 'after' } });
  const apply = vi.fn(async (request: ApplyProposal) => ack(request));
  const reconcile = vi.fn<(_: Parameters<ProjectTransport['reconcile']>[0]) => Promise<ReconciledDocument>>();
  const save = vi.fn<ProjectTransport['save']>();
  const transport: ProjectTransport = { apply, reconcile, save, validate: async () => {}, checkpoint: async request => ({ id: 'checkpoint', head: request.expected, body: result, reason: request.reason, parentId: 'before' }) };
  const session = new DocumentSession(access, document, transport, { autosave: false, newId: () => 'apply-operation' });
  const commit = vi.fn(() => result);
  const run = () => session.applyPrepared(proposal, prepared, () => ({ body: result, commit }));
  const receipt = async (request: ApplyProposal): Promise<OperationReceipt> => {
    const { session: _session, writerLease: _lease, ...logicalAccess } = request.access;
    return { operationId: request.operationId, operationKind: 'apply', payloadHash: await bodyHash(canonicalJson({ ...request, access: logicalAccess })), result: ack(request).result };
  };
  return { session, transport, apply, reconcile, save, document, original, result, proposal, prepared, ack, commit, run, receipt };
}

describe('durable Apply session handoff', () => {
  it('gates input and navigation until commit, then adopts one saved generation without an autosave', async () => {
    const h = await setup(); const wait = gate<ApplyAck>(); h.apply.mockImplementation(() => wait.promise);
    const pending = h.run(); await vi.waitFor(() => expect(h.apply).toHaveBeenCalledOnce());
    expect(h.session.state.editable).toBe(false); expect(h.commit).not.toHaveBeenCalled();
    expect(() => h.session.update(body('a keystroke'))).toThrow(/pending document operation/u);
    const destination = vi.fn(async () => 'other chapter'); const switching = h.session.detachAfter(destination);
    await Promise.resolve(); expect(destination).not.toHaveBeenCalled();
    wait.resolve(h.ack(h.apply.mock.calls[0][0])); await pending; await switching;
    expect(h.commit).toHaveBeenCalledOnce(); expect(h.save).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.result);
    expect(h.session.state.savedGeneration).toBe('1'); expect(h.session.state.dirty).toBe(false); expect(destination).toHaveBeenCalledOnce();
  });
  it('waits for composition and refuses a source edited before the guard was acquired', async () => {
    const h = await setup(); h.session.setComposing(true); const applying = h.run();
    await Promise.resolve(); expect(h.apply).not.toHaveBeenCalled();
    h.session.setComposing(false); await applying; expect(h.commit).toHaveBeenCalledOnce();
    const stale = await setup(); stale.proposal.current = false;
    await expect(stale.run()).rejects.toMatchObject({ code: 'SuggestionStale' });
    expect(stale.apply).not.toHaveBeenCalled(); expect(stale.session.state.editable).toBe(true);
  });
  it('does not display an uncommitted result when Rust refuses the prepared version', async () => {
    const h = await setup(); h.apply.mockRejectedValue({ code: 'PreparedVersionConflict', detail: 'Suggestion wording changed.' });
    await expect(h.run()).rejects.toMatchObject({ code: 'PreparedVersionConflict' });
    expect(h.commit).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.original); expect(h.session.state.editable).toBe(true);
  });
  it('settles a lost acknowledgment using the exact receipt without repeating Apply', async () => {
    const h = await setup(); h.apply.mockRejectedValue(new Error('lost acknowledgment'));
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.apply.mock.calls[0][0];
    expect(h.commit).not.toHaveBeenCalled(); expect(h.session.state.editable).toBe(false);
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: h.ack(request).document, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.apply).toHaveBeenCalledOnce(); expect(h.commit).toHaveBeenCalledOnce();
    expect(h.session.state.phase).toBe('editing'); expect(h.session.state.savedGeneration).toBe('1'); expect(h.session.body).toEqual(h.result);
  });
  it('reuses the immutable operation only after a fence proves it absent', async () => {
    const h = await setup(); h.apply.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Disconnected' });
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const original = h.apply.mock.calls[0][0];
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: h.document, receipts: [] });
    await h.session.reconcile(); expect(h.apply).toHaveBeenCalledTimes(2);
    expect(h.apply.mock.calls[1][0]).toEqual({ ...original, access: { ...access, writerLease: 'fenced' } });
    expect(h.commit).toHaveBeenCalledOnce(); expect(h.session.state.editable).toBe(true);
  });
  it('preserves a later durable body for comparison instead of replaying a historical Apply', async () => {
    const h = await setup(); h.apply.mockRejectedValue(new Error('lost acknowledgment'));
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.apply.mock.calls[0][0];
    const later = body('later author text'); const latest: DocumentRecord = { ...h.document, head: { ...h.document.head, version: '2', bodyHash: await bodyHash(canonicalJson(later)) }, body: later };
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: latest, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.commit).not.toHaveBeenCalled(); expect(h.session.state.phase).toBe('conflict');
    expect(h.session.body).toEqual(h.original); expect(h.session.savedConflict?.body).toEqual(later); expect(h.apply).toHaveBeenCalledOnce();
  });
  it('rejects foreign acknowledgments and changed receipt identities without dispatching', async () => {
    const h = await setup(); h.apply.mockImplementation(async request => ({ ...h.ack(request), access: { ...access, session: 'different-renderer' } }));
    await expect(h.run()).rejects.toMatchObject({ code: 'ProtocolError' }); const request = h.apply.mock.calls[0][0];
    const wrong = await h.receipt(request); wrong.result.applied!.preparedId = 'another-preparation';
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: h.ack(request).document, receipts: [wrong] });
    await expect(h.session.reconcile()).rejects.toMatchObject({ code: 'ProtocolError' });
    expect(h.commit).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.original); expect(h.session.state.editable).toBe(false);
  });
  it('keeps a post-commit display failure recoverable without another Apply or save', async () => {
    const h = await setup(); h.commit.mockImplementationOnce(() => { throw new Error('editor callback failed'); });
    await expect(h.run()).rejects.toMatchObject({ code: 'UncertainOutcome' }); const request = h.apply.mock.calls[0][0];
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: h.ack(request).document, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.apply).toHaveBeenCalledOnce(); expect(h.commit).toHaveBeenCalledTimes(2);
    expect(h.save).not.toHaveBeenCalled(); expect(h.session.body).toEqual(h.result);
  });
  it('retains the actual visible buffer after a post-dispatch failure for later conflict recovery', async () => {
    const h = await setup(); let visible = h.original;
    const pending = h.session.applyPrepared(h.proposal, h.prepared, () => ({ body: h.result, read: () => visible,
      commit: () => { visible = h.result; throw { code: 'PersistenceUnavailable', detail: 'A display callback failed after the durable ACK.' }; } }));
    await expect(pending).rejects.toMatchObject({ code: 'PersistenceUnavailable' });
    expect(h.session.state.phase).toBe('reconciling'); expect(h.session.body).toEqual(visible); expect(h.session.state.editable).toBe(false);
    const request = h.apply.mock.calls[0][0]; const later = body('later saved prose');
    h.reconcile.mockResolvedValue({ access: { ...access, writerLease: 'fenced' }, document: { ...h.document, body: later,
      head: { ...h.document.head, version: '2', bodyHash: await bodyHash(canonicalJson(later)) } }, receipts: [await h.receipt(request)] });
    await h.session.reconcile(); expect(h.session.state.phase).toBe('conflict');
    expect(h.session.body).toEqual(h.result); expect(h.session.savedConflict?.body).toEqual(later);
  });
});
