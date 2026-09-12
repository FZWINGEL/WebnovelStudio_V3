// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HistoryPanel } from '../editor/HistoryPanel';
import * as ipc from '../ipc/history';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import type { ProjectAccess, Revision } from '../ipc/projects';

vi.mock('../ipc/history', () => ({ listDocumentHistory: vi.fn(), readDocumentRevision: vi.fn() }));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body = (text: string): WnsDocument => ({ schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text }] }] } });
const current = body('Current writing');
let host: HTMLDivElement; let root: Root; let revisions: Revision[];
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: Error) => void; return { promise: new Promise<T>((accept, fail) => { resolve = accept; reject = fail; }), resolve: (value: T) => resolve(value), reject: (reason: Error) => reject(reason) }; }
function summary(revision: Revision): ipc.RevisionSummary { return { id: revision.id, head: revision.head, reason: revision.reason, createdAt: '2026-09-05T14:00:00Z' }; }
async function render(props: Partial<React.ComponentProps<typeof HistoryPanel>> = {}) {
  await act(async () => root.render(<HistoryPanel access={access} documentId="chapter" body={current} visible disabled={false} onClose={() => {}} onRestore={async () => {}} {...props} />));
}
async function choose(id: string) {
  await act(async () => { const select = host.querySelector('select')!; select.value = id; select.dispatchEvent(new Event('change', { bubbles: true })); });
}
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === label)!; }
async function click(label: string) { await act(async () => button(label).click()); }
async function waitFor(assertion: () => void) { await vi.waitFor(async () => { await act(async () => {}); assertion(); }); }
beforeEach(async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks();
  revisions = await Promise.all(['oldest', 'recent'].map(async (id, index) => {
    const value = body(id === 'oldest' ? 'Earlier promise' : 'Recent promise');
    return { id, head: { documentId: 'chapter', version: String(index + 1), bodyHash: await bodyHash(canonicalJson(value)) }, body: value, reason: 'manual', parentId: null };
  }));
  vi.mocked(ipc.listDocumentHistory).mockResolvedValue({ items: revisions.map(summary).reverse(), nextBeforeVersion: null });
  vi.mocked(ipc.readDocumentRevision).mockImplementation(async (_access, _document, id) => revisions.find(item => item.id === id)!);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('saved versions comparison', () => {
  it('lists metadata first and reads only the selected version, with an explicit restore action', async () => {
    const restore = vi.fn(async () => {}); await render({ onRestore: restore });
    expect(ipc.listDocumentHistory).toHaveBeenCalledOnce(); expect(ipc.readDocumentRevision).not.toHaveBeenCalled();
    await choose('oldest'); await waitFor(() => expect(host.textContent).toContain('Earlier promise'));
    expect(ipc.readDocumentRevision).toHaveBeenCalledExactlyOnceWith(access, 'chapter', 'oldest'); expect(restore).not.toHaveBeenCalled();
    expect(host.textContent).toContain('The writing you have now will remain in Saved versions.');
    await click('Restore this version'); expect(restore).toHaveBeenCalledExactlyOnceWith(revisions[0]); expect(host.textContent).toContain('Restored.');
  });
  it('discards an older selection response that arrives after a newer choice', async () => {
    const old = deferred<Revision>(); vi.mocked(ipc.readDocumentRevision).mockImplementation(async (_access, _document, id) => id === 'oldest' ? old.promise : revisions[1]);
    await render(); await choose('oldest'); await choose('recent'); await waitFor(() => expect(host.textContent).toContain('Recent promise'));
    await act(async () => old.resolve(revisions[0])); expect(host.textContent).toContain('Recent promise'); expect(host.textContent).not.toContain('Earlier promise');
  });
  it('refuses a mismatched or corrupt read and lets the author retry the same selection', async () => {
    vi.mocked(ipc.readDocumentRevision).mockResolvedValueOnce({ ...revisions[0], body: body('unverified prose') });
    await render(); await choose('oldest'); await waitFor(() => expect(host.querySelector('[role=alert]')).not.toBeNull());
    expect(host.querySelector('.saved-prose')).toBeNull(); expect(host.textContent).not.toContain('unverified prose');
    await click('Try reading again'); await waitFor(() => expect(host.textContent).toContain('Earlier promise'));
  });
  it('drops reads from a prior lease and keeps a matching body non-restorable', async () => {
    const old = deferred<Revision>(); vi.mocked(ipc.readDocumentRevision).mockReturnValueOnce(old.promise);
    await render(); await choose('oldest'); await render({ access: { ...access, writerLease: 'new-lease' }, body: revisions[1].body });
    await act(async () => old.resolve(revisions[0])); expect(host.querySelector('.saved-prose')).toBeNull();
    await choose('recent'); await waitFor(() => expect(host.textContent).toContain('Recent promise'));
    expect(button('Restore this version').disabled).toBe(true); expect(host.textContent).toContain('matches this version');
  });
  it('loads older metadata with its cursor without reading any manuscript body', async () => {
    vi.mocked(ipc.listDocumentHistory).mockResolvedValueOnce({ items: [summary(revisions[1])], nextBeforeVersion: '2' });
    await render(); await click('Load older versions');
    expect(ipc.listDocumentHistory).toHaveBeenLastCalledWith(access, 'chapter', '2'); expect(host.querySelectorAll('option')).toHaveLength(3);
    expect(ipc.readDocumentRevision).not.toHaveBeenCalled();
  });
  it('keeps restore progress visible and blocks other history actions until it settles', async () => {
    const pending = deferred<void>(); const onClose = vi.fn();
    await render({ onClose, onRestore: () => pending.promise }); await choose('oldest');
    await waitFor(() => expect(host.textContent).toContain('Earlier promise'));
    await click('Restore this version');
    expect(button('Back to writing').disabled).toBe(true); expect(button('Refresh').disabled).toBe(true);
    expect(host.querySelector('select')!.disabled).toBe(true); await click('Back to writing'); expect(onClose).not.toHaveBeenCalled();
    await act(async () => pending.resolve()); expect(button('Back to writing').disabled).toBe(false);
    expect(host.textContent).toContain('Restored.');
  });
  it('drops late failures and restore completions after the panel is hidden or rebound', async () => {
    const pendingRead = deferred<Revision>(); vi.mocked(ipc.readDocumentRevision).mockReturnValueOnce(pendingRead.promise);
    await render(); await choose('oldest'); await render({ visible: false });
    await act(async () => pendingRead.reject(new Error('Old read failed'))); await render();
    expect(host.textContent).not.toContain('Old read failed');
    const pendingRestore = deferred<void>(); await render({ onRestore: () => pendingRestore.promise }); await choose('oldest');
    await waitFor(() => expect(host.textContent).toContain('Earlier promise')); await click('Restore this version');
    await render({ visible: false }); await render();
    await act(async () => pendingRestore.resolve()); expect(host.textContent).not.toContain('Restored.');
  });
  it('disables history reads while the session requires reconciliation', async () => {
    vi.mocked(ipc.listDocumentHistory).mockResolvedValue({ items: revisions.map(summary), nextBeforeVersion: '1' });
    await render(); await render({ disabled: true });
    expect(host.querySelector('select')!.disabled).toBe(true);
    expect(button('Refresh').disabled).toBe(true); expect(button('Load older versions').disabled).toBe(true);
    await click('Refresh'); await click('Load older versions'); expect(ipc.listDocumentHistory).toHaveBeenCalledOnce();
    expect(ipc.readDocumentRevision).not.toHaveBeenCalled();
  });
});
