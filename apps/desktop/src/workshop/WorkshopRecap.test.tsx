// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Head } from '../ipc/projects';
import type { WorkshopDecision, WorkshopQuestion, WorkshopSession } from '../ipc/workshop';
import { WorkshopRecap } from './WorkshopRecap';

const sessionDefaults: WorkshopSession = {
  id: 'session-1', title: 'Neighborhood workshop', lens: 'overview', parentSessionId: null, branchKind: 'working',
  brief: '', direction: '', stillOpen: '', focusQuestion: '', focusReason: '', focusDocumentId: null,
  anchorDocumentId: 'workshop-session-1', depth: 'sketch', outsideDirection: false, includedDocumentIds: [],
  workingText: '', workingTitle: '', workingGeneration: '0', selectedDetails: [], choices: [], questions: [],
  composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null,
};

const question = (overrides: Partial<WorkshopQuestion>): WorkshopQuestion => ({
  id: 'question', text: 'What remains unknown?', reason: 'Leave room for discovery.', status: 'open', unknownTo: 'both', ...overrides,
});

const head = (documentId: string, version: string): Head => ({ documentId, version, bodyHash: `hash-${documentId}-${version}` });

const decision = (overrides: Partial<WorkshopDecision> = {}): WorkshopDecision => ({
  id: 'decision-1', sessionId: 'session-1', title: 'Guild repair rules', documentId: 'document-1', revisionId: 'revision-1',
  head: head('document-1', '3'), candidateIds: [], rationale: 'Keep the guild morally mixed.', status: 'chosen', fixed: false,
  protectedText: [], access: 'authorRoom', supersedesId: null, ...overrides,
});

const makeSession = (overrides: Partial<WorkshopSession> = {}): WorkshopSession => ({ ...sessionDefaults, ...overrides });

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

async function render(session: WorkshopSession, decisions: WorkshopDecision[] = [], saved = true, onOpenDocument = vi.fn()) {
  await act(async () => root.render(<WorkshopRecap session={session} decisions={decisions} saved={saved} onOpenDocument={onOpenDocument} />));
  return onOpenDocument;
}

function valueFor(label: string): string {
  const term = [...host.querySelectorAll('dt')].find(element => element.textContent === label);
  return term?.nextElementSibling?.textContent ?? '';
}

describe('WorkshopRecap', () => {
  it('keeps deferred and mysterious questions visible, omits irrelevant ones, and resumes a reopened focus', async () => {
    const focus = 'Who pays for public repairs?';
    const deferred = question({ id: 'deferred', text: focus, reason: 'Follow the costs later.', status: 'notNow', unknownTo: 'author' });
    const irrelevant = question({ id: 'irrelevant', text: 'Which banner is used?', status: 'notRelevant', unknownTo: 'both' });
    const mystery = question({ id: 'mystery', text: 'What is beneath the old bridge?', reason: 'Keep the answer unresolved.', status: 'keepMysterious', unknownTo: 'reader' });
    const session = makeSession({ workingTitle: 'Guild repairs', focusQuestion: focus, focusReason: 'This should not be repeated.', questions: [deferred, irrelevant, mystery] });

    await render(session);
    expect(valueFor('Next time')).toBe('Resume Guild repairs.');
    expect(valueFor('Next time')).not.toContain('This should not be repeated.');
    expect(valueFor('Still worth exploring')).toContain('Who pays for public repairs? · For later · unknown to you');
    expect(valueFor('Still worth exploring')).toContain('What is beneath the old bridge? · Intentionally mysterious · unknown to the reader');
    expect(valueFor('Still worth exploring')).not.toContain('Which banner is used?');

    const reopened = makeSession({ ...session, questions: [question({ ...deferred, status: 'open' }), irrelevant, mystery] });
    await render(reopened);
    expect(valueFor('Next time')).toContain(`${focus}This should not be repeated.`);
    expect(valueFor('Still worth exploring')).toContain(`${focus} · Open · unknown to you`);
  });

  it('describes an untouched exploration honestly while retaining developed and what-if labels', async () => {
    await render(makeSession({ title: 'A new exploration' }));
    expect(valueFor('We developed')).toBe('No working material developed yet.');

    await render(makeSession({ branchKind: 'whatIf', workingTitle: 'Guild repair rules', workingText: 'The guild protects people and its own status.' }));
    expect(valueFor('We developed')).toBe('Guild repair rules · Separate what-if exploration');
  });

  it('labels the chosen source revision separately while opening current material for the session only', async () => {
    const onOpenDocument = vi.fn();
    const session = makeSession({ workingText: 'A developed working version.' });
    const chosen = decision();
    const otherSessionChoice = decision({ id: 'decision-2', sessionId: 'other-session', title: 'Other session choice', documentId: 'document-2' });

    await render(session, [chosen, otherSessionChoice], true, onOpenDocument);
    expect(valueFor('You chose')).toContain('Open current material: Guild repair rules');
    expect(valueFor('You chose')).toContain('Chosen source revision · version 3');
    expect(valueFor('You chose')).toContain('Why this was chosen: Keep the guild morally mixed.');
    expect(valueFor('You chose')).toContain('Author intention; writing access is separate.');
    expect(valueFor('You chose')).not.toContain('Other session choice');
    expect(valueFor('You chose')).not.toContain('Read exact saved source version');

    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Open current material: Guild repair rules')!.click());
    expect(onOpenDocument).toHaveBeenCalledExactlyOnceWith('document-1');
  });

  it('preserves the saved and pending exploration watermarks', async () => {
    const session = makeSession({ workingText: 'A working version.' });
    await render(session, [], true);
    expect(host.textContent).toContain('Your latest exploration changes are saved.');
    expect(host.textContent).not.toContain('still waiting to be saved');

    await render(session, [], false);
    expect(host.textContent).toContain('Latest exploration changes are still waiting to be saved.');
    expect(host.textContent).not.toContain('Your latest exploration changes are saved.');
  });
});
