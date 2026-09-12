import { useEffect, useRef, useState } from 'react';
import type { ReviewedPromiseHistoryResult } from '../ipc/context';
import { promisePhaseLabels } from '../ipc/reviews';

export function PromiseHistoryView({ result, onRead, onClose }: {
  result: ReviewedPromiseHistoryResult; onRead(handle: string): void; onClose(): void;
}) {
  const [visibleCount, setVisibleCount] = useState(20);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  const { history } = result; const uncertainty = new Set(history.uncertainty);
  return <section className="context-evidence-history" aria-label="Recorded promise history">
    <div className="header-actions"><h3 tabIndex={-1} ref={heading}>{history.labelVariants[0] ?? 'Recorded promise history'}</h3><button onClick={onClose}>Close promise history</button></div>
    <p className="small-copy">Reviewed observations in chapter order. Other payoffs or changes may be missing. Opening this history adds nothing to the model’s request.</p>
    {!result.current && <p className="stale-notice">Earlier story version. Prepare a new request to include later changes.</p>}
    <p>{history.hasRecordedPayoff ? 'A payoff is recorded in this evidence. Read its passage and any other outcomes before drawing a conclusion.' : 'No payoff is recorded in this evidence. That does not prove the promise remains unresolved.'}</p>
    {history.labelVariants.length > 1 && <p className="small-copy">Also recorded as: {history.labelVariants.slice(1).join(', ')}.</p>}
    {uncertainty.has('conflictingOutcomes') && <p className="small-copy">Different outcomes are recorded. Their fictional order or relationship is not established here.</p>}
    {(uncertainty.has('disclosureLimited') || uncertainty.has('excludedSources')) && <p className="small-copy">Some story material is outside this request’s information boundary.</p>}
    {!history.observations.length && <p>No observation for this promise appears in the permitted saved evidence.</p>}
    <ol>{history.observations.slice(0, visibleCount).map((item, index) => <li key={`${item.sourceHandle}/${item.recordId}`}>
      <button className="text-button" onClick={() => onRead(item.sourceHandle)} aria-label={`Read ${item.sourceDisplayName} for promise observation ${index + 1}`}>{item.sourceDisplayName}</button>
      <p><strong>{promisePhaseLabels[item.phase]}</strong> · {item.timing === 'atPassage' ? 'At this passage' : item.timing === 'earlier' ? 'An earlier time; exact order not established' : 'Timing unclear'}</p>
      <p>{item.note}</p><blockquote>{item.evidence.quote}</blockquote>
      <p className="small-copy">{item.audience === 'reader' ? 'Explicitly reader-disclosed' : 'Author room only'}</p>
    </li>)}</ol>
    {visibleCount < history.observations.length && <button onClick={() => setVisibleCount(count => count + 20)}>Show more promise observations ({history.observations.length - visibleCount} remaining)</button>}
  </section>;
}
