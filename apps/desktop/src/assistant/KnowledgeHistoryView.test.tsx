// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReviewedKnowledgeHistoryResult } from '../ipc/context';
import { KnowledgeHistoryView } from './KnowledgeHistoryView';

const result: ReviewedKnowledgeHistoryResult = { snapshotId: 'snapshot', current: true, history: {
  characterId: 'mei', topicId: 'gate', labelVariants: ['Mei'], incomplete: true, uncertainty: ['noEligibleObservations', 'multipleRecordedAttitudes'], observations: [{
    recordId: 'record', sourceHandle: 'chapter', source: { projectId: 'p', documentId: 'd', revisionId: 'r', bodyHash: 'h' }, sourceDisplayName: 'Chapter 1', sourceOrder: 0,
    character: { id: 'mei', label: 'Mei' }, topic: { id: 'gate', label: 'The gate' }, attitude: 'believes', statement: 'Mei believes the gate is watched.', timing: 'unknown', audience: 'reader', evidence: { blockId: 'p', fromUtf16: 0, toUtf16: 10, quote: 'The gate watched her.', quoteHash: 'a'.repeat(64) },
  }],
} };
let host: HTMLDivElement; let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('character knowledge history', () => {
  it('shows uncertainty and opens the exact retained source', async () => {
    const onRead = vi.fn(); await act(async () => root.render(<KnowledgeHistoryView result={result} onRead={onRead} onClose={() => {}} />));
    expect(host.textContent).toContain('does not establish that the character is unaware'); expect(host.textContent).toContain('Different attitudes are recorded');
    await act(async () => (host.querySelector('button.text-button') as HTMLButtonElement).click()); expect(onRead).toHaveBeenCalledWith('chapter');
  });
  it('does not state a current mental state from an empty history', async () => {
    await act(async () => root.render(<KnowledgeHistoryView result={{ ...result, history: { ...result.history, observations: [], uncertainty: ['noEligibleObservations'] } }} onRead={() => {}} onClose={() => {}} />));
    expect(host.textContent).toContain('does not establish that the character is unaware'); expect(host.textContent).not.toContain('currently unaware');
  });
});
