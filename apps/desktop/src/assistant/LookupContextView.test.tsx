// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { KnowledgeHistory, LookupPacketInput, PromiseHistory, SourceDescriptor, SourceRef } from '../ipc/context';
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
    expect(onRead).toHaveBeenCalledWith('chapter-1', source);
    expect(host.textContent).not.toContain('"kind":"search"');
  });

  it('uses the frozen lookup source projection when the snapshot descriptor is absent', async () => {
    const projectedName = 'Chapter 1 · Frozen lookup title';
    const lookup = packet({
      sourceProjection: { schemaVersion: 'story-lookup-source.v1', sources: [{ handle: 'chapter-1', source, displayName: projectedName }] },
      exchanges: [{
        request: { kind: 'search', id: 'search-projection', query: 'pendant', mode: 'literal', limit: 4 },
        result: { kind: 'search', result: { snapshotId: 'snapshot', hits: [{ passage: { handle: 'chapter-1', source, blockId: 'block-1', blockOrder: 0, text: 'Frozen evidence.' }, startUtf16: 0, endUtf16: 15 }], sourceMatches: [{ handle: 'chapter-1', source, displayName: 'Stale renderer label', kind: 'reviewedAuthority', current: false, coverage: 'verbatim', disclosure: { readerPosition: null, visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] }], searchedSources: 1, hasMore: false, coverage: 'Frozen exact text' } },
      }],
    });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[]} onRead={() => {}} />));
    expect(host.textContent).toContain(projectedName);
    expect(host.textContent).not.toContain('Stale renderer label');
  });

  it('uses the matching snapshot descriptor for legacy lookup packets', async () => {
    const lookup = packet({ exchanges: [{
      request: { kind: 'read', id: 'read-legacy', handle: 'chapter-1' },
      result: { kind: 'read', handle: 'chapter-1', source, passages: [], complete: true },
    }] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('Read 1 · Chapter 1 · The Pendant');
  });

  it('does not relabel a foreign source when a projection identity does not match', async () => {
    const foreignSource: SourceRef = { ...source, revisionId: 'foreign-revision' };
    const lookup = packet({
      sourceProjection: { schemaVersion: 'story-lookup-source.v1', sources: [{ handle: 'chapter-1', source, displayName: 'Wrong frozen title' }] },
      exchanges: [{
        request: { kind: 'search', id: 'search-foreign', query: 'pendant', mode: 'literal', limit: 4 },
        result: { kind: 'search', result: { snapshotId: 'snapshot', hits: [{ passage: { handle: 'chapter-1', source: foreignSource, blockId: 'block-foreign', blockOrder: 0, text: 'Foreign evidence.' }, startUtf16: 0, endUtf16: 17 }], sourceMatches: [], searchedSources: 1, hasMore: false, coverage: 'Foreign exact text' } },
      }],
    });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('chapter-1');
    expect(host.textContent).not.toContain('Wrong frozen title');
    expect(host.textContent).not.toContain('Chapter 1 · The Pendant');
  });

  it('labels no-match uncertainty, unavailable gaps, and partial reads', async () => {
    const lookup = packet({ exchanges: [
      { request: { kind: 'search', id: 'search-1', query: 'missing vow', mode: 'literal', limit: 6 }, result: { kind: 'search', result: { snapshotId: 'snapshot', hits: [], sourceMatches: [], searchedSources: 1, hasMore: false, coverage: 'Chapter 1 exact text' } } },
      { request: { kind: 'read', id: 'read-1', handle: 'chapter-1', blockIds: ['block-1'] }, result: { kind: 'read', handle: 'chapter-1', source, passages: [{ handle: 'chapter-1', source, blockId: 'block-1', blockOrder: 0, text: 'Only one passage was supplied.' }], complete: false } },
      { request: { kind: 'read', id: 'read-2', handle: 'missing-source' }, result: { kind: 'unavailable', code: 'SourceUnavailable', detail: 'The source is outside the permitted snapshot.' } },
    ] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('This does not establish that the event never happened.');
    expect(host.textContent).toContain('Partial read prepared');
    expect(host.textContent).toContain('Lookup gap · SourceUnavailable');
  });

  it('renders reviewed memory identities and paged history with exact source links', async () => {
    const onRead = vi.fn();
    const history: KnowledgeHistory = {
      characterId: 'character-private-id', topicId: 'topic-private-id', labelVariants: ['Mei', 'The Jade Disciple'], incomplete: true,
      uncertainty: ['multipleRecordedAttitudes'], observations: [{
        recordId: 'observation-private-id', sourceHandle: 'chapter-1', source, sourceDisplayName: descriptor.displayName, sourceOrder: 0,
        character: { id: 'character-private-id', label: 'Mei' }, topic: { id: 'topic-private-id', label: 'the pendant' }, attitude: 'believes',
        statement: 'Mei believes the pendant is a warning.', timing: 'atPassage', audience: 'reader',
        evidence: { blockId: 'block-1', fromUtf16: 0, toUtf16: 20, quote: 'The pendant felt like a warning.', quoteHash: 'q'.repeat(64) },
      }],
    };
    const lookup = packet({ reviewedMemory: 'reviewed-memory.v1', exchanges: [
      { request: { kind: 'findEntities', id: 'find-1', entityKind: 'character', query: 'Mei', offset: 0, limit: 1 }, result: { kind: 'findEntities', entityKind: 'character', query: 'Mei', entries: [{ entity: { id: 'character-private-id', label: 'Mei' }, labelVariants: ['Mei'], sourceHandle: 'chapter-1', source }], offset: 0, totalMatches: 2, nextOffset: 1, incomplete: true } },
      { request: { kind: 'knowledgeHistory', id: 'history-1', characterId: 'character-private-id', topicId: 'topic-private-id', offset: 0, limit: 1 }, result: { kind: 'knowledgeHistory', history, offset: 0, totalObservations: 2, nextOffset: 1 } },
    ] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={onRead} />));
    expect(host.textContent).toContain('Reviewed character identity');
    expect(host.textContent).toContain('Mei believes the pendant is a warning.');
    expect(host.textContent).toContain('More matching identities are available from offset 1');
    expect(host.textContent).toContain('More evidence is available from offset 1');
    expect(host.textContent).toContain('Different attitudes are recorded');
    expect(host.textContent).not.toContain('character-private-id');
    expect(host.textContent).not.toContain('observation-private-id');
    const button = [...host.querySelectorAll('button')].find(item => item.textContent === 'Open exact source') as HTMLButtonElement;
    await act(async () => button.click());
    expect(onRead).toHaveBeenCalledWith('chapter-1', source);
  });

  it('distinguishes prepared and unconfirmed lookup evidence from delivered evidence', async () => {
    const lookup = packet({ exchanges: [{
      request: { kind: 'findEntities', id: 'find-1', entityKind: 'object', query: 'pendant', limit: 1 },
      result: { kind: 'findEntities', entityKind: 'object', query: 'pendant', entries: [], offset: 0, totalMatches: 0, nextOffset: null, incomplete: true },
    }] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} delivery="unconfirmed" onRead={() => {}} />));
    expect(host.textContent).toContain('Delivery not confirmed');
    expect(host.textContent).toContain('prepared, but delivery is not confirmed');
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} delivery="prepared" onRead={() => {}} />));
    expect(host.textContent).toContain('Prepared for model');
    expect(host.textContent).toContain('No reviewed objects matched this query');
    const unavailable = packet({ reviewedMemory: 'reviewed-memory.v1', exchanges: [{
      request: { kind: 'knowledgeHistory', id: 'history-private', characterId: 'private-character-id', limit: 1 },
      result: { kind: 'unavailable', code: 'MemoryUnavailable', detail: 'private-character-id is outside the permitted snapshot' },
    }] });
    await act(async () => root.render(<LookupContextView lookup={unavailable} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('Lookup gap · MemoryUnavailable');
    expect(host.textContent).not.toContain('private-character-id');
  });

  it('keeps promise payoff metadata separate from page observations and resolution claims', async () => {
    const history: PromiseHistory = {
      promiseId: 'promise-id', labelVariants: ['Return the key'], incomplete: true, hasRecordedPayoff: true,
      uncertainty: [], observations: [{
        recordId: 'setup-id', sourceHandle: 'chapter-1', source, sourceDisplayName: descriptor.displayName, sourceOrder: 0,
        promise: { id: 'promise-id', label: 'Return the key' }, phase: 'setup', timing: 'atPassage', note: 'The promise is made.', audience: 'reader',
        evidence: { blockId: 'block-1', fromUtf16: 0, toUtf16: 8, quote: 'She promised.', quoteHash: 'q'.repeat(64) },
      }],
    };
    const lookup = packet({ exchanges: [{
      request: { kind: 'promiseHistory', id: 'promise-1', promiseId: 'promise-id', offset: 20, limit: 1 },
      result: { kind: 'promiseHistory', history, offset: 20, totalObservations: 21, nextOffset: null },
    }] });
    await act(async () => root.render(<LookupContextView lookup={lookup} sources={[descriptor]} onRead={() => {}} />));
    expect(host.textContent).toContain('Prepared for model');
    expect(host.textContent).toContain('At least one eligible observation records a payoff');
    expect(host.textContent).toContain('This page may omit that payoff');
    expect(host.textContent).toContain('does not prove that the promise is resolved');
  });
});
