// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DiscussionRun } from '../ipc/discussions';
import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopCandidate, WorkshopDecision, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { NextExplorationContext } from './NextExplorationContext';

let host: HTMLDivElement;
let root: Root;

const body = { schemaVersion: 1 as const, body: { type: 'doc' as const, content: [] } };
const head = (documentId: string, version = '3') => ({ documentId, version, bodyHash: `hash-${documentId}-${version}` });

function session(id: string, overrides: Partial<WorkshopSession> = {}): WorkshopSession {
  return {
    id, title: id, lens: 'overview', parentSessionId: null, branchKind: 'working', brief: '', direction: '', stillOpen: '',
    focusQuestion: '', focusReason: '', focusDocumentId: null, anchorDocumentId: `workshop-${id}`, depth: 'develop', outsideDirection: false,
    includedDocumentIds: [], workingText: '', workingTitle: '', workingGeneration: '1', selectedDetails: [], choices: [], questions: [],
    composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null, ...overrides,
  };
}

function state(sessions: WorkshopSession[], decisions: WorkshopDecision[] = []): WorkshopState {
  return { schemaVersion: 1, currentSessionId: sessions.at(-1)?.id ?? null, sessions, preferences: [], decisions, relationships: [], impacts: [], presets: [] };
}

function run(id: string, overrides: Partial<DiscussionRun> = {}): DiscussionRun {
  return {
    id, threadId: `thread-${id}`, owner: { projectId: 'project', operationNamespace: 'workshop', runId: id }, operationId: `operation-${id}`,
    payloadHash: `payload-${id}`, target: { documentId: `workshop-${id}`, version: '1', bodyHash: `hash-workshop-${id}-1` }, packetId: `packet-${id}`,
    previousRunId: null, status: 'completed', dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null,
    createdAt: '2026-09-07T00:00:00Z', updatedAt: '2026-09-07T00:00:00Z', ...overrides,
  };
}

function candidate(id: string, content = `Saved content for ${id}.`): WorkshopCandidate {
  return { id, title: `Direction ${id}`, content, dimensionValue: 'A dimension', implications: [], assumptions: [], affectedTargets: [], preservedDetails: [], changedDetails: [] };
}

function result(sessionId: string, id: string, candidates: WorkshopCandidate[], overrides: Partial<WorkshopResult> = {}): WorkshopResult {
  return {
    run: run(id), sessionId, workingGeneration: '1', action: 'directions', output: {
      schemaVersion: 'story-workshop-output.v1', requestKind: 'world', question: 'Question', questionReason: 'Reason', dimension: 'Dimension',
      interpretation: { youSaid: 'Said', possibleDirection: 'Possible', stillOpen: 'Open' }, candidates,
    }, validationError: null, stale: false, ...overrides,
  };
}

function decision(id: string, sessionId: string, documentId: string, overrides: Partial<WorkshopDecision> = {}): WorkshopDecision {
  return { id, sessionId, title: id, documentId, revisionId: `${id}-revision`, head: head(documentId, '7'), candidateIds: [], rationale: `Why ${id}`, status: 'chosen', fixed: false, protectedText: [], access: 'authorRoom', supersedesId: null, ...overrides };
}

function render(props: Partial<Parameters<typeof NextExplorationContext>[0]> = {}) {
  const active = session('active', {
    parentSessionId: 'parent', direction: 'Current direction', outsideDirection: true, workingText: 'The current working element', brief: 'Brief fallback',
    composer: 'Composer fallback', selectedDetails: [{ id: 'detail', candidateId: null, text: 'Keep this detail', fixed: true }], focusQuestion: 'What changes next?',
    focusReason: 'Compare consequences.', selectedScope: 'A focused passage', originalNotes: 'Original author note', includedDocumentIds: ['world'],
    choices: [{ candidateId: 'moment-choice', status: 'saved', rationale: 'Keep the rhythm', includeInContext: true }],
  });
  const parent = session('parent', { title: 'Parent exploration', workingGeneration: '1' });
  const unrelated = session('unrelated', { title: 'Unrelated exploration' });
  const documents: DocumentRecord[] = [
    { head: head('world', '9'), title: 'World notes', kind: 'world', metadataVersion: '1', body, lastCheckpointId: null },
    { head: head('other', '2'), title: 'Other notes', kind: 'world', metadataVersion: '1', body: { ...body, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'current' }, content: [{ type: 'text', text: 'CURRENT BODY MUST NOT APPEAR' }] }] } }, lastCheckpointId: null },
  ];
  const base = {
    state: state([parent, active, unrelated], [
      decision('ancestor-choice', 'parent', 'world', { protectedText: ['Stale protection should stay hidden when unprotected.'] }),
      decision('unrelated-choice', 'unrelated', 'other'),
      decision('archived-protection', 'unrelated', 'world', { status: 'archived', fixed: true, protectedText: ['Exact protected phrase'] }),
      decision('unrelated-protection', 'unrelated', 'other', { status: 'archived', fixed: true, protectedText: ['Must stay out'] }),
    ]),
    session: active,
    results: [result('parent', 'moment-run', [candidate('moment-choice', 'The bell rang twice.')], { action: 'moment', stale: true })],
    documents,
  };
  const merged = { ...base, ...props };
  act(() => root.render(<NextExplorationContext {...merged} />));
  return merged;
}

beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('NextExplorationContext', () => {
  it('shows the actual fallback order and planned context without calling an external transport', () => {
    const open = vi.fn();
    render({ onOpenDocument: open });
    expect(host.textContent).toContain('Planned context');
    expect(host.textContent).toContain('The current working element');
    expect(host.textContent).toContain('Explore outside current direction');
    expect(host.textContent).toContain('Yes — alternatives may go beyond the current direction.');
    expect(host.textContent).toContain('Keep this detail');
    expect(host.textContent).toContain('Brief, scope, question, and notes');
    expect(host.textContent).toContain('A focused passage');
    expect(host.textContent).toContain('What changes next?');
    expect(host.textContent).toContain('Original author note');
    expect(open).not.toHaveBeenCalled();
  });

  it('limits chosen material to session lineage and preserves exact historical decision metadata', () => {
    render();
    const chosen = host.querySelector('section[aria-label="Chosen related decisions"]')!;
    expect(chosen.textContent).toContain('ancestor-choice');
    expect(chosen.textContent).not.toContain('unrelated-choice');
    expect(chosen.textContent).toContain('saved version 7');
    expect(chosen.textContent).not.toContain('ancestor-choice-revision');
    expect(chosen.textContent).toContain('Author rationale: Why ancestor-choice');
    expect(chosen.textContent).not.toContain('Protected text');
    expect(chosen.textContent).not.toContain('Stale protection should stay hidden when unprotected.');
    expect(host.textContent).not.toContain('CURRENT BODY MUST NOT APPEAR');
  });

  it('keeps relevant archived protection and labels explicitly included noncanon alternatives with stale status', () => {
    render();
    expect(host.textContent).toContain('Archived protection remains relevant to this preview.');
    expect(host.textContent).toContain('Exact protected phrase');
    expect(host.textContent).not.toContain('Must stay out');
    expect(host.textContent).toContain('The bell rang twice.');
    expect(host.textContent).toContain('Noncanon experiment.');
    expect(host.textContent).toContain('Stale source; review against current work.');
    expect(host.textContent).toContain('Reason saved: Keep the rhythm');
  });

  it('does not lose a saved alternative when its completed result is not loaded', () => {
    const values = render({ results: [] });
    expect(values.session.choices[0].includeInContext).toBe(true);
    expect(host.textContent).toContain('Saved alternative');
    expect(host.textContent).toContain('No completed saved result is loaded for this choice.');
    expect(host.textContent).not.toContain('Rust');
  });
});
