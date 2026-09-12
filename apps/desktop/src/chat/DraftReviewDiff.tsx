import type { DocumentRecord } from '../ipc/projects';
import { documentBlocks, type WnsDocument } from '../editor';

type DiffToken = { kind: 'same' | 'removed' | 'added'; text: string };

export interface DraftDocumentDiffProps {
  /** The immutable source captured by the adoption preview. */
  before: DocumentRecord | null;
  /** The immutable proposed body captured by the adoption preview. */
  after: WnsDocument;
  title?: string;
  /** Render only the lightweight summary or only the detailed paragraph diff. */
  mode?: 'summary' | 'details' | 'all';
}

interface BlockDiff {
  index: number;
  before: string | null;
  after: string | null;
  status: 'unchanged' | 'changed' | 'added' | 'removed';
  tokens: DiffToken[];
}

export interface DocumentDiffSummary {
  changedBlocks: number;
  addedBlocks: number;
  removedBlocks: number;
  addedWords: number;
  removedWords: number;
}


/**
 * Keep the comparison deterministic and deliberately structural. Block index is
 * the only alignment rule; this component must not make semantic claims about
 * why the author changed the text.
 */

function words(text: string): string[] { return text.match(/\S+/gu) ?? []; }

function wordDiff(before: string, after: string): DiffToken[] {
  const left = words(before);
  const right = words(after);
  // A long block should remain cheap and legible in the review surface. The
  // paragraph-level change is still exact when the fine-grained diff is capped.
  if (left.length > 240 || right.length > 240) return [
    ...(before ? [{ kind: 'removed' as const, text: before }] : []),
    ...(after ? [{ kind: 'added' as const, text: after }] : []),
  ];
  const table: number[][] = Array.from({ length: left.length + 1 }, () => Array<number>(right.length + 1).fill(0));
  for (let i = left.length - 1; i >= 0; i -= 1) {
    for (let j = right.length - 1; j >= 0; j -= 1) {
      table[i][j] = left[i] === right[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const result: DiffToken[] = [];
  const push = (kind: DiffToken['kind'], text: string) => {
    const previous = result.at(-1);
    if (previous?.kind === kind) previous.text += ` ${text}`;
    else result.push({ kind, text });
  };
  let i = 0; let j = 0;
  while (i < left.length && j < right.length) {
    if (left[i] === right[j]) { push('same', left[i]); i += 1; j += 1; }
    else if (table[i + 1][j] >= table[i][j + 1]) { push('removed', left[i]); i += 1; }
    else { push('added', right[j]); j += 1; }
  }
  while (i < left.length) { push('removed', left[i]); i += 1; }
  while (j < right.length) { push('added', right[j]); j += 1; }
  return result;
}

function blockDiff(before: string[], after: string[]): BlockDiff[] {
  const count = Math.max(before.length, after.length);
  return Array.from({ length: count }, (_, index) => {
    const left = before[index] ?? null;
    const right = after[index] ?? null;
    const status = left === null ? 'added' : right === null ? 'removed' : left === right ? 'unchanged' : 'changed';
    return { index, before: left, after: right, status, tokens: left !== null && right !== null ? wordDiff(left, right) : wordDiff(left ?? '', right ?? '') };
  });
}

function countTokens(tokens: DiffToken[], kind: DiffToken['kind']): number {
  return tokens.filter(token => token.kind === kind).reduce((total, token) => total + words(token.text).length, 0);
}

export function summarizeDocumentDiff(before: DocumentRecord | null, after: WnsDocument): DocumentDiffSummary {
  const blocks = blockDiff(documentBlocks(before?.body ?? null), documentBlocks(after));
  return {
    changedBlocks: blocks.filter(block => block.status === 'changed').length,
    addedBlocks: blocks.filter(block => block.status === 'added').length,
    removedBlocks: blocks.filter(block => block.status === 'removed').length,
    addedWords: blocks.reduce((sum, block) => sum + countTokens(block.tokens, 'added'), 0),
    removedWords: blocks.reduce((sum, block) => sum + countTokens(block.tokens, 'removed'), 0),
  };
}

function summaryText(summary: DocumentDiffSummary, before: DocumentRecord | null): string {
  if (before === null) return `New document: ${summary.addedBlocks} paragraph/block${summary.addedBlocks === 1 ? '' : 's'}, ${summary.addedWords} word${summary.addedWords === 1 ? '' : 's'} added.`;
  if (summary.changedBlocks === 0 && summary.addedBlocks === 0 && summary.removedBlocks === 0) return 'Text content is unchanged; formatting or block metadata is not summarized here.';
  const blocks: string[] = [];
  if (summary.changedBlocks) blocks.push(`${summary.changedBlocks} paragraph/block${summary.changedBlocks === 1 ? '' : 's'} changed`);
  if (summary.addedBlocks) blocks.push(`${summary.addedBlocks} paragraph/block${summary.addedBlocks === 1 ? '' : 's'} added`);
  if (summary.removedBlocks) blocks.push(`${summary.removedBlocks} paragraph/block${summary.removedBlocks === 1 ? '' : 's'} removed`);
  const wordsAdded = `${summary.addedWords} word${summary.addedWords === 1 ? '' : 's'} added`;
  const wordsRemoved = `${summary.removedWords} word${summary.removedWords === 1 ? '' : 's'} removed`;
  return `${blocks.join(', ')}; ${wordsAdded}; ${wordsRemoved}.`;
}

function renderTokens(tokens: DiffToken[], keyPrefix: string, side: 'before' | 'after') {
  const visible = tokens.filter(token => side === 'before' ? token.kind !== 'added' : token.kind !== 'removed');
  return visible.flatMap((token, index) => {
    const key = `${keyPrefix}-${index}`;
    const element = token.kind === 'removed'
      ? <del key={key}>{token.text}</del>
      : token.kind === 'added'
        ? <ins key={key}>{token.text}</ins>
        : <span key={key}>{token.text}</span>;
    return index === 0 ? [element] : [' ', element];
  });
}

function blockLabel(block: BlockDiff): string {
  const number = block.index + 1;
  if (block.status === 'added') return `Paragraph/block ${number}, added`;
  if (block.status === 'removed') return `Paragraph/block ${number}, removed`;
  if (block.status === 'changed') return `Paragraph/block ${number}, changed`;
  return `Paragraph/block ${number}, unchanged`;
}

export function DraftReviewDiff({ before, after, title, mode = 'all' }: DraftDocumentDiffProps) {
  const blocks = blockDiff(documentBlocks(before?.body ?? null), documentBlocks(after));
  const summary = summarizeDocumentDiff(before, after);
  return <section className="chat-draft-diff" aria-label={`Deterministic changes for ${title ?? 'proposed document'}`}>
    {(mode === 'summary' || mode === 'all') && <>
      <h4>What changed</h4>
      <p className="chat-draft-diff-summary">{summaryText(summary, before)}</p>
    </>}
    {(mode === 'details' || mode === 'all') && <details className="chat-draft-diff-details" open>
      <summary tabIndex={0}>Detailed word and paragraph changes</summary>
      <ol aria-label="Paragraph changes">
        {blocks.map(block => <li key={block.index} className={`chat-draft-diff-block is-${block.status}`}>
          <h5>{blockLabel(block)}</h5>
          {block.status === 'unchanged' ? <p>{block.before || 'Empty block'}</p> : <div className="chat-draft-diff-sides">
            <section aria-label={`Before paragraph/block ${block.index + 1}`}><strong>Before</strong><p>{block.before === null ? 'No paragraph/block at this position.' : renderTokens(block.tokens, `before-${block.index}`, 'before')}</p></section>
            <section aria-label={`After paragraph/block ${block.index + 1}`}><strong>After</strong><p>{block.after === null ? 'No paragraph/block at this position.' : renderTokens(block.tokens, `after-${block.index}`, 'after')}</p></section>
          </div>}
        </li>)}
      </ol>
    </details>}
  </section>;
}
