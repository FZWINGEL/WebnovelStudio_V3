import { describe, expect, it, vi } from 'vitest';
import { ComposerSession } from './composer';
import type { DiscussionDraft, SaveDiscussionDraft } from '../ipc/discussions';

const access = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
function ack(request: SaveDiscussionDraft): DiscussionDraft { return { documentId: request.documentId, version: (BigInt(request.expectedVersion) + 1n).toString(), text: request.text, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, previousRunId: request.previousRunId, updatedAt: 'today' }; }
describe('unsent discussion persistence', () => {
  it('does not clear a retry choice when an unrelated send acknowledgment arrives', async () => {
    const write = vi.fn(async (request: SaveDiscussionDraft) => ack(request));
    const session = new ComposerSession('document', null, () => access, write);
    const text = { text: 'Keep the ending.', scope: null, pinnedDocumentIds: [] };
    session.update({ ...text, previousRunId: 'stopped-run' });
    expect(session.clearIfUnchanged(text)).toBe(false);
    await session.save();
    const restored = new ComposerSession('document', ack(write.mock.calls[0][0]), () => access, write);
    expect(restored.body.previousRunId).toBe('stopped-run');
    expect(restored.dirty).toBe(false);
  });
  it('retains immutable retry content after a lost reply and then saves later typing', async () => {
    let fail = true;
    const requests: SaveDiscussionDraft[] = [];
    const write = vi.fn(async (request: SaveDiscussionDraft) => { requests.push(structuredClone(request)); if (fail) { fail = false; throw new Error('lost reply'); } return ack(request); });
    const session = new ComposerSession('document', null, () => access, write);
    session.update({ text: 'Keep the ending.', scope: null, pinnedDocumentIds: [] });
    await expect(session.save()).rejects.toThrow('lost reply');
    session.update({ text: 'Keep the ending. Make the argument more emotional.', scope: null, pinnedDocumentIds: ['chapter-seven'] });
    await session.save();
    expect(requests).toHaveLength(3);
    expect(requests[0]).toEqual(requests[1]);
    expect(requests[2].expectedVersion).toBe('1');
    expect(requests[2].operationId).not.toBe(requests[1].operationId);
    expect(session.body.text).toContain('more emotional');
    expect(session.dirty).toBe(false);
  });
  it('a late send acknowledgement cannot erase a newer composer draft', () => {
    const session = new ComposerSession('document', null, () => access, async request => ack(request));
    const sent = { text: 'Make this quieter.', scope: null, pinnedDocumentIds: [] };
    session.update(sent);
    session.update({ ...sent, text: 'One more thought.' });
    expect(session.clearIfUnchanged(sent)).toBe(false);
    expect(session.body.text).toBe('One more thought.');
  });
  it('retries under a fresh writer lease without changing the operation or draft scope', async () => {
    let currentAccess = access; let fails = true;
    const write = vi.fn(async (request: SaveDiscussionDraft) => { if (fails) { fails = false; throw new Error('fenced'); } return ack(request); });
    const session = new ComposerSession('document', null, () => currentAccess, write);
    session.update({ text: 'Protect this line.', scope: { kind: 'passage', sourceBodyHash: 'old-source', quote: 'Promise.', start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: 'p', utf16Offset: 8 } }, pinnedDocumentIds: [] });
    await expect(session.save()).rejects.toThrow('fenced');
    currentAccess = { ...access, writerLease: 'fresh-lease' };
    await session.save();
    expect(write.mock.calls[1][0]).toEqual({ ...write.mock.calls[0][0], access: currentAccess });
  });
});
