import type { WorkshopCandidate, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';

export interface SelectedBranchCandidate {
  candidate: WorkshopCandidate;
  result: WorkshopResult;
}

/**
 * Return the direct session lineage, stopping safely if old or malformed data
 * contains a missing parent or a cycle. The first item is the active session.
 */
export function sessionLineage(state: WorkshopState, session: WorkshopSession): WorkshopSession[] {
  const byId = new Map(state.sessions.map(item => [item.id, item]));
  const seen = new Set<string>();
  const lineage: WorkshopSession[] = [];
  let current: WorkshopSession | undefined = session;
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    lineage.push(current);
    current = current.parentSessionId ? byId.get(current.parentSessionId) : undefined;
  }
  return lineage;
}

/**
 * Resolve candidate claims that a branch explicitly selected. Rejected and
 * archived choices in the active session always win over inherited details.
 * Stale results remain in the return value so the comparison can label them;
 * the core adoption boundary will refuse them when an author tries to use
 * them against changed work.
 */
export function selectedBranchCandidates(
  state: WorkshopState,
  session: WorkshopSession,
  results: readonly WorkshopResult[],
): SelectedBranchCandidate[] {
  const lineage = sessionLineage(state, session);
  const lineageIds = new Set(lineage.map(item => item.id));
  // The active tray is the branch's authority. Ancestor sessions only make
  // their retained results eligible; a child removing a copied detail must
  // not silently reintroduce the parent's candidate.
  const selectedIds = new Set(session.selectedDetails.flatMap(detail => detail.candidateId ? [detail.candidateId] : []));
  const rejectedIds = new Set(session.choices.filter(choice => choice.status === 'rejected' || choice.status === 'archived').map(choice => choice.candidateId));
  const seen = new Set<string>();
  const selected: SelectedBranchCandidate[] = [];

  for (const result of results) {
    if (!lineageIds.has(result.sessionId) || result.run.status !== 'completed' || result.run.dispatchState !== 'delivered'
      || result.validationError || !result.output) continue;
    for (const candidate of result.output.candidates) {
      if (!selectedIds.has(candidate.id) || rejectedIds.has(candidate.id) || seen.has(candidate.id)) continue;
      seen.add(candidate.id);
      selected.push({ candidate, result });
    }
  }
  return selected;
}
