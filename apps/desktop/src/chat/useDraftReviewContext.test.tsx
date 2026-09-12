// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { WnsDocument } from '../kernel';
import type { AssistantDraft, ProjectConversationView } from '../ipc/projectChat';
import type { DocumentRecord, ProjectAccess } from '../ipc/projects';

const { readPage } = vi.hoisted(() => ({ readPage: vi.fn() }));
vi.mock('../ipc/projectChat', async importOriginal => ({ ...(await importOriginal<typeof import('../ipc/projectChat')>()), readProjectConversation: readPage }));
import { useDraftReviewContext } from './useDraftReviewContext';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'Draft' }] }] } };
const draftDocument: DocumentRecord = { head: { documentId: 'draft-1', version: '1', bodyHash: 'a'.repeat(64) }, title: 'Draft', kind: 'world', metadataVersion: '1', body, lastCheckpointId: null, role: 'assistantDraft' };
const draft: AssistantDraft = { document: draftDocument, conversationId: 'conversation-1', originRunId: 'old-run', packetId: 'packet-1', initialRevisionId: 'revision-1', target: null, disposition: 'pending', dispositionVersion: '1', stale: false };
const currentRun = { id: 'current-run', outputText: JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'Current', assumptions: [] }) };
const oldRun = { id: 'old-run', outputText: JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'Old', assumptions: [{ key: 'a', text: 'Keep the harbor quiet.' }] }) };
const item = (run: Record<string, unknown>, sequence: string, instruction: string): ProjectConversationView['items'][number] => ({ id: `item-${run.id}`, sequence, kind: 'request', referenceId: String(run.id), payload: { instruction, run }, createdAt: '2026-01-01T00:00:00Z' });
const view = (overrides: Partial<ProjectConversationView> = {}): ProjectConversationView => ({ id: 'conversation-1', composer: { conversationId: 'conversation-1', version: '1', body: { text: 'Keep this composer text', sourceRefs: [], taskDraftRefs: [] } }, items: [item(currentRun, '40', 'Current request')], olderBefore: '40', activeRun: null, drafts: [draft], sourceEpoch: '1', policyEpoch: '1', earlierWorkshop: false, workerIssues: [], ...overrides });

function Harness({ currentView, currentAccess = access, enabled = true }: { currentView: ProjectConversationView | null; currentAccess?: ProjectAccess; enabled?: boolean }) {
  const result = useDraftReviewContext({ access: currentAccess, view: currentView, enabled });
  return <output data-loading={String(result.loading)} data-error={result.error ?? ''} data-composer={currentView?.composer.body.text ?? ''}>{JSON.stringify(result.context)}</output>;
}

let host: HTMLDivElement;
let root: Root;
async function settle(): Promise<void> { await act(async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); }); }

beforeEach(() => {
  readPage.mockReset();
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('useDraftReviewContext', () => {
  it('walks two older pages to recover an exact request for an old draft', async () => {
    readPage.mockResolvedValueOnce({ ...view(), items: [item({ id: 'middle-run', outputText: '' }, '30', 'Middle request')], olderBefore: '30', drafts: [] });
    readPage.mockResolvedValueOnce({ ...view(), items: [item(oldRun, '20', 'Original request for the harbor')], olderBefore: null, drafts: [] });
    await act(async () => root.render(<Harness currentView={view()} />));
    await settle();
    expect(readPage).toHaveBeenNthCalledWith(1, access, '40', 100);
    expect(readPage).toHaveBeenNthCalledWith(2, access, '30', 100);
    expect(readPage).toHaveBeenCalledTimes(2);
    expect(host.querySelector('output')?.textContent).toContain('Original request for the harbor');
    expect(host.querySelector('output')?.textContent).toContain('Keep the harbor quiet.');
    expect(host.querySelector('output')?.dataset.composer).toBe('Keep this composer text');
  });

  it('does not repeat old-page reads when polling recreates an otherwise identical view', async () => {
    readPage.mockResolvedValueOnce({ ...view(), items: [item(oldRun, '20', 'Original request for the harbor')], olderBefore: null, drafts: [] });
    const first = view();
    await act(async () => root.render(<Harness currentView={first} />));
    await settle();
    await act(async () => root.render(<Harness currentView={{ ...first, items: [...first.items] }} />));
    await settle();
    expect(readPage).toHaveBeenCalledTimes(1);
  });

  it('fails closed on a repeated cursor and does not update after unmount', async () => {
    readPage.mockResolvedValue({ ...view(), items: [], olderBefore: '40' });
    await act(async () => root.render(<Harness currentView={view()} />));
    await settle();
    expect(host.querySelector('output')?.dataset.error).toContain('same older-page cursor');

    let resolve!: (value: ProjectConversationView) => void;
    readPage.mockReset(); readPage.mockReturnValueOnce(new Promise<ProjectConversationView>(done => { resolve = done; }));
    await act(async () => root.render(<Harness currentView={view()} />));
    await act(async () => root.unmount());
    resolve({ ...view(), items: [item(oldRun, '20', 'Late request')], olderBefore: null, drafts: [] });
    await settle();
  });

  it('clears cached provenance when the project identity changes', async () => {
    readPage.mockResolvedValueOnce({ ...view(), items: [item(oldRun, '20', 'Project one request')], olderBefore: null, drafts: [] });
    await act(async () => root.render(<Harness currentView={view()} />));
    await settle();
    const projectTwo = { ...access, projectId: 'project-2', operationNamespace: 'namespace-2', session: 'session-2', writerLease: 'lease-2' };
    readPage.mockResolvedValueOnce({ ...view(), items: [item({ ...oldRun, id: 'old-run' }, '20', 'Project two request')], olderBefore: null, drafts: [] });
    await act(async () => root.render(<Harness currentView={view({ id: 'conversation-2' })} currentAccess={projectTwo} />));
    await settle();
    expect(readPage).toHaveBeenNthCalledWith(2, projectTwo, '40', 100);
    expect(host.querySelector('output')?.textContent).toContain('Project two request');
    expect(host.querySelector('output')?.textContent).not.toContain('Project one request');
  });
});
