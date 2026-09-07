import type { WorkshopDecision, WorkshopSession } from '../ipc/workshop';

/** A local projection of the author's work; opening or closing never calls a model. */
export function WorkshopRecap({ session, decisions, saved, onOpenDocument }: {
  session: WorkshopSession; decisions: WorkshopDecision[]; saved: boolean; onOpenDocument(id: string): void;
}) {
  const chosen = decisions.filter(decision => decision.sessionId === session.id && decision.status === 'chosen');
  const open = session.questions.filter(question => question.status === 'open' || question.status === 'notNow' || question.status === 'keepMysterious');
  const disposedFocus = session.questions.find(question => question.text === session.focusQuestion && question.status !== 'open');
  const nextQuestion = session.focusQuestion && !disposedFocus ? session.focusQuestion : null;
  const developed = !!session.workingText.trim() || session.selectedDetails.length > 0 || chosen.length > 0;
  const developedLabel = developed ? session.workingTitle || session.title : 'No working material developed yet.';
  const resumeLabel = `Resume ${session.workingTitle || session.title}.`;
  return <>
    <p className="small-copy">{saved ? 'Your latest exploration changes are saved.' : 'Latest exploration changes are still waiting to be saved.'}</p>
    <dl><dt>We developed</dt><dd>{developedLabel}{session.branchKind === 'whatIf' ? ' · Separate what-if exploration' : ''}</dd>
      <dt>You chose</dt><dd>{chosen.length ? <ul>{chosen.map(decision => <li key={decision.id}><button onClick={() => onOpenDocument(decision.documentId)}>Open current material: {decision.title}</button><span className="small-copy">Chosen source revision · version {decision.head.version}</span>{decision.rationale && <p><strong>Why this was chosen:</strong> {decision.rationale}</p>}<span className="small-copy">Author intention; writing access is separate.</span></li>)}</ul> : 'No saved story decision yet. Your working version remains available.'}</dd>
      <dt>Still worth exploring</dt><dd>{session.stillOpen && <p>{session.stillOpen}</p>}{open.length ? <ul>{open.map(question => <li key={question.id}>{question.text} · {question.status === 'keepMysterious' ? 'Intentionally mysterious' : question.status === 'notNow' ? 'For later' : 'Open'} · unknown to {question.unknownTo === 'author' ? 'you' : question.unknownTo === 'reader' ? 'the reader' : 'you and the reader'}</li>)}</ul> : !session.stillOpen && 'Follow another question whenever you want.'}</dd>
      <dt>Next time</dt><dd>{nextQuestion || resumeLabel}{nextQuestion && session.focusReason && <p>{session.focusReason}</p>}</dd>
    </dl>
  </>;
}
