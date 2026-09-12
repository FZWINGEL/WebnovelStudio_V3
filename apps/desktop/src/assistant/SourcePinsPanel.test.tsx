// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SourcePinsPanel } from './SourcePinsPanel';
import { readSourcePins, saveSourcePins, type SourcePinSet, type SourcePinsView } from '../ipc/sourcePins';
import { DocumentSession } from '../editor/session';
import type { ProjectAccess } from '../ipc/projects';
import { bodyHash, canonicalJson, type WnsDocument } from '../kernel';

vi.mock('../ipc/sourcePins', () => ({ readSourcePins: vi.fn(), saveSourcePins: vi.fn() }));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph' }, content: [{ type: 'text', text: 'Keep the promise.' }] }] } };
const sources = [{ id: 'chapter-7', title: 'The old promise' }, { id: 'note', title: 'Hidden ending' }];
function empty(documentId = 'chapter'): SourcePinsView { return {
  project: { scope: 'project', targetDocumentId: null, version: '0', sourceDocumentIds: [], audience: 'authorRoom' },
  document: { scope: 'document', targetDocumentId: documentId, version: '0', sourceDocumentIds: [], audience: 'authorRoom' },
}; }
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((r, j) => { resolve = r; reject = j; }); return { promise, resolve, reject }; }
async function makeSession(documentId = 'chapter', projectId = 'project') {
  const head = { documentId, version: '0', bodyHash: await bodyHash(canonicalJson(body)) };
  const record = { head, title: 'Chapter', kind: 'chapter', metadataVersion: '0', body, lastCheckpointId: null };
  return new DocumentSession({ ...access, projectId }, record, {
    validate: vi.fn(), save: vi.fn(), checkpoint: vi.fn(),
    reconcile: vi.fn(async () => ({ access: { ...access, projectId, writerLease: 'next-lease' }, document: record, receipts: [] })),
  });
}
let host: HTMLDivElement; let root: Root; let session: DocumentSession; let changed: ReturnType<typeof vi.fn<() => void>>;
async function render(props: Partial<React.ComponentProps<typeof SourcePinsPanel>> = {}) {
  await act(async () => root.render(<SourcePinsPanel session={session} documentId="chapter" sources={sources} adoption={null} restricted={false} disabled={false} onChanged={changed} {...props} />));
}
function button(label: string) { return [...host.querySelectorAll('button')].find(button => button.textContent === label)!; }
async function choose(index: number, value: string) { await act(async () => { const select = host.querySelectorAll('select')[index]; select.value = value; select.dispatchEvent(new Event('change', { bubbles: true })); }); }
async function click(label: string) { await act(async () => button(label).click()); }
beforeEach(async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(readSourcePins).mockReset().mockImplementation(async (_access, documentId) => empty(documentId));
  vi.mocked(saveSourcePins).mockReset().mockImplementation(async request => ({ scope: request.scope, targetDocumentId: request.targetDocumentId,
    version: (BigInt(request.expectedVersion) + 1n).toString(), sourceDocumentIds: request.sourceDocumentIds, audience: 'authorRoom' }));
  session = await makeSession(); changed = vi.fn(); host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('persistent discussion source controls', () => {
  it('requires explicit confirmation, retains scope and named sources, and removes through CAS', async () => {
    await render({ adoption: { documentId: 'chapter-7', nonce: 1 } });
    expect(host.querySelector('details')!.open).toBe(true); expect(host.querySelector('select')!.value).toBe('chapter-7');
    expect(saveSourcePins).not.toHaveBeenCalled();
    await choose(1, 'project'); await click('Keep source');
    expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'project', targetDocumentId: null, sourceDocumentIds: ['chapter-7'], expectedVersion: '0' }));
    expect(host.querySelector('.persistent-source-list')!.textContent).toContain('The old promiseThis project');
    button('Remove').focus();
    await click('Remove'); expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'project', expectedVersion: '1', sourceDocumentIds: [] }));
    expect(document.activeElement).toBe(host.querySelector('select'));
    expect(changed).toHaveBeenCalledTimes(2); expect(session.body).toEqual(body);
  });
  it('keeps an uncertain operation immutable and checks the same ID after reconciliation', async () => {
    const pending = vi.fn();
    vi.mocked(saveSourcePins).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The acknowledgment was lost.' });
    await render({ onPendingChange: pending }); await choose(0, 'note'); await click('Keep source');
    const original = structuredClone(vi.mocked(saveSourcePins).mock.calls[0][0]);
    expect(button('Keep source').disabled).toBe(true); expect(changed).not.toHaveBeenCalled();
    expect(pending).toHaveBeenLastCalledWith(true);
    await render({ onPendingChange: pending, disabled: true });
    expect(button('Check saved sources').disabled).toBe(false);
    vi.mocked(readSourcePins).mockResolvedValueOnce({ ...empty(), document: { ...empty().document, version: '2', sourceDocumentIds: ['chapter-7'] } });
    button('Check saved sources').focus();
    await click('Check saved sources');
    await act(async () => { await vi.waitFor(() => expect(saveSourcePins).toHaveBeenCalledTimes(2)); });
    expect(vi.mocked(saveSourcePins).mock.calls[1][0]).toEqual({ ...original, access: { ...original.access, writerLease: 'next-lease' } });
    expect(changed).toHaveBeenCalledOnce(); expect(session.body).toEqual(body); expect(pending).toHaveBeenLastCalledWith(false);
    expect(host.querySelector('.persistent-source-list')!.textContent).toContain('The old promise');
    expect(host.querySelector('.persistent-source-list')!.textContent).not.toContain('Hidden ending');
    expect(document.activeElement).toBe(host.querySelector('summary'));
    await render({ onPendingChange: pending }); await click('Remove');
    expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ expectedVersion: '2', sourceDocumentIds: [] }));
  });
  it('does not replay a confirmed save when reading the current choices fails', async () => {
    vi.mocked(saveSourcePins).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The acknowledgment was lost.' });
    await render(); await choose(0, 'note'); await click('Keep source');
    vi.mocked(readSourcePins).mockRejectedValueOnce(new Error('Read interrupted.'));
    await click('Check saved sources');
    await act(async () => { await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce()); });
    expect(host.textContent).toContain('Your source changes were saved');
    expect(button('Check saved sources')).toBeUndefined(); expect(changed).toHaveBeenCalledOnce();
    await click('Reload sources'); expect(saveSourcePins).toHaveBeenCalledTimes(2);
  });
  it('does not trust a mismatched acknowledgment or automatically try another mutation', async () => {
    vi.mocked(saveSourcePins).mockResolvedValueOnce({ ...empty().document, version: '1', sourceDocumentIds: ['wrong-source'] });
    await render(); await choose(0, 'note'); await click('Keep source');
    expect(button('Check saved sources')).toBeDefined(); expect(changed).not.toHaveBeenCalled();
    expect(saveSourcePins).toHaveBeenCalledOnce(); expect(host.querySelector('.persistent-source-list')!.textContent).not.toContain('Hidden ending');
  });
  it('releases a definite refusal, keeps the selection, and reloads the changed document scope explicitly', async () => {
    const pending = vi.fn();
    vi.mocked(saveSourcePins).mockRejectedValueOnce({ code: 'SourcePinVersionConflict', detail: 'The source choices changed. Reload sources.' });
    await render({ onPendingChange: pending }); await choose(0, 'note'); await click('Keep source');
    expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'document', targetDocumentId: 'chapter', expectedVersion: '0' }));
    expect(host.querySelector('select')!.value).toBe('note'); expect(pending).toHaveBeenLastCalledWith(false);
    expect(saveSourcePins).toHaveBeenCalledOnce(); expect(changed).not.toHaveBeenCalled();
    vi.mocked(readSourcePins).mockResolvedValueOnce({ ...empty(), document: { ...empty().document, version: '2', sourceDocumentIds: ['chapter-7'] } });
    await click('Reload sources'); expect(host.querySelector('.persistent-source-list')!.textContent).toContain('The old promiseThis document');
    await choose(0, 'note'); await click('Keep source');
    expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ expectedVersion: '2', sourceDocumentIds: ['chapter-7', 'note'] }));
  });
  it('ignores old project reads and writes after switching owners', async () => {
    const initial = deferred<SourcePinsView>(); vi.mocked(readSourcePins).mockReturnValueOnce(initial.promise);
    await render(); const next = await makeSession('next', 'other-project');
    await render({ session: next, documentId: 'next' });
    await act(async () => initial.resolve({ ...empty(), document: { ...empty().document, version: '1', sourceDocumentIds: ['note'] } }));
    expect(host.querySelector('.persistent-source-list')!.textContent).toBe('');
    const save = deferred<SourcePinSet>(); vi.mocked(saveSourcePins).mockReturnValueOnce(save.promise);
    await choose(0, 'note'); await click('Keep source');
    await render({ session, documentId: 'chapter' });
    await act(async () => save.resolve({ ...empty('next').document, version: '1', sourceDocumentIds: ['note'] }));
    expect(changed).not.toHaveBeenCalled(); expect(host.querySelector('.persistent-source-list')!.textContent).toBe('');
  });
  it('shows the discussion-only boundary during edit requests and retains unavailable sources for removal', async () => {
    vi.mocked(readSourcePins).mockResolvedValue({ ...empty(), project: { ...empty().project, version: '3', sourceDocumentIds: ['removed-document'] } });
    await render({ restricted: true });
    expect(host.textContent).toContain('will not be added to this edit request');
    expect(host.querySelector('.persistent-source-list')!.textContent).toContain('Unavailable source');
    await click('Remove'); expect(saveSourcePins).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'project', expectedVersion: '3', sourceDocumentIds: [] }));
  });
});
