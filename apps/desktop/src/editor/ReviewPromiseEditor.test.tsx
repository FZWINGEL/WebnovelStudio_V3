// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Scope } from '../editor/selection';
import type { PromiseRecord, ReviewedEntityChoice } from '../ipc/reviews';
import { ReviewPromiseEditor } from '../editor/ReviewPromiseEditor';
import { evidenceQuoteHash } from './reviewEvidence';

const quote = 'Ren promised the key.';
const scope = (end = 'p'): Scope => ({ generation: 1, from: 1, to: 1 + quote.length, quote, source: {} as Scope['source'], start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: end, utf16Offset: quote.length }, inlineOnly: end === 'p', replacementAllowed: true, uniformMarks: [], formattingNote: '' });
const record = (id = 'promise'): PromiseRecord => ({ id: 'record', promise: { id, label: 'Return the key' }, phase: 'setup', timing: 'atPassage', note: 'Ren makes a promise.', audience: 'authorRoom', evidence: { blockId: 'p', fromUtf16: 0, toUtf16: quote.length, quote, quoteHash: 'a'.repeat(64) } });
let host: HTMLDivElement; let root: Root;
const button = (name: string) => [...host.querySelectorAll('button')].find(item => item.textContent === name)!;
const field = (name: string) => [...host.querySelectorAll('label')].find(item => item.firstChild?.textContent === name)!.querySelector('input,select') as HTMLInputElement | HTMLSelectElement;
async function change(name: string, value: string) { await act(async () => { const item = field(name); if (item.tagName === 'INPUT') Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(item, value); else item.value = value; item.dispatchEvent(new Event(item.tagName === 'INPUT' ? 'input' : 'change', { bubbles: true })); }); }
const click = async (name: string) => act(async () => button(name).click());
async function render(records: PromiseRecord[] = [], projectPromises: ReviewedEntityChoice[] = [], capture = () => scope()) {
  const onChange = vi.fn();
  await act(async () => root.render(<ReviewPromiseEditor records={records} projectPromises={projectPromises} captureSelection={capture} onChange={onChange} />)); return onChange;
}
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); });

describe('promise evidence editor', () => {
  it('creates an opaque promise with an exact source and defaults to author-only', async () => {
    const changed = await render(); await click('Add promise detail');
    await change('Promise name', 'Return the key'); await change('Promise note', 'Ren promises to return it.'); await click('Keep promise');
    await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce());
    expect(changed.mock.calls[0][0][0]).toMatchObject({ promise: { label: 'Return the key' }, phase: 'setup', timing: 'unknown', audience: 'authorRoom', evidence: { quote, fromUtf16: 0, toUtf16: quote.length, quoteHash: await evidenceQuoteHash(quote) } });
    expect(changed.mock.calls[0][0][0].promise.id).toMatch(/^[0-9a-f-]{36}$/u);
  });
  it('reuses an explicitly chosen same-label identity for a recorded payoff', async () => {
    const choices = ['a', 'b'].map(id => ({ entity: { id, label: 'Return the key' }, labelVariants: ['Return the key'], firstDocumentId: id, firstDocumentTitle: `Chapter ${id}` }));
    const changed = await render([], choices); await click('Add promise detail');
    expect((field('Promise') as HTMLSelectElement).value).toBe('__new__');
    expect([...host.querySelectorAll('option')].map(item => item.textContent)).toContain('Return the key · Chapter b');
    await change('Promise', 'b'); await change('What this passage records', 'payoff'); await change('Promise note', 'Ren returns the key.');
    await act(async () => (host.querySelector('input[type="checkbox"]') as HTMLInputElement).click()); await click('Keep promise');
    await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce());
    expect(changed.mock.calls[0][0][0]).toMatchObject({ promise: { id: 'b' }, phase: 'payoff', audience: 'reader' });
  });
  it('requires one paragraph and a short explicit note', async () => {
    const changed = await render([], [], () => scope('other')); await click('Add promise detail'); expect(host.textContent).toContain('within one paragraph'); expect(changed).not.toHaveBeenCalled();
    await render(); await click('Add promise detail'); await change('Promise name', 'Promise'); await click('Keep promise');
    expect(host.textContent).toContain('one short note');
  });
  it('emits explicit removal without changing the saved input', async () => {
    const saved = [record()]; const changed = await render(saved); await click('Remove promise');
    expect(changed).toHaveBeenCalledWith([]); expect(saved).toHaveLength(1);
  });
  it('discards a delayed hash after the author changes the observation', async () => {
    let finish!: (value: ArrayBuffer) => void;
    vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(resolve => { finish = resolve; }));
    const changed = await render([record()]); await click('Edit promise'); await click('Keep promise');
    await change('What this passage records', 'cancelled'); await act(async () => finish(new Uint8Array(32).buffer));
    expect(changed).not.toHaveBeenCalled(); expect((field('What this passage records') as HTMLSelectElement).value).toBe('cancelled');
  });
  it('discards a pending save when review becomes unavailable', async () => {
    let finish!: (value: ArrayBuffer) => void;
    vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(resolve => { finish = resolve; }));
    const saved = [record()]; const changed = await render(saved); await click('Edit promise'); await click('Keep promise');
    await act(async () => root.render(<ReviewPromiseEditor records={saved} disabled captureSelection={() => scope()} onChange={changed} />));
    await act(async () => finish(new Uint8Array(32).buffer)); expect(changed).not.toHaveBeenCalled(); expect(button('Keep promise').disabled).toBe(true);
  });
});
