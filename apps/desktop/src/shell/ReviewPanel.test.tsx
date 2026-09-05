// @vitest-environment jsdom
import { act, useEffect, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { DocumentSession } from '../editor/session';
import type { DocumentRecord, ProjectAccess, ProjectTransport } from '../ipc/projects';
import * as ipc from '../ipc/reviews';
import * as history from '../ipc/history';
import { ReviewPanel } from './ReviewPanel';

vi.mock('../ipc/reviews', () => ({ chapterReviewStatus: vi.fn(), stageAuthorReview: vi.fn(), readReviewStage: vi.fn(), markReady: vi.fn() }));
vi.mock('../ipc/history', () => ({ readDocumentRevision: vi.fn(), listDocumentHistory: vi.fn() }));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body = (text: string): WnsDocument => ({ schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text }] }] } });
let host: HTMLDivElement; let root: Root; let session: DocumentSession; let record: DocumentRecord;
let staged: ipc.ReviewStage; let transport: ProjectTransport;
function Harness({ visible = true, current = session }: { visible?: boolean; current?: DocumentSession }) {
  const [state, setState] = useState(current.state);
  useEffect(() => { setState(current.state); return current.subscribe(() => setState(current.state)); }, [current]);
  return <ReviewPanel session={current} state={state} visible={visible} onClose={() => {}} />;
}
async function render(visible = true, current = session) { await act(async () => root.render(<Harness visible={visible} current={current} />)); }
const button = (name: string) => [...host.querySelectorAll('button')].find(item => item.textContent === name)!;
async function click(name: string) { await act(async () => button(name).click()); }
async function waitFor(check: () => void) { for (let i = 0; i < 40; i++) { try { check(); return; } catch { await act(async () => new Promise(resolve => setTimeout(resolve, 5))); } } check(); }
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(accept => { resolve = accept; }), resolve: (value: T) => resolve(value) }; }

beforeEach(async () => {
  vi.clearAllMocks();
  const prose = body('Mei left the key beside the gate.');
  record = { head: { documentId: 'chapter', version: '1', bodyHash: await bodyHash(canonicalJson(prose)) }, body: prose,
    kind: 'chapter', title: 'At the gate', metadataVersion: '1', lastCheckpointId: 'revision' };
  transport = { validate: async () => {}, save: vi.fn(), checkpoint: vi.fn(), reconcile: vi.fn(async () => ({ access: { ...access, writerLease: 'new-lease' }, document: record, receipts: [] })) };
  session = new DocumentSession(access, record, transport, { autosave: false });
  staged = { id: 'stage', projectId: 'project', operationNamespace: 'namespace', target: record.head,
    revision: { id: 'revision', head: record.head, body: record.body, reason: 'review', parentId: null },
    previousBundleId: null, prefix: [], sourceEpoch: '3', policyEpoch: '0', createdAt: '2026-09-06T00:00:00Z' };
  vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'noReview', activeBundleId: null, pendingStageId: null, reason: null, canStage: true });
  vi.mocked(ipc.stageAuthorReview).mockImplementation(async () => structuredClone(staged));
  vi.mocked(ipc.readReviewStage).mockImplementation(async () => structuredClone(staged));
  vi.mocked(ipc.markReady).mockResolvedValue({ id: 'bundle', projectId: 'project', operationNamespace: 'namespace', target: record.head, stageId: 'stage', createdAt: '2026-09-06T00:01:00Z' });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('author review', () => {
  it('previews an exact saved chapter before the separate author decision', async () => {
    await render(); expect(ipc.stageAuthorReview).not.toHaveBeenCalled(); expect(ipc.markReady).not.toHaveBeenCalled();
    await click('Review saved chapter'); await waitFor(() => expect(host.textContent).toContain('Mei left the key'));
    expect(ipc.markReady).not.toHaveBeenCalled(); expect(document.activeElement?.id).toBe('review-heading');
    await click('Mark this version reviewed'); expect(ipc.markReady).toHaveBeenCalledOnce();
    expect(vi.mocked(ipc.markReady).mock.calls[0][0]).toMatchObject({ stageId: 'stage' });
    expect(session.body).toEqual(record.body); expect(transport.save).not.toHaveBeenCalled();
    expect(host.textContent).toContain('Your review is saved.');
    expect(document.activeElement?.id).toBe('review-heading');
  });
  it('explains a missing earlier basis without disabling ordinary writing', async () => {
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'reviewNeeded', activeBundleId: null, pendingStageId: null, reason: 'Review The Departure first.', canStage: false });
    await render(); expect(host.textContent).toContain('Review The Departure first.');
    expect(button('Review saved chapter')).toBeUndefined(); expect(session.state.editable).toBe(true);
    expect(ipc.stageAuthorReview).not.toHaveBeenCalled();
  });
  it('refuses a returned review of different or corrupt writing', async () => {
    vi.mocked(ipc.stageAuthorReview).mockResolvedValue({ ...staged, revision: { ...staged.revision, body: body('Different writing') } });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Check review save')).toBeDefined());
    expect(host.querySelector('.saved-prose')).toBeNull(); expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('keeps the staged writing fixed when the author types and asks for a new review', async () => {
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await act(async () => session.update(body('Mei took the key.')));
    expect(host.textContent).toContain('Your writing changed.'); expect(host.querySelector('.saved-prose')!.textContent).toContain('left the key');
    expect(button('Mark this version reviewed')).toBeUndefined(); expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('reconciles a lost acknowledgment and retries the identical decision under the new lease', async () => {
    vi.mocked(ipc.markReady).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Acknowledgment lost.' });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Mark this version reviewed'); expect(button('Check review save')).toBeDefined();
    const original = vi.mocked(ipc.markReady).mock.calls[0][0];
    await click('Check review save'); expect(transport.reconcile).toHaveBeenCalledOnce();
    await waitFor(() => expect(ipc.markReady).toHaveBeenCalledTimes(2));
    expect(vi.mocked(ipc.markReady).mock.calls[1][0]).toEqual({ ...original, access: { ...access, writerLease: 'new-lease' } });
    expect(host.textContent).toContain('Your review is saved.'); expect(session.body).toEqual(record.body);
  });
  it('retains the pending review when its panel is closed and reopened', async () => {
    vi.mocked(ipc.markReady).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Acknowledgment lost.' });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Mark this version reviewed'); await render(false); await render(true);
    expect(button('Check review save')).toBeDefined(); expect(vi.mocked(ipc.markReady).mock.calls).toHaveLength(1);
  });
  it('offers a new explicit review after an earlier source made the stage stale', async () => {
    vi.mocked(ipc.markReady).mockRejectedValueOnce({ code: 'ReviewStageStale', detail: 'The earlier story changed. Prepare it again.' });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Mark this version reviewed'); await waitFor(() => expect(button('Review saved chapter')).toBeDefined());
    expect(host.textContent).toContain('The earlier story changed.'); expect(button('Check review save')).toBeUndefined(); expect(session.state.editable).toBe(true);
    expect(ipc.stageAuthorReview).toHaveBeenCalledOnce(); expect(ipc.markReady).toHaveBeenCalledOnce();
  });
  it('drops a late status from another project', async () => {
    const old = deferred<ipc.ReviewStatus>(); vi.mocked(ipc.chapterReviewStatus).mockReturnValueOnce(old.promise);
    await render(); const other = new DocumentSession({ ...access, projectId: 'other' }, record, transport, { autosave: false });
    await render(true, other); await act(async () => old.resolve({ documentId: 'chapter', title: 'Old project', head: record.head, state: 'ready', activeBundleId: 'old', pendingStageId: null, reason: 'Do not show this old response', canStage: true }));
    expect(host.textContent).not.toContain('Do not show this old response');
  });
  it('clears a failed status read after a successful explicit refresh', async () => {
    vi.mocked(ipc.chapterReviewStatus).mockRejectedValueOnce(new Error('Temporary read problem'));
    await render(); expect(host.textContent).toContain('Temporary read problem');
    await click('Refresh review'); expect(host.textContent).not.toContain('Temporary read problem');
    expect(button('Review saved chapter')).toBeDefined();
  });
  it('resumes a durable unaccepted stage after a fresh panel mount without repeating the author decision', async () => {
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'noReview', activeBundleId: null, pendingStageId: 'stage', reason: null, canStage: true });
    await render(); expect(ipc.readReviewStage).not.toHaveBeenCalled();
    await click('Resume saved review'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    expect(ipc.readReviewStage).toHaveBeenCalledExactlyOnceWith(access, 'stage');
    expect(ipc.stageAuthorReview).not.toHaveBeenCalled(); expect(ipc.markReady).not.toHaveBeenCalled();
    expect(host.textContent).toContain('Mei left the key');
  });
  it('reads an earlier chapter only on expansion and validates its exact reviewed revision', async () => {
    const earlierBody = body('The promise before the journey.');
    const earlierHead = { documentId: 'earlier', version: '4', bodyHash: await bodyHash(canonicalJson(earlierBody)) };
    staged.prefix = [{ documentId: 'earlier', title: 'The promise', bundleId: 'earlier-bundle', revisionId: 'earlier-revision', head: earlierHead }];
    vi.mocked(history.readDocumentRevision).mockResolvedValue({ id: 'earlier-revision', head: earlierHead, body: earlierBody, parentId: null, reason: 'authorReview' });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(host.querySelector('details')).not.toBeNull());
    expect(history.readDocumentRevision).not.toHaveBeenCalled();
    await act(async () => { const details = host.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); });
    await waitFor(() => expect(host.textContent).toContain('The promise before the journey.'));
    expect(history.readDocumentRevision).toHaveBeenCalledExactlyOnceWith(access, 'earlier', 'earlier-revision');
    expect(session.body).toEqual(record.body); expect(ipc.markReady).not.toHaveBeenCalled();
  });
});
