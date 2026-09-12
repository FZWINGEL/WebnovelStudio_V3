// @vitest-environment node
import { describe, expect, it, vi } from 'vitest';
import { ComposerSession } from './composer';
import { DEFAULT_LOOKUP_ALLOWANCE, type DiscussionDraft, type SaveDiscussionDraft } from '../ipc/discussions';

const access = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
function ack(request: SaveDiscussionDraft): DiscussionDraft { return { documentId: request.documentId, version: (BigInt(request.expectedVersion) + 1n).toString(), text: request.text, intent: request.intent, basis: request.basis, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, previousRunId: request.previousRunId, safeBrief: request.safeBrief, lookup: request.lookup, updatedAt: 'today' }; }
describe('unsent discussion persistence', () => {
  it('keeps a continuation basis through lost save acknowledgment, later choice and reopening', async () => {
    const write = vi.fn(async (request: SaveDiscussionDraft) => ack(request));
    write.mockRejectedValueOnce(new Error('lost reply'));
    const session = new ComposerSession('document', null, () => access, write);
    const sent = { text: 'Let her ask about the letter.', scope: null, pinnedDocumentIds: [], intent: 'continue' as const, basis: 'working' as const };
    session.update(sent); await expect(session.save()).rejects.toThrow('lost reply');
    session.update({ ...sent, basis: 'reviewed' });
    expect(session.clearIfUnchanged(sent)).toBe(false);
    await session.save();
    expect(write.mock.calls[0][0]).toEqual(write.mock.calls[1][0]);
    const restored = new ComposerSession('document', ack(write.mock.calls[2][0]), () => access, write);
    expect(restored.body.intent).toBe('continue'); expect(restored.body.basis).toBe('reviewed');
    expect(restored.dirty).toBe(false);
  });
  it('retains exact brief approval and origins across a lost reply, later edits and reopening', async () => {
    const write = vi.fn(async (request: SaveDiscussionDraft) => ack(request));
    write.mockRejectedValueOnce(new Error('lost reply'));
    const session = new ComposerSession('document', null, () => access, write);
    const approved = { text: 'Give him a pause.', scope: null, pinnedDocumentIds: [], intent: 'proposeEdits' as const, safeBrief: { text: 'He recognizes the pendant.', originMessageId: 'private-message', confirmed: true } };
    session.update(approved); await expect(session.save()).rejects.toThrow('lost reply');
    session.update({ ...approved, safeBrief: { ...approved.safeBrief, text: 'He briefly recognizes the pendant.', confirmed: false } });
    expect(session.clearIfUnchanged(approved)).toBe(false);
    await session.save();
    expect(write.mock.calls[0][0]).toEqual(write.mock.calls[1][0]);
    const restored = new ComposerSession('document', ack(write.mock.calls[2][0]), () => access, write);
    expect(restored.body.safeBrief).toEqual({ text: 'He briefly recognizes the pendant.', originMessageId: 'private-message', confirmed: false });
    expect(restored.dirty).toBe(false);
  });
  it('persists an opted-in lookup allowance through a lost reply and reopening', async () => {
    const write = vi.fn(async (request: SaveDiscussionDraft) => ack(request));
    write.mockRejectedValueOnce(new Error('lost reply'));
    const session = new ComposerSession('document', null, () => access, write);
    const body = { text: 'Check whether the pendant was mentioned earlier.', scope: null, pinnedDocumentIds: [], lookup: { ...DEFAULT_LOOKUP_ALLOWANCE } };
    session.update(body);
    await expect(session.save()).rejects.toThrow('lost reply');
    await session.save();
    expect(write.mock.calls[0][0]).toEqual(write.mock.calls[1][0]);
    expect(write.mock.calls[0][0].lookup).toEqual(DEFAULT_LOOKUP_ALLOWANCE);
    const restored = new ComposerSession('document', ack(write.mock.calls[1][0]), () => access, write);
    expect(restored.body.lookup).toEqual(DEFAULT_LOOKUP_ALLOWANCE);
    expect(restored.dirty).toBe(false);
  });
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

  it('persists Suggest edits and treats an intent change as a new composer state', async () => {
    const write = vi.fn(async (request: SaveDiscussionDraft) => ack(request));
    const session = new ComposerSession('document', null, () => access, write);
    session.update({ text: 'Suggest a sharper line.', intent: 'proposeEdits', scope: { kind: 'passage', sourceBodyHash: 'source', quote: 'A line.', start: { blockId: 'paragraph', utf16Offset: 0 }, end: { blockId: 'paragraph', utf16Offset: 7 } }, pinnedDocumentIds: [], previousRunId: 'stopped-run' });
    await session.save();
    const restored = new ComposerSession('document', ack(write.mock.calls[0][0]), () => access, write);
    expect(restored.body.intent).toBe('proposeEdits');
    expect(restored.dirty).toBe(false);
    restored.update({ ...restored.body, intent: 'discuss', previousRunId: undefined });
    expect(restored.dirty).toBe(true);
  });

  it('accepts a semantically identical scope acknowledgment with reordered fields', async () => {
    const scope = { kind: 'blocks' as const, sourceBodyHash: 'source', start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: 'q', utf16Offset: 12 }, quote: 'A complete range.' };
    const write = vi.fn(async (request: SaveDiscussionDraft): Promise<DiscussionDraft> => ({
      ...ack(request),
      // Rust serializes the same scope in kind/start/end/quote/source order.
      scope: request.scope ? { kind: request.scope.kind, start: request.scope.start, end: request.scope.end, quote: request.scope.quote, sourceBodyHash: request.scope.sourceBodyHash } : null,
    }));
    const session = new ComposerSession('document', null, () => access, write);
    session.update({ text: 'Use the full paragraph range.', scope, pinnedDocumentIds: [] });
    await expect(session.save()).resolves.toBeUndefined();
    expect(session.dirty).toBe(false);
  });

  it.each([
    ['endpoint', (scope: NonNullable<SaveDiscussionDraft['scope']>) => ({ ...scope, end: { ...scope.end!, utf16Offset: scope.end!.utf16Offset + 1 } })],
    ['source hash', (scope: NonNullable<SaveDiscussionDraft['scope']>) => ({ ...scope, sourceBodyHash: 'changed-source' })],
    ['quote', (scope: NonNullable<SaveDiscussionDraft['scope']>) => ({ ...scope, quote: 'Changed quotation.' })],
  ] as const)('rejects an acknowledgment with a changed %s and retries the exact request', async (_label, mutate) => {
    const scope = { kind: 'blocks' as const, sourceBodyHash: 'source', start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: 'q', utf16Offset: 12 }, quote: 'A complete range.' };
    let first = true;
    const write = vi.fn(async (request: SaveDiscussionDraft): Promise<DiscussionDraft> => {
      const result = ack(request);
      if (first) { first = false; return { ...result, scope: mutate(scope) }; }
      return result;
    });
    const session = new ComposerSession('document', null, () => access, write);
    session.update({ text: 'Use the full paragraph range.', scope, pinnedDocumentIds: [] });
    await expect(session.save()).rejects.toThrow(/did not match/u);
    expect(session.dirty).toBe(true);
    await session.save();
    expect(write).toHaveBeenCalledTimes(2);
    expect(write.mock.calls[1][0]).toEqual(write.mock.calls[0][0]);
    expect(session.dirty).toBe(false);
  });
});
