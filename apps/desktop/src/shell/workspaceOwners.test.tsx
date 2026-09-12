// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import type { ChatAdoptionTarget } from '../ipc/projectChat';
import type { WorkspaceOperations, WorkspacePresentation } from './workspaceContracts';
import { useDocumentWorkspace } from './documentWorkspace';
import { useLibraryModel } from './libraryModel';

const mocks = vi.hoisted(() => {
  const sessions: any[] = [];
  const createDocument = vi.fn();
  const reconcileProject = vi.fn();
  const readDocument = vi.fn();
  const librarySnapshot = vi.fn();
  const libraryCreate = vi.fn();
  const libraryDuplicate = vi.fn();
  const libraryOpen = vi.fn();
  const DocumentSession = vi.fn(function (access: ProjectAccess, record: DocumentRecord) {
    const session: any = {
      projectAccess: access, body: record.body,
      state: { phase: 'editing', editable: true, head: record.head },
      subscribe: vi.fn(() => () => {}),
      flush: vi.fn(async () => {}),
      reconcile: vi.fn(async () => {}),
      acceptProjectAccess: vi.fn(async (next: ProjectAccess) => { session.projectAccess = next; }),
      withLifecycleGuard: vi.fn(async (work: () => Promise<unknown>) => work()),
    };
    session.detachAfter = vi.fn(async (prepare: () => Promise<unknown>) => {
      await session.flush();
      const result = await prepare();
      session.state.phase = 'disposed';
      return result;
    });
    sessions.push(session);
    return session;
  });
  return { sessions, DocumentSession, createDocument, reconcileProject, readDocument, librarySnapshot, libraryCreate, libraryDuplicate, libraryOpen };
});
vi.mock('../editor', () => ({ DocumentSession: mocks.DocumentSession, canonicalJson: JSON.stringify }));
vi.mock('../ipc/projects', () => ({
  createDocument: mocks.createDocument, reconcileProject: mocks.reconcileProject, readDocument: mocks.readDocument,
  projectTransport: {}, projectMetadata: vi.fn(), renameProject: vi.fn(), renameDocument: vi.fn(),
}));
vi.mock('../ipc/library', () => ({
  librarySnapshot: mocks.librarySnapshot, libraryCreate: mocks.libraryCreate, libraryDuplicate: mocks.libraryDuplicate,
  libraryOpen: mocks.libraryOpen, libraryArchive: vi.fn(), libraryRecover: vi.fn(), libraryResumeImport: vi.fn(), projectBackup: vi.fn(),
}));
vi.mock('../ipc/exports', () => ({ prepareDraftExport: vi.fn(), prepareReviewedDraftExport: vi.fn(), exportPreparedDraft: vi.fn() }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const body = { schemaVersion: 1 as const, body: { type: 'doc' as const, content: [] } };
function record(documentId: string, title = documentId): DocumentRecord {
  return { head: { documentId, version: '0', bodyHash: documentId }, title, kind: 'chapter', metadataVersion: '0', body, lastCheckpointId: null };
}
function opened(projectId: string, documents = [record(projectId + '-chapter')]): OpenedProject {
  return {
    project: { projectId, title: projectId, operationNamespace: projectId + '-namespace', formatVersion: 28 },
    access: { projectId, operationNamespace: projectId + '-namespace', session: 'renderer', writerLease: 'initial-lease' },
    documents, metadataVersion: '0', viewState: null, libraryWarning: null,
  };
}
let root: Root;
let host: HTMLDivElement;
let workspace: ReturnType<typeof useDocumentWorkspace>;
let library: ReturnType<typeof useLibraryModel>;
let operations: WorkspaceOperations;
let jobs: Promise<void>[];
let present: ReturnType<typeof vi.fn<(event: WorkspacePresentation) => void>>;
let flushParticipants: ReturnType<typeof vi.fn<() => Promise<void>>>;
let errors: string[];
function Harness() {
  workspace = useDocumentWorkspace({ rendererId: 'renderer', operations, present, participants: { flush: flushParticipants, refreshConversation: vi.fn() } });
  library = useLibraryModel({ rendererId: 'renderer', operations, present, navigation: workspace.navigation });
  return null;
}
async function mount(project?: OpenedProject) {
  await act(async () => root.render(<Harness />));
  if (project) await act(async () => { await workspace.navigation.replaceProject(async () => project); });
  present.mockClear();
  flushParticipants.mockClear();
}
async function command(action: () => void) {
  await act(async () => { action(); await Promise.all(jobs); });
}
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localStorage.clear();
  vi.clearAllMocks();
  for (const mock of [mocks.createDocument, mocks.reconcileProject, mocks.readDocument, mocks.libraryCreate, mocks.libraryDuplicate, mocks.libraryOpen]) mock.mockReset();
  mocks.sessions.length = 0;
  mocks.librarySnapshot.mockResolvedValue({ entries: [], pending: [] });
  jobs = []; errors = [];
  present = vi.fn();
  flushParticipants = vi.fn(async () => {});
  operations = {
    perform: work => {
      const job = work().catch(error => { errors.push(error instanceof Error ? error.message : String(error)); });
      jobs.push(job); return job;
    },
    exclusive: async (_message, work) => work(),
    notice: vi.fn(), error: message => { errors.push(message); },
  };
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('document workspace ownership', () => {
  it('keeps the editor and presentation when its save fails before replacement', async () => {
    await mount(opened('first'));
    const session = workspace.currentSession()!;
    const prepare = vi.fn(async () => opened('second'));
    vi.mocked(session.flush).mockRejectedValueOnce(new Error('Save refused'));
    await act(async () => { await expect(workspace.navigation.replaceProject(prepare)).rejects.toThrow('Save refused'); });
    expect(prepare).not.toHaveBeenCalled();
    expect(workspace.currentSession()).toBe(session);
    expect(session.state.phase).toBe('editing');
    expect(workspace.view.project?.project.projectId).toBe('first');
    expect(present).not.toHaveBeenCalled();
  });

  it('retains the editor after a deferred destination failure and publishes only after successful preparation', async () => {
    await mount(opened('first'));
    const session = workspace.currentSession()!;
    const failed = deferred<OpenedProject>();
    let rejected!: Promise<unknown>;
    await act(async () => { rejected = workspace.navigation.replaceProject(() => failed.promise).catch(error => error); });
    expect(session.state.phase).toBe('editing');
    expect(present).not.toHaveBeenCalled();
    await act(async () => { failed.reject(new Error('Open cancelled')); await rejected; });
    expect(workspace.currentSession()).toBe(session);
    expect(present).not.toHaveBeenCalled();
    const success = deferred<OpenedProject>();
    let pending!: Promise<OpenedProject>;
    await act(async () => { pending = workspace.navigation.replaceProject(() => success.promise); });
    expect(present).not.toHaveBeenCalled();
    await act(async () => { success.resolve(opened('second')); await pending; });
    expect(session.state.phase).toBe('disposed');
    expect(workspace.view.project?.project.projectId).toBe('second');
    expect(present.mock.calls).toEqual([[{ kind: 'activated' }]]);
  });

  it('does not clear the project when returning to the library cannot refresh its catalog', async () => {
    await mount(opened('first'));
    const session = workspace.currentSession();
    await act(async () => { await expect(workspace.navigation.returnToLibrary(async () => { throw new Error('Catalog unavailable'); })).rejects.toThrow('Catalog unavailable'); });
    expect(workspace.currentSession()).toBe(session);
    expect(workspace.view.project?.project.projectId).toBe('first');
    expect(present).not.toHaveBeenCalled();
  });

  it('ignores a late library import callback after a project has opened', async () => {
    await mount();
    await command(() => library.actions.openImport());
    const onImported = library.importDialog!.onImported;
    await act(async () => { await workspace.navigation.replaceProject(async () => opened('chosen')); });
    const session = workspace.currentSession();
    present.mockClear();
    await act(async () => { onImported(opened('late-import')); });
    expect(workspace.view.project?.project.projectId).toBe('chosen');
    expect(workspace.currentSession()).toBe(session);
    expect(present).not.toHaveBeenCalled();
  });

  it('refuses a deferred project replacement that lost its source identity', async () => {
    await mount(opened('first'));
    const late = deferred<OpenedProject>();
    let result!: Promise<unknown>;
    await act(async () => { result = workspace.navigation.replaceProject(() => late.promise).catch(error => error); });
    await act(async () => { await workspace.navigation.replaceProject(async () => opened('chosen')); });
    const session = workspace.currentSession();
    present.mockClear();
    await act(async () => { late.resolve(opened('late')); expect(await result).toMatchObject({ message: 'The project changed while the destination was opening. Open it again.' }); });
    expect(workspace.view.project?.project.projectId).toBe('chosen');
    expect(workspace.currentSession()).toBe(session);
    expect(present).not.toHaveBeenCalled();
  });

  it.each(['chosen', 'first'])('fences a late access acknowledgment after replacing the source with project %s', async destination => {
    await mount(opened('first'));
    const old = workspace.currentSession()!;
    const ack = deferred<void>();
    vi.mocked(old.acceptProjectAccess).mockImplementationOnce(async access => { await ack.promise; Object.assign(old, { projectAccess: access }); });
    let pending!: Promise<void>;
    await act(async () => { pending = workspace.actions.acceptChatAccess({ ...old.projectAccess, writerLease: 'late-lease' }); });
    await act(async () => { await workspace.navigation.replaceProject(async () => opened(destination)); });
    const session = workspace.currentSession();
    await act(async () => { ack.resolve(); await pending; });
    expect(workspace.view.project?.project.projectId).toBe(destination);
    expect(workspace.view.project?.access.writerLease).toBe('initial-lease');
    expect(workspace.currentSession()).toBe(session);
  });

  it('does not apply a prepared adoption read after another project has activated', async () => {
    const first = opened('first');
    await mount(first);
    await act(async () => { await workspace.actions.prepareChatAdoption([{ documentId: first.documents[0].head.documentId } as ChatAdoptionTarget]); });
    const read = deferred<DocumentRecord>();
    mocks.readDocument.mockReturnValueOnce(read.promise);
    let pending!: Promise<void>;
    await act(async () => { pending = workspace.actions.acceptChatDocuments(first.documents); });
    await act(async () => { await workspace.navigation.replaceProject(async () => opened('chosen')); });
    const session = workspace.currentSession();
    await act(async () => { read.resolve({ ...first.documents[0], title: 'Late adoption' }); await pending; });
    expect(workspace.view.project?.project.projectId).toBe('chosen');
    expect(workspace.currentSession()).toBe(session);
    expect(workspace.view.project?.documents).toEqual(opened('chosen').documents);
  });

  it.each(['replaceProject', 'returnToLibrary'] as const)('keeps an adoption-restored editor when %s started while detached', async destination => {
    const first = opened('first');
    await mount(first);
    const source = workspace.currentSession()!;
    await act(async () => { await workspace.actions.prepareChatAdoption([{ documentId: first.documents[0].head.documentId } as ChatAdoptionTarget]); });
    expect(source.state.phase).toBe('disposed');
    expect(workspace.currentSession()).toBeNull();

    // Adoption is awaiting its receipt with no mounted session. A project
    // picker or catalog refresh can start now, then remain pending while the
    // receipt rereads Working and restores the editor in this same project.
    const preparation = deferred<void>();
    const prepare = vi.fn(() => preparation.promise);
    let navigation!: Promise<unknown>;
    await act(async () => {
      navigation = (destination === 'replaceProject'
        ? workspace.navigation.replaceProject(async () => { await prepare(); return opened('other'); })
        : workspace.navigation.returnToLibrary(prepare)).catch(error => error);
    });
    expect(prepare).toHaveBeenCalledOnce();
    const adopted = { ...first.documents[0], title: 'Adopted chapter', head: { ...first.documents[0].head, version: '1' } };
    mocks.readDocument.mockResolvedValueOnce(adopted);
    await act(async () => { await workspace.actions.acceptChatDocuments([adopted]); });
    const restored = workspace.currentSession();
    expect(restored).not.toBeNull();
    expect(restored).not.toBe(source);
    expect(workspace.view.active?.record).toEqual(adopted);

    let result: unknown;
    await act(async () => { preparation.resolve(); result = await navigation; });
    expect(workspace.view.project?.project.projectId).toBe('first');
    expect(workspace.currentSession()).toBe(restored);
    expect(workspace.view.active?.record).toEqual(adopted);
    expect(present).not.toHaveBeenCalled();
    if (destination === 'replaceProject') expect(result).toMatchObject({ message: 'The project changed while the destination was opening. Open it again.' });
    else expect(result).toBeUndefined();
  });

  it('retains the exact uncertain chapter intent and uses the mounted writer lease after reconciliation', async () => {
    const first = opened('first');
    await mount(first);
    const session = workspace.currentSession()!;
    vi.mocked(session.reconcile).mockImplementation(async () => { Object.assign(session, { projectAccess: { ...first.access, writerLease: 'editor-reconciled' } }); });
    mocks.createDocument.mockRejectedValueOnce(new Error('Lost acknowledgment')).mockRejectedValueOnce(new Error('Still uncertain'))
      .mockImplementationOnce(async (_access, intent) => ({ ...record(intent.documentId, intent.title), body: intent.body }));
    mocks.reconcileProject.mockRejectedValueOnce(new Error('Recovery offline')).mockResolvedValueOnce({ ...first, access: { ...first.access, writerLease: 'snapshot-lease' } });
    await act(async () => { await expect(workspace.actions.prepareChatChapter(null, 'Chapter two')).rejects.toThrow('The chapter was not prepared'); });
    expect(workspace.currentSession()).toBe(session);
    await act(async () => { await expect(workspace.actions.prepareChatChapter(null, 'Different chapter')).rejects.toThrow('The chapter was not prepared'); });
    expect(mocks.createDocument).toHaveBeenCalledTimes(1);
    await act(async () => { await workspace.actions.prepareChatChapter(null, 'Chapter two'); });
    expect(mocks.createDocument).toHaveBeenCalledTimes(3);
    const intents = mocks.createDocument.mock.calls.map(call => call[1]);
    expect(intents[1]).toEqual(intents[0]);
    expect(intents[2]).toEqual(intents[0]);
    expect(mocks.createDocument.mock.calls[2][0].writerLease).toBe('editor-reconciled');
    expect(workspace.view.project?.access.writerLease).toBe('editor-reconciled');
    expect(workspace.view.active?.record.head.documentId).toBe(intents[0].documentId);
    expect(errors).toContain('Finish the pending document creation before preparing another chapter.');
  });
});

describe('library operation ownership', () => {
  it('keeps one create identity across a failed attempt, resetting the form and focus only after success', async () => {
    await mount();
    await command(() => { library.actions.setNewProject(true); library.actions.setTitle('My story'); });
    mocks.libraryCreate.mockRejectedValueOnce(new Error('Create acknowledgment lost')).mockResolvedValueOnce(opened('created'));
    await command(() => library.actions.create(' My story '));
    expect(library.view.title).toBe('My story');
    expect(present).not.toHaveBeenCalled();
    await command(() => library.actions.create('My story'));
    expect(mocks.libraryCreate.mock.calls[1][0]).toBe(mocks.libraryCreate.mock.calls[0][0]);
    expect(mocks.libraryCreate.mock.calls[1].slice(1)).toEqual(['My story', 'renderer']);
    expect(library.view.title).toBe('');
    expect(workspace.view.project?.project.projectId).toBe('created');
    expect(present.mock.calls).toEqual([[{ kind: 'activated' }], [{ kind: 'focus', selector: '.app-header .brand strong', projectId: 'created' }]]);
  });

  it('duplicates with the lease produced by the document save barrier', async () => {
    const first = opened('first');
    await mount(first);
    const session = workspace.currentSession()!;
    vi.mocked(session.flush).mockImplementation(async () => { Object.assign(session, { projectAccess: { ...first.access, writerLease: 'saved-lease' } }); });
    mocks.libraryDuplicate.mockResolvedValueOnce(opened('copy'));
    await command(() => library.actions.duplicate());
    expect(mocks.libraryDuplicate.mock.calls[0][1]).toEqual({ ...first.access, writerLease: 'saved-lease' });
    expect(workspace.view.project?.project.projectId).toBe('copy');
  });

  it('does not duplicate from a stale captured project when the preparation source changes', async () => {
    await mount(opened('first'));
    const barrier = deferred<void>();
    flushParticipants.mockReturnValueOnce(barrier.promise);
    await act(async () => { library.actions.duplicate(); });
    await act(async () => { await workspace.navigation.replaceProject(async () => opened('chosen')); });
    await act(async () => { barrier.resolve(); await Promise.all(jobs); });
    expect(mocks.libraryDuplicate).not.toHaveBeenCalled();
    expect(workspace.view.project?.project.projectId).toBe('chosen');
    expect(errors).toContain('The project changed while the copy was being prepared. Try again.');
  });
});
