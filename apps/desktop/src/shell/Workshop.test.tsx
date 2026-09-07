// @vitest-environment jsdom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CompiledPacket } from '../ipc/context';
import type { DiscussionRun, DiscussionStart } from '../ipc/discussions';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import type {
  WorkshopCandidate, WorkshopPreference, WorkshopRelationship, WorkshopResult, WorkshopSession, WorkshopSnapshot,
  WorkshopState, WorkshopView,
} from '../ipc/workshop';
import { Workshop, type WorkshopHandle } from './Workshop';
import { LENSES, NOTES_ORGANIZATION_BRIEF, NOTES_ORGANIZATION_SCOPE, ORGANIZE_NOTES_INSTRUCTION, WORLD_QUESTIONS } from '../workshop/catalog';

const mocks = vi.hoisted(() => ({
  readWorkshop: vi.fn(),
  saveWorkshop: vi.fn(),
  workshopHistory: vi.fn(),
  startWorkshop: vi.fn(),
  previewWorkshopAdoption: vi.fn(),
  adoptWorkshop: vi.fn(),
  readDocument: vi.fn(),
  readDocumentAliases: vi.fn(),
  setDocumentAliases: vi.fn(),
  retryDiscussionSave: vi.fn(),
  stopDiscussion: vi.fn(),
  providers: {
    state: null as any,
    busy: false,
    error: '',
    refresh: vi.fn(),
    checkConnection: vi.fn(),
    checkClaudeConnection: vi.fn(),
    save: vi.fn(),
    saveStoryMemory: vi.fn(),
  },
}));

vi.mock('../ipc/workshop', () => ({
  readWorkshop: mocks.readWorkshop,
  saveWorkshop: mocks.saveWorkshop,
  workshopHistory: mocks.workshopHistory,
  startWorkshop: mocks.startWorkshop,
  previewWorkshopAdoption: mocks.previewWorkshopAdoption,
  adoptWorkshop: mocks.adoptWorkshop,
}));
vi.mock('../ipc/projects', () => ({ readDocument: mocks.readDocument }));
vi.mock('../ipc/context', async importOriginal => ({ ...await importOriginal<typeof import('../ipc/context')>(), readDocumentAliases: mocks.readDocumentAliases, setDocumentAliases: mocks.setDocumentAliases }));
vi.mock('../ipc/discussions', () => ({ retryDiscussionSave: mocks.retryDiscussionSave, stopDiscussion: mocks.stopDiscussion }));
vi.mock('../providers/ProviderContext', () => ({ useProviders: () => mocks.providers }));
vi.mock('../assistant/ContextInspector', () => ({ ContextInspector: () => null }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'workshop', session: 'session', writerLease: 'lease' };
const body = { schemaVersion: 1 as const, body: { type: 'doc' as const, content: [] } };

function providerState(ready = true) {
  return {
    settings: { revision: '0', active: { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null }, favorites: [] },
    catalog: { models: [{ key: { providerId: 'mock', modelId: 'mock-story-context' }, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready, statusDetail: ready ? 'Ready' : 'Connect a model in Settings.' }] },
    dispatch: { kind: 'localMock' as const, detail: 'Deterministic local test provider.' },
    codexConnection: { ready: false, detail: 'Not connected.' },
    storyMemory: { revision: '0', providerId: 'mock', providerLabel: 'Local', modelId: 'mock-story-context', reasoning: null, serviceTier: null, ready: true, detail: 'Local test memory.' },
  };
}

function session(overrides: Partial<WorkshopSession> = {}): WorkshopSession {
  return {
    id: 'session-1', title: 'A new exploration', lens: 'overview', parentSessionId: null, branchKind: 'working',
    brief: '', direction: '', stillOpen: '', focusQuestion: 'What are you excited about?', focusReason: 'Start with an attraction.',
    focusDocumentId: null, anchorDocumentId: 'workshop-session-1', depth: 'sketch', outsideDirection: false,
    includedDocumentIds: [], workingText: '', workingTitle: '', workingGeneration: '0', selectedDetails: [], choices: [], questions: [],
    composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null, ...overrides,
  };
}

function state(overrides: Partial<WorkshopState> = {}): WorkshopState {
  return { schemaVersion: 1, currentSessionId: 'session-1', sessions: [session()], preferences: [], decisions: [], relationships: [], impacts: [], presets: [], ...overrides };
}

function view(overrides: Partial<WorkshopView> = {}): WorkshopView {
  return { version: '1', state: state(), results: [], ...overrides };
}

function run(overrides: Partial<DiscussionRun> = {}): DiscussionRun {
  return {
    id: 'run-1', threadId: 'thread-1', owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, runId: 'run-1' },
    operationId: 'operation-1', payloadHash: 'payload-hash', target: { documentId: 'workshop-session-1', version: '1', bodyHash: 'body-hash' },
    packetId: 'packet-1', previousRunId: null, status: 'completed', dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null,
    createdAt: '2026-09-07T00:00:00Z', updatedAt: '2026-09-07T00:00:00Z', ...overrides,
  };
}

const packet: CompiledPacket = {
  messages: [{ role: 'user', content: 'Explore this idea.' }],
  options: { modelId: 'mock-story-context', maxOutputTokens: '1000', tokenAccountingMethod: 'mock' },
  receipt: {
    packetId: 'packet-1', sessionId: 'session-1', snapshotId: 'snapshot-1', invocationOrdinal: '0', sourceHandles: [], coverage: [],
    omissions: [], inputHash: 'input-hash', inputTokens: '10', tokenAccountingMethod: 'mock',
  },
};

function start(runValue: DiscussionRun = run()): DiscussionStart {
  return {
    threadId: runValue.threadId,
    run: runValue,
    userMessage: { id: 'message-1', threadId: runValue.threadId, runId: runValue.id, role: 'user', content: 'Explore this idea.', scope: null, packetId: runValue.packetId, createdAt: runValue.createdAt },
    packet,
  };
}

function candidate(id: string, content = `A concrete direction for ${id}.\n\nA daily detail that makes it distinct.`): WorkshopCandidate {
  return {
    id, title: `Direction ${id}`, content, dimensionValue: `Mechanism ${id}`,
    implications: [{ text: 'A possible consequence', basis: 'the proposed mechanism', assumption: 'the practice remains accessible' }],
    assumptions: ['Access remains uneven.'], affectedTargets: [], preservedDetails: ['The starting attraction'], changedDetails: ['The daily practice'],
  };
}

function result(overrides: Partial<WorkshopResult> = {}): WorkshopResult {
  return {
    run: run(), sessionId: 'session-1', workingGeneration: '0', action: 'directions', stale: false, validationError: null,
    output: {
      schemaVersion: 'story-workshop-output.v1', requestKind: 'world', question: 'How could this world work?', questionReason: 'Compare mechanisms.',
      dimension: 'Core mechanism', interpretation: { youSaid: 'A place', possibleDirection: 'A specific practice', stillOpen: 'Its consequences' },
      candidates: [candidate('a'), candidate('b'), candidate('c')],
    },
    ...overrides,
  };
}

function record(documentId: string, title: string, kind = 'world'): DocumentRecord {
  return { head: { documentId, version: '1', bodyHash: `${documentId}-hash` }, title, kind, metadataVersion: '1', body, lastCheckpointId: null };
}

const project: OpenedProject = {
  project: { projectId: access.projectId, operationNamespace: access.operationNamespace, title: 'Test story', formatVersion: 28 },
  access, documents: [record('world-1', 'Existing world')], metadataVersion: '1', viewState: null, libraryWarning: null,
};

function relationshipFixture() {
  const documents = [record('mei', 'Mei', 'character'), record('guild', 'Repair guild', 'world')];
  const relationship: WorkshopRelationship = { id: 'mei-trusts-guild', fromDocumentId: 'mei', toDocumentId: 'guild', type: 'trusts',
    description: 'Mei relies on the guild to keep her sister safe.', uncertainty: 'Whether the guild will honor its promise.',
    status: 'tentative', sourceHeads: documents.map(document => document.head) };
  const original = session({ lens: 'world', workingText: 'My separate world draft.', selectedDetails: [{ id: 'keep', candidateId: null, text: 'My separate world draft.', fixed: true }] });
  return { documents, relationship, original, project: { ...project, documents }, view: view({ state: state({ sessions: [original], relationships: [relationship] }) }) };
}

function selectValue(label: string, value: string) {
  const element = [...host.querySelectorAll('label')].find(item => item.firstChild?.textContent === label)?.querySelector('select');
  if (!element) throw new Error(`Missing select: ${label}`);
  element.value = value;
  element.dispatchEvent(new Event('change', { bubbles: true }));
}

let host: HTMLDivElement;
let root: Root;
let currentView: WorkshopView;
let savedDocuments: DocumentRecord[];
let workshopHandle = createRef<WorkshopHandle>();
let aliasesByDocument: Record<string, string[]>;
let onDocumentsChanged: ReturnType<typeof vi.fn<(documents: DocumentRecord[]) => void>>;

function setValue(element: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  const prototype = element instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value')!.set!.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

function exactButton(label: string): HTMLButtonElement {
  const button = [...host.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.trim() === label);
  if (!button) throw new Error(`Missing button: ${label}`);
  return button;
}

async function waitFor(check: () => void): Promise<void> {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try { check(); return; } catch { await act(async () => new Promise(resolve => setTimeout(resolve, 5))); }
  }
  check();
}

async function render(value = currentView, projectValue = project, navigationBusy = false): Promise<void> {
  currentView = value;
  await act(async () => root.render(<Workshop ref={workshopHandle} project={projectValue} navigationBusy={navigationBusy} onOpenDocument={vi.fn()} onDocumentsChanged={onDocumentsChanged} onError={vi.fn()} />));
  await waitFor(() => expect(host.textContent).toContain('Develop or edit directly'));
}
async function setNavigationBusy(value: boolean): Promise<void> {
  await act(async () => root.render(<Workshop ref={workshopHandle} project={project} navigationBusy={value} onOpenDocument={vi.fn()} onDocumentsChanged={onDocumentsChanged} onError={vi.fn()} />));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  window.innerWidth = 1300;
  vi.clearAllMocks();
  workshopHandle = createRef<WorkshopHandle>();
  aliasesByDocument = {};
  mocks.readDocumentAliases.mockImplementation(async (_access, documentId: string) => ({ documentId, aliases: [...(aliasesByDocument[documentId] ?? [])], sourceEpoch: '7' }));
  mocks.setDocumentAliases.mockImplementation(async (_access, documentId: string, _epoch, aliases: string[]) => {
    aliasesByDocument[documentId] = [...aliases];
    return { source: '8', policy: '0' };
  });
  mocks.providers.state = providerState(true);
  currentView = view();
  savedDocuments = [...project.documents];
  onDocumentsChanged = vi.fn<(documents: DocumentRecord[]) => void>(documents => { savedDocuments = documents; });
  mocks.readWorkshop.mockImplementation(async () => structuredClone(currentView));
  mocks.saveWorkshop.mockImplementation(async (request: { state: WorkshopState }) => {
    currentView = { ...currentView, version: String(Number(currentView.version) + 1), state: structuredClone(request.state) };
    return { version: currentView.version, state: structuredClone(request.state) } satisfies WorkshopSnapshot;
  });
  mocks.workshopHistory.mockResolvedValue([]);
  mocks.startWorkshop.mockResolvedValue(start());
  mocks.previewWorkshopAdoption.mockImplementation(async (request: { sessionId: string; expectedVersion: string; targets: any[]; rationale: string; candidateIds: string[]; relationships?: any[]; impactDrafts?: any[] }) => ({
    id: 'preview-1', sessionId: request.sessionId, expectedVersion: request.expectedVersion, targets: request.targets,
    before: savedDocuments.filter(document => request.targets.some(target => target.expected && target.documentId === document.head.documentId)),
    rationale: request.rationale, protectedText: [], candidateIds: request.candidateIds,
    relationships: request.relationships?.map(link => ({ ...link, status: 'tentative', sourceHeads: [link.fromExpected, link.toExpected].filter(Boolean) })),
    impacts: request.impactDrafts?.map((impact, index) => ({ ...impact, id: `impact-${index + 1}`, decisionId: 'preview-decision', candidateId: request.candidateIds[index] ?? request.candidateIds[0] ?? 'candidate', status: 'needsReview' })),
  }));
  mocks.adoptWorkshop.mockResolvedValue({ snapshot: { version: '9', state: state({ decisions: [{ id: 'decision-1', sessionId: 'session-1', title: 'Direction a', documentId: 'new-document', revisionId: 'revision-1', head: { documentId: 'new-document', version: '1', bodyHash: 'new-hash' }, candidateIds: ['candidate-a'], rationale: 'A considered choice', status: 'chosen', fixed: false, protectedText: [], access: 'authorRoom', supersedesId: null }] }) }, documents: [record('new-document', 'Direction a')], decisionIds: ['decision-1'] });
  mocks.readDocument.mockResolvedValue(record('world-1', 'Existing world'));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('Story Workshop behavioral contracts', () => {
  it('keeps protection visible and removable after archiving a fixed story choice', async () => {
    const decision: WorkshopState['decisions'][number] = { id: 'fixed-choice', sessionId: 'session-1', title: 'Mixed motives', documentId: 'world-1', revisionId: 'protected-revision', head: project.documents[0].head, candidateIds: [], rationale: 'Preserve credible safety concerns.', status: 'chosen', fixed: true, protectedText: ['The guild tests dangerous repairs.'], access: 'authorRoom', supersedesId: null };
    await render(view({ state: state({ sessions: [session({ focusDocumentId: 'world-1' })], decisions: [decision] }) }));
    expect(host.querySelector('[aria-label="Protection for this material"]')!.textContent).toContain('Mixed motives');
    await act(async () => exactButton('Archive choice').click());
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.decisions[0]).toMatchObject({ status: 'archived', fixed: true });
    const details = host.querySelector('.workshop-decisions details')!;
    expect(details.querySelector('summary')!.textContent).toContain('archived · Keep fixed');
    await act(async () => details.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click());
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.decisions[0]).toMatchObject({ status: 'archived', fixed: false });
    expect(host.querySelector('[aria-label="Protection for this material"]')).toBeNull();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('edits all interpretation fields as controlled session values and preserves them, clears, and source metadata across reopen', async () => {
    const source = session({ brief: 'The author’s starting attraction.', direction: 'An earlier working direction.', stillOpen: 'A deliberate unknown.',
      originalNotes: 'Keep these original notes.', focusDocumentId: 'world-1', anchorDocumentId: 'stable-anchor', includedDocumentIds: ['world-1'],
      workingText: 'The working version remains separate.', workingGeneration: '7', selectedDetails: [{ id: 'selected-detail', candidateId: null, text: 'Keep this detail.', fixed: true }] });
    const preference: WorkshopPreference = { id: 'preference-1', label: 'Measured wonder', family: 'Tone', meaning: 'Keep the tone restrained.', examples: '', timing: '', polarity: 'want', strength: 'soft', scope: 'project', targetId: null, confirmed: true };
    const decision: WorkshopState['decisions'][number] = { id: 'decision-1', sessionId: source.id, title: 'Chosen direction', documentId: 'world-1', revisionId: 'revision-1', head: project.documents[0].head,
      candidateIds: [], rationale: 'Preserve this choice.', status: 'chosen', fixed: true, protectedText: ['Keep this detail.'], access: 'authorRoom', supersedesId: null };
    await render(view({ state: state({ sessions: [source], preferences: [preference], decisions: [decision] }), results: [result()] }));
    const panel = host.querySelector<HTMLDetailsElement>('.workshop-interpretation')!;
    expect(panel.open).toBe(true);
    const fields = () => [...host.querySelectorAll<HTMLTextAreaElement>('.workshop-interpretation textarea')];
    expect(fields().map(field => field.value)).toEqual([source.brief, source.direction, source.stillOpen]);

    await act(async () => setValue(fields()[0], 'The author’s revised attraction.'));
    await act(async () => setValue(fields()[1], 'A direction the author can reshape.'));
    await act(async () => setValue(fields()[2], 'The cost is still intentionally open.'));
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.sessions[0]).toMatchObject({ brief: 'The author’s revised attraction.', direction: 'A direction the author can reshape.', stillOpen: 'The cost is still intentionally open.',
      originalNotes: source.originalNotes, focusDocumentId: source.focusDocumentId, anchorDocumentId: source.anchorDocumentId, includedDocumentIds: source.includedDocumentIds,
      workingText: source.workingText, workingGeneration: source.workingGeneration, selectedDetails: source.selectedDetails });
    expect(currentView.state.preferences).toEqual([preference]);
    expect(currentView.state.decisions).toEqual([decision]);

    await act(async () => root.unmount());
    root = createRoot(host);
    await render(currentView);
    expect(fields().map(field => field.value)).toEqual(['The author’s revised attraction.', 'A direction the author can reshape.', 'The cost is still intentionally open.']);
    expect(host.textContent).toContain('A place');
    expect(host.textContent).toContain('A specific practice');
    expect(host.textContent).toContain('Its consequences');

    await act(async () => setValue(fields()[0], ''));
    await act(async () => setValue(fields()[1], ''));
    await act(async () => setValue(fields()[2], ''));
    await act(async () => workshopHandle.current!.flush());
    await act(async () => root.unmount());
    root = createRoot(host);
    await render(currentView);
    expect(fields().map(field => field.value)).toEqual(['', '', '']);
    expect(currentView.state.sessions[0]).toMatchObject({ originalNotes: source.originalNotes, focusDocumentId: source.focusDocumentId,
      anchorDocumentId: source.anchorDocumentId, includedDocumentIds: source.includedDocumentIds, workingText: source.workingText, workingGeneration: source.workingGeneration, selectedDetails: source.selectedDetails });
    expect(currentView.state.preferences).toEqual([preference]);
    expect(currentView.state.decisions).toEqual([decision]);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
  });

  it('copies exactly one selected AI suggestion only after an explicit click and does not seed fields from a newer result', async () => {
    const source = session({ brief: 'Author brief', direction: 'Author direction', stillOpen: 'Author unknown' });
    const first = result();
    const newer = result({ run: run({ id: 'run-2', threadId: 'thread-2', operationId: 'operation-2', packetId: 'packet-2', sequence: '2' }), output: {
      ...first.output!, interpretation: { youSaid: 'New model reading', possibleDirection: 'New model direction', stillOpen: 'New model unknown' },
    } });
    await render(view({ state: state({ sessions: [source] }), results: [first, newer] }));
    const panel = () => host.querySelector<HTMLDetailsElement>('.workshop-interpretation')!;
    const fields = () => [...host.querySelectorAll<HTMLTextAreaElement>('.workshop-interpretation textarea')];
    expect(panel().open).toBe(true);
    expect(fields().map(field => field.value)).toEqual([source.brief, source.direction, source.stillOpen]);

    await act(async () => { panel().open = false; panel().dispatchEvent(new Event('toggle', { bubbles: true })); });
    expect(panel().open).toBe(false);
    await act(async () => selectValue('Saved requests', 'run-1'));
    expect(panel().open).toBe(true);
    await act(async () => selectValue('Saved requests', 'run-2'));
    expect(panel().open).toBe(true);
    expect(fields().map(field => field.value)).toEqual([source.brief, source.direction, source.stillOpen]);
    await act(async () => exactButton('Use suggested Possible direction').click());
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.sessions[0]).toMatchObject({ brief: source.brief, direction: 'New model direction', stillOpen: source.stillOpen });
    expect(fields().map(field => field.value)).toEqual([source.brief, 'New model direction', source.stillOpen]);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('opens original notes in an independent Notebook proposal before dispatch and leaves the parent working material untouched', async () => {
    const source = session({ brief: 'Source interpretation', direction: 'Unrelated parent direction', stillOpen: 'Parent question', originalNotes: 'Exact note one.\nAmbiguous note two.',
      focusDocumentId: 'world-1', includedDocumentIds: ['world-1'], workingText: 'Parent working draft.',
      selectedDetails: [{ id: 'parent-detail', candidateId: 'parent-candidate', text: 'Parent selected detail.', fixed: false }],
      choices: [{ candidateId: 'parent-candidate', status: 'saved', rationale: 'Keep for parent.', includeInContext: true }] });
    const preference: WorkshopPreference = { id: 'source-preference', label: 'Source-only preference', family: 'World', meaning: 'Keep this exploration local.', examples: '', timing: '', polarity: 'want', strength: 'soft', scope: 'exploration', targetId: source.id, confirmed: true };
    let savedChildBeforeDispatch = false;
    mocks.startWorkshop.mockImplementationOnce(async (_access, _operationId, exploration) => {
      savedChildBeforeDispatch = currentView.state.sessions.some(item => item.id === exploration.sessionId && item.composer === '');
      const started = start();
      currentView = { ...currentView, results: [result({ sessionId: exploration.sessionId, run: started.run })] };
      return started;
    });
    await render(view({ state: state({ sessions: [source], preferences: [preference] }) }));
    const parentWorking = host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!;
    await act(async () => {
      parentWorking.focus();
      parentWorking.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', code: 'KeyA', ctrlKey: true, bubbles: true }));
      parentWorking.select();
      parentWorking.dispatchEvent(new KeyboardEvent('keyup', { key: 'a', code: 'KeyA', ctrlKey: true, bubbles: true }));
      document.dispatchEvent(new Event('selectionchange', { bubbles: true }));
    });
    await waitFor(() => expect(host.textContent).toContain('Feedback scope: selected passage'));
    expect(host.textContent).toContain('Parent working draft.');
    await act(async () => exactButton('Organize these notes').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());

    const child = currentView.state.sessions.find(item => item.id !== source.id)!;
    expect(savedChildBeforeDispatch).toBe(true);
    expect(currentView.state.sessions.find(item => item.id === source.id)).toEqual(source);
    expect(child).toMatchObject({ title: 'Organize original notes', workingTitle: 'Organized notes', parentSessionId: null, branchKind: 'working',
      lens: 'notebook', brief: NOTES_ORGANIZATION_BRIEF, direction: '', stillOpen: '', originalNotes: source.originalNotes, workingText: '', selectedDetails: [], choices: [],
      focusDocumentId: null, includedDocumentIds: [], selectedScope: NOTES_ORGANIZATION_SCOPE, focusQuestion: 'How should these notes be organized for future writing?' });
    expect(child.id).not.toBe(source.id);
    expect(currentView.state.preferences).toEqual([preference, expect.objectContaining({ ...preference, id: expect.any(String), targetId: child.id })]);
    expect(currentView.state.preferences[1].id).not.toBe(preference.id);
    const request = mocks.startWorkshop.mock.calls[0][2];
    expect(request).toMatchObject({ sessionId: child.id, action: 'directions', selectedScope: NOTES_ORGANIZATION_SCOPE, selectedText: '', workingSelection: null });
    expect(request.instruction).toContain(ORGANIZE_NOTES_INSTRUCTION);
    expect(mocks.saveWorkshop.mock.invocationCallOrder.at(-1)!).toBeLessThan(mocks.startWorkshop.mock.invocationCallOrder[0]);
    expect(mocks.previewWorkshopAdoption).not.toHaveBeenCalled();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();

    await waitFor(() => expect(host.textContent).toContain('Direction a'));
    await act(async () => exactButton('Develop this').click());
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.sessions.find(item => item.id === source.id)).toEqual(source);
    expect(currentView.state.sessions.find(item => item.id === child.id)?.workingText).toContain('A concrete direction for a.');
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
  });

  it('keeps organization evidence changes in the child task brief and marks retained candidates stale', async () => {
    const organization = session({ lens: 'notebook', brief: NOTES_ORGANIZATION_BRIEF, originalNotes: 'Original notes to organize.', selectedScope: NOTES_ORGANIZATION_SCOPE });
    const savedResult = result({ sessionId: organization.id, workingGeneration: '0' });
    await render(view({ state: state({ sessions: [organization] }), results: [savedResult] }));
    await act(async () => exactButton('Bring existing notes').click());
    const notes = [...host.querySelectorAll<HTMLTextAreaElement>('.workshop-brief textarea')].at(-1)!;
    await act(async () => setValue(notes, 'Changed notes require a fresh proposal.'));
    await waitFor(() => expect(host.textContent).toContain('Stale result'));
    await act(async () => workshopHandle.current!.flush());

    expect(currentView.state.sessions[0]).toMatchObject({ brief: NOTES_ORGANIZATION_BRIEF, originalNotes: 'Changed notes require a fresh proposal.', workingGeneration: '1' });
    expect(currentView.results[0].workingGeneration).toBe('0');
    expect(currentView.state.sessions[0].workingText).toBe('');
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('keeps notes organization explicit after paste and blocks it when the provider is unavailable', async () => {
    await render();
    expect(host.querySelector('.workshop-notes-organization')).toBeNull();
    await act(async () => exactButton('Bring existing notes').click());
    const notesFields = () => [...host.querySelectorAll<HTMLTextAreaElement>('.workshop-brief textarea')];
    await act(async () => setValue(notesFields().at(-1)!, 'Pasted notes remain author material.'));
    await waitFor(() => expect(exactButton('Organize these notes').disabled).toBe(false));
    expect(mocks.startWorkshop).not.toHaveBeenCalled();

    mocks.providers.state = providerState(false);
    await render(view({ state: state({ sessions: [session({ originalNotes: 'Unavailable provider notes.' })] }) }));
    const before = structuredClone(currentView.state);
    expect(exactButton('Organize these notes').disabled).toBe(true);
    await act(async () => exactButton('Organize these notes').click());
    expect(currentView.state).toEqual(before);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('retries uncertain notes organization with the same child and pending request identity', async () => {
    mocks.startWorkshop.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Notes organization acknowledgment was lost.' }).mockResolvedValueOnce(start());
    const source = session({ originalNotes: 'Retry these exact notes.' });
    await render(view({ state: state({ sessions: [source] }) }));
    await act(async () => exactButton('Organize these notes').click());
    await waitFor(() => expect(host.textContent).toContain('Notes organization acknowledgment was lost.'));
    const firstCall = structuredClone(mocks.startWorkshop.mock.calls[0]);
    const childId = currentView.state.currentSessionId;
    expect(currentView.state.sessions).toHaveLength(2);
    expect(exactButton('Organize these notes').disabled).toBe(true);
    await act(async () => exactButton('Check request status').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledTimes(2));
    expect(currentView.state.sessions).toHaveLength(2);
    expect(currentView.state.currentSessionId).toBe(childId);
    expect(mocks.startWorkshop.mock.calls[1]).toEqual(firstCall);
  });

  it('blocks leaving while Explore is saving its request basis before a run identity exists', async () => {
    await render();
    let releaseSave!: () => void;
    mocks.saveWorkshop.mockImplementationOnce((request: { state: WorkshopState }) => new Promise<WorkshopSnapshot>(resolve => {
      releaseSave = () => {
        currentView = { ...currentView, version: String(Number(currentView.version) + 1), state: structuredClone(request.state) };
        resolve({ version: currentView.version, state: structuredClone(request.state) });
      };
    }));
    await act(async () => setValue(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!, 'An unsaved idea to explore.'));
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(releaseSave).toBeTypeOf('function'));
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    await expect(workshopHandle.current!.flush()).rejects.toThrow(/pending Workshop request/);
    await act(async () => releaseSave());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    await act(async () => workshopHandle.current!.flush());
    expect(mocks.startWorkshop).toHaveBeenCalledOnce();
  });
  it('keeps world, lens, and generated questions set aside until the author explicitly reopens them', async () => {
    const world = WORLD_QUESTIONS[0];
    const people = LENSES.find(lens => lens.id === 'people')!;
    const offered = result().output!.question;
    const questions: WorkshopSession['questions'] = [
      { id: 'world-question', text: world.text, reason: world.reason, status: 'keepMysterious', unknownTo: 'reader' },
      { id: 'people-question', text: people.question, reason: people.reason, status: 'notRelevant', unknownTo: 'both' },
      { id: 'offered-question', text: offered, reason: 'Saved for later.', status: 'notNow', unknownTo: 'author' },
    ];
    await render(view({ state: state({ sessions: [session({ lens: 'world', focusQuestion: 'A current question', questions })] }), results: [result()] }));
    await act(async () => exactButton(world.title).click());
    expect(host.querySelector('.workshop-question h2')!.textContent).toBe('A current question');
    expect(host.textContent).toContain('This question is marked Keep mysterious.');
    await act(async () => exactButton('People').click());
    expect(host.querySelector('.workshop-question h2')!.textContent).toBe('A current question');
    expect(host.textContent).toContain('This question is marked Not relevant.');
    await act(async () => exactButton('Explore this question').click());
    expect(host.querySelector('.workshop-question h2')!.textContent).toBe('A current question');
    expect(host.textContent).toContain('This question is marked Not now.');
    await act(async () => exactButton('World').click());
    const record = [...host.querySelectorAll('.workshop-question-record')].find(element => element.querySelector('p')!.textContent === world.text)!;
    await act(async () => {
      const select = record.querySelector('select')!;
      select.value = 'open'; select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => exactButton(world.title).click());
    expect(host.querySelector('.workshop-question h2')!.textContent).toBe(world.text);
    expect(host.querySelector('.workshop-question > p')!.textContent).toBe(world.reason);
    await act(async () => exactButton('Not now').click());
    await act(async () => workshopHandle.current!.flush());
    expect(currentView.state.sessions[0].questions.find(question => question.id === 'world-question')).toMatchObject({ status: 'notNow', unknownTo: 'reader' });
    await act(async () => root.unmount()); root = createRoot(host);
    await render(currentView);
    await act(async () => exactButton(world.title).click());
    expect(host.textContent).toContain('This question is marked Not now.');
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });
  it('keeps unfinished names in the Workshop across lens changes and refuses leaving or another material until resolved', async () => {
    const fixture = relationshipFixture();
    await render(fixture.view, fixture.project);
    await act(async () => selectValue('Saved person, place, or group', 'mei'));
    const input = () => host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')!;
    await waitFor(() => expect(input()?.disabled).toBe(false));
    await act(async () => setValue(input(), '林乔\nLin Qiao\nAsh Wren'));
    await act(async () => expect(workshopHandle.current!.flush()).rejects.toThrow(/names/i));
    await act(async () => selectValue('Saved person, place, or group', 'guild'));
    expect(input().value).toContain('Ash Wren');
    expect(host.querySelector<HTMLSelectElement>('.workshop-names select')!.value).toBe('mei');
    await act(async () => exactButton('Themes & tone').click());
    expect(input().value).toContain('Ash Wren');
    await act(async () => exactButton('Discard changes').click());
    await act(async () => workshopHandle.current!.flush());
    await act(async () => selectValue('Saved person, place, or group', 'guild'));
    await waitFor(() => expect(input().value).toBe(''));
    expect(mocks.setDocumentAliases).not.toHaveBeenCalled();
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('saves optional names for the chosen document and reads them again without generating or changing story material', async () => {
    await render(view({ state: state({ sessions: [session({ lens: 'world' })] }) }));
    await act(async () => selectValue('Saved person, place, or group', 'world-1'));
    await waitFor(() => expect(host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')?.disabled).toBe(false));
    await act(async () => setValue(host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')!, '林乔\nLin Qiao\nAsh Wren'));
    await act(async () => exactButton('Save names').click());
    await waitFor(() => expect(mocks.setDocumentAliases).toHaveBeenCalledOnce());
    expect(mocks.setDocumentAliases.mock.calls[0].slice(0, 3)).toEqual([access, 'world-1', '7']);
    expect(new Set(mocks.setDocumentAliases.mock.calls[0][3])).toEqual(new Set(['林乔', 'Lin Qiao', 'Ash Wren']));
    await act(async () => exactButton('Close names').click());
    await act(async () => selectValue('Saved person, place, or group', 'world-1'));
    await waitFor(() => expect(host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')?.value).toContain('林乔'));
    await act(async () => workshopHandle.current!.flush());
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('forks an independent saved exploration and keeps the parent intact across reopen', async () => {
    const original = session({ title: 'Guild knowledge', workingTitle: 'Repair rules', workingText: 'The guild teaches only apprentices.',
      selectedDetails: [{ id: 'fixed-rule', candidateId: null, text: 'The guild', fixed: true }],
      activeRunId: 'old-parent-run', originalNotes: 'Keep the original attraction.' });
    const preference: WorkshopPreference = { id: 'local-preference', label: 'Public teaching', family: 'World', meaning: 'Explore access to knowledge.',
      examples: '', timing: '', polarity: 'want', strength: 'soft', scope: 'exploration', targetId: original.id, confirmed: true };
    await render(view({ state: state({ sessions: [original], preferences: [preference] }) }));
    await act(async () => exactButton('Explore a what-if').click());
    await waitFor(() => expect(currentView.state.sessions).toHaveLength(2));
    const child = currentView.state.sessions.find(item => item.id !== original.id)!;
    expect(child).toMatchObject({ parentSessionId: original.id, branchKind: 'whatIf', workingText: original.workingText, activeRunId: null });
    expect(child.anchorDocumentId).not.toBe(original.anchorDocumentId);
    const childPreference = currentView.state.preferences.find(item => item.targetId === child.id)!;
    expect(childPreference).toMatchObject({ ...preference, id: expect.any(String), targetId: child.id });
    expect(childPreference.id).not.toBe(preference.id);
    await act(async () => setValue(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!, 'The guild publishes repair manuals.'));
    await waitFor(() => expect(currentView.state.sessions.find(item => item.id === child.id)?.workingText).toBe('The guild publishes repair manuals.'));
    expect(currentView.state.sessions.find(item => item.id === original.id)).toEqual(original);
    expect(currentView.state.preferences.find(item => item.id === preference.id)).toEqual(preference);
    expect(savedDocuments).toEqual(project.documents);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();

    await act(async () => root.unmount());
    root = createRoot(host);
    await render(currentView);
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!.value).toBe('The guild publishes repair manuals.');
    await act(async () => exactButton(original.title).click());
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!.value).toBe(original.workingText);
    await act(async () => exactButton(`What if · ${child.title}`).click());
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!.value).toBe('The guild publishes repair manuals.');
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('carries inherited candidate impacts into a what-if preview and adopts only after confirmation', async () => {
    const inherited = candidate('inherited', 'Repair manuals become public.');
    inherited.affectedTargets = [{ documentId: 'world-1', reason: 'Public instruction may change the guild apprenticeship.' }];
    const original = session({ lens: 'world', title: 'Guild knowledge', focusDocumentId: 'world-1', workingTitle: 'Repair rules', workingText: inherited.content,
      selectedDetails: [{ id: 'inherited-detail', candidateId: inherited.id, text: inherited.content, fixed: false }] });
    await render(view({ state: state({ sessions: [original] }), results: [result({ output: { ...result().output!, candidates: [inherited] } })] }));
    await act(async () => exactButton('Explore a what-if').click());
    await waitFor(() => expect(currentView.state.sessions).toHaveLength(2));
    const branchId = currentView.state.currentSessionId!;
    await act(async () => exactButton('Use this version').click());
    expect(mocks.previewWorkshopAdoption).not.toHaveBeenCalled();
    expect(host.querySelector('[aria-label="Review affected material"]')?.textContent).toContain(inherited.affectedTargets[0].reason);
    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    expect(mocks.previewWorkshopAdoption.mock.calls[0][0]).toMatchObject({ sessionId: branchId, candidateIds: [inherited.id],
      impactDrafts: [{ documentId: 'world-1', kind: 'possibleTension', reason: inherited.affectedTargets[0].reason }],
      targets: [{ documentId: 'world-1', expected: project.documents[0].head, mode: 'add' }] });
    expect(currentView.state.sessions.find(item => item.id === original.id)).toEqual(original);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
    await act(async () => exactButton('Confirm Use this version').click());
    await waitFor(() => expect(mocks.adoptWorkshop).toHaveBeenCalledOnce());
    expect(onDocumentsChanged).toHaveBeenCalledOnce();
  });

  it('locks duplicate interpretation and original-note controls while adoption acknowledgment is uncertain', async () => {
    const source = session({ brief: 'Author brief', direction: 'Author direction', stillOpen: 'Author unknown', originalNotes: 'Original notes', workingText: 'A working version.' });
    await render(view({ state: state({ sessions: [source] }), results: [result()] }));
    await act(async () => exactButton('Use this version').click());
    await waitFor(() => expect(host.textContent).toContain('Where should this version go?'));
    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    mocks.adoptWorkshop.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Adoption acknowledgment was lost.' });
    await act(async () => exactButton('Confirm Use this version').click());
    await waitFor(() => expect(host.textContent).toContain('Adoption acknowledgment was lost.'));

    expect(exactButton('Check adoption result').disabled).toBe(false);
    expect(host.querySelector<HTMLDetailsElement>('.workshop-interpretation')?.open).toBe(true);
    expect([...host.querySelectorAll<HTMLTextAreaElement>('.workshop-interpretation textarea')].every(field => field.disabled)).toBe(true);
    expect([...host.querySelectorAll<HTMLButtonElement>('.workshop-interpretation button')].filter(button => button.textContent?.startsWith('Use suggested')).every(button => button.disabled)).toBe(true);

    await act(async () => exactButton('Bring existing notes').click());
    expect([...host.querySelectorAll<HTMLTextAreaElement>('.workshop-brief textarea')].every(field => field.disabled)).toBe(true);
    expect([...host.querySelectorAll<HTMLTextAreaElement>('.workshop-context textarea')].every(field => field.disabled)).toBe(true);
  });

  it('reconciles an uncertain request with the same identity even after going offline', async () => {
    mocks.startWorkshop.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Acknowledgment lost.' });
    await render(view({ state: state({ sessions: [session(), session({ id: 'other-session', anchorDocumentId: 'other-anchor', title: 'Another exploration' })] }) }));
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(host.textContent).toContain('Acknowledgment lost.'));
    const original = structuredClone(mocks.startWorkshop.mock.calls[0]);
    expect(exactButton('Another exploration').disabled).toBe(true);
    await act(async () => selectValue('Exploration', 'other-session'));
    await act(async () => exactButton('New exploration').click());
    await act(async () => exactButton('Explore a what-if').click());
    expect(currentView.state.currentSessionId).toBe('session-1');
    expect(currentView.state.sessions).toHaveLength(2);
    expect(host.textContent).toContain('Check the pending request before opening another exploration.');
    mocks.providers.state = providerState(false);
    await render();
    expect(exactButton('Check request status').disabled).toBe(false);
    await act(async () => exactButton('Check request status').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledTimes(2));
    expect(mocks.startWorkshop.mock.calls[1]).toEqual(original);
  });

  it('uses the frozen selected range after reopen even when the same words occur twice', async () => {
    await render(view({ state: state({ sessions: [session({ workingText: 'Keep. Echo. Echo. End.' })] }), results: [result({ workingSelection: { from: 12, to: 16, text: 'Echo' } })] }));
    await act(async () => exactButton('Develop this').click());
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!.value).toBe(`Keep. Echo. ${candidate('a').content}. End.`);
  });

  it('refuses to redirect a whole-version result into a newly selected passage', async () => {
    await render(view({ state: state({ sessions: [session({ workingText: 'Keep this passage.' })] }), results: [result()] }));
    const editor = host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!;
    await act(async () => { editor.focus(); editor.setSelectionRange(5, 9); document.dispatchEvent(new Event('selectionchange')); });
    expect(host.textContent).toContain('Feedback scope: selected passage');
    await act(async () => exactButton('Develop this').click());
    expect(editor.value).toBe('Keep this passage.');
    expect(host.textContent).toContain('generated for the whole working version');
  });
  it('does not rebase a stale scoped result onto another occurrence of the same text', async () => {
    await render(view({ state: state({ sessions: [session({ workingText: 'Keep. Echo. Echo. End.', workingGeneration: '1' })] }), results: [result({ stale: true, workingSelection: { from: 12, to: 16, text: 'Echo' } })] }));
    const editor = host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!;
    await act(async () => { editor.focus(); editor.setSelectionRange(6, 10); document.dispatchEvent(new Event('selectionchange')); });
    await act(async () => exactButton('Review against current work').click());
    await act(async () => exactButton('Develop this').click());
    expect(editor.value).toBe('Keep. Echo. Echo. End.');
    expect(host.textContent).toContain('Select the passage and explore again');
  });

  it('cycles optional questions without reopening dismissed questions or generating', async () => {
    await render();
    await act(async () => exactButton('Show a different question').click());
    const first = host.querySelector('.workshop-question h2')!.textContent;
    await act(async () => exactButton('Not relevant').click());
    await act(async () => exactButton('Show a different question').click());
    expect(host.querySelector('.workshop-question h2')!.textContent).not.toBe(first);
    for (let index = 0; index < 12; index += 1) {
      await act(async () => exactButton('Show a different question').click());
      expect(host.querySelector('.workshop-question h2')!.textContent).not.toBe(first);
    }
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('does not start generation when it initializes, changes lens, edits preferences, or navigates', async () => {
    await render();
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    await act(async () => exactButton('World').click());
    await act(async () => exactButton('Add preference').click());
    await act(async () => exactButton('Cancel').click());
    await act(async () => exactButton('New').click());
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('can seed a world-first exploration without chapter, genre, or protagonist fields', async () => {
    await render();
    const brief = host.querySelector<HTMLTextAreaElement>('.workshop-brief textarea')!;
    setValue(brief, 'A coastal archive where people repair broken weather instruments.');
    await act(async () => exactButton('World').click());
    expect(host.textContent).not.toContain('Genre');
    expect(host.textContent).not.toContain('Protagonist');
    expect(host.textContent).not.toContain('Chapter outline');
    await act(async () => host.querySelector<HTMLButtonElement>('.workshop-generation-footer .primary-button')!.click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    const exploration = mocks.startWorkshop.mock.calls[0][2] as { action: string; instruction: string; selectedScope: string };
    expect(exploration.action).toBe('directions');
    expect(exploration.instruction).toContain('three');
    expect(exploration.selectedScope).toContain('Whole working version');
  });

  it('keeps candidate/detail work in the workshop until an explicit adoption commit', async () => {
    await render(view({ results: [result()] }));
    expect(host.textContent).toContain('Direction a');
    await act(async () => exactButton('Select details').click());
    await act(async () => exactButton('Select full direction').click());
    await act(async () => exactButton('Review against current work').click());
    await act(async () => exactButton('Develop this').click());
    expect((host.querySelector('.workshop-working-text') as HTMLTextAreaElement).value).toContain('A concrete direction for a.');
    expect(savedDocuments).toEqual(project.documents);
    await act(async () => exactButton('Use this version').click());
    await waitFor(() => expect(host.textContent).toContain('Where should this version go?'));
    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    const previewRequest = mocks.previewWorkshopAdoption.mock.calls[0][0];
    expect(previewRequest.candidateIds).toEqual(['a']);
    expect(previewRequest.targets[0].mode).toBe('add');
    expect(host.textContent).toContain('Choose this version for your story');
    expect(onDocumentsChanged).not.toHaveBeenCalled();
    await act(async () => exactButton('Confirm Use this version').click());
    await waitFor(() => expect(onDocumentsChanged).toHaveBeenCalledOnce());
    expect(savedDocuments.map(document => document.head.documentId)).toContain('new-document');
  });

  it('prepares consequence rejection locally and carries exact provisional context through save and reopen', async () => {
    const existingChoice = { candidateId: 'a', status: 'saved' as const, rationale: 'Keep the hopeful pressure', includeInContext: false };
    const existingDetail = { id: 'existing-detail', candidateId: null, text: 'The author draft stays intact.', fixed: false };
    const source = session({ workingText: 'The author draft stays intact.', composer: 'Keep my original direction.', selectedDetails: [existingDetail], choices: [existingChoice] });
    await render(view({ state: state({ sessions: [source] }), results: [result()] }));
    await act(async () => exactButton('Select details').click());
    const card = host.querySelector('.candidate-card')!;
    const implicationActions = card.querySelectorAll<HTMLButtonElement>('.candidate-implication-actions button');
    await act(async () => implicationActions[1].click());
    await waitFor(() => expect(currentView.state.sessions[0].composer).toContain('Questioned candidate: Direction a'));

    const prepared = currentView.state.sessions[0];
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(prepared).toMatchObject({
      workingText: source.workingText,
      selectedScope: 'Direction a',
      choices: [existingChoice],
      selectedDetails: [existingDetail],
    });
    expect(prepared.composer).toContain('Questioned candidate: Direction a');
    expect(prepared.composer).toContain('Candidate direction: A concrete direction for a.');
    expect(prepared.composer).toContain('Implication: A possible consequence');
    expect(prepared.composer).toContain('Basis: the proposed mechanism');
    expect(prepared.composer).toContain('Assumption: the practice remains accessible');
    expect(prepared.composer).toContain('Reject this assumption');
    expect(savedDocuments).toEqual(project.documents);

    await act(async () => root.unmount());
    root = createRoot(host);
    await render(currentView);
    expect(host.querySelector<HTMLTextAreaElement>('[aria-label="Steer this exploration"] textarea')?.value).toContain('Keep my original direction.');
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    const exploration = mocks.startWorkshop.mock.calls[0][2] as { instruction: string; selectedScope: string; selectedText: string };
    expect(exploration).toMatchObject({ selectedScope: 'Direction a', selectedText: '' });
    expect(exploration.instruction).toContain('Questioned candidate: Direction a');
    expect(exploration.instruction).toContain('Basis: the proposed mechanism');
    expect(exploration.instruction).toContain('Reject this assumption');
    expect(savedDocuments).toEqual(project.documents);
  });

  it.each([['consequences', 'Show consequences'], ['directions', 'Give alternatives']])('targets a candidate for %s without adding it to the tray or changing the working body', async (action, label) => {
    const source = session({ workingText: 'The author draft stays intact.' });
    await render(view({ state: state({ sessions: [source] }), results: [result()] }));
    await act(async () => exactButton('Select details').click());
    await act(async () => exactButton(label).click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    expect(currentView.state.sessions[0]).toMatchObject({ workingText: source.workingText, selectedDetails: [], choices: [] });
    expect(mocks.startWorkshop.mock.calls[0][2]).toMatchObject({ action, selectedScope: 'Direction a', selectedText: candidate('a').content });
    if (action === 'directions') {
      expect(mocks.startWorkshop.mock.calls[0][2].instruction).toContain('Compare alternatives along the existing dimension: Core mechanism.');
      expect(mocks.startWorkshop.mock.calls[0][2].instruction).toContain('Preserve author-chosen invariants and Keep fixed details');
    }
    expect(savedDocuments).toEqual(project.documents);
  });

  it('links two new character and world destinations by their stable target IDs before confirmation', async () => {
    const linked = candidate('linked', 'Mira learns the archive language.\n\nThe archive answers only to patient hands.');
    const seeded = session({
      lens: 'people', workingTitle: 'Mira', workingText: linked.content,
      selectedDetails: [{ id: 'selected-linked', candidateId: linked.id, text: linked.content, fixed: false }],
    });
    await render(view({
      state: state({ sessions: [seeded] }),
      results: [result({ output: { ...result().output!, candidates: [linked] } })],
    }));
    await act(async () => exactButton('Use this version').click());
    await waitFor(() => expect(host.textContent).toContain('Where should this version go?'));

    await act(async () => exactButton('Include related material in this decision').click());
    const materialFields = () => [...host.querySelectorAll<HTMLFieldSetElement>('.workshop-adoption fieldset')]
      .filter(fieldset => fieldset.querySelector('legend')?.textContent?.startsWith('Material'));
    const second = materialFields()[1];
    expect(second).toBeTruthy();
    const secondSelects = second.querySelectorAll<HTMLSelectElement>('select');
    await act(async () => setValue(second.querySelector<HTMLInputElement>('input[required]')!, 'The patient archive'));
    await act(async () => {
      secondSelects[1].value = 'world';
      secondSelects[1].dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => setValue(second.querySelector<HTMLTextAreaElement>('textarea')!, 'The archive answers only to patient hands.'));
    await act(async () => exactButton('Include a relationship').click());
    const relationships = host.querySelector<HTMLElement>('[aria-label="Relationships in this adoption"]')!;
    const relationshipSelects = relationships.querySelectorAll<HTMLSelectElement>('select');
    const newParticipantIds = [...relationshipSelects[0].options]
      .filter(option => option.textContent?.includes('New in this decision')).map(option => option.value);
    expect(newParticipantIds).toHaveLength(2);
    await act(async () => {
      relationshipSelects[0].value = newParticipantIds[0];
      relationshipSelects[0].dispatchEvent(new Event('change', { bubbles: true }));
      relationshipSelects[1].value = newParticipantIds[1];
      relationshipSelects[1].dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => setValue(relationships.querySelector<HTMLTextAreaElement>('textarea')!, 'Mira trusts the archive with her unfinished translations.'));

    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    const previewRequest = mocks.previewWorkshopAdoption.mock.calls[0][0] as {
      targets: Array<{ documentId: string; kind: string; title: string }>;
      relationships: Array<{ fromDocumentId: string; toDocumentId: string; description: string }>;
    };
    expect(previewRequest.targets).toHaveLength(2);
    expect(new Set(previewRequest.targets.map(target => target.documentId)).size).toBe(2);
    expect(previewRequest.targets.map(target => target.kind).sort()).toEqual(['character', 'world']);
    expect(previewRequest.targets.map(target => target.documentId)).toEqual(expect.not.arrayContaining(['world-1']));
    expect(previewRequest.relationships).toHaveLength(1);
    expect(previewRequest.relationships[0].fromDocumentId).toBe(previewRequest.targets[0].documentId);
    expect(previewRequest.relationships[0].toDocumentId).toBe(previewRequest.targets[1].documentId);
    expect(previewRequest.relationships[0].description).toContain('unfinished translations');
    expect(host.textContent).toContain('Relationships to choose');
    expect(onDocumentsChanged).not.toHaveBeenCalled();

    await act(async () => exactButton('Confirm Use this version').click());
    await waitFor(() => expect(onDocumentsChanged).toHaveBeenCalledOnce());
  });

  it('keeps story review flags while excluding internal anchors from new and historical impacts', async () => {
    const affected = candidate('affected', 'The archive grants access by patience.');
    affected.affectedTargets = [
      { documentId: 'world-1', reason: 'This direction may tension the archive access rule.' },
      { documentId: 'workshop-session-1', reason: 'Internal blank anchor should never need review.' },
    ];
    const seeded = session({
      lens: 'world', workingTitle: 'Archive access', workingText: affected.content,
      selectedDetails: [{ id: 'selected-affected', candidateId: affected.id, text: affected.content, fixed: false }],
    });
    await render(view({
      state: state({ sessions: [seeded], impacts: [{ id: 'internal-impact', documentId: 'workshop-session-1', decisionId: 'old-decision', kind: 'possibleTension', reason: 'Historical internal anchor flag.', status: 'needsReview' }] }),
      results: [result({ output: { ...result().output!, candidates: [affected] } })],
    }));
    await act(async () => exactButton('Use this version').click());
    await waitFor(() => expect(host.querySelector('[aria-label="Review affected material"]')).toBeTruthy());
    expect(host.textContent).toContain('Possible tension');
    expect(host.textContent).toContain('This direction may tension the archive access rule.');
    expect(host.textContent).not.toContain('Internal blank anchor should never need review.');
    expect(host.textContent).not.toContain('Historical internal anchor flag.');

    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    const previewRequest = mocks.previewWorkshopAdoption.mock.calls[0][0] as {
      targets: Array<{ documentId: string }>;
      impactDrafts: Array<{ documentId: string; kind: string; reason: string }>;
    };
    expect(previewRequest.impactDrafts).toEqual([{
      documentId: 'world-1', kind: 'possibleTension', reason: 'This direction may tension the archive access rule.',
    }]);
    expect(previewRequest.targets.some(target => target.documentId === 'world-1')).toBe(false);
    expect(host.textContent).toContain('Review flags to save');
    expect(host.textContent).toContain('Existing world · Possible tension');
    expect(host.textContent).toContain('Needs review; its content will not be automatically repaired.');
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('prepares voice guidance as an explicit action without starting or adopting a request', async () => {
    const themed = session({ lens: 'themes', workingTitle: 'Voice sample', workingText: 'A short sample with a measured rhythm.' });
    await render(view({ state: state({ sessions: [themed] }) }));
    await act(async () => exactButton('Propose voice guidance from this sample').click());
    const action = [...host.querySelectorAll<HTMLLabelElement>('label')]
      .find(label => label.textContent?.startsWith('Next action'))?.querySelector<HTMLSelectElement>('select');
    expect(action?.value).toBe('voiceGuidance');
    await waitFor(() => {
      const direction = [...host.querySelectorAll<HTMLLabelElement>('label')]
        .find(label => label.textContent?.startsWith('Your direction'))?.querySelector<HTMLTextAreaElement>('textarea');
      expect(direction?.value).toContain('voice qualities');
    });
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('offers two-treatment moments as noncanon alternatives without replacing the author sample', async () => {
    const source = session({ lens: 'themes', workingText: 'A repairer hears the broken clock strike thirteen.' });
    const moment = result({ action: 'moment', output: { ...result().output!, candidates: [candidate('intimate'), candidate('wondrous')] } });
    await render(view({ state: state({ sessions: [source] }), results: [moment] }));
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(2);
    expect(host.textContent).toContain('Noncanon experiment');
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')?.value).toBe(source.workingText);
    await act(async () => selectValue('Next action', 'moment'));
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    expect(mocks.startWorkshop.mock.calls[0][2].instruction).toContain('SAME situation in two or three');
    expect(mocks.previewWorkshopAdoption).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('derives voice guidance from a noncanon result without moving its events into the working version or tray', async () => {
    const source = session({ lens: 'themes', workingText: 'The author keeps this scene unchanged.' });
    const sample = candidate('sample');
    sample.content = 'A noncanon messenger reveals the ending.';
    const moment = result({ action: 'moment', output: { ...result().output!, candidates: [sample, candidate('other')] } });
    await render(view({ state: state({ sessions: [source] }), results: [moment] }));
    expect([...host.querySelectorAll('.candidate-card button')].some(button => button.textContent === 'Develop this')).toBe(false);
    await act(async () => exactButton('Propose voice guidance').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    expect(mocks.startWorkshop.mock.calls[0][2]).toMatchObject({ action: 'voiceGuidance', selectedText: sample.content, selectedScope: 'Voice qualities from the sample', workingSelection: null });
    expect(mocks.startWorkshop.mock.calls[0][2].instruction).toContain('STYLE instructions only');
    expect(currentView.state.sessions[0].workingText).toBe(source.workingText);
    expect(currentView.state.sessions[0].selectedDetails).toEqual([]);
    expect(currentView.state.decisions).toEqual([]);
    expect(mocks.previewWorkshopAdoption).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('requires a convention and an explicit transformation before subversion generates', async () => {
    await render(view({ results: [result()] }));
    await act(async () => exactButton('Select details').click());
    await act(async () => exactButton('Subvert this direction').click());
    expect(currentView.state.sessions[0].choices).toEqual([]);
    expect(currentView.state.sessions[0].selectedDetails).toEqual([]);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    const labeled = (name: string) => [...host.querySelectorAll('label')].find(label => label.firstChild?.textContent === name)!;
    const transformation = labeled('Transformation').querySelector('select')!;
    expect(labeled('Convention to transform').querySelector('input')!.value).toBe('');
    expect(transformation.value).toBe('');
    await act(async () => {
      transformation.value = 'Change who pays the cost';
      transformation.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => exactButton('Explore').click());
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    await act(async () => {
      setValue(labeled('Convention to transform').querySelector('input')!, 'Inherited special power');
      transformation.value = 'Change who pays the cost';
      transformation.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    expect(mocks.startWorkshop.mock.calls[0][2]).toMatchObject({ action: 'subvert', selectedScope: 'Inherited special power' });
    expect(mocks.startWorkshop.mock.calls[0][2].instruction).toContain('Transformation: Change who pays the cost');
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('does not replace a manually edited working version when a late result arrives', async () => {
    const queued = result({ run: run({ status: 'running', outputText: 'partial' }), output: null });
    const late = view({ version: '2', state: state({ sessions: [session({ workingText: 'A remote late version', workingGeneration: '1' })] }), results: [result()] });
    mocks.readWorkshop.mockReset().mockResolvedValueOnce(view({ results: [queued] })).mockResolvedValueOnce(late);
    let resolveSave!: (value: WorkshopSnapshot) => void;
    mocks.saveWorkshop.mockImplementationOnce(() => new Promise<WorkshopSnapshot>(resolve => { resolveSave = resolve; }));
    await render();
    const working = host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!;
    setValue(working, 'My local manual version');
    await act(async () => new Promise(resolve => setTimeout(resolve, 700)));
    expect((host.querySelector('.workshop-working-text') as HTMLTextAreaElement).value).toBe('My local manual version');
    resolveSave({ version: '2', state: currentView.state });
  });

  it('keeps editing and saving available while generation is offline', async () => {
    mocks.providers.state = providerState(false);
    await render();
    const working = host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!;
    setValue(working, 'Manual setting notes remain editable offline.');
    await act(async () => new Promise(resolve => setTimeout(resolve, 320)));
    expect((host.querySelector('.workshop-working-text') as HTMLTextAreaElement).value).toContain('Manual setting notes');
    expect(host.textContent).toContain('You can keep editing, saving, and organizing here.');
    expect(host.querySelector<HTMLButtonElement>('.workshop-generation-footer .primary-button')?.disabled).toBe(true);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(mocks.saveWorkshop).toHaveBeenCalled();
  });

  it('rejects a local hard preference that conflicts with a confirmed project constraint', async () => {
    const existing: WorkshopPreference = {
      id: 'pref-1', label: 'Harem-centered relationships', family: 'Content boundaries', meaning: 'Keep this out of the story.', examples: '', timing: '',
      polarity: 'avoid', strength: 'hard', scope: 'project', targetId: null, confirmed: true,
    };
    await render(view({ state: state({ preferences: [existing] }) }));
    await act(async () => exactButton('Add preference').click());
    const form = host.querySelector('.workshop-preference-form')!;
    setValue(form.querySelector<HTMLInputElement>('input[required]')!, existing.label);
    setValue(form.querySelector<HTMLTextAreaElement>('textarea[required]')!, 'Make this a required relationship pattern.');
    const selects = form.querySelectorAll<HTMLSelectElement>('select');
    await act(async () => { selects[0].value = 'want'; selects[0].dispatchEvent(new Event('change', { bubbles: true })); });
    await act(async () => { selects[1].value = 'project'; selects[1].dispatchEvent(new Event('change', { bubbles: true })); });
    await act(async () => (form.querySelector('details summary') as HTMLElement).click());
    const hard = form.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    await act(async () => hard.click());
    await act(async () => (form.querySelector<HTMLButtonElement>('.primary-button')!).click());
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('conflicts with the project’s Never preference');
    expect(mocks.saveWorkshop).not.toHaveBeenCalled();
  });

  it('renders stopped or partial output as recovery text with no complete candidate cards', async () => {
    const stopped = result({ run: run({ status: 'stopped', outputText: 'A useful partial direction.' }), output: null });
    await render(view({ results: [stopped] }));
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(0);
    expect(host.textContent).toContain('A useful partial direction.');
    expect(host.textContent).toContain('not a completed proposal');
    expect(host.textContent).toContain('Generation stopped');
  });

  it('opens a directed relationship from World as a separate saved exploration and retains its scope after reopen', async () => {
    const fixture = relationshipFixture();
    mocks.readDocument.mockImplementation(async (_access, id: string) => fixture.documents.find(document => document.head.documentId === id));
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Explore this relationship').click());
    await waitFor(() => expect(currentView.state.sessions).toHaveLength(2));
    const exploration = currentView.state.sessions.find(item => item.id !== fixture.original.id)!;
    expect(exploration).toMatchObject({ relationshipId: fixture.relationship.id, selectedScope: 'Relationship: Mei → trusts → Repair guild',
      includedDocumentIds: ['mei', 'guild'], workingText: fixture.relationship.description, focusDocumentId: null });
    expect(currentView.state.sessions.find(item => item.id === fixture.original.id)).toEqual(fixture.original);
    expect(currentView.state.relationships).toEqual([fixture.relationship]);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
    const saved = structuredClone(currentView);
    await act(async () => root.unmount());
    root = createRoot(host);
    await render(saved, fixture.project);
    expect(host.querySelector('[aria-label="Relationship being explored"]')?.textContent).toContain(fixture.relationship.uncertainty);
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(mocks.startWorkshop).toHaveBeenCalledOnce());
    expect(mocks.startWorkshop.mock.calls[0][2]).toMatchObject({ sessionId: exploration.id, selectedScope: exploration.selectedScope });
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('refuses a relationship when an endpoint changes after the list was rendered', async () => {
    const fixture = relationshipFixture();
    mocks.readDocument.mockImplementation(async (_access, id: string) => {
      const document = fixture.documents.find(item => item.head.documentId === id)!;
      return { ...document, head: { ...document.head, version: '2' } };
    });
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Explore this relationship').click());
    await waitFor(() => expect(host.textContent).toContain('A participant changed.'));
    expect(currentView.state).toEqual(fixture.view.state);
    expect(mocks.saveWorkshop).not.toHaveBeenCalled();
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('refreshes completed result freshness after a saved relationship edit without generation', async () => {
    const fixture = relationshipFixture();
    fixture.view.state.sessions[0].relationshipId = fixture.relationship.id;
    fixture.view.results = [result()];
    mocks.readWorkshop.mockImplementation(async () => ({ ...structuredClone(currentView), results: currentView.results.map(saved => ({ ...saved,
      stale: currentView.state.relationships[0].description !== fixture.relationship.description })) }));
    await render(fixture.view, fixture.project);
    expect(host.textContent).not.toContain('Stale result');
    await act(async () => exactButton('Review relationship').click());
    const form = host.querySelector('.workshop-relationships form')!;
    await act(async () => setValue(form.querySelector<HTMLTextAreaElement>('textarea[required]')!, 'Mei no longer trusts the guild to protect her sister.'));
    await act(async () => exactButton('Save relationship').click());
    await waitFor(() => expect(host.textContent).toContain('Stale result'));
    expect(mocks.readWorkshop).toHaveBeenCalledTimes(2);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')?.value).toBe(fixture.original.workingText);
  });

  it('ignores delayed relationship sources after switching explorations', async () => {
    const fixture = relationshipFixture();
    const elsewhere = session({ id: 'elsewhere', title: 'Elsewhere', anchorDocumentId: 'workshop-elsewhere', workingText: 'Another draft.' });
    fixture.view.state.sessions.push(elsewhere);
    const pending: Array<() => void> = [];
    mocks.readDocument.mockImplementation((_access, id: string) => new Promise(resolve => pending.push(() => resolve(fixture.documents.find(document => document.head.documentId === id)))));
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Explore this relationship').click());
    await act(async () => exactButton('Elsewhere').click());
    await act(async () => { pending.forEach(resolve => resolve()); });
    await waitFor(() => expect(currentView.state.currentSessionId).toBe('elsewhere'));
    expect(currentView.state.sessions).toEqual(fixture.view.state.sessions);
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')?.value).toBe('Another draft.');
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });

  it('requires an explicit adoption destination for a relationship and carries the working text into its preview', async () => {
    const fixture = relationshipFixture();
    const scoped = { ...fixture.original, lens: 'people' as const, relationshipId: fixture.relationship.id, workingTitle: 'Mei and the guild',
      focusDocumentId: 'mei', workingText: 'Mei trusts the guild with repairs, but keeps her sister outside its influence.' };
    await render(view({ state: state({ sessions: [scoped], relationships: [fixture.relationship] }) }), fixture.project);
    await act(async () => exactButton('Use this version').click());
    expect(exactButton('Preview all changes').disabled).toBe(true);
    expect(host.querySelectorAll('.workshop-adoption fieldset')).toHaveLength(0);
    expect(mocks.previewWorkshopAdoption).not.toHaveBeenCalled();
    await act(async () => exactButton('Choose a destination').click());
    await act(async () => exactButton('Preview all changes').click());
    await waitFor(() => expect(mocks.previewWorkshopAdoption).toHaveBeenCalledOnce());
    const request = mocks.previewWorkshopAdoption.mock.calls[0][0];
    expect(request.targets).toHaveLength(1);
    expect(request.targets[0]).toMatchObject({ expected: null, kind: 'note', title: 'Mei and the guild' });
    expect(request.targets[0].documentId).not.toBe('mei');
    expect(JSON.stringify(request.targets[0].body)).toContain(scoped.workingText);
    expect(mocks.adoptWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('gives brought-in material an explicit element scope while preserving manual work', async () => {
    const fixture = relationshipFixture();
    fixture.view.state.sessions[0].relationshipId = fixture.relationship.id;
    mocks.readDocument.mockResolvedValue(fixture.documents[1]);
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Bring existing notes').click());
    await act(async () => selectValue('Saved material', 'guild'));
    await waitFor(() => expect(currentView.state.sessions[0].selectedScope).toBe('Element: Repair guild'));
    expect(currentView.state.sessions[0]).toMatchObject({ relationshipId: null, focusDocumentId: 'guild', workingText: fixture.original.workingText });
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
    expect(onDocumentsChanged).not.toHaveBeenCalled();
  });

  it('keeps new local edits when a delayed notes read finishes', async () => {
    const fixture = relationshipFixture();
    fixture.view.state.sessions[0].selectedDetails = [];
    let finish!: (document: DocumentRecord) => void;
    mocks.readDocument.mockImplementation(() => new Promise<DocumentRecord>(resolve => { finish = resolve; }));
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Bring existing notes').click());
    await act(async () => selectValue('Saved material', 'guild'));
    await act(async () => setValue(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!, 'My newer manual world draft.'));
    await act(async () => finish(fixture.documents[1]));
    await waitFor(() => expect(currentView.state.sessions[0].workingText).toBe('My newer manual world draft.'));
    expect(currentView.state.sessions[0].focusDocumentId).toBe(null);
    expect(host.textContent).toContain('Your exploration changed while these notes opened.');
  });
  it('makes the Workshop inert during navigation and rejects a composition flush until input ends', async () => {
    await render();
    const workshop = host.querySelector('.story-workshop')!;
    expect(workshop.hasAttribute('inert')).toBe(false);
    await setNavigationBusy(true);
    expect(workshop.hasAttribute('inert')).toBe(true);
    expect(workshop.getAttribute('aria-busy')).toBe('true');
    await setNavigationBusy(false);
    expect(workshop.hasAttribute('inert')).toBe(false);
    expect(workshop.getAttribute('aria-busy')).toBe(null);

    const workbench = host.querySelector('.workshop-workbench')!;
    await act(async () => workbench.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true })));
    await expect(workshopHandle.current!.flush()).rejects.toThrow(/entering the current text/i);
    await act(async () => workbench.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true })));
    await expect(workshopHandle.current!.flush()).resolves.toBeUndefined();
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });
  it('ignores a notes read that crosses navigation busy and resumes local editing after the boundary releases', async () => {
    const fixture = relationshipFixture();
    fixture.view.state.sessions[0].selectedDetails = [];
    let finish!: (document: DocumentRecord) => void;
    mocks.readDocument.mockImplementation(() => new Promise<DocumentRecord>(resolve => { finish = resolve; }));
    await render(fixture.view, fixture.project);
    await act(async () => exactButton('Bring existing notes').click());
    await act(async () => selectValue('Saved material', 'guild'));
    await setNavigationBusy(true);
    expect(host.querySelector('.story-workshop')?.hasAttribute('inert')).toBe(true);
    await act(async () => finish(fixture.documents[1]));
    await setNavigationBusy(false);
    await waitFor(() => expect(host.querySelector('.story-workshop')?.hasAttribute('inert')).toBe(false));
    expect(currentView.state.sessions[0]).toMatchObject({ workingText: fixture.original.workingText, focusDocumentId: null });
    expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')?.value).toBe(fixture.original.workingText);

    const resumedDraft = 'Local draft after navigation releases.';
    await act(async () => setValue(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')!, resumedDraft));
    await waitFor(() => expect(host.querySelector<HTMLTextAreaElement>('.workshop-working-text')?.value).toBe(resumedDraft));
    expect(currentView.state.sessions[0].focusDocumentId).toBe(null);
    expect(mocks.startWorkshop).not.toHaveBeenCalled();
  });
});
