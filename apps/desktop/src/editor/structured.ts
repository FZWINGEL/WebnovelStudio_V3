import { closeHistory } from '@tiptap/pm/history';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { EditorState, Transaction } from '@tiptap/pm/state';
import type { ScopeGrant } from '../ipc/context';
import type { StructuredBlock } from '../ipc/proposals';
import { safeHref, snapshotFromEditor, type Block } from '../kernel';
import { blocksQuote } from './revisionScope';

const idPattern = /^[A-Za-z0-9_-]{1,64}$/u;
function object(value: unknown, allowed: string[]): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(key => !allowed.includes(key))) throw new Error('The suggested formatting contains unsupported fields.');
  return value as Record<string, unknown>;
}

export function validateStructuredBlocks(value: unknown): asserts value is StructuredBlock[] {
  if (!Array.isArray(value) || value.length > 128) throw new Error('A suggestion can contain up to 128 paragraphs, headings, and scene breaks.');
  let units = 0;
  for (const item of value) {
    const block = object(item, ['type', 'attrs', 'content']);
    if (block.type === 'sceneBreak') { object(block, ['type']); continue; }
    if (block.type !== 'paragraph' && block.type !== 'heading') throw new Error('The suggested paragraph style is unsupported.');
    if (block.type === 'heading') {
      const attrs = object(block.attrs, ['level']);
      if (![1, 2, 3].includes(attrs.level as number)) throw new Error('Use heading levels 1 to 3.');
    } else if ('attrs' in block) throw new Error('Suggested paragraphs cannot assign block identities.');
    if (!Array.isArray(block.content)) throw new Error('The suggested paragraph content is missing.');
    for (const child of block.content) {
      const inline = object(child, ['type', 'text', 'marks']);
      if (inline.type === 'hardBreak') { object(inline, ['type']); units += 1; continue; }
      if (inline.type !== 'text' || typeof inline.text !== 'string' || !inline.text.length || /[\r\n]/u.test(inline.text)) throw new Error('Use paragraph breaks or Shift+Enter for new lines.');
      units += inline.text.length;
      if (inline.marks !== undefined && !Array.isArray(inline.marks)) throw new Error('The suggested text formatting is invalid.');
      const seen = new Set<string>();
      for (const item of (inline.marks ?? []) as unknown[]) {
        const mark = object(item, ['type', 'attrs']);
        if (typeof mark.type !== 'string' || !['bold', 'italic', 'link'].includes(mark.type) || seen.has(mark.type)) throw new Error('The suggested text repeats or uses unsupported formatting.');
        seen.add(mark.type);
        if (mark.type === 'link') {
          const attrs = object(mark.attrs, ['href']);
          if (typeof attrs.href !== 'string' || !safeHref(attrs.href)) throw new Error('The suggested link is unsafe or invalid.');
        } else object(mark, ['type']);
      }
    }
  }
  if (units > 100_000) throw new Error('The suggestion is too long. Revise a smaller section.');
}

export function structuredEditorBlocks(blocks: StructuredBlock[], ids: string[]): Block[] {
  validateStructuredBlocks(blocks);
  if (ids.length !== blocks.length || new Set(ids).size !== ids.length || ids.some(id => !idPattern.test(id))) throw new Error('The suggestion needs a unique identity for each paragraph.');
  return blocks.map((block, index) => ({ ...structuredClone(block), attrs: { ...(block.type === 'heading' ? block.attrs : {}), id: ids[index] } }) as Block);
}

export function structuredFromEditor(doc: PMNode): StructuredBlock[] {
  return snapshotFromEditor(doc.toJSON()).body.content.map(block => block.type === 'sceneBreak'
    ? { type: 'sceneBreak' }
    : block.type === 'heading'
      ? { type: 'heading', attrs: { level: block.attrs.level }, content: block.content ?? [] }
      : { type: 'paragraph', content: block.content ?? [] });
}

/** Locate the captured full blocks; a passage or append never grants this edit. */
export function structuredRange(doc: PMNode, scope: ScopeGrant): { first: number; last: number; from: number; to: number } {
  if (scope.kind !== 'blocks' && scope.kind !== 'wholeDocument') throw new Error('This suggestion needs an explicit paragraph or whole-chapter scope.');
  const all: Array<{ node: PMNode; pos: number }> = [];
  doc.forEach((node, pos) => all.push({ node, pos }));
  let first = 0; let last = all.length - 1;
  if (scope.kind === 'wholeDocument') {
    if (scope.start || scope.end) throw new Error('The whole-chapter scope has unexpected endpoints.');
  } else {
    first = all.findIndex(block => block.node.attrs.id === scope.start?.blockId);
    last = all.findIndex(block => block.node.attrs.id === scope.end?.blockId);
    if (first < 0 || last < first || scope.start?.utf16Offset !== 0 || scope.end?.utf16Offset !== (all[last].node.isTextblock ? all[last].node.content.size : 0)) throw new Error('The scope must select complete paragraphs at their exact boundaries.');
  }
  if (last < 0) throw new Error('The original chapter has no valid blocks.');
  const source = snapshotFromEditor(doc.toJSON()).body.content.slice(first, last + 1);
  if (blocksQuote(source) !== scope.quote) throw new Error('The selected paragraphs no longer match their exact source.');
  return { first, last, from: all[first].pos, to: all[last].pos + all[last].node.nodeSize };
}

/** IDs are supplied on replay; allocation happens only on first preparation. */
export function prepareStructuredReplacement(state: EditorState, scope: ScopeGrant, blocks: StructuredBlock[], ids?: string[]): Transaction {
  validateStructuredBlocks(blocks);
  const range = structuredRange(state.doc, scope);
  if (!blocks.length && range.first === 0 && range.last === state.doc.childCount - 1) throw new Error('Keep at least one paragraph in the chapter.');
  const used = new Set<string>();
  state.doc.forEach(node => used.add(node.attrs.id as string));
  const fresh = ids ?? blocks.map(() => crypto.randomUUID());
  if (fresh.some(id => used.has(id))) throw new Error('Replacement paragraphs cannot reuse source block identities.');
  const nodes = structuredEditorBlocks(blocks, fresh).map(block => state.schema.nodeFromJSON(block));
  const tr = closeHistory(state.tr).replaceWith(range.from, range.to, nodes);
  if (tr.doc.childCount !== state.doc.childCount - (range.last - range.first + 1) + blocks.length) throw new Error('The editor changed the requested paragraph boundaries.');
  return tr;
}
