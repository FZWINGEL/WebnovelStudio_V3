// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import type { Revision, DocumentRecord, Head, OpenedProject, ProjectAccess } from '../ipc/projects';
import * as history from '../ipc/history';
import type { DiscussionRun } from '../ipc/discussions';
import type { WorkshopCandidate, WorkshopDecision, WorkshopRelationship, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { BranchComparison } from './BranchComparison';

vi.mock('../ipc/history', () => ({ readDocumentRevision: vi.fn() }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const textBody = (text: string): WnsDocument => ({
  schemaVersion: 1,
  body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph' }, ...(text ? { content: [{ type: 'text' as const, text }] } : {}) }] },
});
const session = (id: string, parentSessionId: string | null = null): WorkshopSession => ({
  id, title: id === 'parent' ? 'Working exploration' : 'Alternate exploration', lens: 'overview', parentSessionId,
  branchKind: parentSessionId ? 'whatIf' : 'working', brief: 'A seed', direction: '', stillOpen: '', focusQuestion: '', focusReason: '',
  focusDocumentId: null, anchorDocumentId: `workshop-${id}`, depth: 'sketch', outsideDirection: false, includedDocumentIds: [],
  workingText: id === 'parent' ? 'Parent working passage.' : 'Alternate working passage.', workingTitle: '', workingGeneration: '1',
  selectedDetails: [], choices: [], questions: [], composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null,
});
const candidate = (id: string, affectedTargets: WorkshopCandidate['affectedTargets'] = []): WorkshopCandidate => ({
  id, title: `Candidate ${id}`, content: `Content ${id}`, dimensionValue: 'A dimension', implications: [], assumptions: [], affectedTargets,
  preservedDetails: [], changedDetails: [],
});
const run = (id: string, overrides: Partial<DiscussionRun> = {}): DiscussionRun => ({
  id, threadId: id, owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, runId: id }, operationId: id, intent: 'discuss',
  payloadHash: 'payload', target: { documentId: 'anchor', version: '1', bodyHash: 'anchor-hash' }, packetId: `packet-${id}`, previousRunId: null,
  status: 'completed', dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null, createdAt: '2026-09-07T00:00:00Z', updatedAt: '2026-09-07T00:00:00Z', ...overrides,
});
const result = (sessionId: string, id: string, values: WorkshopCandidate[], overrides: Partial<WorkshopResult> = {}): WorkshopResult => ({
  run: run(id), sessionId, workingGeneration: '1', action: 'directions', output: {
    schemaVersion: 'story-workshop-output.v1', requestKind: 'directions', question: 'Question', questionReason: 'Reason', dimension: 'Dimension',
    interpretation: { youSaid: 'Said', possibleDirection: 'Direction', stillOpen: 'Open' }, candidates: values,
  }, validationError: null, stale: false, ...overrides,
});
const state = (sessions: WorkshopSession[], decisions: WorkshopDecision[] = [], relationships: WorkshopRelationship[] = [], impacts: WorkshopState['impacts'] = []): WorkshopState => ({
  schemaVersion: 1, currentSessionId: sessions.at(-1)?.id ?? null, sessions, preferences: [], decisions, relationships, impacts, presets: [],
});
const documentRecord = (head: Head, title: string, body: WnsDocument): DocumentRecord => ({ head, title, kind: 'note', metadataVersion: 'metadata', body, lastCheckpointId: null });
const projectFor = (documents: DocumentRecord[]): OpenedProject => ({ project: { projectId: access.projectId, operationNamespace: access.operationNamespace, title: 'Project', formatVersion: 1 }, access, documents, metadataVersion: 'metadata', viewState: null, libraryWarning: null });
async function head(documentId: string, version: string, body: WnsDocument): Promise<Head> { return { documentId, version, bodyHash: await bodyHash(canonicalJson(body)) }; }
function decision(sessionId: string, documentId: string, savedHead: Head, overrides: Partial<WorkshopDecision> = {}): WorkshopDecision {
  return { id: `decision-${sessionId}-${documentId}-${savedHead.version}`, sessionId, title: `Chosen ${documentId}`, documentId, revisionId: `revision-${documentId}-${savedHead.version}`, head: savedHead, candidateIds: [], rationale: 'Preserve this source.', status: 'chosen', fixed: false, protectedText: [], access: 'authorRoom', supersedesId: null, ...overrides };
}
function revision(id: string, savedHead: Head, body: WnsDocument): Revision { return { id, head: savedHead, body, reason: 'manual', parentId: null }; }
async function settle(assertion: () => void) { await vi.waitFor(async () => { await act(async () => {}); assertion(); }); }

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  vi.mocked(history.readDocumentRevision).mockRejectedValue(new Error('Revision unavailable'));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

async function render(props: Parameters<typeof BranchComparison>[0]) { await act(async () => root.render(<BranchComparison {...props} />)); }
async function openComparison() {
  const details = host.querySelector('.workshop-branch-comparison') as HTMLDetailsElement;
  await act(async () => { details.open = true; details.dispatchEvent(new Event('toggle')); });
}

describe('BranchComparison', () => {
  it('shows changed fields, selected evidence, related links, and no invented effects', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id); alternate.focusDocumentId = 'alternate-doc'; alternate.lens = 'world';
    alternate.selectedDetails = [{ id: 'detail', candidateId: 'choice', text: 'A chosen detail', fixed: false }];
    const body = textBody('Current world source.'); const sourceHead = await head('world-doc', '1', body); const alternateHead = await head('alternate-doc', '1', body);
    const relationship: WorkshopRelationship = { id: 'relationship', fromDocumentId: 'alternate-doc', toDocumentId: 'world-doc', type: 'pressures', description: 'The alternate pressure strains the guild.', uncertainty: 'Needs author review.', status: 'tentative', sourceHeads: [alternateHead, sourceHead] };
    const decisions = [decision(parent.id, 'world-doc', sourceHead)];
    const props = { project: projectFor([documentRecord(sourceHead, 'World notes', body), documentRecord(alternateHead, 'Alternate notes', body)]), state: state([parent, alternate], decisions, [relationship]), session: alternate, results: [result(alternate.id, 'run', [candidate('choice', [{ documentId: 'world-doc', reason: 'The guild pressure may need reconciliation.' }])])], onOpenDocument: vi.fn() };
    await render(props);
    expect(history.readDocumentRevision).not.toHaveBeenCalled();
    expect(host.textContent).toContain('Parent working passage.'); expect(host.textContent).toContain('Alternate working passage.');
    expect(host.textContent).toContain('Changed fields'); expect(host.textContent).toContain('Likely affected material');
    expect(host.textContent).toContain('The guild pressure may need reconciliation.'); expect(host.textContent).toContain('The alternate pressure strains the guild.');
    expect(host.textContent).not.toContain('This comparison does not infer an effect');
    await openComparison(); await settle(() => expect(history.readDocumentRevision).toHaveBeenCalled());
  });

  it('loads exact chosen revisions on open and keeps current prose out when the saved head differs', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id);
    const accepted = textBody('Accepted historical source.'); const current = textBody('Current replacement source.');
    const savedHead = await head('source', '1', accepted); const currentHead = await head('source', '2', current);
    const chosen = decision(parent.id, 'source', savedHead, { revisionId: 'revision-source-1' });
    const props = { project: projectFor([documentRecord(currentHead, 'Source notes', current)]), state: state([parent, alternate], [chosen]), session: alternate, results: [], onOpenDocument: vi.fn() };
    vi.mocked(history.readDocumentRevision).mockResolvedValue(revision('revision-source-1', savedHead, accepted));
    await render(props); await openComparison();
    await settle(() => expect(host.textContent).toContain('Accepted historical source.'));
    expect(host.textContent).not.toContain('Current replacement source.'); expect(host.textContent).toContain('current source has changed');
    expect(host.textContent).toContain('Open current source: Source notes');
    const revisedDecision = { ...chosen, title: 'Updated source choice', rationale: 'Updated author rationale.' };
    await render({ ...props, state: state([parent, alternate], [revisedDecision]) });
    expect(host.textContent).toContain('Updated source choice'); expect(host.textContent).toContain('Updated author rationale.');
    expect(history.readDocumentRevision).toHaveBeenCalledTimes(1);
    await act(async () => [...host.querySelectorAll('button')].find(item => item.textContent === 'Open current source: Source notes')!.click());
    expect(props.onOpenDocument).toHaveBeenCalledExactlyOnceWith('source');
  });

  it('isolates an unavailable exact revision while retaining a readable sibling source', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id); const body = textBody('Readable source.');
    const keptHead = await head('kept', '1', body); const missingHead = await head('missing', '3', textBody('Missing source body.'));
    const kept = decision(parent.id, 'kept', keptHead, { title: 'Kept source', revisionId: 'revision-kept' });
    const missing = decision(parent.id, 'missing', missingHead, { title: 'Missing source', revisionId: 'revision-missing', rationale: 'Keep this provenance visible.' });
    vi.mocked(history.readDocumentRevision).mockImplementation(async (_access, documentId) => documentId === 'kept' ? revision('revision-kept', keptHead, body) : Promise.reject(new Error('Gone')));
    const props = { project: projectFor([documentRecord(keptHead, 'Kept notes', body)]), state: state([parent, alternate], [kept, missing]), session: alternate, results: [], onOpenDocument: vi.fn() };
    await render(props); await openComparison();
    await settle(() => expect(host.textContent).toContain('Readable source.'));
    expect(host.textContent).toContain('Missing source'); expect(host.textContent).toContain('Saved source unavailable'); expect(host.textContent).toContain('Keep this provenance visible.');
    expect(host.textContent).not.toContain('Missing source body.'); expect([...host.querySelectorAll('button')].some(item => item.textContent?.includes('Missing source'))).toBe(false);
  });

  it('labels stale selected evidence and nested parents without substituting unrelated results', async () => {
    const rootSession = session('root'); const parent = session('parent', rootSession.id); const alternate = session('alternate', parent.id);
    alternate.selectedDetails = [{ id: 'detail', candidateId: 'selected', text: 'Selected', fixed: false }];
    const sourceBody = textBody('Source.'); const sourceHead = await head('source', '1', sourceBody);
    const props = { project: projectFor([documentRecord(sourceHead, 'Source', sourceBody)]), state: state([rootSession, parent, alternate]), session: alternate, results: [result(parent.id, 'stale-run', [candidate('selected', [{ documentId: 'source', reason: 'Stale selected evidence.' }]), candidate('unrelated', [{ documentId: 'source', reason: 'Unrelated must stay hidden.' }])], { stale: true })], onOpenDocument: vi.fn() };
    await render(props);
    expect(host.textContent).toContain('Nested what-if'); expect(host.textContent).toContain('Stale selected evidence.'); expect(host.textContent).toContain('Stale evidence');
    expect(host.textContent).not.toContain('Unrelated must stay hidden.');
  });

  it('reports absent evidence instead of inferring a semantic effect', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id); alternate.workingText = parent.workingText;
    await render({ project: projectFor([]), state: state([parent, alternate]), session: alternate, results: [], onOpenDocument: vi.fn() });
    expect(host.textContent).toContain('No recorded decision, relationship, or affected-material evidence distinguishes these sessions.');
  });

  it('labels an unadopted working rewrite while retaining the chosen focus', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id); alternate.focusDocumentId = 'focus';
    const body = textBody('Focus source.'); const focusHead = await head('focus', '4', body);
    const chosen = decision(parent.id, 'focus', focusHead, { title: 'Chosen focus', revisionId: 'revision-focus' });
    await render({ project: projectFor([documentRecord(focusHead, 'Focus', body)]), state: state([parent, alternate], [chosen]), session: alternate, results: [], onOpenDocument: vi.fn() });
    expect(host.textContent).toContain('Proposed working change'); expect(host.textContent).toContain('Chosen focus · chosen version 4');
    expect(host.textContent).toContain('remains in place until you explicitly use this alternate');
  });

  it('reads only the parent and alternate latest revisions for a repeatedly adopted source', async () => {
    const parent = session('parent'); const alternate = session('alternate', parent.id); alternate.focusDocumentId = 'focus';
    const oldBody = textBody('Parent revision.'); const newBody = textBody('Alternate revision.');
    const oldHead = await head('focus', '1', oldBody); const newHead = await head('focus', '2', newBody);
    const oldDecision = decision(parent.id, 'focus', oldHead, { revisionId: 'revision-focus-1', status: 'superseded' });
    const newDecision = decision(alternate.id, 'focus', newHead, { revisionId: 'revision-focus-2', title: 'New focus choice' });
    vi.mocked(history.readDocumentRevision).mockImplementation(async (_access, documentId, revisionId) => documentId === 'focus' && revisionId === 'revision-focus-1'
      ? revision('revision-focus-1', oldHead, oldBody) : revision('revision-focus-2', newHead, newBody));
    await render({ project: projectFor([documentRecord(newHead, 'Focus', newBody)]), state: state([parent, alternate], [oldDecision, newDecision]), session: alternate, results: [], onOpenDocument: vi.fn() });
    await openComparison(); await settle(() => expect(history.readDocumentRevision).toHaveBeenCalledTimes(2));
    expect(host.textContent).toContain('Parent revision.'); expect(host.textContent).toContain('Alternate revision.');
    expect(host.textContent).toContain('superseded');
  });
});
