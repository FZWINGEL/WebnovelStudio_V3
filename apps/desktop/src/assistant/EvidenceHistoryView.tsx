import { useEffect, useRef, useState } from 'react';
import type { ReviewedHistoryResult } from '../ipc/context';

/** Saved observations; source order never claims a complete transfer chain. */
export function EvidenceHistoryView({ result, onRead, onClose }: {
  result: ReviewedHistoryResult; onRead(handle: string): void; onClose(): void;
}) {
  const [visibleCount, setVisibleCount] = useState(20);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  const { history } = result;
  const uncertainty = new Set(history.uncertainty);
  return <section className="context-evidence-history" aria-label="Recorded object history">
    <div className="header-actions"><h3 tabIndex={-1} ref={heading}>{history.labelVariants[0] ?? 'Recorded object history'}</h3><button onClick={onClose}>Close history</button></div>
    <p className="small-copy">Reviewed observations in chapter order. Passages may omit transfers; this does not establish the current holder. Opening this history adds nothing to the model’s request.</p>
    {!result.current && <p className="stale-notice">Earlier story version. Prepare a new request to include later changes.</p>}
    {history.labelVariants.length > 1 && <p className="small-copy">Also recorded as: {history.labelVariants.slice(1).join(', ')}.</p>}
    {uncertainty.has('differingHolders') && <p className="small-copy">Different holders are recorded. Read the evidence before assuming how possession changed.</p>}
    {(uncertainty.has('disclosureLimited') || uncertainty.has('excludedSources')) && <p className="small-copy">Some story material is outside this request’s information boundary.</p>}
    {!history.observations.length && <p>No observation for this object appears in the permitted saved evidence. It may be missing from the recorded details.</p>}
    <ol>{history.observations.slice(0, visibleCount).map((item, index) => <li key={`${item.sourceHandle}/${item.recordId}`}>
      <button className="text-button" onClick={() => onRead(item.sourceHandle)} aria-label={`Read ${item.sourceDisplayName} for observation ${index + 1}`}>{item.sourceDisplayName}</button>
      <p><strong>{item.holder?.label ?? 'Holder unknown'}</strong> · {item.timing === 'atPassage' ? 'At this passage' : item.timing === 'earlier' ? 'An earlier time; exact order not established' : 'Timing unclear'}</p>
      <blockquote>{item.evidence.quote}</blockquote>
    </li>)}</ol>
    {visibleCount < history.observations.length && <button onClick={() => setVisibleCount(count => count + 20)}>Show more observations ({history.observations.length - visibleCount} remaining)</button>}
  </section>;
}
