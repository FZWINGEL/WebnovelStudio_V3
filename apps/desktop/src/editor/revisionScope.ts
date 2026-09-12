import type { DiscussionScope } from '../ipc/discussions';
import type { Block, WnsDocument } from '../kernel';

export function blockText(block: Block): string {
  return block.type === 'sceneBreak' ? '' : (block.content ?? []).map(inline => inline.type === 'hardBreak' ? '\n' : inline.text).join('');
}

/** Match the restricted snapshot quotation, preserving literal hard breaks. */
export function blocksQuote(blocks: Block[]): string {
  const parts = blocks.map(blockText);
  while (parts.length && parts.at(-1) === '') parts.pop();
  return parts.join('\n\n');
}

/** Called only after the author explicitly chooses a broader editing scope. */
export function captureRevisionScope(body: WnsDocument, sourceBodyHash: string, kind: 'blocks' | 'wholeDocument', selected?: DiscussionScope | null): DiscussionScope {
  if (kind === 'wholeDocument') return { kind, start: null, end: null, quote: blocksQuote(body.body.content), sourceBodyHash };
  if (!selected?.start || !selected.end || selected.sourceBodyHash !== sourceBodyHash) throw new Error('Select the passage again before choosing its paragraphs.');
  const first = body.body.content.findIndex(block => block.attrs.id === selected.start!.blockId);
  const last = body.body.content.findIndex(block => block.attrs.id === selected.end!.blockId);
  if (first < 0 || last < first) throw new Error('The selected paragraphs are no longer available.');
  const blocks = body.body.content.slice(first, last + 1);
  return { kind, sourceBodyHash, start: { blockId: blocks[0].attrs.id, utf16Offset: 0 }, end: { blockId: blocks.at(-1)!.attrs.id, utf16Offset: blockText(blocks.at(-1)!).length }, quote: blocksQuote(blocks) };
}
