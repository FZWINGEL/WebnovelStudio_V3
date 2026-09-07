// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CompiledPacket } from '../ipc/context';
import type { DiscussionRun, DiscussionStart } from '../ipc/discussions';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import type {
  WorkshopCandidate, WorkshopPreference, WorkshopResult, WorkshopSession, WorkshopSnapshot,
  WorkshopState, WorkshopView,
} from '../ipc/workshop';
import { Workshop } from './Workshop';

const mocks = vi.hoisted(() => ({
  readWorkshop: vi.fn(),
  saveWorkshop: vi.fn(),
  workshopHistory: vi.fn(),
  startWorkshop: vi.fn(),
  previewWorkshopAdoption: vi.fn(),
  adoptWorkshop: vi.fn(),
  readDocument: vi.fn(),
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
vi.mock('../ipc/discussions', () => ({ retryDiscussionSave: mocks.retryDiscussionSave, stopDiscussion: mocks.stopDiscussion }));
vi.mock('../providers/ProviderContext', () => ({ useProviders: () => mocks.providers }));
vi.mock('../assistant/ContextInspector', () => ({ ContextInspector: () => null }));
vi.mock('../workshop/Relationships', () => ({ Relationships: () => null }));

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

let host: HTMLDivElement;
let root: Root;
let currentView: WorkshopView;
let savedDocuments: DocumentRecord[];
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

async function render(value = currentView): Promise<void> {
  currentView = value;
  await act(async () => root.render(<Workshop project={project} onOpenDocument={vi.fn()} onDocumentsChanged={onDocumentsChanged} onError={vi.fn()} />));
  await waitFor(() => expect(host.textContent).toContain('Develop or edit directly'));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  window.innerWidth = 1300;
  vi.clearAllMocks();
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
  mocks.previewWorkshopAdoption.mockImplementation(async (request: { sessionId: string; expectedVersion: string; targets: unknown[]; rationale: string; candidateIds: string[] }) => ({
    id: 'preview-1', sessionId: request.sessionId, expectedVersion: request.expectedVersion, targets: request.targets, before: [], rationale: request.rationale, protectedText: [], candidateIds: request.candidateIds,
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
  it('reconciles an uncertain request with the same identity even after going offline', async () => {
    mocks.startWorkshop.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'Acknowledgment lost.' });
    await render();
    await act(async () => exactButton('Explore').click());
    await waitFor(() => expect(host.textContent).toContain('Acknowledgment lost.'));
    const original = structuredClone(mocks.startWorkshop.mock.calls[0]);
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
});
