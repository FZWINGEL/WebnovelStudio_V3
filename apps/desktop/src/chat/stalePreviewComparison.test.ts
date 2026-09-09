import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChatAdoptionPreview } from '../ipc/projectChat';
import { readStalePreviewComparison } from './stalePreviewComparison';

const mocks = vi.hoisted(() => ({ read: vi.fn(), document: vi.fn() }));
vi.mock('../ipc/projectChat', () => ({ readProjectConversation: mocks.read }));
vi.mock('../ipc/projects', () => ({ readDocument: mocks.document }));
const access = { projectId: 'p', operationNamespace: 'n', session: 's', writerLease: 'l' };
const current = { head: { documentId: 'target', version: '4', bodyHash: 'new' }, title: 'World', kind: 'world', metadataVersion: '1', lastCheckpointId: null,
  body: { schemaVersion: 1, body: { type: 'doc', content: [] } } };
const preview = { id: 'preview', projectId: 'p', operationNamespace: 'n', conversationId: 'c', version: '1', digest: 'digest', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '0',
  targets: [{ documentId: 'target' }, { documentId: 'new-target', before: null }] } as ChatAdoptionPreview;
beforeEach(() => {
  vi.resetAllMocks();
  mocks.read.mockResolvedValue({ id: 'c', sourceEpoch: '3', policyEpoch: '2' });
  mocks.document.mockImplementation(async (_access, id) => { if (id === 'new-target') throw { code: 'DocumentNotFound' }; return current; });
});
describe('saved stale-preview comparison', () => {
  it('retains the original preview and reports saved target plus broad source and policy changes', async () => {
    const exact = JSON.stringify(preview);
    const result = await readStalePreviewComparison(access, preview);
    expect(result).toEqual({ previewId: 'preview', targets: [{ documentId: 'target', current }, { documentId: 'new-target', current: null }],
      sourceEpoch: '1', currentSourceEpoch: '3', policyEpoch: '1', currentPolicyEpoch: '2' });
    expect(JSON.stringify(preview)).toBe(exact);
  });
  it('refuses a mixed comparison when a source changes during the reads', async () => {
    mocks.read.mockResolvedValueOnce({ id: 'c', sourceEpoch: '3', policyEpoch: '2' }).mockResolvedValueOnce({ id: 'c', sourceEpoch: '4', policyEpoch: '2' });
    await expect(readStalePreviewComparison(access, preview)).rejects.toThrow('story changed');
  });
  it('does not disguise failed permission reads as missing documents or read another project', async () => {
    mocks.document.mockRejectedValue({ code: 'LeaseExpired', detail: 'Reconcile first' });
    await expect(readStalePreviewComparison(access, preview)).rejects.toMatchObject({ code: 'LeaseExpired' });
    mocks.document.mockClear();
    await expect(readStalePreviewComparison({ ...access, projectId: 'other' }, preview)).rejects.toThrow('another project');
    expect(mocks.document).not.toHaveBeenCalled();
  });
});
