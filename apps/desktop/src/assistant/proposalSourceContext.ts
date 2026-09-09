import type { Block, Inline, WnsDocument } from '../editor/document';
import { blockText, blocksQuote } from '../editor/revisionScope';
import type { ScopeGrant } from '../ipc/context';
import type { Head } from '../ipc/projects';
import type { StructuredBlock } from '../ipc/proposals';

export type ProposalSourceContextPart = {
  label: string;
  blocks?: StructuredBlock[];
  text?: string;
};

export interface ProposalSourceContext {
  sourceVersion: string;
  scopeLabel: string;
  parts: ProposalSourceContextPart[];
  note?: string;
  unavailable?: string;
}

const scopeLabels: Record<ScopeGrant['kind'], string> = {
  passage: 'Selected passage',
  blocks: 'Selected paragraphs',
  wholeDocument: 'Whole document',
  append: 'Continuation after chapter ending',
};

function structuredBlock(block: Block, content?: Inline[]): StructuredBlock {
  if (block.type === 'sceneBreak') return { type: 'sceneBreak' };
  if (block.type === 'heading') return { type: 'heading', attrs: { level: block.attrs.level }, content: content ?? block.content ?? [] };
  return { type: 'paragraph', content: content ?? block.content ?? [] };
}

function structuredBlocks(blocks: Block[]): StructuredBlock[] {
  return blocks.map(block => structuredBlock(block));
}

function inlineLength(inline: Inline): number {
  return inline.type === 'hardBreak' ? 1 : inline.text.length;
}

function sliceBlock(block: Block, start: number, end: number): StructuredBlock | null {
  if (block.type === 'sceneBreak') return null;
  if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start || end > blockText(block).length) return null;
  const content: Inline[] = [];
  let offset = 0;
  for (const inline of block.content ?? []) {
    const length = inlineLength(inline);
    const from = Math.max(start, offset);
    const to = Math.min(end, offset + length);
    if (to > from) {
      if (inline.type === 'hardBreak') content.push({ type: 'hardBreak' });
      else content.push({ ...inline, text: inline.text.slice(from - offset, to - offset) });
    }
    offset += length;
  }
  return structuredBlock(block, content);
}

function uniqueBlock(blocks: Block[], id: string | undefined): { block: Block; index: number } | null {
  if (!id) return null;
  const matches = blocks.flatMap((block, index) => block.attrs.id === id ? [{ block, index }] : []);
  return matches.length === 1 ? matches[0] : null;
}

function endpointValid(endpoint: ScopeGrant['start'], block: Block, allowSceneBreak = false): endpoint is NonNullable<ScopeGrant['start']> {
  if (!endpoint || (block.type === 'sceneBreak' && !allowSceneBreak) || !Number.isInteger(endpoint.utf16Offset) || endpoint.utf16Offset < 0) return false;
  return endpoint.utf16Offset <= blockText(block).length;
}

function unavailable(sourceVersion: string, scope: ScopeGrant, message: string): ProposalSourceContext {
  return { sourceVersion, scopeLabel: scopeLabels[scope.kind] ?? 'Unsupported scope', parts: [], unavailable: `Protected source context unavailable: ${message}` };
}

/**
 * Derive review context from the proposal's immutable source snapshot. This
 * intentionally does not inspect the live editor or current document head, so
 * stale and historical proposals remain honest about what they used.
 */
export function deriveProposalSourceContext(source: Head, sourceBody: WnsDocument, scope: ScopeGrant): ProposalSourceContext {
  const sourceVersion = source.version;
  if (!Object.prototype.hasOwnProperty.call(scopeLabels, scope.kind)) return unavailable(sourceVersion, scope, 'the proposal scope is unsupported.');
  const blocks = sourceBody?.body?.content;
  if (!Array.isArray(blocks) || sourceBody.schemaVersion !== 1) return unavailable(sourceVersion, scope, 'the captured source document is malformed.');
  if (scope.sourceHash !== source.bodyHash) return unavailable(sourceVersion, scope, 'the scope does not match the captured source version.');

  try {
    if (scope.kind === 'wholeDocument') {
      if (scope.start || scope.end || scope.quote !== blocksQuote(blocks)) return unavailable(sourceVersion, scope, 'the whole-document boundaries do not match the captured source.');
      return {
        sourceVersion,
        scopeLabel: scopeLabels[scope.kind],
        parts: [{ label: 'Frozen source document', blocks: structuredBlocks(blocks) }],
        note: 'No outside prose is protected because this proposal covers the whole source document.',
      };
    }

    if (scope.kind === 'append') {
      const ending = uniqueBlock(blocks, scope.end?.blockId);
      if (!ending || ending.index !== blocks.length - 1 || scope.start || !endpointValid(scope.end, ending.block, true) || scope.end!.utf16Offset !== blockText(ending.block).length || scope.quote !== blockText(ending.block) || scope.prefix !== null || scope.suffix !== null) {
        return unavailable(sourceVersion, scope, 'the continuation anchor does not match the captured chapter ending.');
      }
      return {
        sourceVersion,
        scopeLabel: scopeLabels[scope.kind],
        parts: [{ label: 'Frozen source before continuation', blocks: structuredBlocks(blocks) }],
        note: 'The existing source is protected; only new paragraphs after the chapter ending are editable.',
      };
    }

    const start = uniqueBlock(blocks, scope.start?.blockId);
    const end = uniqueBlock(blocks, scope.end?.blockId);
    if (!start || !end || !endpointValid(scope.start, start.block, scope.kind === 'blocks') || !endpointValid(scope.end, end.block, scope.kind === 'blocks') || start.index > end.index) {
      return unavailable(sourceVersion, scope, 'the selected source boundaries are unavailable.');
    }

    if (scope.kind === 'blocks') {
      if (scope.start!.utf16Offset !== 0 || scope.end!.utf16Offset !== blockText(end.block).length || scope.quote !== blocksQuote(blocks.slice(start.index, end.index + 1))) {
        return unavailable(sourceVersion, scope, 'the selected paragraph boundaries do not match the captured source.');
      }
      return {
        sourceVersion,
        scopeLabel: scopeLabels[scope.kind],
        parts: [
          { label: 'Frozen source before selected paragraphs', blocks: structuredBlocks(blocks.slice(0, start.index)) },
          { label: 'Selected paragraphs', blocks: structuredBlocks(blocks.slice(start.index, end.index + 1)) },
          { label: 'Frozen source after selected paragraphs', blocks: structuredBlocks(blocks.slice(end.index + 1)) },
        ],
      };
    }

    const startOffset = scope.start!.utf16Offset;
    const endOffset = scope.end!.utf16Offset;
    const selected = start.index === end.index
      ? blockText(start.block).slice(startOffset, endOffset)
      : [blockText(start.block).slice(startOffset), ...blocks.slice(start.index + 1, end.index).map(blockText), blockText(end.block).slice(0, endOffset)].join('\n\n');
    if (scope.quote !== selected) return unavailable(sourceVersion, scope, 'the selected passage does not match the captured source.');
    const prefix = sliceBlock(start.block, 0, startOffset);
    const selectedBlocks = start.index === end.index
      ? [sliceBlock(start.block, startOffset, endOffset)].filter((block): block is StructuredBlock => !!block)
      : [
        sliceBlock(start.block, startOffset, blockText(start.block).length),
        ...structuredBlocks(blocks.slice(start.index + 1, end.index)),
        sliceBlock(end.block, 0, endOffset),
      ].filter((block): block is StructuredBlock => !!block);
    const suffix = sliceBlock(end.block, endOffset, blockText(end.block).length);
    const prefixText = blockText(start.block).slice(0, startOffset);
    const suffixText = blockText(end.block).slice(endOffset);
    if ((scope.prefix !== null && scope.prefix !== prefixText) || (scope.suffix !== null && scope.suffix !== suffixText)) {
      return unavailable(sourceVersion, scope, 'the protected passage context does not match the captured source.');
    }
    return {
      sourceVersion,
      scopeLabel: scopeLabels[scope.kind],
      parts: [
        { label: 'Frozen source before selection', blocks: [...structuredBlocks(blocks.slice(0, start.index)), ...(prefix ? [prefix] : [])] },
        { label: 'Selected passage', blocks: selectedBlocks },
        { label: 'Frozen source after selection', blocks: [...(suffix ? [suffix] : []), ...structuredBlocks(blocks.slice(end.index + 1))] },
      ],
    };
  } catch {
    return unavailable(sourceVersion, scope, 'the captured source boundaries are malformed.');
  }
}
