import { useState } from 'react';
import type { ReviewedEvidenceCoverage, ReviewedKnowledgeSet, SourceDescriptor } from '../ipc/context';
import { knowledgeAttitudeLabels } from '../ipc/reviews';

export function permittedKnowledgeRows(sets: ReviewedKnowledgeSet[], coverage: ReviewedEvidenceCoverage[], restricted: boolean, used: boolean) {
  return sets.flatMap(set => {
    const delivered = coverage.find(item => item.sourceHandle === set.sourceHandle && item.bundleId === set.bundleId && item.recordsHash === set.recordsHash);
    const ids = new Set(delivered?.recordIds ?? []);
    return set.records
      .filter(record => !restricted || record.audience === 'reader')
      .filter(record => !used || ids.has(record.id))
      .map(record => ({ set, record, delivered }));
  });
}

export function KnowledgeContextRows({ rows, sources, used, busy, onRead, onHistory }: {
  rows: ReturnType<typeof permittedKnowledgeRows>; sources: SourceDescriptor[]; used: boolean; busy: boolean;
  onRead(handle: string): void; onHistory(characterId: string, topicId: string): void;
}) {
  const [visible, setVisible] = useState(20);
  return <>{rows.slice(0, visible).map(({ set, record, delivered }) => <li key={`${set.bundleId}/${record.id}`} className="context-reviewed-evidence">
    <strong>{record.character.label}</strong><span className="context-detail">{record.topic.label} · {knowledgeAttitudeLabels[record.attitude]} · {record.audience === 'reader' ? 'Reader-visible' : 'Author room only'}</span>
    <p>{record.statement}</p><blockquote>{record.evidence.quote}</blockquote>
    <span className="context-detail">{record.timing === 'atPassage' ? 'At this passage' : record.timing === 'earlier' ? 'An earlier time' : 'Timing unclear'}. This is an author-recorded interpretation, not an inferred world truth.{used && delivered && !delivered.completeRecordSet ? ' Some character knowledge was not included.' : ''}</span>
    <div><button className="text-button" disabled={busy} onClick={() => onRead(set.sourceHandle)}>Read {sources.find(source => source.handle === set.sourceHandle)?.displayName ?? 'original source'}</button></div>
    <button className="quiet-button" disabled={busy} onClick={() => onHistory(record.character.id, record.topic.id)} aria-label={`Find knowledge history for ${record.character.label} about ${record.topic.label}`}>Find knowledge history</button>
  </li>)}{visible < rows.length && <li><button onClick={() => setVisible(count => count + 20)}>Show more character knowledge ({rows.length - visible} remaining)</button></li>}</>;
}
