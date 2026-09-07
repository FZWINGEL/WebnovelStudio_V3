import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopDecision, WorkshopResult, WorkshopSession, WorkshopState } from '../ipc/workshop';
import { sessionLineage } from './branchEvidence';

export interface NextExplorationContextProps {
  state: WorkshopState;
  session: WorkshopSession;
  results: WorkshopResult[];
  documents: DocumentRecord[];
  onOpenDocument?: (documentId: string) => void;
}

type IncludedAlternative = {
  candidateId: string;
  rationale: string;
  title: string;
  content: string | null;
  source: string;
  stale: boolean;
  noncanon: boolean;
};

function textOr(value: string | null | undefined, fallback: string): string {
  return value && value.trim() ? value : fallback;
}

function documentTitle(documents: DocumentRecord[], documentId: string): string {
  return documents.find(document => document.head.documentId === documentId)?.title ?? 'Saved material';
}

function currentElement(session: WorkshopSession): string {
  // Keep this order in step with workshop_generation::from_session_with_material.
  return [session.workingText, session.brief, session.composer].find(value => value.trim())
    ?? 'No working story element has been chosen yet.';
}

function savedResult(result: WorkshopResult): boolean {
  return result.run.status === 'completed'
    && result.run.dispatchState === 'delivered'
    && result.validationError === null
    && result.output !== null;
}

function candidateForChoice(
  state: WorkshopState,
  session: WorkshopSession,
  results: WorkshopResult[],
  candidateId: string,
): { result: WorkshopResult; title: string; content: string; stale: boolean } | null {
  const lineageIds = new Set(sessionLineage(state, session).map(item => item.id));
  // The core accepts saved alternatives from this session or an ancestor. A
  // result is only useful here once its immutable candidate output was saved.
  const candidates = results
    .filter(result => lineageIds.has(result.sessionId) && savedResult(result))
    .flatMap(result => result.output!.candidates.filter(candidate => candidate.id === candidateId).map(candidate => ({ result, candidate })));
  const match = candidates[candidates.length - 1];
  if (!match) return null;
  const sourceSession = state.sessions.find(item => item.id === match.result.sessionId);
  return {
    result: match.result,
    title: match.candidate.title,
    content: match.candidate.content,
    stale: match.result.stale || !sourceSession || sourceSession.workingGeneration !== match.result.workingGeneration,
  };
}

function includedAlternatives(state: WorkshopState, session: WorkshopSession, results: WorkshopResult[]): IncludedAlternative[] {
  return session.choices
    .filter(choice => choice.status === 'saved' && choice.includeInContext)
    .map(choice => {
      const match = candidateForChoice(state, session, results, choice.candidateId);
      if (!match) return {
        candidateId: choice.candidateId,
        rationale: choice.rationale,
        title: 'Saved alternative',
        content: null,
        source: 'No completed saved result is loaded for this choice.',
        stale: true,
        noncanon: false,
      };
      const sourceSession = state.sessions.find(item => item.id === match.result.sessionId);
      return {
        candidateId: choice.candidateId,
        rationale: choice.rationale,
        title: match.title,
        content: match.content,
        source: `Saved result from ${sourceSession?.title ?? 'an earlier exploration'}`,
        stale: match.stale,
        noncanon: match.result.action === 'moment',
      };
    });
}

function relationshipFor(state: WorkshopState, session: WorkshopSession) {
  return session.relationshipId ? state.relationships.find(item => item.id === session.relationshipId) : undefined;
}

/** Mirrors the fixed-decision relevance check used for workshop requests. */
function fixedDecisionIsRelevant(state: WorkshopState, session: WorkshopSession, decision: WorkshopDecision): boolean {
  const lineageIds = new Set(sessionLineage(state, session).map(item => item.id));
  const relationship = relationshipFor(state, session);
  return session.focusDocumentId === decision.documentId
    || session.includedDocumentIds.includes(decision.documentId)
    || relationship?.fromDocumentId === decision.documentId
    || relationship?.toDocumentId === decision.documentId
    || lineageIds.has(decision.sessionId);
}

function decisionSource(documents: DocumentRecord[], decision: WorkshopDecision): string {
  return `${documentTitle(documents, decision.documentId)} · saved version ${decision.head.version}`;
}

function SourceButton({ documentId, documents, onOpenDocument }: { documentId: string; documents: DocumentRecord[]; onOpenDocument?: (documentId: string) => void }) {
  if (!onOpenDocument) return null;
  return <button type="button" onClick={() => onOpenDocument(documentId)}>Open current source: {documentTitle(documents, documentId)}</button>;
}

function DecisionCard({ decision, documents, onOpenDocument, protectedDecision = false }: { decision: WorkshopDecision; documents: DocumentRecord[]; onOpenDocument?: (documentId: string) => void; protectedDecision?: boolean }) {
  return <article className="workshop-context-decision">
    <strong>{decision.title}</strong>
    <p className="small-copy">{protectedDecision && decision.status === 'archived' ? 'Archived protection remains relevant to this preview.' : decision.status === 'archived' ? 'Archived choice.' : decision.fixed ? 'Keep fixed.' : 'Chosen author decision.'}</p>
    <p className="small-copy">{decisionSource(documents, decision)}</p>
    <p className="small-copy">This preview keeps the saved choice’s version metadata. The current document body is not shown as a substitute for that choice.</p>
    {decision.rationale.trim() && <p className="workshop-preserve-lines">Author rationale: {decision.rationale}</p>}
    {decision.fixed && (decision.protectedText.length ? <><h5>Protected text</h5><ul>{decision.protectedText.map((text, index) => <li className="workshop-preserve-lines" key={`${decision.id}-protected-${index}`}>{text}</li>)}</ul></> : protectedDecision ? <p className="small-copy">The whole saved source is protected. This preview does not substitute the current document body.</p> : <p className="small-copy">This decision has no narrower protected text.</p>)}
    <SourceButton documentId={decision.documentId} documents={documents} onOpenDocument={onOpenDocument} />
  </article>;
}

function BriefDetails({ session }: { session: WorkshopSession }) {
  return <details>
    <summary>Brief, scope, question, and notes</summary>
    <dl>
      <dt>Brief</dt><dd className="workshop-preserve-lines">{textOr(session.brief, 'No brief recorded.')}</dd>
      <dt>Current scope</dt><dd>{textOr(session.selectedScope, 'Whole working version')}</dd>
      <dt>Question</dt><dd className="workshop-preserve-lines">{textOr(session.focusQuestion, 'No focus question recorded.')}</dd>
      {session.focusReason.trim() && <><dt>Why this question</dt><dd className="workshop-preserve-lines">{session.focusReason}</dd></>}
      <dt>Original notes</dt><dd className="workshop-preserve-lines">{textOr(session.originalNotes, 'No original notes recorded.')}</dd>
    </dl>
  </details>;
}

export function NextExplorationContext({ state, session, results, documents, onOpenDocument }: NextExplorationContextProps) {
  const lineageIds = new Set(sessionLineage(state, session).map(item => item.id));
  const chosen = state.decisions.filter(decision => decision.status === 'chosen' && lineageIds.has(decision.sessionId));
  const protectedDecisions = state.decisions.filter(decision => decision.fixed && fixedDecisionIsRelevant(state, session, decision));
  const alternatives = includedAlternatives(state, session, results);

  return <section className="workshop-context-preview" aria-label="Planned context">
    <h3>Planned context</h3>
    <p className="small-copy">This is the current preview for the next exploration. Sources and available space are checked when you send. Inspect the saved request to see exactly what was supplied.</p>

    <details aria-label="Direction and current element">
      <summary>Direction and current element</summary>
      <dl>
        <dt>Current direction</dt><dd className="workshop-preserve-lines">{textOr(session.direction, 'No direction recorded.')}</dd>
        <dt>Explore outside current direction</dt><dd>{session.outsideDirection ? 'Yes — alternatives may go beyond the current direction.' : 'No — stay within the current direction.'}</dd>
        <dt>Current element</dt><dd className="workshop-preserve-lines">{currentElement(session)}</dd>
      </dl>
      {session.outsideDirection && <p className="small-copy">Hard constraints and protected details remain in force.</p>}
    </details>

    <BriefDetails session={session} />

    <details aria-label="Selected details">
      <summary>Selected details ({session.selectedDetails.length})</summary>
      {session.selectedDetails.length ? <ul>{session.selectedDetails.map(detail => <li className="workshop-preserve-lines" key={detail.id}>{detail.fixed && <strong>Keep fixed: </strong>}{detail.text}</li>)}</ul> : <p className="small-copy">No selected details.</p>}
    </details>

    <section aria-label="Chosen related decisions">
      <details>
        <summary>Chosen related decisions ({chosen.length})</summary>
        <p className="small-copy">Only chosen decisions from this exploration and its parent explorations are planned here. Their saved version remains the source.</p>
        {chosen.length ? chosen.map(decision => <DecisionCard key={decision.id} decision={decision} documents={documents} onOpenDocument={onOpenDocument} />) : <p className="small-copy">No chosen related decisions.</p>}
      </details>
    </section>

    <section aria-label="Relevant protected decisions">
      <details>
        <summary>Relevant protected decisions ({protectedDecisions.length})</summary>
        <p className="small-copy">Protection remains visible when an archived saved choice still applies to this exploration.</p>
        {protectedDecisions.length ? protectedDecisions.map(decision => <DecisionCard key={decision.id} decision={decision} documents={documents} onOpenDocument={onOpenDocument} protectedDecision />) : <p className="small-copy">No relevant protected decisions.</p>}
      </details>
    </section>

    <section aria-label="Deliberately included saved alternatives">
      <details>
        <summary>Deliberately included saved alternatives ({alternatives.length})</summary>
        <p className="small-copy">Only saved alternatives explicitly marked for context are listed. Rejected and unrelated results stay out; an explicitly included moment remains labeled as a noncanon experiment.</p>
        {alternatives.length ? alternatives.map(alternative => <article className="workshop-context-alternative" key={alternative.candidateId}>
          <strong>{alternative.title}</strong>
          <p className="small-copy">{alternative.noncanon ? 'Noncanon experiment. ' : ''}{alternative.source} · {alternative.stale ? 'Stale source; review against current work.' : 'Current saved result.'}</p>
          {alternative.content ? <p className="workshop-preserve-lines">{alternative.content}</p> : <p className="small-copy">The saved alternative is not loaded in this preview.</p>}
          {alternative.rationale.trim() && <p className="small-copy">Reason saved: {alternative.rationale}</p>}
        </article>) : <p className="small-copy">No saved alternatives are deliberately included.</p>}
      </details>
    </section>

    {!!session.includedDocumentIds.length && <section aria-label="Explicitly included saved sources">
      <details>
        <summary>Explicitly included saved sources ({session.includedDocumentIds.length})</summary>
        <ul>{session.includedDocumentIds.map(documentId => <li key={documentId}><span>{documentTitle(documents, documentId)}</span>{onOpenDocument && <><br /><SourceButton documentId={documentId} documents={documents} onOpenDocument={onOpenDocument} /></>}</li>)}</ul>
      </details>
    </section>}
  </section>;
}
