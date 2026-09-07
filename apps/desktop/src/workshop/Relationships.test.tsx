// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentRecord, Head } from '../ipc/projects';
import type { WorkshopRelationship, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { Relationships } from './Relationships';

let host: HTMLDivElement;
let root: Root;

const makeHead = (documentId: string, version = '1', bodyHash = `hash-${documentId}-${version}`): Head => ({ documentId, version, bodyHash });
const makeDocument = (documentId: string, title: string, kind: 'character' | 'world', version = '1'): DocumentRecord => ({
  head: makeHead(documentId, version), title, kind, metadataVersion: '1',
  body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, lastCheckpointId: null,
});
const makeRelationship = (from: DocumentRecord, to: DocumentRecord, overrides: Partial<WorkshopRelationship> = {}): WorkshopRelationship => ({
  id: 'relationship-1', fromDocumentId: from.head.documentId, toDocumentId: to.head.documentId, type: 'trusts',
  description: `${from.title} trusts ${to.title}.`, uncertainty: 'The reason remains uncertain.', status: 'chosen',
  sourceHeads: [from.head, to.head], ...overrides,
});
const makeSession = (overrides: Partial<WorkshopSession> = {}): WorkshopSession => ({
  id: 'session-1', title: 'People exploration', lens: 'people', parentSessionId: null, branchKind: 'working',
  brief: '', direction: '', stillOpen: '', focusQuestion: 'What do they want?', focusReason: 'Pressure reveals choices.',
  focusDocumentId: 'person-a', anchorDocumentId: 'workshop-session-1', depth: 'sketch', outsideDirection: false,
  includedDocumentIds: [], workingText: '', workingTitle: '', workingGeneration: '0', selectedDetails: [], choices: [], questions: [],
  composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null, ...overrides,
});
const makeState = (relationships: WorkshopRelationship[]): WorkshopState => ({
  schemaVersion: 1, currentSessionId: 'session-1', sessions: [makeSession()], preferences: [], decisions: [], relationships, impacts: [], presets: [],
});

function render(overrides: Partial<Parameters<typeof Relationships>[0]> = {}) {
  const personA = makeDocument('person-a', 'Mira', 'character');
  const personB = makeDocument('person-b', 'The archive', 'world');
  const props: Parameters<typeof Relationships>[0] = {
    state: makeState([makeRelationship(personA, personB)]), documents: [personA, personB], session: makeSession(),
    onChange: vi.fn(), onOpenDocument: vi.fn(), ...overrides,
  };
  act(() => root.render(<Relationships {...props} />));
  return props;
}

function button(label: string): HTMLButtonElement | undefined {
  return [...host.querySelectorAll('button')].find(item => item.textContent === label) as HTMLButtonElement | undefined;
}

async function click(label: string) {
  const target = button(label);
  if (!target) throw new Error(`Missing button: ${label}`);
  await act(async () => target.click());
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('Relationships', () => {
  it('prepares a fresh saved directional relationship without persistence', async () => {
    const personA = makeDocument('person-a', 'Mira', 'character');
    const personB = makeDocument('person-b', 'The archive', 'world');
    const relationship = makeRelationship(personA, personB);
    const onExploreRelationship = vi.fn();
    const onChange = vi.fn();
    render({ documents: [personA, personB], state: makeState([relationship]), onExploreRelationship, onChange });

    await click('Explore this relationship');

    expect(onExploreRelationship).toHaveBeenCalledWith(relationship);
    expect(onExploreRelationship.mock.calls[0][0].uncertainty).toBe('The reason remains uncertain.');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('refuses exploration when an endpoint is missing or its source is stale', async () => {
    const personA = makeDocument('person-a', 'Mira', 'character');
    const personB = makeDocument('person-b', 'The archive', 'world');
    const onExploreRelationship = vi.fn();

    render({ documents: [personA], state: makeState([makeRelationship(personA, personB)]), onExploreRelationship });
    let explore = button('Explore this relationship');
    expect(explore?.disabled).toBe(true);
    expect(host.textContent).toContain('Review this relationship first: one or both participants are no longer available.');
    await act(async () => explore?.click());
    expect(onExploreRelationship).not.toHaveBeenCalled();

    const currentB = makeDocument('person-b', 'The archive', 'world', '2');
    render({ documents: [personA, currentB], state: makeState([makeRelationship(personA, currentB, { sourceHeads: [personA.head, makeHead('person-b', '1')] })]), onExploreRelationship });
    explore = button('Explore this relationship');
    expect(explore?.disabled).toBe(true);
    expect(host.textContent).toContain('Review this relationship first: a participant source changed.');
    await act(async () => explore?.click());
    expect(onExploreRelationship).not.toHaveBeenCalled();
  });

  it('does not offer exploration for archived relationships', () => {
    const personA = makeDocument('person-a', 'Mira', 'character');
    const personB = makeDocument('person-b', 'The archive', 'world');
    const archived = makeRelationship(personA, personB, { status: 'archived' });
    render({ documents: [personA, personB], state: makeState([archived]), onExploreRelationship: vi.fn() });
    expect(button('Explore this relationship')).toBeUndefined();
  });

  it('resets the filter, editor, and error when the session or focused person changes', async () => {
    const personA = makeDocument('person-a', 'Mira', 'character');
    const personB = makeDocument('person-b', 'The archive', 'world');
    const personC = makeDocument('person-c', 'Jon', 'character');
    const first = makeRelationship(personA, personB, { id: 'relationship-ab' });
    const second = makeRelationship(personB, personC, { id: 'relationship-bc' });
    const state = makeState([first, second]);
    const onChange = vi.fn();
    render({ documents: [personA, personB, personC], state, session: makeSession({ focusDocumentId: personA.head.documentId }), onChange });
    const filter = host.querySelector('select') as HTMLSelectElement;
    expect(filter.value).toBe('person-a');
    expect(host.textContent).toContain('Mira trusts The archive.');
    expect(host.textContent).not.toContain('The archive trusts Jon.');

    await act(async () => {
      filter.value = 'person-b';
      filter.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(host.textContent).toContain('The archive trusts Jon.');
    const secondArticle = [...host.querySelectorAll('article')].find(item => item.textContent?.includes('The archive trusts Jon.'));
    const reviewButton = secondArticle?.querySelector('button') as HTMLButtonElement | null;
    await act(async () => {
      reviewButton?.click();
    });
    const form = host.querySelector('form') as HTMLFormElement;
    const formSelects = form.querySelectorAll('select');
    await act(async () => {
      formSelects[1].value = 'person-b';
      formSelects[1].dispatchEvent(new Event('change', { bubbles: true }));
      (form.querySelector('button.primary-button') as HTMLButtonElement | null)?.click();
    });
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('Choose two different saved participants');

    const nextSession = makeSession({ id: 'session-2', focusDocumentId: personC.head.documentId });
    await act(async () => root.render(<Relationships state={state} documents={[personA, personB, personC]} session={nextSession} onChange={onChange} onOpenDocument={vi.fn()} />));
    expect((host.querySelector('select') as HTMLSelectElement).value).toBe('person-c');
    expect(host.querySelector('form')).toBeNull();
    expect(host.querySelector('[role="alert"]')).toBeNull();
  });

  it('disables relationship actions while the parent surface is locked', () => {
    render({ disabled: true, onExploreRelationship: vi.fn() });
    expect(button('Connect people or groups')?.disabled).toBe(true);
    expect(button('Explore this relationship')?.disabled).toBe(true);
    expect((host.querySelector('select') as HTMLSelectElement).disabled).toBe(true);
  });
});
