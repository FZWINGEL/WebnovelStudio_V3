import { sameHead } from '../kernel';
import { useEffect, useMemo, useRef, useState } from 'react';
import { readDocumentRevision } from '../ipc/history';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor';
import type { DocumentRecord, Head, OpenedProject } from '../ipc/projects';
import type { WorkshopDecision, WorkshopRelationship, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { describeWorkshopError } from './store';
import { plainText } from './text';
import { selectedBranchCandidates, sessionLineage, type SelectedBranchCandidate } from './branchEvidence';

type RevisionRead = { decision: WorkshopDecision; body: WnsDocument | null; error: string | null };
type FieldChange = { label: string; parent: string; alternate: string };
type DecisionChange = { kind: 'added' | 'removed' | 'changed'; parent: WorkshopDecision | null; alternate: WorkshopDecision | null };
type AffectedMaterial = { documentId: string; reason: string; candidateTitle: string | null; stale: boolean };

async function readExactRevision(access: OpenedProject['access'], decision: WorkshopDecision): Promise<WnsDocument> {
  const revision = await readDocumentRevision(access, decision.documentId, decision.revisionId);
  if (!revision || revision.id !== decision.revisionId || !sameHead(revision.head, decision.head)
    || await bodyHash(canonicalJson(revision.body)) !== decision.head.bodyHash) {
    throw new Error('The saved source revision did not match this chosen decision.');
  }
  return revision.body;
}

function documentLabel(project: OpenedProject, documentId: string | null): string {
  if (!documentId) return 'No focused document';
  const source = project.documents.find(document => document.head.documentId === documentId);
  return source?.title ?? 'Unavailable source';
}

function currentSource(project: OpenedProject, documentId: string): DocumentRecord | undefined {
  return project.documents.find(document => document.head.documentId === documentId);
}

function branchDecisions(state: WorkshopState, session: WorkshopSession): WorkshopDecision[] {
  const lineageIds = new Set(sessionLineage(state, session).map(item => item.id));
  return state.decisions.filter(decision => decision.status !== 'archived' && lineageIds.has(decision.sessionId));
}

function relevantDecisions(state: WorkshopState, session: WorkshopSession, selected: SelectedBranchCandidate[]): WorkshopDecision[] {
  const lineageIds = new Set(sessionLineage(state, session).map(item => item.id));
  const candidateIds = new Set(selected.map(item => item.candidate.id));
  const focusIds = new Set(sessionLineage(state, session).flatMap(item => item.focusDocumentId ? [item.focusDocumentId] : []));
  const latest = (scope: Set<string>) => {
    const byDocument = new Map<string, WorkshopDecision>();
    for (const decision of state.decisions) {
      if (decision.status !== 'archived' && scope.has(decision.sessionId)) byDocument.set(decision.documentId, decision);
    }
    return [...byDocument.values()];
  };
  const parent = session.parentSessionId ? state.sessions.find(item => item.id === session.parentSessionId) : undefined;
  const pair = [
    ...(parent ? latest(new Set(sessionLineage(state, parent).map(item => item.id))) : []),
    ...latest(lineageIds),
  ];
  const chosenByDocument = new Map<string, WorkshopDecision>();
  for (const decision of state.decisions) {
    if (decision.status === 'chosen' && (focusIds.has(decision.documentId) || decision.candidateIds.some(candidateId => candidateIds.has(candidateId)))) {
      chosenByDocument.set(decision.documentId, decision);
    }
  }
  const chosen = [...chosenByDocument.values()];
  const unique = new Map<string, WorkshopDecision>();
  for (const decision of [...pair, ...chosen]) unique.set(decision.id, decision);
  return [...unique.values()];
}

function textValue(value: string): string { return value.trim() || 'Nothing recorded'; }

function selectedDetailsValue(session: WorkshopSession): string {
  if (!session.selectedDetails.length) return 'No selected details';
  return session.selectedDetails.map(detail => (detail.fixed ? 'Keep fixed: ' : '') + detail.text).join('\n');
}

function includedDocumentsValue(project: OpenedProject, session: WorkshopSession): string {
  if (!session.includedDocumentIds.length) return 'No additional material';
  return session.includedDocumentIds.map(id => documentLabel(project, id)).join('\n');
}

function changedFields(project: OpenedProject, parent: WorkshopSession, alternate: WorkshopSession): FieldChange[] {
  const values: Array<[string, string, string]> = [
    ['Lens', parent.lens, alternate.lens],
    ['Depth', parent.depth, alternate.depth],
    ['Focus', documentLabel(project, parent.focusDocumentId), documentLabel(project, alternate.focusDocumentId)],
    ['Working title', textValue(parent.workingTitle), textValue(alternate.workingTitle)],
    ['Direction', textValue(parent.direction), textValue(alternate.direction)],
    ['Brief', textValue(parent.brief), textValue(alternate.brief)],
    ['Still open', textValue(parent.stillOpen), textValue(alternate.stillOpen)],
    ['Selected scope', textValue(parent.selectedScope), textValue(alternate.selectedScope)],
    ['Selected details', selectedDetailsValue(parent), selectedDetailsValue(alternate)],
    ['Included material', includedDocumentsValue(project, parent), includedDocumentsValue(project, alternate)],
    ['Working text', parent.workingText === alternate.workingText ? 'Same as parent' : 'Different from parent', parent.workingText === alternate.workingText ? 'Same as parent' : 'Edited in alternate'],
  ];
  return values.filter(([, before, after]) => before !== after).map(([label, before, after]) => ({ label, parent: before, alternate: after }));
}

function decisionChanges(parent: WorkshopDecision[], alternate: WorkshopDecision[]): DecisionChange[] {
  const byDocument = new Map<string, { parent: WorkshopDecision | null; alternate: WorkshopDecision | null }>();
  for (const decision of parent) byDocument.set(decision.documentId, { parent: decision, alternate: null });
  for (const decision of alternate) {
    const entry = byDocument.get(decision.documentId) ?? { parent: null, alternate: null };
    entry.alternate = decision;
    byDocument.set(decision.documentId, entry);
  }
  const changes: DecisionChange[] = [];
  for (const { parent: before, alternate: after } of byDocument.values()) {
    if (!before && after) changes.push({ kind: 'added', parent: null, alternate: after });
    else if (before && !after) changes.push({ kind: 'removed', parent: before, alternate: null });
    else if (before && after && !sameHead(before.head, after.head)) changes.push({ kind: 'changed', parent: before, alternate: after });
  }
  return changes;
}

function decisionStatus(decision: WorkshopDecision): string {
  return decision.status === 'superseded' ? 'superseded' : 'chosen';
}

function affectedMaterial(state: WorkshopState, selected: SelectedBranchCandidate[], decisions: WorkshopDecision[]): AffectedMaterial[] {
  const selectedIds = new Set(selected.map(item => item.candidate.id));
  const decisionIds = new Set(decisions.map(item => item.id));
  const values = new Map<string, AffectedMaterial>();
  const add = (documentId: string, reason: string, candidateTitle: string | null, stale: boolean) => {
    if (!documentId || documentId.startsWith('workshop-')) return;
    const key = documentId + '\u0000' + reason;
    const prior = values.get(key);
    values.set(key, prior ? { ...prior, stale: prior.stale || stale } : { documentId, reason, candidateTitle, stale });
  };
  for (const { candidate, result } of selected) {
    const stale = result.stale || state.sessions.find(item => item.id === result.sessionId)?.workingGeneration !== result.workingGeneration;
    for (const target of candidate.affectedTargets) add(target.documentId, target.reason, candidate.title, stale);
  }
  for (const impact of state.impacts) {
    if ((impact.candidateId && selectedIds.has(impact.candidateId)) || (impact.decisionId && decisionIds.has(impact.decisionId))) {
      add(impact.documentId, impact.reason, null, false);
    }
  }
  return [...values.values()];
}

function relationshipIsCurrent(project: OpenedProject, relationship: WorkshopRelationship): boolean {
  return relationship.sourceHeads.length > 0 && relationship.sourceHeads.every(head => project.documents.some(document => sameHead(document.head, head)));
}

function relatedRelationships(state: WorkshopState, ids: Set<string>): WorkshopRelationship[] {
  return state.relationships.filter(relationship => relationship.status !== 'archived'
    && (ids.has(relationship.fromDocumentId) || ids.has(relationship.toDocumentId)));
}

export function BranchComparison({ project, state, session, results, onOpenDocument }: {
  project: OpenedProject;
  state: WorkshopState;
  session: WorkshopSession;
  results: WorkshopResult[];
  onOpenDocument(documentId: string): void;
}) {
  const parent = state.sessions.find(item => item.id === session.parentSessionId);
  const selected = useMemo(() => selectedBranchCandidates(state, session, results), [state, session, results]);
  const sourceDecisions = useMemo(() => parent ? relevantDecisions(state, session, selected) : [], [parent, state, session, selected]);
  const sourceKey = sourceDecisions.map(decision => decision.id + ':' + decision.revisionId + ':' + decision.head.documentId + ':' + decision.head.version + ':' + decision.head.bodyHash).join('|');
  const identity = project.project.projectId + '/' + project.access.operationNamespace + '/' + project.access.session + '/' + project.access.writerLease + '/' + session.id + '/' + (parent?.id ?? '') + '/' + sourceKey;
  const sequence = useRef(0);
  const [sources, setSources] = useState<RevisionRead[]>([]);
  const [sourcesIdentity, setSourcesIdentity] = useState('');
  const [reading, setReading] = useState(false);
  const [open, setOpen] = useState(false);
  const activeIdentity = useRef(identity);
  activeIdentity.current = identity;

  useEffect(() => {
    const request = ++sequence.current;
    let disposed = false;
    const owns = () => !disposed && sequence.current === request && activeIdentity.current === identity;
    setSources([]);
    setSourcesIdentity('');
    if (!open || !parent || !sourceDecisions.length) {
      setReading(false);
      return () => { disposed = true; };
    }
    setReading(true);
    void Promise.all(sourceDecisions.map(async decision => {
      try {
        return { decision, body: await readExactRevision(project.access, decision), error: null } satisfies RevisionRead;
      } catch (reason) {
        return { decision, body: null, error: describeWorkshopError(reason) || 'The saved source revision is unavailable.' } satisfies RevisionRead;
      }
    })).then(value => { if (owns()) { setSources(value); setSourcesIdentity(identity); } }).finally(() => { if (owns()) setReading(false); });
    return () => { disposed = true; };
  }, [identity, open]);

  if (!parent) return <details className="workshop-branch-compare workshop-branch-comparison" aria-label="What-if comparison"><summary>Compare with the working exploration</summary><p className="small-copy">This exploration has no saved parent yet. What-if comparisons stay isolated until you create a branch.</p></details>;

  const fields = changedFields(project, parent, session);
  const decisions = decisionChanges(branchDecisions(state, parent), branchDecisions(state, session));
  const material = affectedMaterial(state, selected, sourceDecisions);
  const relatedIds = new Set<string>([
    ...sourceDecisions.map(decision => decision.documentId),
    ...material.map(item => item.documentId),
    ...[parent.focusDocumentId, session.focusDocumentId].filter((id): id is string => !!id),
  ]);
  const relationships = relatedRelationships(state, relatedIds);
  const evidenceEmpty = !fields.length && !decisions.length && !material.length && !relationships.length;
  const visibleSources = sourcesIdentity === identity ? sources : [];
  const currentDecisionById = new Map(sourceDecisions.map(decision => [decision.id, decision]));
  const focusDocumentId = session.focusDocumentId ?? parent.focusDocumentId;
  const proposedFocus = focusDocumentId
    ? [...state.decisions].reverse().find(decision => decision.status === 'chosen' && decision.documentId === focusDocumentId)
      ?? [...branchDecisions(state, parent)].reverse().find(decision => decision.documentId === focusDocumentId)
    : undefined;

  return <details className="workshop-branch-compare workshop-branch-comparison" aria-label="What-if comparison" open={open} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>Compare with the working exploration</summary>
    <div className="workshop-section-heading"><h2>Compare with parent exploration</h2><span>Read only</span></div>
    <p className="small-copy">This alternate remains separate from the working story. The comparison records what changed and what the saved evidence suggests may need review.</p>
    {parent.branchKind === 'whatIf' && <p className="workshop-notice">Nested what-if: this alternate compares with its direct parent, “{parent.title}”, which is itself separate from the working story.</p>}
    <div className="workshop-branch-comparison-columns">
      <section><h3>Parent · {parent.title}</h3><p className="workshop-prose">{parent.workingText || parent.direction || parent.brief || 'No working text recorded.'}</p></section>
      <section><h3>Alternate · {session.title}</h3><p className="workshop-prose">{session.workingText || session.direction || session.brief || 'No working text recorded.'}</p></section>
    </div>
    <section className="workshop-comparison-section" aria-label="Changed fields">
      <h3>Changed fields</h3>
      {fields.length ? <dl>{fields.map(field => <div key={field.label}><dt>{field.label}</dt><dd><strong>Parent</strong><span>{field.parent}</span><strong>Alternate</strong><span>{field.alternate}</span></dd></div>)}</dl> : <p className="small-copy">No meaningful working fields changed.</p>}
    </section>
    <section className="workshop-comparison-section" aria-label="Changed chosen decisions">
      <h3>Changed chosen decisions</h3>
      {decisions.length ? <ul>{decisions.map(change => <li key={(change.parent?.id ?? 'none') + ':' + (change.alternate?.id ?? 'none')}><strong>{change.kind === 'added' ? 'Alternate choice added' : change.kind === 'removed' ? 'Parent choice has no alternate replacement' : 'Choice revision differs'}</strong><span>{change.alternate?.title ?? change.parent?.title}</span><span>{change.parent ? 'Parent saved version ' + change.parent.head.version + ' · ' + decisionStatus(change.parent) : 'No parent choice'}</span><span>{change.alternate ? 'Alternate saved version ' + change.alternate.head.version + ' · ' + decisionStatus(change.alternate) : 'No alternate replacement'}</span></li>)}</ul> : parent.workingText !== session.workingText && proposedFocus ? <ul><li><strong>Proposed working change</strong><span>{proposedFocus.title} · chosen version {proposedFocus.head.version}</span><span>The chosen focus remains in place until you explicitly use this alternate.</span></li></ul> : <p className="small-copy">No chosen decision changed between these sessions.</p>}
    </section>
    <section className="workshop-comparison-section" aria-label="Chosen source revisions">
      <h3>Chosen source revisions</h3>
      {reading && <p role="status">Reading exact saved source revisions…</p>}
      {!open && <p className="small-copy">Open this comparison to read chosen source revisions.</p>}
      {open && !reading && !sources.length && <p className="small-copy">No chosen source revision is recorded for this comparison.</p>}
      <ul>{visibleSources.map(item => {
        const decision = currentDecisionById.get(item.decision.id) ?? item.decision;
        const current = currentSource(project, decision.documentId);
        const openLabel = current ? (sameHead(current.head, decision.head) ? 'Open source' : 'Open current source') : null;
        return <li key={decision.id} className="workshop-comparison-source"><strong>{decision.title}</strong><span>Saved source revision · version {decision.head.version}</span>{decision.rationale && <p><strong>Why this was chosen:</strong> {decision.rationale}</p>}{item.error ? <p role="alert">Saved source unavailable: {item.error}</p> : <details open><summary>Read exact saved source version</summary><p className="workshop-prose">{plainText(item.body!) || 'The saved source is empty.'}</p></details>}{current && !sameHead(current.head, decision.head) && <span className="small-copy">The current source has changed since this choice.</span>}{openLabel && current && <button onClick={() => onOpenDocument(decision.documentId)}>{openLabel}: {current.title}</button>}</li>;
      })}</ul>
    </section>
    <section className="workshop-comparison-section" aria-label="Likely affected material">
      <h3>Likely affected material</h3>
      <p className="small-copy">Only saved candidate evidence and existing review flags are shown here. Unknown effects remain unknown.</p>
      {material.length ? <ul>{material.map(item => { const source = currentSource(project, item.documentId); return <li key={item.documentId + ':' + item.reason}><strong>{source?.title ?? 'Saved material'}</strong><span>{item.reason}</span>{item.candidateTitle && <span className="small-copy">Recorded by selected candidate: {item.candidateTitle}</span>}{item.stale && <span className="small-copy">Stale evidence · review against the current working version.</span>}{source && <button onClick={() => onOpenDocument(item.documentId)}>Review source: {source.title}</button>}</li>; })}</ul> : <p className="small-copy">No selected candidate or saved review flag identifies likely affected material.</p>}
    </section>
    <section className="workshop-comparison-section" aria-label="Related relationships">
      <h3>Related relationships</h3>
      {relationships.length ? <ul>{relationships.map(relationship => <li key={relationship.id}><strong>{documentLabel(project, relationship.fromDocumentId)} → {relationship.type} → {documentLabel(project, relationship.toDocumentId)}</strong><span>{relationship.description}</span>{relationship.uncertainty && <span className="small-copy">Uncertainty: {relationship.uncertainty}</span>}{!relationshipIsCurrent(project, relationship) && <span className="small-copy">Source changed or unavailable · review this relationship.</span>}<div className="workshop-actions">{currentSource(project, relationship.fromDocumentId) && <button onClick={() => onOpenDocument(relationship.fromDocumentId)}>Open {documentLabel(project, relationship.fromDocumentId)}</button>}{currentSource(project, relationship.toDocumentId) && <button onClick={() => onOpenDocument(relationship.toDocumentId)}>Open {documentLabel(project, relationship.toDocumentId)}</button>}</div></li>)}</ul> : <p className="small-copy">No related saved relationship is connected to the changed material.</p>}
    </section>
    {evidenceEmpty && <p className="workshop-notice">No recorded decision, relationship, or affected-material evidence distinguishes these sessions. This comparison does not infer an effect.</p>}
  </details>;
}
