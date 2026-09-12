// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../kernel';
import { RecoveryCopy } from '../editor/RecoveryCopy';
import type { RecoveryCopyResult } from '../ipc/recovery';

const { save } = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock('../ipc/recovery', () => ({ saveRecoveryCopy: save }));
let host: HTMLDivElement; let root: Root;
const snapshot = (): WnsDocument => ({ schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text: 'Unsaved prose 🌙', marks: [{ type: 'italic' }] }] }] } });
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; }); return { promise, resolve, reject }; }
async function receipt(body: WnsDocument): Promise<RecoveryCopyResult> { return { path: 'D:/Recovery/chapter.md', snapshotHash: await bodyHash(canonicalJson(body)), sha256: 'file-hash', utf8Bytes: 20 }; }
async function render(capture = () => snapshot()) { await act(async () => root.render(<RecoveryCopy capture={capture} />)); }
async function click() { await act(async () => host.querySelector('button')!.click()); }
async function settle(assertion: () => void) { await vi.waitFor(async () => { await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); }); assertion(); }); }
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); save.mockReset(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it('copies the immutable click-time buffer while later typing continues', async () => {
  let live = snapshot(); const captured = structuredClone(live); const pending = deferred<RecoveryCopyResult | null>(); save.mockReturnValue(pending.promise);
  const capture = vi.fn(() => live); await render(capture); expect(save).not.toHaveBeenCalled();
  await click(); live = snapshot(); live.body.content.push({ type: 'paragraph', attrs: { id: 'later' }, content: [{ type: 'text', text: 'Later typing' }] });
  await settle(() => expect(save).toHaveBeenCalledWith(captured));
  await click(); expect(capture).toHaveBeenCalledOnce(); expect(save).toHaveBeenCalledOnce();
  await act(async () => pending.resolve(await receipt(captured)));
  expect(host.textContent).toContain('Recovery copy saved: D:/Recovery/chapter.md');
  expect(host.textContent).toContain('This does not save the project'); expect(live.body.content).toHaveLength(2);
});

it('keeps the buffer and never retries on cancellation', async () => {
  save.mockResolvedValue(null); const live = snapshot(); await render(() => live); await click();
  await settle(() => expect(host.textContent).toContain('Recovery copy cancelled'));
  expect(save).toHaveBeenCalledOnce(); expect(live).toEqual(snapshot()); expect(host.querySelector('button')!.disabled).toBe(false);
});

it('reports a failed destination without changing or resending the buffer', async () => {
  save.mockRejectedValue({ code: 'PersistenceUnavailable', detail: 'Destination is read-only.' });
  const live = snapshot(); await render(() => live); await click(); await settle(() => expect(host.textContent).toContain('Destination is read-only'));
  expect(host.textContent).toContain('Your text remains in the editor'); expect(live).toEqual(snapshot()); expect(save).toHaveBeenCalledOnce();
});

it('does not claim success for an acknowledgment belonging to other text', async () => {
  save.mockResolvedValue({ ...await receipt(snapshot()), snapshotHash: 'other-buffer' }); await render(); await click();
  await settle(() => expect(host.textContent).toContain('Check the chosen folder'));
  expect(host.textContent).not.toContain('Recovery copy saved:'); expect(save).toHaveBeenCalledOnce();
});

it('does not replay a lost acknowledgment', async () => {
  save.mockRejectedValue(new Error('The native connection was lost.')); await render(); await click();
  await settle(() => expect(host.textContent).toContain('The native connection was lost'));
  expect(host.textContent).toContain('Check the chosen folder');
  expect(save).toHaveBeenCalledOnce();
});
