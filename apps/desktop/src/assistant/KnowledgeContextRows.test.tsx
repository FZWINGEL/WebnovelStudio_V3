// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReviewedKnowledgeSet, SourceDescriptor } from '../ipc/context';
import type { KnowledgeRecord } from '../ipc/reviews';
import { KnowledgeContextRows, permittedKnowledgeRows } from './KnowledgeContextRows';

const source = { handle: 'chapter', displayName: 'Chapter 1' } as SourceDescriptor;
const evidence = { blockId: 'p', fromUtf16: 0, toUtf16: 5, quote: 'Mei knows.', quoteHash: 'a'.repeat(64) };
const knowledge = (id: string, audience: 'reader' | 'authorRoom'): KnowledgeRecord => ({ id, character: { id: 'mei', label: 'Mei' }, topic: { id: 'gate', label: 'The gate' }, attitude: 'knows', statement: `${id} statement`, timing: 'unknown', audience, evidence });
const set: ReviewedKnowledgeSet = { projectId: 'p', operationNamespace: 'n', bundleId: 'b', recordsHash: 'h', sourceHandle: source.handle, source: { projectId: 'p', documentId: 'd', revisionId: 'r', bodyHash: 'x' }, records: [knowledge('reader', 'reader'), knowledge('private', 'authorRoom')] };
let host: HTMLUListElement; let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('ul'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('reviewed knowledge context rows', () => {
  it('filters private observations before rendering restricted context and opens exact sources', async () => {
    const onRead = vi.fn(); const onHistory = vi.fn();
    const restricted = permittedKnowledgeRows([set], [], true, false); expect(restricted.map(item => item.record.id)).toEqual(['reader']);
    await act(async () => root.render(<KnowledgeContextRows rows={restricted} sources={[source]} used={false} busy={false} onRead={onRead} onHistory={onHistory} />));
    expect(host.textContent).toContain('reader statement'); expect(host.textContent).not.toContain('private statement');
    await act(async () => (host.querySelector('button.text-button') as HTMLButtonElement).click()); expect(onRead).toHaveBeenCalledWith('chapter');
    await act(async () => host.querySelector('button[aria-label="Find knowledge history for Mei about The gate"]')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))); expect(onHistory).toHaveBeenCalledWith('mei', 'gate');
  });
  it('separates delivered rows from available rows by record id', () => {
    const coverage = [{ sourceHandle: 'chapter', bundleId: 'b', recordsHash: 'h', projectionHash: 'p', completeRecordSet: false, recordIds: ['reader'] }];
    expect(permittedKnowledgeRows([set], coverage, false, true).map(item => item.record.id)).toEqual(['reader']);
    expect(permittedKnowledgeRows([set], coverage, false, false).map(item => item.record.id)).toEqual(['reader', 'private']);
  });
});
