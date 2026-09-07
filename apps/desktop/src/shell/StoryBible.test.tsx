// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import type { Revision, DocumentRecord, Head, OpenedProject, ProjectAccess } from '../ipc/projects';
import * as projects from '../ipc/projects';
import * as history from '../ipc/history';
import * as workshop from '../ipc/workshop';
import type { WorkshopDecision, WorkshopView } from '../ipc/workshop';
import { StoryBible } from './StoryBible';

vi.mock('../ipc/projects', () => ({ readDocument: vi.fn() }));
vi.mock('../ipc/history', () => ({ readDocumentRevision: vi.fn() }));
vi.mock('../ipc/workshop', () => ({ readWorkshop: vi.fn(), saveWorkshop: vi.fn() }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const project: OpenedProject = {
  project: { projectId: access.projectId, operationNamespace: access.operationNamespace, title: 'Test project', formatVersion: 1 },
  access, documents: [], metadataVersion: 'metadata', viewState: null, libraryWarning: null,
};

const textBody = (text: string): WnsDocument => ({
  schemaVersion: 1,
  body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph' }, ...(text ? { content: [{ type: 'text' as const, text }] } : {}) }] },
});

async function head(documentId: string, version: string, body: WnsDocument): Promise<Head> {
  return { documentId, version, bodyHash: await bodyHash(canonicalJson(body)) };
}

function decision(savedHead: Head, overrides: Partial<WorkshopDecision> = {}): WorkshopDecision {
  return {
    id: `decision-${savedHead.documentId}`,
    sessionId: 'session', title: `Chosen ${savedHead.documentId}`, documentId: savedHead.documentId,
    revisionId: `revision-${savedHead.documentId}`, head: savedHead, candidateIds: [], rationale: 'Author chose this version.',
    status: 'chosen', fixed: false, protectedText: [], access: 'authorRoom', supersedesId: null, ...overrides,
  };
}

function view(decisions: WorkshopDecision[]): WorkshopView {
  return {
    version: '1',
    state: { schemaVersion: 1, currentSessionId: 'session', sessions: [], preferences: [], decisions, relationships: [], impacts: [], presets: [] },
    results: [],
  };
}

function record(savedHead: Head, title: string, body: WnsDocument): DocumentRecord {
  return { head: savedHead, title, kind: 'note', metadataVersion: 'metadata', body, lastCheckpointId: null };
}

function revision(id: string, savedHead: Head, body: WnsDocument): Revision {
  return { id, head: savedHead, body, reason: 'manual', parentId: null };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(accept => { resolve = accept; });
  return { promise, resolve };
}

let host: HTMLDivElement;
let root: Root;
let documents: Map<string, DocumentRecord>;

async function render(value: OpenedProject = project, onClose = vi.fn(), onOpenDocument = vi.fn()) {
  await act(async () => root.render(<StoryBible project={value} onClose={onClose} onOpenDocument={onOpenDocument} />));
  return { onClose, onOpenDocument };
}

async function settle(assertion: () => void) {
  await vi.waitFor(async () => { await act(async () => {}); assertion(); });
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.open = true; } });
  documents = new Map();
  vi.mocked(projects.readDocument).mockImplementation(async (_value, documentId) => {
    const found = documents.get(documentId);
    if (!found) throw { code: 'DocumentNotFound', detail: `Document ${documentId} is unavailable.` };
    return found;
  });
  vi.mocked(history.readDocumentRevision).mockRejectedValue(new Error('Revision unavailable'));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('chosen Story Bible material', () => {
  it('shows an exact current source and opens the selected document', async () => {
    const body = textBody('Current accepted prose.'); const savedHead = await head('chapter', '3', body);
    documents.set('chapter', record(savedHead, 'Chapter source', body));
    vi.mocked(workshop.readWorkshop).mockResolvedValue(view([decision(savedHead, { revisionId: 'revision-chapter' })]));
    const { onOpenDocument } = await render();
    await settle(() => expect(host.textContent).toContain('Current accepted prose.'));
    expect(history.readDocumentRevision).not.toHaveBeenCalled();
    const open = [...host.querySelectorAll('button')].find(item => item.textContent === 'Open source: Chapter source')! as HTMLButtonElement;
    await act(async () => open.click());
    expect(onOpenDocument).toHaveBeenCalledExactlyOnceWith('chapter');
  });

  it('reads the exact accepted historical revision after the source changes', async () => {
    const accepted = textBody('Accepted historical prose.'); const current = textBody('New unrelated prose.');
    const savedHead = await head('chapter', '3', accepted); const currentHead = await head('chapter', '4', current);
    documents.set('chapter', record(currentHead, 'Chapter source', current));
    const chosen = decision(savedHead, { revisionId: 'revision-chapter' });
    vi.mocked(workshop.readWorkshop).mockResolvedValue(view([chosen]));
    vi.mocked(history.readDocumentRevision).mockResolvedValue(revision('revision-chapter', savedHead, accepted));
    const { onOpenDocument } = await render();
    await settle(() => expect(host.textContent).toContain('Accepted historical prose.'));
    expect(host.textContent).not.toContain('New unrelated prose.');
    expect(host.textContent).toContain('showing the saved version');
    expect(history.readDocumentRevision).toHaveBeenCalledExactlyOnceWith(access, 'chapter', 'revision-chapter');
    expect([...host.querySelectorAll('button')].some(item => item.textContent === 'Open current source: Chapter source')).toBe(true);
    expect(onOpenDocument).not.toHaveBeenCalled();
  });

  it('keeps valid chosen material visible when another source is missing', async () => {
    const keptBody = textBody('Still readable accepted material.'); const keptHead = await head('kept', '1', keptBody);
    const missingBody = textBody('Never shown.'); const missingHead = await head('missing', '2', missingBody);
    documents.set('kept', record(keptHead, 'Kept source', keptBody));
    vi.mocked(workshop.readWorkshop).mockResolvedValue(view([
      decision(keptHead, { title: 'Kept choice' }),
      decision(missingHead, { title: 'Missing choice', rationale: 'Keep the vanished source for review.' }),
    ]));
    await render();
    await settle(() => expect(host.textContent).toContain('Still readable accepted material.'));
    expect(host.textContent).toContain('Missing choice');
    expect(host.textContent).toContain('Saved version unavailable');
    expect(host.textContent).toContain('Keep the vanished source for review.');
    expect([...host.querySelectorAll('button')].some(item => item.textContent?.includes('Missing choice'))).toBe(false);
  });

  it('does not substitute current prose for an unavailable or mismatched accepted revision', async () => {
    const accepted = textBody('Accepted but unavailable prose.'); const current = textBody('Current replacement must stay hidden.');
    const savedHead = await head('chapter', '3', accepted); const currentHead = await head('chapter', '4', current);
    documents.set('chapter', record(currentHead, 'Chapter source', current));
    const chosen = decision(savedHead, { revisionId: 'revision-chapter', rationale: 'Preserve the accepted wording.' });
    vi.mocked(workshop.readWorkshop).mockResolvedValue(view([chosen]));
    vi.mocked(history.readDocumentRevision).mockResolvedValue(revision('wrong-revision', currentHead, current));
    await render();
    await settle(() => expect(host.textContent).toContain('Saved version unavailable'));
    expect(host.textContent).not.toContain('Current replacement must stay hidden.');
    expect(host.textContent).not.toContain('Accepted but unavailable prose.');
    expect(host.textContent).toContain('Preserve the accepted wording.');
    expect([...host.querySelectorAll('button')].some(item => item.textContent?.startsWith('Open'))).toBe(false);
  });

  it('shows a retained historical version after the current source is deleted without offering a live open action', async () => {
    const accepted = textBody('Retained after deletion.'); const savedHead = await head('deleted', '1', accepted);
    const chosen = decision(savedHead, { title: 'Deleted source', revisionId: 'revision-deleted' });
    vi.mocked(workshop.readWorkshop).mockResolvedValue(view([chosen]));
    vi.mocked(history.readDocumentRevision).mockResolvedValue(revision('revision-deleted', savedHead, accepted));
    await render();
    await settle(() => expect(host.textContent).toContain('Retained after deletion.'));
    expect(host.textContent).toContain('Current source unavailable; showing the saved version.');
    expect([...host.querySelectorAll('button')].some(item => item.textContent?.startsWith('Open'))).toBe(false);
  });

  it('drops a late read from a prior project and restores focus on unmount', async () => {
    const old = deferred<WorkshopView>(); const oldBody = textBody('Old project response.'); const oldHead = await head('old', '1', oldBody);
    const nextBody = textBody('New project response.'); const nextHead = await head('new', '1', nextBody);
    documents.set('new', record(nextHead, 'New source', nextBody));
    vi.mocked(workshop.readWorkshop).mockReturnValueOnce(old.promise).mockResolvedValueOnce(view([decision(nextHead, { title: 'New choice' })]));
    const trigger = document.createElement('button'); document.body.append(trigger); trigger.focus();
    const otherProject: OpenedProject = { ...project, project: { ...project.project, projectId: 'other' }, access: { ...access, projectId: 'other' } };
    await render(); await render(otherProject);
    await settle(() => expect(host.textContent).toContain('New project response.'));
    await act(async () => old.resolve(view([decision(oldHead, { title: 'Old choice' })])));
    expect(host.textContent).not.toContain('Old project response.');
    await act(async () => root.unmount());
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
    root = createRoot(host);
  });
});
