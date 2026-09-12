import { useEffect, useRef, useState } from 'react';
import type { ReviewedKnowledgeHistoryResult } from '../ipc/context';
import { knowledgeAttitudeLabels } from '../ipc/reviews';

export function KnowledgeHistoryView({ result, onRead, onClose }: {
  result: ReviewedKnowledgeHistoryResult; onRead(handle: string): void; onClose(): void;
}) {
  const [visibleCount, setVisibleCount] = useState(20);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  const { history } = result;
  const uncertainty = new Set(history.uncertainty);
  return <section className="context-evidence-history" aria-label="Character knowledge history">
    <div className="header-actions"><h3 tabIndex={-1} ref={heading}>{history.labelVariants[0] ?? 'Character knowledge history'}</h3><button onClick={onClose}>Close knowledge history</button></div>
    <p className="small-copy">These are author-recorded observations in source order. They do not establish a definitive current mental state or world truth. Opening this history adds nothing to the model’s request.</p>
    {!result.current && <p className="stale-notice">Earlier story version. Prepare a new request to include later changes.</p>}
    {history.labelVariants.length > 1 && <p className="small-copy">Also recorded as: {history.labelVariants.slice(1).join(', ')}.</p>}
    {uncertainty.has('noEligibleObservations') && <p>No eligible observation for this character and topic appears in the permitted saved evidence. That does not establish that the character is unaware.</p>}
    {uncertainty.has('earlierOrUnknownTiming') && <p className="small-copy">Some observations have earlier or unknown timing; source order does not establish fictional chronology.</p>}
    {uncertainty.has('multipleRecordedAttitudes') && <p className="small-copy">Different attitudes are recorded. Read the passages before drawing a conclusion.</p>}
    {uncertainty.has('disclosureLimited') && <p className="small-copy">Some story material is outside this request’s information boundary.</p>}
    {!history.observations.length && !uncertainty.has('noEligibleObservations') && <p>No observation for this character appears in the permitted saved evidence.</p>}
    <ol>{history.observations.slice(0, visibleCount).map((item, index) => <li key={`${item.sourceHandle}/${item.recordId}`}>
      <button className="text-button" onClick={() => onRead(item.sourceHandle)} aria-label={`Read ${item.sourceDisplayName} for knowledge observation ${index + 1}`}>{item.sourceDisplayName}</button>
      <p><strong>{knowledgeAttitudeLabels[item.attitude]}</strong> · {item.topic.label} · {item.timing === 'atPassage' ? 'At this passage' : item.timing === 'earlier' ? 'An earlier time; exact order not established' : 'Timing unclear'}</p>
      <p>{item.statement}</p><blockquote>{item.evidence.quote}</blockquote>
      <p className="small-copy">{item.audience === 'reader' ? 'Explicitly reader-disclosed' : 'Author room only'}</p>
    </li>)}</ol>
    {visibleCount < history.observations.length && <button onClick={() => setVisibleCount(count => count + 20)}>Show more knowledge observations ({history.observations.length - visibleCount} remaining)</button>}
  </section>;
}
