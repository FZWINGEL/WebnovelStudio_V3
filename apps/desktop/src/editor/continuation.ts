import { closeHistory } from '@tiptap/pm/history';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { EditorState, Transaction } from '@tiptap/pm/state';
import type { ScopeGrant } from '../ipc/context';

/** The first continuation payload is deliberately small and plain. */
export const MAX_CONTINUATION_PARAGRAPHS = 128;
export const MAX_CONTINUATION_PARAGRAPH_UTF16 = 8_192;
export const MAX_CONTINUATION_TOTAL_UTF16 = 100_000;

const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/u;
const HASH_PATTERN = /^[0-9a-f]{64}$/u;

type RuntimeAppendScope = ScopeGrant & { kind: string };

function appendScope(scope: ScopeGrant): RuntimeAppendScope {
  return scope as RuntimeAppendScope;
}

function error(message: string): Error {
  return new Error(`Invalid continuation: ${message}`);
}

function validId(id: unknown): id is string {
  return typeof id === 'string' && ID_PATTERN.test(id);
}

function blockText(block: PMNode): string {
  let text = '';
  block.forEach(child => {
    if (child.isText) text += child.text ?? '';
    else if (child.type.name === 'hardBreak') text += '\n';
  });
  return text;
}

function sameEndpoint(left: { blockId: string; utf16Offset: number } | null | undefined, right: { blockId: string; utf16Offset: number }): boolean {
  return !!left && left.blockId === right.blockId && left.utf16Offset === right.utf16Offset;
}

function sourceIds(doc: PMNode): Set<string> {
  const ids = new Set<string>();
  doc.forEach(block => {
    const id = block.attrs.id;
    if (!validId(id)) throw error('the source contains an invalid block ID');
    if (ids.has(id)) throw error(`the source contains duplicate block ID ${id}`);
    ids.add(id);
  });
  return ids;
}

function validateScope(state: EditorState, scope: ScopeGrant): void {
  const append = appendScope(scope);
  // Rust's append grant is deliberately narrower than a whole-document scope:
  // it authenticates only the final source block and its insertion endpoint.
  const kind = String(append.kind);
  if (kind !== 'append') throw error('append requires a typed append scope');
  if (scope.start !== null && scope.start !== undefined) throw error('append scope must not have a start endpoint');
  if (!scope.end) throw error('append requires an end endpoint');
  const first = state.doc.firstChild;
  const last = state.doc.lastChild;
  if (!first || !last) throw error('the source document has no blocks');
  const lastId = last.attrs.id as unknown;
  if (!validId(lastId)) throw error('the source endpoint has an invalid block ID');
  const endOffset = last.isTextblock ? last.content.size : 0;
  if (!sameEndpoint(scope.end, { blockId: lastId, utf16Offset: endOffset })) {
    throw error('the append scope must end at the end of the source document');
  }
  if (!HASH_PATTERN.test(scope.sourceHash) || !HASH_PATTERN.test(scope.quoteHash)) {
    throw error('the append scope must carry source and quote hashes');
  }
  if (scope.quote !== (last.type.name === 'sceneBreak' ? '' : blockText(last))) throw error('the append scope quote does not match the final source block');
  if (scope.prefix !== null || scope.suffix !== null) throw error('an append scope cannot have prefix or suffix context');
}

function validateParagraphs(paragraphs: string[]): void {
  if (!Array.isArray(paragraphs) || paragraphs.length === 0) throw error('at least one paragraph is required');
  if (paragraphs.length > MAX_CONTINUATION_PARAGRAPHS) throw error(`at most ${MAX_CONTINUATION_PARAGRAPHS} paragraphs are allowed`);
  let total = 0;
  paragraphs.forEach((paragraph, index) => {
    if (typeof paragraph !== 'string' || paragraph.trim().length === 0) throw error(`paragraph ${index + 1} must contain nonblank text`);
    if (/\r|\n/u.test(paragraph)) throw error(`paragraph ${index + 1} must be plain single-line text`);
    const units = paragraph.length;
    if (units > MAX_CONTINUATION_PARAGRAPH_UTF16) throw error(`paragraph ${index + 1} exceeds ${MAX_CONTINUATION_PARAGRAPH_UTF16} UTF-16 units`);
    total += units;
  });
  if (total > MAX_CONTINUATION_TOTAL_UTF16) throw error(`the continuation exceeds ${MAX_CONTINUATION_TOTAL_UTF16} UTF-16 units`);
}

function allocateIds(state: EditorState, paragraphs: string[], supplied: string[] | undefined, emptyChapter: boolean): string[] {
  const existing = sourceIds(state.doc);
  if (supplied && supplied.length !== paragraphs.length) throw error('the supplied ID count must match the paragraph count');
  const ids = supplied ? [...supplied] : [];
  const used = new Set(existing);
  const fresh = (): string => {
    let id = crypto.randomUUID();
    while (!validId(id) || used.has(id)) id = crypto.randomUUID();
    used.add(id);
    return id;
  };
  if (!supplied) {
    if (emptyChapter) {
      ids.push(String(state.doc.firstChild!.attrs.id));
      for (let index = 1; index < paragraphs.length; index += 1) ids.push(fresh());
    } else {
      for (let index = 0; index < paragraphs.length; index += 1) ids.push(fresh());
    }
  }
  const seen = new Set<string>();
  ids.forEach((id, index) => {
    if (!validId(id)) throw error(`paragraph ${index + 1} has an invalid block ID`);
    if (seen.has(id)) throw error(`paragraph ${index + 1} reuses a generated block ID`);
    const mayRetainPlaceholder = emptyChapter && index === 0 && id === state.doc.firstChild!.attrs.id;
    if (existing.has(id) && !mayRetainPlaceholder) throw error(`paragraph ${index + 1} reuses an existing block ID`);
    seen.add(id);
  });
  return ids;
}

function canonicalEmptyChapter(doc: PMNode): boolean {
  if (doc.childCount !== 1) return false;
  const block = doc.firstChild!;
  return block.type.name === 'paragraph'
    && block.content.size === 0
    && Object.keys(block.attrs).length === 1
    && validId(block.attrs.id);
}

/**
 * Prepare one append transaction without dispatching it.
 *
 * `sourceHash` and `quoteHash` are persisted Rust receipts. Their cryptographic
 * validation is asynchronous and belongs to the caller's source check; this
 * synchronous helper validates the exact endpoint and final-block quote before
 * it creates a ProseMirror transaction.
 */
export function prepareContinuation(state: EditorState, scope: ScopeGrant, paragraphs: string[], ids?: string[]): Transaction {
  validateScope(state, scope);
  validateParagraphs(paragraphs);
  const emptyChapter = canonicalEmptyChapter(state.doc);
  const paragraphIds = allocateIds(state, paragraphs, ids, emptyChapter);
  const nodes = paragraphs.map((text, index) => state.schema.nodes.paragraph.create({ id: paragraphIds[index] }, state.schema.text(text)));
  const transaction = closeHistory(state.tr);
  if (emptyChapter) {
    transaction.insertText(paragraphs[0], 1);
    if (nodes.length > 1) transaction.insert(transaction.doc.content.size, nodes.slice(1));
  } else {
    transaction.insert(transaction.doc.content.size, nodes);
  }
  return transaction;
}
