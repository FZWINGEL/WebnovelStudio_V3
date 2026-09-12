// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Scope } from '../editor/selection';
import type { KnowledgeRecord, ReviewedEntityChoice } from '../ipc/reviews';
import { ReviewKnowledgeEditor } from '../editor/ReviewKnowledgeEditor';
import { evidenceQuoteHash } from './reviewEvidence';

const quote = 'Mei knows the gate is watched.';
const scope = (end = 'p'): Scope => ({ generation: 1, from: 1, to: 1 + quote.length, quote, source: {} as Scope['source'], start: { blockId: 'p', utf16Offset: 0 }, end: { blockId: end, utf16Offset: quote.length }, inlineOnly: end === 'p', replacementAllowed: true, uniformMarks: [], formattingNote: '' });
const record = (id = 'knowledge'): KnowledgeRecord => ({ id: 'record', character: { id: 'mei', label: 'Mei' }, topic: { id, label: 'The watched gate' }, attitude: 'knows', statement: 'Mei knows the gate is watched.', timing: 'atPassage', audience: 'authorRoom', evidence: { blockId: 'p', fromUtf16: 0, toUtf16: quote.length, quote, quoteHash: 'a'.repeat(64) } });
const choice = (id: string, label: string, title = 'Chapter 1'): ReviewedEntityChoice => ({ entity: { id, label }, labelVariants: [label], firstDocumentId: id, firstDocumentTitle: title });
let host: HTMLDivElement; let root: Root;
const button = (name: string) => [...host.querySelectorAll('button')].find(item => item.textContent === name)!;
const field = (name: string) => [...host.querySelectorAll('label')].find(item => item.firstChild?.textContent === name)!.querySelector('input,select,textarea') as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
async function change(name: string, value: string) { await act(async () => { const item = field(name); const proto = item instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : item instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(item, value); item.dispatchEvent(new Event(item instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true })); }); }
const click = async (name: string) => act(async () => button(name).click());
async function render(records: KnowledgeRecord[] = [], projectCharacters: ReviewedEntityChoice[] = [], projectTopics: ReviewedEntityChoice[] = [], capture = () => scope()) {
  const onChange = vi.fn();
  await act(async () => root.render(<ReviewKnowledgeEditor records={records} projectCharacters={projectCharacters} projectTopics={projectTopics} captureSelection={capture} onChange={onChange} />)); return onChange;
}
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); });

describe('reviewed character knowledge editor', () => {
  it('creates an opaque character and topic observation with an exact source', async () => {
    const changed = await render(); await click('Add knowledge observation');
    await change('Character name', 'Mei'); await change('Topic name', 'The watched gate'); await change('Recorded attitude', 'knows'); await change('What this character knows or believes', 'Mei knows the gate is watched.'); await click('Keep knowledge');
    await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce());
    expect(changed.mock.calls[0][0][0]).toMatchObject({ character: { label: 'Mei' }, topic: { label: 'The watched gate' }, attitude: 'knows', statement: 'Mei knows the gate is watched.', audience: 'authorRoom', evidence: { quote, quoteHash: await evidenceQuoteHash(quote) } });
    expect(changed.mock.calls[0][0][0].character.id).toMatch(/^[0-9a-f-]{36}$/u);
  });
  it('reuses independent project character and topic identities', async () => {
    const changed = await render([], [choice('mei', 'Mei')], [choice('gate', 'The gate')]); await click('Add knowledge observation');
    await change('Character', 'mei'); await change('Topic', 'gate'); await change('What this character knows or believes', 'Mei suspects the gate is watched.'); await change('Recorded attitude', 'suspects'); await click('Keep knowledge');
    await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce()); expect(changed.mock.calls[0][0][0]).toMatchObject({ character: { id: 'mei' }, topic: { id: 'gate' }, attitude: 'suspects' });
  });
  it('keeps the six attitudes explicit and rejects an overlong statement', async () => {
    await render(); await click('Add knowledge observation');
    expect([...host.querySelectorAll('option')].map(option => option.textContent)).toEqual(expect.arrayContaining(['Knows', 'Believes', 'Suspects', 'Rejects', 'Explicitly unaware', 'Unclear']));
    await change('Character name', 'Mei'); await change('Topic name', 'The gate'); await change('What this character knows or believes', 'x'.repeat(1025)); await click('Keep knowledge');
    expect(host.textContent).toContain('at most 1024 UTF-8 bytes');
  });
  it('requires a single paragraph and preserves explicit removal', async () => {
    const changed = await render([record()], [], [], () => scope('other')); await click('Add knowledge observation'); expect(host.textContent).toContain('within one paragraph'); expect(changed).not.toHaveBeenCalled();
    const removed = await render([record()]); await click('Remove knowledge'); expect(removed).toHaveBeenCalledWith([]);
  });
  it('drops a delayed evidence hash when the author changes the form', async () => {
    let finish!: (value: ArrayBuffer) => void;
    vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(resolve => { finish = resolve; }));
    const changed = await render([record()]); await click('Edit knowledge'); await click('Keep knowledge'); await change('Recorded attitude', 'rejects'); await act(async () => finish(new Uint8Array(32).buffer));
    expect(changed).not.toHaveBeenCalled(); expect((field('Recorded attitude') as HTMLSelectElement).value).toBe('rejects');
  });
  it('drops a pending save when the editor becomes unavailable', async () => {
    let finish!: (value: ArrayBuffer) => void;
    vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(resolve => { finish = resolve; }));
    const saved = [record()]; const changed = await render(saved); await click('Edit knowledge'); await click('Keep knowledge');
    await act(async () => root.render(<ReviewKnowledgeEditor records={saved} disabled captureSelection={() => scope()} onChange={changed} />));
    await act(async () => finish(new Uint8Array(32).buffer)); expect(changed).not.toHaveBeenCalled(); expect(button('Keep knowledge').disabled).toBe(true);
  });
});
