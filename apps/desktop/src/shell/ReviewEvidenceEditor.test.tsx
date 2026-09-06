// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Scope } from '../editor/selection';
import type { PossessionRecord } from '../ipc/reviews';
import { ReviewEvidenceEditor } from './ReviewEvidenceEditor';

const scope = (start = 'p', end = start): Scope => ({ generation: 1, from: 1, to: 8, quote: 'Mei held the key.', source: {} as Scope['source'], start: { blockId: start, utf16Offset: 0 }, end: { blockId: end, utf16Offset: 17 }, inlineOnly: start === end, replacementAllowed: true, uniformMarks: [], formattingNote: '' });
const record = (id: string, label: string): PossessionRecord => ({ id, object: { id, label }, holder: { id: 'mei', label: 'Mei' }, timing: 'unknown', audience: 'authorRoom', evidence: { blockId: 'p', fromUtf16: 0, toUtf16: 17, quote: 'Mei held the key.', quoteHash: 'a'.repeat(64) } });
let host: HTMLDivElement; let root: Root;
const render = async (records: PossessionRecord[], capture = () => scope()) => {
  const onChange = vi.fn();
  await act(async () => root.render(<ReviewEvidenceEditor records={records} captureSelection={capture} onChange={onChange} />));
  return onChange;
};
const button = (name: string) => [...host.querySelectorAll('button')].find(item => item.textContent === name)!;

beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('reviewed evidence editor', () => {
  it('keeps same-label entities distinct and emits exact quoted anchors', async () => {
    const onChange = await render([{ ...record('object-a', 'Key'), holder: null }, { ...record('object-b', 'Key'), holder: null }]);
    await act(async () => { button('Add possession detail').click(); });
    const objectOptions = [...host.querySelectorAll('select')][0].querySelectorAll('option');
    expect([...objectOptions].map(option => option.value)).toEqual(['__new_object__', 'object-a', 'object-b']);
    expect([...objectOptions].map(option => option.textContent)).toEqual(['New object…', 'Key (1)', 'Key (2)']);
    await act(async () => { button('Keep detail').click(); });
    await vi.waitFor(() => expect(onChange).toHaveBeenCalledOnce());
    const next = onChange.mock.calls[0][0] as PossessionRecord[];
    expect(next).toHaveLength(3);
    expect(next[2]).toMatchObject({ object: { id: 'object-a', label: 'Key' }, evidence: { blockId: 'p', fromUtf16: 0, toUtf16: 17, quote: 'Mei held the key.' } });
    expect(next[2].evidence.quoteHash).toMatch(/^[0-9a-f]{64}$/u);
  });
  it('requires one text-bearing block before accepting a new detail', async () => {
    await render([record('object', 'Key')], () => scope('left', 'right'));
    await act(async () => { button('Add possession detail').click(); });
    expect(host.textContent).toContain('within one paragraph');
  });
  it('removes a detail from the complete draft without touching its saved stage', async () => {
    const onChange = await render([record('object', 'Key')]);
    await act(async () => { button('Remove').click(); });
    expect(onChange).toHaveBeenCalledWith([]);
  });
  it('allows a blank project to create opaque entities without name-based merging', async () => {
    const onChange = await render([]);
    await act(async () => { button('Add possession detail').click(); });
    await act(async () => {
      const object = host.querySelector('input[aria-label="New object name"]') as HTMLInputElement;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      setter.call(object, 'Key'); object.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => { button('Keep detail').click(); });
    await vi.waitFor(() => expect(onChange).toHaveBeenCalledOnce());
    const next = onChange.mock.calls[0][0] as PossessionRecord[];
    expect(next).toHaveLength(1); expect(next[0].object).toEqual(expect.objectContaining({ label: 'Key' })); expect(next[0].object.id).toMatch(/^[0-9a-f-]{36}$/u);
  });
  it('drops a hash completion after the author edits the open detail', async () => {
    let resolve!: (value: ArrayBuffer) => void;
    const digest = vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(accept => { resolve = accept; }));
    try {
      const onChange = await render([]);
      await act(async () => { button('Add possession detail').click(); });
      const object = host.querySelector('input[aria-label="New object name"]') as HTMLInputElement;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      setter.call(object, 'Key'); object.dispatchEvent(new Event('input', { bubbles: true }));
      await act(async () => { button('Keep detail').click(); });
      const timing = host.querySelector('select[aria-label="Timing"]') as HTMLSelectElement;
      await act(async () => { timing.value = 'earlier'; timing.dispatchEvent(new Event('change', { bubbles: true })); });
      resolve(new Uint8Array(32).buffer);
      await act(async () => { await Promise.resolve(); await Promise.resolve(); });
      expect(onChange).not.toHaveBeenCalled();
    } finally { digest.mockRestore(); }
  });
  it('disables detail controls and drops a pending hash when the parent becomes busy', async () => {
    let resolve!: (value: ArrayBuffer) => void;
    const digest = vi.spyOn(crypto.subtle, 'digest').mockImplementation(() => new Promise<ArrayBuffer>(accept => { resolve = accept; }));
    try {
      const onChange = await render([]);
      await act(async () => { button('Add possession detail').click(); });
      const object = host.querySelector('input[aria-label="New object name"]') as HTMLInputElement;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      setter.call(object, 'Key'); object.dispatchEvent(new Event('input', { bubbles: true }));
      await act(async () => { button('Keep detail').click(); });
      await act(async () => root.render(<ReviewEvidenceEditor records={[]} disabled captureSelection={() => scope()} onChange={onChange} />));
      expect((host.querySelector('select[aria-label="Timing"]') as HTMLSelectElement).disabled).toBe(true);
      expect(button('Keep detail').disabled).toBe(true); expect(button('Cancel').disabled).toBe(false);
      resolve(new Uint8Array(32).buffer);
      await act(async () => { await Promise.resolve(); await Promise.resolve(); });
      expect(onChange).not.toHaveBeenCalled();
    } finally { digest.mockRestore(); }
  });
});
