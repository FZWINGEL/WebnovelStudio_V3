// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LookupPacketInput, SourceDescriptor, SourceRef } from '../ipc/context';
import { LookupContextView } from './LookupContextView';

const source: SourceRef = { projectId: 'project', documentId: 'chapter-1', revisionId: 'revision-1', bodyHash: 'hash-1' };
const descriptor: SourceDescriptor = {
  handle: 'chapter-1', source, displayName: 'Chapter 1 · The Pendant', kind: 'reviewedAuthority', current: true,
  coverage: 'verbatim', disclosure: { readerPosition: null, visibleToCharacters: [], authorOnly: false, futurePrivate: false },
  storyTime: null, dependencies: [],
};
function packet(overrides: Partial<LookupPacketInput> = {}): LookupPacketInput {
  return {
    allowance: { maxAdditionalInvocations: 2, totalInputBytes: '73728', totalOutputBytes: '196608' },
    completedInvocations: 2,
    exchanges: [],
    ...overrides,
  };
}

let host: HTMLDivElement;
let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('LookupContextView', () => {
  it('renders multi-line exact passages and opens their original source', async () => {
    const onRead = vi.fn();
    const lookup = packet({ exchanges: [{
      request: { kind: 'search', id: 'search-1', query: 'the pendant', mode: 'literal', limit: 6 },
      result: { kind: 'search', result: { snapshotId: 'snapshot', hits: [{ passage: { handle: 'chapter-1', source, blockId: 'block-1', blockOrder: 0, text: 'First line of evidence.\nSecond line of evidence.' }, startUtf16: 0, endUtf16: 48 }], sourceMatches: [], searchedSources: 1, hasMore: false, coverage: 'Chapter 1 exact text' } },
    }] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={onRead} />));
    expect(host.textContent).toContain('First line of evidence.\nSecond line of evidence.');
    expect(host.textContent).toContain('literal search');
    const button = [...host.querySelectorAll('button')].find(item => item.textContent === 'Open exact source') as HTMLButtonElement;
    await act(async () => button.click());
    expect(onRead).toHaveBeenCalledWith('chapter-1');
    expect(host.textContent).not.toContain('"kind":"search"');
  });

  it('labels no-match uncertainty, unavailable gaps, and partial reads', async () => {
    const lookup = packet({ exchanges: [
      { request: { kind: 'search', id: 'search-1', query: 'missing vow', mode: 'literal', limit: 6 }, result: { kind: 'search', result: { snapshotId: 'snapshot', hits: [], sourceMatches: [], searchedSources: 1, hasMore: false, coverage: 'Chapter 1 exact text' } } },
      { request: { kind: 'read', id: 'read-1', handle: 'chapter-1', blockIds: ['block-1'] }, result: { kind: 'read', handle: 'chapter-1', source, passages: [{ handle: 'chapter-1', source, blockId: 'block-1', blockOrder: 0, text: 'Only one passage was supplied.' }], complete: false } },
      { request: { kind: 'read', id: 'read-2', handle: 'missing-source' }, result: { kind: 'unavailable', code: 'SourceUnavailable', detail: 'The source is outside the permitted snapshot.' } },
    ] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('This does not establish that the event never happened.');
    expect(host.textContent).toContain('Partial read supplied');
    expect(host.textContent).toContain('Lookup gap · SourceUnavailable');
  });
});
