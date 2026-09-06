// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import { projectTabPreferenceKey, writeProjectTabs } from './projectTabs';
import { Workspace } from './Workspace';

const mocks = vi.hoisted(() => {
  const events: string[] = [];
  const sessions: any[] = [];
  const librarySnapshot = vi.fn();
  const libraryOpen = vi.fn();
  const readDocument = vi.fn();
  const runCreateIntent = vi.fn();
  const runtimeInfo = vi.fn();
  const createDocument = vi.fn();
  const reconcileProject = vi.fn();
  const projectTransport = { validate: vi.fn(), save: vi.fn(), checkpoint: vi.fn(), reconcile: vi.fn() };
  function mockDocumentSession(access: ProjectAccess, record: DocumentRecord) {
    const session: any = {
      projectAccess: access,
      record,
      state: { phase: 'editing', editable: true, dirty: false, saving: false, error: null, generation: '0', savedGeneration: '0', head: record.head },
      flush: vi.fn(async () => { events.push(`flush:${record.head.documentId}`); }),
      reconcile: vi.fn(async () => { events.push(`reconcile:${record.head.documentId}`); }),
      detach: vi.fn(async () => { events.push(`detach:${record.head.documentId}`); }),
      withLifecycleGuard: vi.fn(async (work: () => Promise<unknown>) => work()),
    };
    session.detachAfter = vi.fn(async (prepare: () => Promise<unknown>) => {
      events.push(`detach:start:${record.head.documentId}`);
      await session.flush();
      const result = await prepare();
      events.push(`detach:end:${record.head.documentId}`);
      session.state.phase = 'disposed';
      return result;
    });
    sessions.push(session);
    return session;
  }
  const DocumentSession = vi.fn(mockDocumentSession);
  return { events, sessions, librarySnapshot, libraryOpen, readDocument, runCreateIntent, runtimeInfo, createDocument, reconcileProject, projectTransport, DocumentSession };
});

vi.mock('../editor/session', () => ({ DocumentSession: mocks.DocumentSession }));
vi.mock('../ipc/library', () => ({
  librarySnapshot: mocks.librarySnapshot,
  libraryOpen: mocks.libraryOpen,
  libraryCreate: vi.fn(),
  libraryArchive: vi.fn(),
  libraryRecover: vi.fn(),
  libraryDuplicate: vi.fn(),
  libraryResumeImport: vi.fn(),
  projectBackup: vi.fn(),
}));
vi.mock('../ipc/projects', () => ({
  projectTransport: mocks.projectTransport,
  createDocument: mocks.createDocument,
  reconcileProject: mocks.reconcileProject,
  readDocument: mocks.readDocument,
  projectMetadata: vi.fn(),
  renameProject: vi.fn(),
  renameDocument: vi.fn(),
}));
vi.mock('../ipc/createIntent', () => ({
  runCreateIntent: mocks.runCreateIntent,
  CreateIntentRecoveryError: class CreateIntentRecoveryError extends Error {},
  CreateIntentUnresolvedError: class CreateIntentUnresolvedError extends Error {},
}));
vi.mock('../ipc/exports', () => ({ prepareDraftExport: vi.fn(), prepareReviewedDraftExport: vi.fn(), exportPreparedDraft: vi.fn() }));
vi.mock('../ipc/native', () => ({ runtimeInfo: mocks.runtimeInfo }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => false }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: vi.fn() }));
vi.mock('./App', () => ({ App: () => null }));
vi.mock('./ExportDialog', () => ({ ExportDialog: () => null }));
vi.mock('./V2ImportDialog', () => ({ V2ImportDialog: () => null }));
vi.mock('../providers/ModelSelector', () => ({ ModelSelector: () => null }));
vi.mock('../providers/ModelSettings', () => ({ ModelSettings: () => null }));
vi.mock('./Writer', () => ({
  Writer: ({ active, navigation }: any) => <section data-testid="writer">
    <strong>{active.record.title}</strong>
    {navigation?.previous && <button onClick={navigation.previous}>Previous chapter</button>}
    {navigation?.next && <button onClick={navigation.next}>Next chapter</button>}
  </section>,
}));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body = { schemaVersion: 1 as const, body: { type: 'doc' as const, content: [] } };

function record(documentId: string, kind: string, title = documentId): DocumentRecord {
  return { head: { documentId, version: '0', bodyHash: `${documentId}-hash` }, title, kind, metadataVersion: '0', body, lastCheckpointId: null };
}

function opened(projectId: string, documents: DocumentRecord[], title = projectId): OpenedProject {
  return { project: { projectId, title, operationNamespace: access.operationNamespace, formatVersion: 28 }, access: { ...access, projectId }, documents, metadataVersion: '0', viewState: null, libraryWarning: null };
}

let host: HTMLDivElement;
let root: Root;
let currentProject: OpenedProject;

function button(label: string): HTMLButtonElement {
  return [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label) as HTMLButtonElement;
}

function tab(label: string): HTMLButtonElement {
  return [...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find(item => item.textContent?.trim().startsWith(label)) as HTMLButtonElement;
}

async function waitFor(check: () => void): Promise<void> {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try { check(); return; } catch { await act(async () => new Promise(resolve => setTimeout(resolve, 5))); }
  }
  check();
}

async function renderWorkspace(): Promise<void> {
  await act(async () => root.render(<Workspace />));
  await waitFor(() => expect(host.textContent).toContain('Your stories'));
}

async function openCurrentProject(): Promise<void> {
  await act(async () => (host.querySelector<HTMLButtonElement>('.project-open')!).click());
  await waitFor(() => expect(host.querySelector('[data-testid="writer"]')).not.toBeNull());
}

async function clickTab(label: string): Promise<void> {
  await act(async () => tab(label).click());
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localStorage.clear();
  mocks.events.length = 0;
  mocks.sessions.length = 0;
  vi.clearAllMocks();
  mocks.librarySnapshot.mockResolvedValue({ entries: [{ projectId: 'project', title: 'project', path: 'project', archived: false, lastOpened: '2026-09-06T00:00:00Z', missing: false }], pending: [] });
  mocks.libraryOpen.mockImplementation(async () => currentProject);
  mocks.readDocument.mockImplementation(async (_projectAccess: ProjectAccess, documentId: string) => {
    mocks.events.push(`read:${documentId}`);
    const found = currentProject.documents.find(item => item.head.documentId === documentId);
    if (!found) throw new Error(`Missing ${documentId}`);
    return found;
  });
  mocks.runtimeInfo.mockResolvedValue({ editorTrial: false });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  localStorage.clear();
});

describe('Workspace project tabs', () => {
  it('flushes and detaches the current session before replacing it on tab switch', async () => {
    const chapter = record('chapter-1', 'chapter', 'Chapter one');
    const world = record('world-1', 'world', 'The world');
    currentProject = opened('project', [chapter, world], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: chapter.head.documentId } });
    await renderWorkspace(); await openCurrentProject();
    await clickTab('Worldbuilding');
    await waitFor(() => expect(host.textContent).toContain('The world'));
    expect(mocks.events).toEqual(['detach:start:chapter-1', 'flush:chapter-1', 'read:world-1', 'detach:end:chapter-1']);
  });

  it('keeps the old tab and editor when the destination read fails', async () => {
    const chapter = record('chapter-1', 'chapter', 'Chapter one');
    const world = record('world-1', 'world', 'The world');
    currentProject = opened('project', [chapter, world], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: chapter.head.documentId } });
    await renderWorkspace(); await openCurrentProject();
    mocks.readDocument.mockRejectedValueOnce(new Error('Could not read destination.'));
    await clickTab('Worldbuilding');
    await waitFor(() => expect(host.textContent).toContain('Could not read destination.'));
    expect(tab('Chapters').getAttribute('aria-selected')).toBe('true');
    expect(host.querySelector('[data-testid="writer"]')?.textContent).toContain('Chapter one');
  });

  it('does not leave the prior category body visible when the selected tab is empty', async () => {
    const chapter = record('chapter-1', 'chapter', 'Chapter one');
    currentProject = opened('project', [chapter], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: chapter.head.documentId } });
    await renderWorkspace(); await openCurrentProject();
    await clickTab('Characters');
    await waitFor(() => expect(host.textContent).toContain('Find the people at the heart of it.'));
    expect(host.querySelector('[data-testid="writer"]')).toBeNull();
    expect(host.textContent).not.toContain('Chapter one');
  });

  it('restores a project tab and its last document without crossing projects', async () => {
    const chapter = record('chapter-1', 'chapter', 'Chapter one');
    const character = record('character-1', 'character', 'Mira');
    currentProject = opened('project', [chapter, character], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: chapter.head.documentId } });
    await renderWorkspace(); await openCurrentProject();
    await clickTab('Characters'); await waitFor(() => expect(host.textContent).toContain('Mira'));
    await act(async () => button('All projects').click());
    await waitFor(() => expect(host.textContent).toContain('Your stories'));
    await openCurrentProject();
    expect(tab('Characters').getAttribute('aria-selected')).toBe('true');
    expect(host.querySelector('[data-testid="writer"]')?.textContent).toContain('Mira');
    expect(JSON.parse(localStorage.getItem(projectTabPreferenceKey('project'))!)).toMatchObject({ activeTab: 'characters', lastDocumentByTab: { characters: 'character-1' } });
  });

  it('activates the new document category after creation', async () => {
    const chapter = record('chapter-1', 'chapter', 'Chapter one');
    const character = record('character-1', 'character', 'Mira');
    currentProject = opened('project', [chapter], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: chapter.head.documentId } });
    mocks.runCreateIntent.mockResolvedValue({ record: character, access, snapshot: currentProject });
    await renderWorkspace(); await openCurrentProject();
    await act(async () => button('Add').click());
    const kind = host.querySelector<HTMLSelectElement>('#document-kind')!;
    kind.value = 'character';
    await act(async () => kind.dispatchEvent(new Event('change', { bubbles: true })));
    const title = host.querySelector<HTMLInputElement>('#document-title')!;
    title.value = 'Mira';
    await act(async () => title.dispatchEvent(new Event('change', { bubbles: true })));
    await act(async () => button('Create').click());
    await waitFor(() => expect(host.textContent).toContain('Mira'));
    expect(tab('Characters').getAttribute('aria-selected')).toBe('true');
    expect(host.querySelector('[data-testid="writer"]')?.textContent).toContain('Mira');
  });

  it('uses chapter list order for next navigation', async () => {
    const first = record('chapter-1', 'chapter', 'First');
    const second = record('chapter-2', 'chapter', 'Second');
    currentProject = opened('project', [first, second], 'project');
    writeProjectTabs('project', { activeTab: 'chapters', lastDocumentByTab: { chapters: first.head.documentId } });
    await renderWorkspace(); await openCurrentProject();
    await act(async () => button('Next chapter').click());
    await waitFor(() => expect(host.textContent).toContain('Second'));
    expect(mocks.readDocument).toHaveBeenCalledWith(expect.objectContaining({ projectId: 'project' }), 'chapter-2');
  });
});
