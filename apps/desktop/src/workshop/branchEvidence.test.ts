import { describe, expect, it } from 'vitest';
import type { DiscussionRun } from '../ipc/discussions';
import type { WorkshopCandidate, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { selectedBranchCandidates } from './branchEvidence';

const session = (id: string, parentSessionId: string | null = null): WorkshopSession => ({
  id, title: id, lens: 'overview', parentSessionId, branchKind: parentSessionId ? 'whatIf' : 'working',
  brief: '', direction: '', stillOpen: '', focusQuestion: '', focusReason: '', focusDocumentId: null,
  anchorDocumentId: `workshop-${id}`, depth: 'sketch', outsideDirection: false, includedDocumentIds: [],
  workingText: '', workingTitle: '', workingGeneration: '1', selectedDetails: [], choices: [], questions: [],
  composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null,
});

const candidate = (id: string): WorkshopCandidate => ({
  id, title: id, content: `Content for ${id}`, dimensionValue: id, implications: [], assumptions: [],
  affectedTargets: [], preservedDetails: [], changedDetails: [],
});

const run = (id: string, overrides: Partial<DiscussionRun> = {}): DiscussionRun => ({
  id, threadId: id, owner: { projectId: 'project', operationNamespace: 'workshop', runId: id },
  operationId: `operation-${id}`, payloadHash: 'hash', target: { documentId: 'anchor', version: '1', bodyHash: 'hash' },
  packetId: `packet-${id}`, previousRunId: null, status: 'completed', dispatchState: 'delivered', sequence: '1',
  outputText: '', stopReason: null, createdAt: '2026-09-07T00:00:00Z', updatedAt: '2026-09-07T00:00:00Z', ...overrides,
});

const result = (sessionId: string, id: string, values: WorkshopCandidate[], overrides: Partial<WorkshopResult> = {}): WorkshopResult => ({
  run: run(id), sessionId, workingGeneration: '1', action: 'directions', output: {
    schemaVersion: 'story-workshop-output.v1', requestKind: 'directions', question: 'Question', questionReason: 'Reason',
    dimension: 'Dimension', interpretation: { youSaid: 'Said', possibleDirection: 'Direction', stillOpen: 'Open' }, candidates: values,
  }, validationError: null, stale: false, ...overrides,
});

const state = (sessions: WorkshopSession[]): WorkshopState => ({
  schemaVersion: 1, currentSessionId: sessions.at(-1)?.id ?? null, sessions, preferences: [], decisions: [], relationships: [], impacts: [], presets: [],
});

describe('selected branch candidate evidence', () => {
  it('uses only the active tray after a child removes a copied parent detail', () => {
    const parent = session('parent');
    parent.selectedDetails = [{ id: 'parent-detail', candidateId: 'parent-choice', text: 'Parent choice', fixed: false }];
    const child = session('child', parent.id);
    child.selectedDetails = [{ id: 'child-detail', candidateId: 'child-choice', text: 'Child choice', fixed: false }];
    const values = selectedBranchCandidates(state([parent, child]), child, [
      result(parent.id, 'parent-run', [candidate('parent-choice')]),
      result(child.id, 'child-run', [candidate('child-choice')]),
    ]);
    expect(values.map(item => item.candidate.id)).toEqual(['child-choice']);
  });

  it('keeps stale completed evidence for comparison but excludes rejected and incomplete output', () => {
    const current = session('current');
    current.selectedDetails = [
      { id: 'keep', candidateId: 'keep', text: 'Keep', fixed: false },
      { id: 'reject', candidateId: 'reject', text: 'Reject', fixed: false },
      { id: 'stale', candidateId: 'stale', text: 'Stale', fixed: false },
    ];
    current.choices = [{ candidateId: 'reject', status: 'rejected', rationale: 'No', includeInContext: false }];
    const values = selectedBranchCandidates(state([current]), current, [
      result(current.id, 'rejected-run', [candidate('reject')]),
      result(current.id, 'stale-run', [candidate('stale')], { stale: true }),
      result(current.id, 'failed-run', [candidate('keep')], { run: run('failed-run', { status: 'failed' }), output: null }),
      result(current.id, 'kept-run', [candidate('keep')]),
    ]);
    expect(values.map(item => item.candidate.id)).toEqual(['stale', 'keep']);
    expect(values[0].result.stale).toBe(true);
  });
});
