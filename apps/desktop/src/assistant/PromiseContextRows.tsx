import { useState } from 'react';
import type { ReviewedEvidenceCoverage, ReviewedPromiseSet, SourceDescriptor } from '../ipc/context';
import { promisePhaseLabels } from '../ipc/reviews';

export function permittedPromiseRows(sets: ReviewedPromiseSet[], coverage: ReviewedEvidenceCoverage[], restricted: boolean, used: boolean) {
  return sets.flatMap(set => {
    const delivered = coverage.find(item => item.sourceHandle === set.sourceHandle && item.bundleId === set.bundleId && item.recordsHash === set.recordsHash);
    const ids = new Set(delivered?.recordIds ?? []);
    return set.records.filter(record => !restricted || record.audience === 'reader').filter(record => !used || ids.has(record.id)).map(record => ({ set, record, delivered }));
  });
}

export function PromiseContextRows({ rows, sources, used, busy, onRead, onHistory }: {
  rows: ReturnType<typeof permittedPromiseRows>; sources: SourceDescriptor[]; used: boolean; busy: boolean;
  onRead(handle: string): void; onHistory(promiseId: string): void;
}) {
  const [visible, setVisible] = useState(20);
  return <>{rows.slice(0, visible).map(({ set, record, delivered }) => <li key={`${set.bundleId}/${record.id}`} className="context-reviewed-evidence">
    <strong>{record.promise.label}</strong><span className="context-detail">{promisePhaseLabels[record.phase]} · {record.audience === 'reader' ? 'Reader-visible' : 'Author room only'}</span>
    <p>{record.note}</p><blockquote>{record.evidence.quote}</blockquote>
    <span className="context-detail">{record.timing === 'atPassage' ? 'At this passage' : record.timing === 'earlier' ? 'An earlier time' : 'Timing unclear'}. Reviewed observations may omit later developments.{used && delivered && !delivered.completeRecordSet ? ' Some promise details were not included.' : ''}</span>
    <div><button className="text-button" disabled={busy} onClick={() => onRead(set.sourceHandle)}>Read {sources.find(source => source.handle === set.sourceHandle)?.displayName ?? 'original source'}</button></div>
    <button className="quiet-button" disabled={busy} onClick={() => onHistory(record.promise.id)} aria-label={`Find promise history for ${record.promise.label}`}>Find promise history</button>
  </li>)}{visible < rows.length && <li><button onClick={() => setVisible(count => count + 20)}>Show more promise details ({rows.length - visible} remaining)</button></li>}</>;
}
