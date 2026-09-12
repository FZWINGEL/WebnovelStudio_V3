// @vitest-environment jsdom
import { act, StrictMode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SourceRef } from '../ipc/context';
import * as memory from '../ipc/memory';
import type { ProjectAccess } from '../ipc/projects';
import { ReviewSummaryEditor, type ReviewSummaryDraft } from '../editor/ReviewSummaryEditor';

vi.mock('../ipc/memory', async importOriginal => ({ ...await importOriginal<typeof memory>(), readMemory: vi.fn() }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const target = { documentId: 'chapter', version: '4', bodyHash: 'body-hash' };
const source: SourceRef = { projectId: access.projectId, documentId: target.documentId, revisionId: 'revision', bodyHash: target.bodyHash };
const current = { id: 'summary-1', text: 'Mei still protects the gate.', audience: 'authorRoom' as const };
let host: HTMLDivElement;
let root: Root;
let value: ReviewSummaryDraft;
const onChange = vi.fn<(value: ReviewSummaryDraft) => void>();

async function render(overrides: Partial<React.ComponentProps<typeof ReviewSummaryEditor>> = {}) {
  await act(async () => root.render(<ReviewSummaryEditor access={access} documentId={target.documentId} target={target} current={current}
    canInherit value={value} disabled={false} onChange={onChange} {...overrides} />));
}
async function renderStrict(overrides: Partial<React.ComponentProps<typeof ReviewSummaryEditor>> = {}) {
  await act(async () => root.render(<StrictMode><ReviewSummaryEditor access={access} documentId={target.documentId} target={target} current={current}
    canInherit value={value} disabled={false} onChange={onChange} {...overrides} /></StrictMode>));
}
function button(name: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === name)!; }
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(accept => { resolve = accept; }), resolve }; }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.clearAllMocks();
  value = { choice: 'inherit', text: '', audience: 'authorRoom' };
  onChange.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('accepted narrative summary editor', () => {
  it('keeps an exact accepted summary separate from explicit edits and clearing', async () => {
    await render();
    expect((host.querySelector('#review-summary-text') as HTMLTextAreaElement).value).toBe(current.text);
    expect(host.textContent).toContain('Using the accepted summary from this exact chapter');
    await act(async () => button('Clear summary').click());
    expect(onChange).toHaveBeenCalledWith({ choice: 'clear', text: '', audience: 'authorRoom' });
  });

  it('copies only a current nonhistorical memory candidate as editable text', async () => {
    const candidate = { schemaVersion: 'story-memory.v1', source, items: [
      { text: 'Mei guards the gate.', uncertainty: null, evidence: [{ blockId: 'p', fromUtf16: 0, toUtf16: 4, quote: 'Mei' }] },
      { text: 'The oath may be broken.', uncertainty: 'The evidence is incomplete.', evidence: [] },
    ] };
    vi.mocked(memory.readMemory).mockResolvedValue({ documentId: target.documentId, jobs: [{
      id: 'job', owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, jobId: 'job' }, operationId: 'operation',
      payloadHash: 'payload', target, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1', disclosurePolicyVersion: '1',
      providerBinding: null, status: 'completed', dispatchState: 'dispatched', stopReason: null, result: {
        jobId: 'job', eventId: 'event', rawOutput: null, outcome: 'completed', confirmedStdinBytes: null, usage: null, cleanup: 'settled',
        error: null, validationError: null, candidate, effectiveIdentity: null, createdAt: '2026-09-06T00:00:00Z',
      }, view: null, historical: false, createdAt: '2026-09-06T00:00:00Z', updatedAt: '2026-09-06T00:00:00Z',
    }], views: [{ id: 'view', jobId: 'job', projectId: access.projectId, operationNamespace: access.operationNamespace, documentId: target.documentId,
      target, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1', disclosurePolicyVersion: '1', candidate, current: true,
      sourceChanged: false, policyAvailable: true, historical: false, createdAt: '2026-09-06T00:00:00Z' }] });
    await render(); await act(async () => button('Use generated memory as a starting point').click());
    expect(memory.readMemory).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith({ choice: 'set', audience: 'authorRoom', text: 'Mei guards the gate.\n\nThe oath may be broken.\nNeeds checking: The evidence is incomplete.' });
    expect(host.textContent).toContain('editable author summary');
  });

  it('refuses historical memory instead of turning it into an accepted summary', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue({ documentId: target.documentId, jobs: [], views: [] });
    await render(); await act(async () => button('Use generated memory as a starting point').click());
    expect(onChange).not.toHaveBeenCalled();
    expect(host.textContent).toContain('No current story memory result matches');
  });

  it('does not apply a late memory response after the draft context changes', async () => {
    const pending = deferred<memory.MemoryRead>(); vi.mocked(memory.readMemory).mockReturnValueOnce(pending.promise);
    await render(); await act(async () => button('Use generated memory as a starting point').click());
    await render({ value: { choice: 'set', text: 'The author changed this draft.', audience: 'authorRoom' } });
    await act(async () => pending.resolve({ documentId: target.documentId, jobs: [], views: [] }));
    expect(onChange).not.toHaveBeenCalled();
    expect(button('Use generated memory as a starting point').disabled).toBe(false);
  });

  it('rejects memory returned for another chapter', async () => {
    vi.mocked(memory.readMemory).mockResolvedValue({ documentId: 'other-chapter', jobs: [], views: [] });
    await render(); await act(async () => button('Use generated memory as a starting point').click());
    expect(onChange).not.toHaveBeenCalled();
    expect(host.textContent).toContain('belongs to another chapter');
  });

  it('rejects a candidate whose source revision differs from its current view', async () => {
    const candidate = { schemaVersion: 'story-memory.v1', source: { ...source, revisionId: 'different-revision' }, items: [{ text: 'Old memory', uncertainty: null, evidence: [] }] };
    vi.mocked(memory.readMemory).mockResolvedValue({ documentId: target.documentId, jobs: [], views: [{ id: 'view', jobId: 'job', projectId: access.projectId,
      operationNamespace: access.operationNamespace, documentId: target.documentId, target, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1',
      disclosurePolicyVersion: '1', candidate, current: true, sourceChanged: false, policyAvailable: true, historical: false, createdAt: '2026-09-06T00:00:00Z' }] });
    await render(); await act(async () => button('Use generated memory as a starting point').click());
    expect(onChange).not.toHaveBeenCalled();
    expect(host.textContent).toContain('No current story memory result matches');
  });

  it('accepts a current memory result under React StrictMode', async () => {
    const candidate = { schemaVersion: 'story-memory.v1', source, items: [{ text: 'Mei guards the gate.', uncertainty: null, evidence: [] }] };
    vi.mocked(memory.readMemory).mockResolvedValue({ documentId: target.documentId, jobs: [], views: [{ id: 'view', jobId: 'job', projectId: access.projectId,
      operationNamespace: access.operationNamespace, documentId: target.documentId, target, source, snapshotId: 'snapshot', packetId: 'packet', contextSourceEpoch: '1',
      disclosurePolicyVersion: '1', candidate, current: true, sourceChanged: false, policyAvailable: true, historical: false, createdAt: '2026-09-06T00:00:00Z' }] });
    await renderStrict(); await act(async () => button('Use generated memory as a starting point').click());
    expect(onChange).toHaveBeenCalledWith({ choice: 'set', text: 'Mei guards the gate.', audience: 'authorRoom' });
  });

  it('requires an explicit replacement when the saved summary basis is stale', async () => {
    await render({ canInherit: false, value: { choice: 'required', text: current.text, audience: current.audience } });
    expect(host.textContent).toContain('belongs to another saved chapter');
    expect((host.querySelector('#review-summary-text') as HTMLTextAreaElement).disabled).toBe(false);
  });
});
