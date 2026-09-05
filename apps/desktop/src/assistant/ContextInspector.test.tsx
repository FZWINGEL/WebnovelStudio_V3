// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContextInspector } from './ContextInspector';
import * as context from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';

vi.mock('../ipc/context', () => ({
  preparedStoryContext: vi.fn(), preparedStoryContextIsCurrent: vi.fn(), storyContextSnapshot: vi.fn(),
  readStoryContextSource: vi.fn(), searchStoryContext: vi.fn(),
}));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const descriptor = (handle: string, title: string): context.SourceDescriptor => ({ handle, displayName: title, source: { projectId: 'project', documentId: handle, revisionId: handle, bodyHash: 'hash' }, kind: 'currentDraft', current: true, coverage: 'verbatim', disclosure: { readerPosition: '1', visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] });
const first = descriptor('first', 'The promise'); const second = descriptor('second', 'The separation');
const packet: context.CompiledPacket = { messages: [], options: { modelId: 'mock-story-context', maxOutputTokens: '100', tokenAccountingMethod: 'mock' }, receipt: { packetId: 'packet', sessionId: 'context', snapshotId: 'snapshot', invocationOrdinal: '0', sourceHandles: ['first'], coverage: [{ handle: 'first', label: 'fullText', detail: 'verbatim' }], omissions: ['handle:second;reason:optional source omitted by input budget;blocks:2'], inputHash: 'hash', inputTokens: '100', tokenAccountingMethod: 'mock' } };
const frozen: context.FrozenContext = { snapshot: { snapshotId: 'snapshot', projectId: 'project', basis: 'working', target: first.source, contextSourceEpoch: '1', orderingEpoch: '1', disclosurePolicyVersion: '0', sources: [first, second] }, policy: { version: '0', audience: 'authorRoom', readerFrontier: null, characterId: null, characterGrants: [], allowAlternatives: false, allowHistorical: false }, purpose: 'discuss', aliases: {}, excludedSourceCount: 0 };
let host: HTMLDivElement; let root: Root;
async function render(packetId = 'packet', refreshKey = '1', delivered = true) {
  await act(async () => root.render(<ContextInspector access={access} packetId={packetId} delivered={delivered} refreshKey={refreshKey} />));
}
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks();
  vi.mocked(context.preparedStoryContext).mockResolvedValue(packet);
  vi.mocked(context.storyContextSnapshot).mockResolvedValue(frozen);
  vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(true);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('historical context inspection', () => {
  it('keeps the exact adopted instruction visible when its packet becomes historical', async () => {
    const record = { handle: 'guidance-v1', projectId: access.projectId, version: { guidanceId: 'guidance', versionId: 'v1', version: '1', scope: 'document' as const, documentId: first.source.documentId, text: 'Keep the ending.\nHer sister survives.', textHash: 'hash', active: true, originMessageId: null, createdAt: 'then' } };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, guidanceHandles: [record.handle] } });
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, guidance: [record] });
    await render();
    expect(host.textContent).toContain('Used · 1 source · 1 instruction');
    expect(host.querySelector('.context-guidance p')?.textContent).toBe(record.version.text);
    expect(host.textContent).toContain('version 1');
    vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(false);
    await render('packet', 'guidance-edited');
    expect(host.textContent).toContain('Needs refresh');
    expect(host.querySelector('.context-guidance p')?.textContent).toBe(record.version.text);
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
  });

  it('distinguishes prepared input from delivered sources and renders omitted source titles', async () => {
    await render('packet', '1', false);
    expect(host.textContent).toContain('Prepared · 1 source');
    expect(host.textContent).toContain('Available · 2 sources');
    expect(host.textContent).toContain('This request has not been sent to a model.');
    expect(host.textContent).toContain('The separation: 2 blocks were not included');
    expect(host.textContent).not.toContain('handle:second');
    await render('packet', '1', true);
    expect(host.textContent).toContain('Used · 1 source');
  });

  it('clears loaded evidence when changed policy denies the historical receipt', async () => {
    await render();
    expect(host.textContent).toContain('The promise');
    vi.mocked(context.preparedStoryContext).mockRejectedValue({ code: 'ContextPolicyChanged', detail: 'Source permissions changed.' });
    await render('packet', '2');
    expect(host.textContent).toContain('Source permissions changed.');
    expect(host.textContent).not.toContain('The promise');
  });

  it('does not display a late source read after switching to another packet', async () => {
    let resolve!: (value: context.SourceRead) => void;
    vi.mocked(context.readStoryContextSource).mockReturnValue(new Promise(accept => { resolve = accept; }));
    await render();
    const button = Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'The promise')!;
    await act(async () => button.click());
    await render('different-packet');
    await act(async () => resolve({ descriptor: first, usedValidatedProjection: false, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [{ handle: 'first', source: first.source, blockId: 'block', blockOrder: 0, text: 'Evidence from the old request.' }] }));
    expect(host.textContent).not.toContain('Evidence from the old request.');
  });
});
