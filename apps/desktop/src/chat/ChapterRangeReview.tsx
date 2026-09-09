import type { DiscussionScope } from '../ipc/discussions';
import type { Head } from '../ipc/projects';
import type { WnsDocument } from '../editor/document';
import { blockText } from '../editor/revisionScope';

export interface SuggestedChapterRange {
  target: Head;
  scope: DiscussionScope;
  explanation: string;
}

export function suggestedChapterRange(feedback: {
  target: Head; answer: string; rangeProposal?: { sourceHead: Head; firstBlockId: string; lastBlockId: string; quote: string } | null;
}, body: WnsDocument): SuggestedChapterRange | null {
  const proposed = feedback.rangeProposal;
  if (!proposed) return null;
  const last = body.body.content.find(block => block.attrs.id === proposed.lastBlockId);
  return {
    target: proposed.sourceHead,
    explanation: 'The assistant suggests reviewing these paragraphs from its chapter feedback.',
    scope: { kind: 'blocks', sourceBodyHash: proposed.sourceHead.bodyHash,
      start: { blockId: proposed.firstBlockId, utf16Offset: 0 },
      end: { blockId: proposed.lastBlockId, utf16Offset: last ? blockText(last).length : 0 }, quote: proposed.quote },
  };
}

/** Selecting a scope prepares the next request; it never sends or applies an edit. */
export function ChapterRangeReview({ range, busy, stale, error, onConfirm }: {
  range: SuggestedChapterRange; busy: boolean; stale: boolean; error?: string;
  onConfirm(): void;
}) {
  return <section className="chat-chapter-range" aria-label="Suggested passage for revision">
    <h3>Suggested passage</h3>
    <p>{range.explanation}</p>
    <p>Chapter version {range.target.version}. Confirm these paragraphs to prepare a separate edit request. The chapter stays unchanged.</p>
    <blockquote className="chat-prose" tabIndex={0} aria-label="Complete proposed passage">{range.scope.quote}</blockquote>
    {stale && <p role="status">The chapter changed. Ask for a fresh suggestion or select a passage in the current chapter.</p>}
    {error && <p role="alert">{error}</p>}
    <button type="button" disabled={busy || stale} onClick={onConfirm}>{busy ? 'Checking this passage…' : 'Use this passage for an edit'}</button>
  </section>;
}
