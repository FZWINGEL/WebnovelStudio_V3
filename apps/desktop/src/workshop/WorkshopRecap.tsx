import type { WorkshopDecision, WorkshopSession } from '../ipc/workshop';

/** A local projection of the author's work; opening or closing never calls a model. */
export function WorkshopRecap({ session, decisions, saved, onOpenDocument }: {
  session: WorkshopSession; decisions: WorkshopDecision[]; saved: boolean; onOpenDocument(id: string): void;
}) {
  const chosen = decisions.filter(decision => decision.sessionId === session.id && decision.status === 'chosen');
  const open = session.questions.filter(question => question.status === 'open' || question.status === 'notNow' || question.status === 'keepMysterious');
  return <>
    <p className="small-copy">{saved ? 'Your latest exploration changes are saved.' : 'Latest exploration changes are still waiting to be saved.'}</p>
    <dl><dt>We developed</dt><dd>{session.workingTitle || session.title}{session.branchKind === 'whatIf' ? ' · Separate what-if exploration' : ''}</dd>
      <dt>You chose</dt><dd>{chosen.length ? <ul>{chosen.map(decision => <li key={decision.id}><button onClick={() => onOpenDocument(decision.documentId)}>{decision.title} · version {decision.head.version}</button>{decision.rationale && <p>{decision.rationale}</p>}<span className="small-copy">Author intention; writing access is separate.</span></li>)}</ul> : 'No saved story decision yet. Your working version remains available.'}</dd>
      <dt>Still worth exploring</dt><dd>{session.stillOpen && <p>{session.stillOpen}</p>}{open.length ? <ul>{open.map(question => <li key={question.id}>{question.text} · {question.status === 'keepMysterious' ? 'Intentionally mysterious' : question.status === 'notNow' ? 'For later' : 'Open'} · unknown to {question.unknownTo === 'author' ? 'you' : question.unknownTo === 'reader' ? 'the reader' : 'you and the reader'}</li>)}</ul> : !session.stillOpen && 'Follow another question whenever you want.'}</dd>
      <dt>Next time</dt><dd>{session.focusQuestion || `Resume ${session.title}.`}{session.focusReason && <p>{session.focusReason}</p>}</dd>
    </dl>
  </>;
}
