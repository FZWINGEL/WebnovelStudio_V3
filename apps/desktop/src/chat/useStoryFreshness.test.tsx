// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DiscussionRun } from '../ipc/discussions';
import type { ProjectAccess } from '../ipc/projects';
import { useStoryFreshness } from './useStoryFreshness';

const mocks = vi.hoisted(() => ({
  preparedStoryContext: vi.fn(),
  storyContextSnapshot: vi.fn(),
}));

vi.mock('../ipc/context', () => mocks);

const access: ProjectAccess = { projectId: 'project-1', session: 'session-1', writerLease: 'lease-1', operationNamespace: 'namespace-1' };
const run = (id: string, packetId = `packet-${id}`): DiscussionRun => ({
  id, threadId: 'thread-1', owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, runId: id },
  operationId: `operation-${id}`, target: { documentId: 'chapter-1', version: '1', bodyHash: 'hash' }, packetId,
  payloadHash: 'payload', previousRunId: null, status: 'running', dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null,
  createdAt: '2026-09-09T00:00:00Z', updatedAt: '2026-09-09T00:00:00Z',
});

function packet(snapshotId: string, packetId: string) { return { receipt: { packetId, snapshotId } }; }
function frozen(source: string, policy: string, snapshotId = `snapshot-${source}`) { return { snapshot: { projectId: access.projectId, snapshotId, contextSourceEpoch: source, disclosurePolicyVersion: policy } }; }

let host: HTMLDivElement;
let root: Root;

function Probe({ activeRun, source = '1', policy = '1' }: { activeRun: DiscussionRun | null; source?: string; policy?: string }) {
  const state = useStoryFreshness({ access, run: activeRun, currentSourceEpoch: source, currentPolicyEpoch: policy });
  return <output data-status={state.status}>{state.status}:{state.detail ?? ''}</output>;
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  mocks.preparedStoryContext.mockReset(); mocks.storyContextSnapshot.mockReset();
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('useStoryFreshness', () => {
  it('marks an active response based on older sources as soon as the live epoch changes', async () => {
    mocks.preparedStoryContext.mockResolvedValue(packet('snapshot-1', 'packet-run-1'));
    mocks.storyContextSnapshot.mockResolvedValue(frozen('1', '1', 'snapshot-1'));
    await act(async () => root.render(<Probe activeRun={run('run-1')} />));
    await act(async () => Promise.resolve());
    expect(host.querySelector('output')?.dataset.status).toBe('current');
    await act(async () => root.render(<Probe activeRun={run('run-1')} source="2" />));
    expect(host.querySelector('output')?.dataset.status).toBe('stale');
    expect(host.textContent).toContain('Story sources changed');
    expect(mocks.preparedStoryContext).toHaveBeenCalledOnce();
  });

  it('discards a late read from the previous run identity', async () => {
    let resolveOld!: (value: unknown) => void;
    let resolveNew!: (value: unknown) => void;
    mocks.preparedStoryContext
      .mockReturnValueOnce(new Promise(resolve => { resolveOld = resolve; }))
      .mockReturnValueOnce(new Promise(resolve => { resolveNew = resolve; }));
    mocks.storyContextSnapshot
      .mockResolvedValueOnce(frozen('1', '1', 'snapshot-old'))
      .mockResolvedValueOnce(frozen('2', '1', 'snapshot-new'));
    await act(async () => root.render(<Probe activeRun={run('old')} source="1" />));
    await act(async () => root.render(<Probe activeRun={run('new')} source="2" />));
    await act(async () => resolveOld(packet('snapshot-old', 'packet-old')));
    expect(host.querySelector('output')?.dataset.status).toBe('checking');
    await act(async () => resolveNew(packet('snapshot-new', 'packet-new')));
    await act(async () => Promise.resolve());
    expect(host.querySelector('output')?.dataset.status).toBe('current');
  });
});
