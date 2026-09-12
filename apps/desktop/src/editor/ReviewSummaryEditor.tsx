import { useEffect, useRef, useState } from 'react';
import { readMemory, type DigestCandidate, type MemoryRead } from '../ipc/memory';
import type { Head, ProjectAccess } from '../ipc/projects';
import type { ReviewMember, ReviewSummaryAudience } from '../ipc/reviews';

export const REVIEW_SUMMARY_MAX_CHARS = 16 * 1024;

export type ReviewSummaryChoice = 'inherit' | 'set' | 'clear' | 'required';
export interface ReviewSummaryDraft {
  choice: ReviewSummaryChoice;
  text: string;
  audience: ReviewSummaryAudience;
}

function sameHeadTarget(source: { documentId: string; version: string; bodyHash: string }, target: Head): boolean {
  return source.documentId === target.documentId && source.version === target.version && source.bodyHash === target.bodyHash;
}
function sameSourceTarget(source: { projectId: string; documentId: string; bodyHash: string }, access: ProjectAccess, target: Head): boolean {
  return source.projectId === access.projectId && source.documentId === target.documentId && source.bodyHash === target.bodyHash;
}
function sameSource(left: { projectId: string; documentId: string; revisionId: string; bodyHash: string }, right: { projectId: string; documentId: string; revisionId: string; bodyHash: string }): boolean {
  return left.projectId === right.projectId && left.documentId === right.documentId && left.revisionId === right.revisionId && left.bodyHash === right.bodyHash;
}

function candidateForTarget(read: MemoryRead, access: ProjectAccess, target: Head): DigestCandidate | null {
  const candidates = read.views
    .filter(view => view.current === true && view.sourceChanged === false && view.policyAvailable === true && view.historical !== true
      && view.projectId === access.projectId && view.operationNamespace === access.operationNamespace && view.documentId === target.documentId)
    .filter(view => sameHeadTarget(view.target, target) && sameSourceTarget(view.source, access, target))
    .filter(view => !!view.source.revisionId && !!view.candidate && sameSource(view.candidate.source, view.source))
    .map(view => view.candidate)
    .filter((candidate): candidate is DigestCandidate => !!candidate && sameSourceTarget(candidate.source, access, target));
  return candidates.at(-1) ?? null;
}

function summaryBytes(text: string): number { return new TextEncoder().encode(text).length; }
function wordCount(text: string): number { return text.trim() ? text.trim().split(/\s+/u).length : 0; }
function memoryText(candidate: DigestCandidate): string {
  const text = candidate.items.map(item => item.uncertainty ? `${item.text}\nNeeds checking: ${item.uncertainty}` : item.text).join('\n\n');
  if (summaryBytes(text) > REVIEW_SUMMARY_MAX_CHARS) throw new Error('Generated memory is larger than the 16 KiB summary limit. Edit it manually or choose a shorter memory result.');
  return text;
}

function detail(error: unknown): string {
  return error && typeof error === 'object' && 'detail' in error ? String(error.detail)
    : error instanceof Error ? error.message : 'The current story memory could not be used as a starting point.';
}

/**
 * Plain-text author summary editor. It only reads an already saved memory
 * result when the caller explicitly asks to use it as editable starting text.
 */
export function ReviewSummaryEditor({ access, documentId, target, current, canInherit, value, disabled, onChange }: {
  access: ProjectAccess;
  documentId: string;
  target: Head;
  current: { id: string; text: string; audience: ReviewSummaryAudience } | null;
  canInherit: boolean;
  value: ReviewSummaryDraft;
  disabled: boolean;
  onChange(value: ReviewSummaryDraft): void;
}) {
  const [memoryReading, setMemoryReading] = useState(false);
  const [memoryError, setMemoryError] = useState('');
  const [memoryNotice, setMemoryNotice] = useState('');
  const request = useRef(0);
  const mounted = useRef(false);
  const contextKey = JSON.stringify({ projectId: access.projectId, operationNamespace: access.operationNamespace, session: access.session, writerLease: access.writerLease,
    documentId, target, disabled, value, canInherit, current });
  const liveContext = useRef(contextKey); liveContext.current = contextKey;
  const previousContext = useRef(contextKey);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (previousContext.current === contextKey) return;
    previousContext.current = contextKey;
    request.current += 1;
    setMemoryReading(false);
  }, [contextKey]);
  const inheritedText = canInherit && current ? current.text : '';
  const inheritedAudience = canInherit && current ? current.audience : 'authorRoom';
  const displayText = value.choice === 'inherit' ? inheritedText : value.text;
  const displayAudience = value.choice === 'inherit' ? inheritedAudience : value.audience;

  async function useMemory(): Promise<void> {
    if (disabled || memoryReading) return;
    const id = ++request.current;
    setMemoryReading(true); setMemoryError(''); setMemoryNotice('');
    try {
      const read = await readMemory(access, documentId);
      if (!mounted.current || id !== request.current || liveContext.current !== contextKey) return;
      if (read.documentId !== documentId) throw new Error('The returned story memory belongs to another chapter. Refresh story memory first.');
      const candidate = candidateForTarget(read, access, target);
      if (!candidate) throw new Error('No current story memory result matches this exact saved chapter. Refresh story memory first.');
      const text = memoryText(candidate);
      if (!text.trim()) throw new Error('The current story memory has no summary items to copy.');
      onChange({ choice: 'set', text, audience: value.choice === 'inherit' ? inheritedAudience : value.audience });
      setMemoryNotice('Copied generated memory into an editable author summary. Review and save it explicitly.');
    } catch (error) {
      if (mounted.current && id === request.current && liveContext.current === contextKey) setMemoryError(detail(error));
    } finally {
      if (mounted.current && id === request.current) setMemoryReading(false);
    }
  }

  function setText(text: string): void {
    if (summaryBytes(text) > REVIEW_SUMMARY_MAX_CHARS) { setMemoryError('Summary text is limited to 16 KiB. Shorten it before saving.'); return; }
    onChange({ choice: 'set', text, audience: displayAudience }); setMemoryError('');
  }
  function setAudience(audience: ReviewSummaryAudience): void { onChange({ choice: 'set', text: displayText, audience }); }
  function inherit(): void { onChange({ choice: 'inherit', text: inheritedText, audience: inheritedAudience }); setMemoryError(''); setMemoryNotice(''); }
  function clear(): void { onChange({ choice: 'clear', text: '', audience: displayAudience }); setMemoryError(''); setMemoryNotice(''); }

  return <section className="review-summary-editor" aria-labelledby="review-summary-heading">
    <div className="review-summary-editor-heading"><div><h3 id="review-summary-heading">Accepted narrative summary</h3><p>Optional author-reviewed orientation for this exact chapter and its earlier reviewed basis. Generated story memory stays separate.</p></div><span>{wordCount(displayText).toLocaleString()} words</span></div>
    {value.choice === 'required' && <p className="review-summary-warning" role="status">The previous summary belongs to another saved chapter or earlier reviewed basis. Choose Use this summary, edit it, or Clear summary before saving this review.</p>}
    {value.choice === 'inherit' && current && canInherit && <p className="review-summary-status" role="status">Using the accepted summary from this exact chapter and reviewed basis.</p>}
    {value.choice === 'clear' && <p className="review-summary-status" role="status">This review will explicitly clear any older accepted summary.</p>}
    <label htmlFor="review-summary-text">Summary text</label>
    <textarea id="review-summary-text" value={displayText} disabled={disabled || value.choice === 'clear'} onChange={event => setText(event.target.value)} placeholder="Optional: describe the chapter's established situation, promises, and relevant state." />
    <div className="review-summary-options"><label htmlFor="review-summary-audience">Audience</label><select id="review-summary-audience" value={displayAudience} disabled={disabled || value.choice === 'clear'} onChange={event => setAudience(event.target.value as ReviewSummaryAudience)}><option value="authorRoom">Author room · planning and review</option><option value="reader">Reader-approved · safe for reader-facing context</option></select></div>
    <div className="review-summary-actions">
      {canInherit && <button type="button" disabled={disabled || value.choice === 'inherit'} onClick={inherit}>Keep accepted summary</button>}
      {value.choice === 'required' && <button type="button" disabled={disabled} onClick={() => onChange({ choice: 'set', text: value.text, audience: value.audience })}>Use this summary</button>}
      <button type="button" disabled={disabled || value.choice === 'clear'} onClick={clear}>Clear summary</button>
      {value.choice === 'clear' && <button type="button" disabled={disabled} onClick={() => onChange({ choice: 'set', text: '', audience: displayAudience })}>Write a summary instead</button>}
      <button type="button" disabled={disabled || memoryReading} onClick={() => void useMemory()}>{memoryReading ? 'Reading generated memory…' : 'Use generated memory as a starting point'}</button>
    </div>
    {memoryError && <p className="review-summary-error" role="alert">{memoryError}</p>}
    {memoryNotice && <p className="review-summary-status" role="status">{memoryNotice}</p>}
  </section>;
}
