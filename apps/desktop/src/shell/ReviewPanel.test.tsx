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

vi.mock('../ipc/reviews', async importOriginal => ({ ...await importOriginal<typeof ipc>(), reviewedPromiseCatalog: vi.fn(), chapterReviewStatus: vi.fn(), stageAuthorReview: vi.fn(), readReviewedRecordSet: vi.fn(), reviewedEntityCatalog: vi.fn(), readReviewStage: vi.fn(), markReady: vi.fn() }));
vi.mock('../ipc/history', () => ({ readDocumentRevision: vi.fn(), listDocumentHistory: vi.fn() }));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body = (text: string): WnsDocument => ({ schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text }] }] } });
let host: HTMLDivElement; let root: Root; let session: DocumentSession; let record: DocumentRecord;
let staged: ipc.ReviewStage; let transport: ProjectTransport;
const detail = (): ipc.PossessionRecord => ({ id: 'detail-1', object: { id: 'object-key', label: 'Key' }, holder: { id: 'holder-mei', label: 'Mei' }, timing: 'unknown', audience: 'authorRoom', evidence: { blockId: 'p', fromUtf16: 0, toUtf16: 17, quote: 'Mei held the key.', quoteHash: 'a'.repeat(64) } });
async function summaryHashFor(summary: ipc.SummaryRevision): Promise<string> {
  return bodyHash(JSON.stringify({ id: summary.id, text: summary.text, audience: summary.audience,
    source: { projectId: summary.source.projectId, documentId: summary.source.documentId, revisionId: summary.source.revisionId, bodyHash: summary.source.bodyHash },
    dependencies: summary.dependencies.map(member => ({ documentId: member.documentId, title: member.title, bundleId: member.bundleId, revisionId: member.revisionId,
      head: { documentId: member.head.documentId, version: member.head.version, bodyHash: member.head.bodyHash } })) }));
}
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
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
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
  vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue(null);
  vi.mocked(ipc.reviewedEntityCatalog).mockResolvedValue({ projectId: access.projectId, operationNamespace: access.operationNamespace, sourceEpoch: '0', entities: [] });
  vi.mocked(ipc.reviewedPromiseCatalog).mockResolvedValue({ projectId: access.projectId, operationNamespace: access.operationNamespace, sourceEpoch: '0', entities: [] });
  vi.mocked(ipc.stageAuthorReview).mockImplementation(async () => structuredClone(staged));
  vi.mocked(ipc.readReviewStage).mockImplementation(async () => structuredClone(staged));
  vi.mocked(ipc.markReady).mockResolvedValue({ id: 'bundle', projectId: 'project', operationNamespace: 'namespace', target: record.head, stageId: 'stage', createdAt: '2026-09-06T00:01:00Z' });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('author review', () => {
  it('removes prior status actions while a fresh read is pending or fails', async () => {
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValueOnce({ documentId: 'chapter', title: record.title, head: record.head, state: 'noReview', activeBundleId: null, pendingStageId: 'old-stage', reason: null, canStage: true });
    await render(); expect(button('Resume saved review')).toBeDefined();
    let reject!: (reason: unknown) => void;
    vi.mocked(ipc.chapterReviewStatus).mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
    await click('Refresh review');
    expect(button('Resume saved review')).toBeUndefined(); expect(button('Review saved chapter')).toBeUndefined();
    await act(async () => reject(new Error('Could not read current review.')));
    expect(host.textContent).toContain('Could not read current review.');
    expect(button('Resume saved review')).toBeUndefined(); expect(button('Review saved chapter')).toBeUndefined();
    expect(ipc.readReviewStage).not.toHaveBeenCalled(); expect(button('Refresh review').disabled).toBe(false);
  });
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
  it('stages an explicit narrative summary and validates the returned identity', async () => {
    staged.records = [detail()];
    staged.summary = { id: 'summary-1', text: 'Mei guards the gate.', audience: 'authorRoom', source: { projectId: access.projectId, documentId: record.head.documentId, revisionId: 'revision', bodyHash: record.head.bodyHash }, dependencies: [] };
    staged.summaryHash = await summaryHashFor(staged.summary);
    vi.mocked(ipc.stageAuthorReview).mockImplementation(async request => {
      const result = structuredClone(staged);
      if (request.summary?.kind === 'set' && result.summary) { result.summary = { ...result.summary, text: request.summary.text, audience: request.summary.audience }; result.summaryHash = await summaryHashFor(result.summary); }
      return result;
    });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    const editor = host.querySelector('#review-summary-text') as HTMLTextAreaElement;
    await act(async () => { const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setValue.call(editor, 'The gate is under threat.'); editor.dispatchEvent(new Event('input', { bubbles: true })); });
    await waitFor(() => expect(button('Save reviewed details')).toBeDefined()); await click('Save reviewed details'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].summary).toEqual({ kind: 'set', text: 'The gate is under threat.', audience: 'authorRoom' });
    expect(host.textContent).toContain('Key');
    await click('Edit'); await waitFor(() => expect(host.querySelector('[aria-label="Edit possession detail"]')).not.toBeNull());
    const selects = host.querySelectorAll('select');
    await act(async () => { (selects[2] as HTMLSelectElement).value = 'earlier'; selects[2].dispatchEvent(new Event('change', { bubbles: true })); });
    await click('Keep detail'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined()); await click('Save reviewed details');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[2][0].summary).toEqual({ kind: 'set', text: 'The gate is under threat.', audience: 'authorRoom' });
  });
  it('refuses a staged summary with a tampered fingerprint', async () => {
    staged.summary = { id: 'summary-1', text: 'Mei guards the gate.', audience: 'authorRoom', source: { projectId: access.projectId, documentId: record.head.documentId, revisionId: 'revision', bodyHash: record.head.bodyHash }, dependencies: [] };
    staged.summaryHash = await summaryHashFor(staged.summary);
    vi.mocked(ipc.stageAuthorReview).mockResolvedValue({ ...structuredClone(staged), summaryHash: '0'.repeat(64) });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Check review save')).toBeDefined());
    expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('refuses a mark acknowledgment with a changed summary fingerprint', async () => {
    staged.summary = { id: 'summary-1', text: 'Mei guards the gate.', audience: 'authorRoom', source: { projectId: access.projectId, documentId: record.head.documentId, revisionId: 'revision', bodyHash: record.head.bodyHash }, dependencies: [] };
    staged.summaryHash = await summaryHashFor(staged.summary);
    vi.mocked(ipc.markReady).mockResolvedValue({ id: 'bundle', projectId: 'project', operationNamespace: 'namespace', target: record.head, stageId: 'stage', createdAt: '2026-09-06T00:01:00Z', summary: staged.summary, summaryHash: '0'.repeat(64) });
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined()); await click('Mark this version reviewed'); await waitFor(() => expect(button('Check review save')).toBeDefined());
    expect(ipc.markReady).toHaveBeenCalledOnce();
  });
  it('requires an explicit summary decision before staging after the reviewed basis changes', async () => {
    const oldBody = body('The old saved chapter.');
    const oldHead = { documentId: record.head.documentId, version: '3', bodyHash: await bodyHash(canonicalJson(oldBody)) };
    const oldRevision = { id: 'old-revision', head: oldHead, body: oldBody, reason: 'review', parentId: null };
    const oldSummary: ipc.SummaryRevision = { id: 'old-summary', text: 'The old chapter held the gate.', audience: 'authorRoom', source: { projectId: access.projectId, documentId: oldHead.documentId, revisionId: oldRevision.id, bodyHash: oldHead.bodyHash }, dependencies: [] };
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'changedProse', activeBundleId: 'bundle', pendingStageId: null, reason: 'Writing changed since review.', canStage: true });
    vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue({ bundleId: 'bundle', projectId: access.projectId, operationNamespace: access.operationNamespace, target: oldHead, revision: oldRevision, records: [], current: false, summary: oldSummary, summaryHash: await summaryHashFor(oldSummary) });
    await render(); expect(host.textContent).toContain('belongs to another saved chapter');
    await click('Review saved chapter'); expect(ipc.stageAuthorReview).not.toHaveBeenCalled(); expect(host.textContent).toContain('Use this summary');
    await click('Clear summary'); await click('Review saved chapter');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[0][0].summary).toEqual({ kind: 'clear' });
  });
  it('preserves an explicit summary clear when a staged detail is restaged', async () => {
    const oldBody = body('The old saved chapter.');
    const oldHead = { documentId: record.head.documentId, version: '3', bodyHash: await bodyHash(canonicalJson(oldBody)) };
    const oldRevision = { id: 'old-revision', head: oldHead, body: oldBody, reason: 'review', parentId: null };
    const oldSummary: ipc.SummaryRevision = { id: 'old-summary', text: 'The old chapter held the gate.', audience: 'authorRoom', source: { projectId: access.projectId, documentId: oldHead.documentId, revisionId: oldRevision.id, bodyHash: oldHead.bodyHash }, dependencies: [] };
    staged.records = [detail()];
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'changedProse', activeBundleId: 'bundle', pendingStageId: null, reason: 'Writing changed since review.', canStage: true });
    vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue({ bundleId: 'bundle', projectId: access.projectId, operationNamespace: access.operationNamespace, target: oldHead, revision: oldRevision, records: [], current: false, summary: oldSummary, summaryHash: await summaryHashFor(oldSummary) });
    vi.mocked(ipc.stageAuthorReview).mockImplementation(async _request => { const result = structuredClone(staged); delete result.summary; delete result.summaryHash; return result; });
    await render(); await click('Clear summary'); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Edit'); const selects = host.querySelectorAll('select'); await act(async () => { (selects[2] as HTMLSelectElement).value = 'earlier'; selects[2].dispatchEvent(new Event('change', { bubbles: true })); });
    await click('Keep detail'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined()); await click('Save reviewed details');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].summary).toEqual({ kind: 'clear' });
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
  it('blocks the review decision while a detail form is open', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Edit');
    expect(button('Mark this version reviewed').disabled).toBe(true);
    expect(host.textContent).toContain('Finish this reviewed detail before saving the review.');
    expect(button('Keep detail').disabled).toBe(false); expect(button('Cancel').disabled).toBe(false);
    await click('Cancel'); await waitFor(() => expect(button('Mark this version reviewed').disabled).toBe(false));
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
  it('sends the complete edited record array and retries the identical stage payload', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Edit');
    const selects = host.querySelectorAll('select');
    await act(async () => { (selects[2] as HTMLSelectElement).value = 'earlier'; selects[2].dispatchEvent(new Event('change', { bubbles: true })); });
    await click('Keep detail'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined());
    vi.mocked(ipc.stageAuthorReview).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Acknowledgment lost.' }).mockResolvedValue(structuredClone(staged));
    await click('Save reviewed details'); expect(button('Check review save')).toBeDefined();
    const original = vi.mocked(ipc.stageAuthorReview).mock.calls[1][0];
    expect(original.records).toEqual([expect.objectContaining({ id: 'detail-1', timing: 'earlier' })]);
    await click('Check review save'); await waitFor(() => expect(ipc.stageAuthorReview).toHaveBeenCalledTimes(3));
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[2][0]).toEqual({ ...original, access: { ...access, writerLease: 'new-lease' } });
  });
  it('sends an explicit empty array when the author removes inherited details', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Remove'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined());
    await click('Save reviewed details');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].records).toEqual([]);
  });
  it('keeps a stage uncertain when the acknowledgment returns a different reviewed record set', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Remove'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined());
    vi.mocked(ipc.stageAuthorReview).mockResolvedValueOnce({ ...structuredClone(staged), records: [detail()] });
    await click('Save reviewed details'); await waitFor(() => expect(button('Check review save')).toBeDefined());
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].records).toEqual([]);
    expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('restages the full typed set after prose changes instead of silently inheriting an older bundle', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    const nextBody = body('Mei held the key. Later, she left.');
    const nextHead = { documentId: 'chapter', version: '2', bodyHash: await bodyHash(canonicalJson(nextBody)) };
    vi.mocked(transport.save).mockImplementation(async request => ({ projectId: access.projectId, documentId: 'chapter', session: access.session, operationNamespace: access.operationNamespace, operationId: request.operationId, head: nextHead, savedGeneration: request.localGeneration }));
    vi.mocked(ipc.stageAuthorReview).mockImplementationOnce(async request => ({ ...structuredClone(staged), target: request.expected, revision: { id: 'revision-2', head: request.expected, body: nextBody, reason: 'review', parentId: 'revision' }, records: structuredClone(request.records ?? []) }));
    await act(async () => session.update(nextBody));
    await waitFor(() => expect(button('Review latest saved chapter')).toBeDefined()); await click('Review latest saved chapter'); await waitFor(() => expect(ipc.stageAuthorReview).toHaveBeenCalledTimes(2));
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].records).toEqual([expect.objectContaining({ id: 'detail-1' })]);
  });
  it('keeps edited details visible when a definite restage refusal occurs', async () => {
    staged.records = [detail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined());
    await click('Edit');
    const selects = host.querySelectorAll('select');
    await act(async () => { (selects[2] as HTMLSelectElement).value = 'earlier'; selects[2].dispatchEvent(new Event('change', { bubbles: true })); });
    await click('Keep detail'); await waitFor(() => expect(button('Save reviewed details')).toBeDefined());
    vi.mocked(ipc.stageAuthorReview).mockRejectedValueOnce({ code: 'ReviewStageStale', detail: 'The saved review is stale.' });
    await click('Save reviewed details'); await waitFor(() => expect(host.textContent).toContain('The saved review is stale.'));
    expect(host.textContent).toContain('Mei held the key.'); expect(button('Review saved chapter')).toBeDefined();
  });
  it('reads the selected record set after reload and offers the same-prose detail update', async () => {
    const saved = detail();
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'ready', activeBundleId: 'bundle', pendingStageId: null, reason: null, canStage: true });
    vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue({ bundleId: 'bundle', projectId: 'project', operationNamespace: 'namespace', target: record.head, revision: staged.revision, records: [saved], recordsHash: 'b'.repeat(64), current: true });
    await render(); await waitFor(() => expect(host.textContent).toContain('Mei held the key.'));
    expect(ipc.readReviewedRecordSet).toHaveBeenCalledExactlyOnceWith(access, 'chapter'); expect(button('Update reviewed details')).toBeDefined();
    expect(ipc.stageAuthorReview).not.toHaveBeenCalled();
  });
  it('keeps stale historical details available for explicit removal before restaging', async () => {
    const saved = detail();
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'changedProse', activeBundleId: 'bundle', pendingStageId: null, reason: 'Writing changed since review.', canStage: true });
    vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue({ bundleId: 'bundle', projectId: 'project', operationNamespace: 'namespace', target: { ...record.head, version: '0' }, revision: staged.revision, records: [saved], recordsHash: 'b'.repeat(64), current: false });
    await render(); await waitFor(() => expect(button('Remove')).toBeDefined());
    await click('Remove'); await click('Review saved chapter');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[0][0].records).toEqual([]);
  });
});

const promiseDetail = (): ipc.PromiseRecord => ({ id: 'promise-record', promise: { id: 'return-key', label: 'Return the key' }, phase: 'setup', timing: 'unknown', note: 'Ren promises to return the key.', audience: 'authorRoom', evidence: detail().evidence });
describe('reviewed promises', () => {
  it('retains an inherited promise in the staged review without implicitly clearing it', async () => {
    staged.promises = [promiseDetail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Promises (1)')).toBeDefined()); await click('Promises (1)');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[0][0].promises).toBeUndefined();
    expect(host.textContent).toContain('Ren promises to return the key.');
    await click('Mark this version reviewed'); await waitFor(() => expect(ipc.markReady).toHaveBeenCalledOnce()); expect(session.body).toEqual(record.body);
  });
  it('retries the exact explicit promise clear without changing possession records', async () => {
    staged.records = [detail()]; staged.promises = [promiseDetail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Promises (1)')).toBeDefined()); await click('Promises (1)'); await click('Remove promise');
    vi.mocked(ipc.stageAuthorReview).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Lost promise acknowledgment.' }).mockImplementation(async request => ({ ...structuredClone(staged), promises: request.promises }));
    await click('Save reviewed details');
    const original = vi.mocked(ipc.stageAuthorReview).mock.calls[1][0];
    expect(original.promises).toEqual([]); expect(original.records).toEqual([expect.objectContaining({ id: 'detail-1' })]);
    await click('Check review save'); await waitFor(() => expect(ipc.stageAuthorReview).toHaveBeenCalledTimes(3));
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[2][0]).toEqual({ ...original, access: { ...access, writerLease: 'new-lease' } });
    await waitFor(() => expect(button('Mark this version reviewed')).toBeDefined()); expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('refuses a different promise set in the stage acknowledgment', async () => {
    staged.promises = [promiseDetail()];
    await render(); await click('Review saved chapter'); await waitFor(() => expect(button('Promises (1)')).toBeDefined()); await click('Promises (1)'); await click('Remove promise');
    await click('Save reviewed details'); await waitFor(() => expect(button('Check review save')).toBeDefined());
    expect(button('Check review save')).toBeDefined(); expect(vi.mocked(ipc.stageAuthorReview).mock.calls[1][0].promises).toEqual([]); expect(ipc.markReady).not.toHaveBeenCalled();
  });
  it('resumes a saved promise and blocks review while its form is unfinished', async () => {
    staged.promises = [promiseDetail()];
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'noReview', activeBundleId: null, pendingStageId: 'stage', reason: null, canStage: true });
    await render(); await click('Resume saved review');
    await waitFor(() => expect(button('Mark this version reviewed')?.disabled).toBe(false));
    await click('Promises (1)'); await click('Edit promise');
    expect(button('Mark this version reviewed').disabled).toBe(true); expect(button('Possessions (0)').disabled).toBe(true);
    await click('Cancel'); expect(button('Mark this version reviewed').disabled).toBe(false); expect(ipc.stageAuthorReview).not.toHaveBeenCalled();
  });
  it('keeps old promise evidence for explicit removal before restaging changed prose', async () => {
    const saved = promiseDetail();
    vi.mocked(ipc.chapterReviewStatus).mockResolvedValue({ documentId: 'chapter', title: record.title, head: record.head, state: 'changedProse', activeBundleId: 'bundle', pendingStageId: null, reason: 'Writing changed.', canStage: true });
    vi.mocked(ipc.readReviewedRecordSet).mockResolvedValue({ bundleId: 'bundle', projectId: access.projectId, operationNamespace: access.operationNamespace, target: { ...record.head, version: '0' }, revision: staged.revision, records: [], promises: [saved], current: false });
    await render(); await click('Promises (1)'); await click('Remove promise'); await click('Review saved chapter');
    expect(vi.mocked(ipc.stageAuthorReview).mock.calls[0][0].promises).toEqual([]); expect(session.body).toEqual(record.body);
  });
});
