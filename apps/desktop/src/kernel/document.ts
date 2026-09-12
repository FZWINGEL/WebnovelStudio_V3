import type { SnapshotReceipt as WireSnapshotReceipt } from '../ipc/generated/kernel';
import type { TypedReplacementInline, TypedReplacementMark } from '../ipc/generated/documents';

export type Mark = TypedReplacementMark;
export type Inline = TypedReplacementInline;
export type Block =
  | { type: 'paragraph'; attrs: { id: string }; content?: Inline[] }
  | { type: 'heading'; attrs: { id: string; level: number }; content?: Inline[] }
  | { type: 'sceneBreak'; attrs: { id: string } };
export interface WnsDocument { schemaVersion: 1; body: { type: 'doc'; content: Block[] } }
// Generated from Rust; the snapshot body is narrowed to the editor's model,
// which is the same refinement `ipc/projects` makes for a document.
export type SnapshotReceipt = Omit<WireSnapshotReceipt, 'snapshot'> & { snapshot: WnsDocument };

export function safeHref(href: string): boolean {
  if (!/^(https?:\/\/|mailto:)/iu.test(href) || /[\s\p{Cc}]/u.test(href) || href.includes('\\')) return false;
  try {
    const url = new URL(href);
    if (url.protocol === 'mailto:') {
      if (href.includes('%')) return false;
      const [local, domain] = url.pathname.split('@');
      return /^[^@/?#]+@[^@/?#]+\.[^@/?#]+$/u.test(url.pathname) && !url.search && !url.hash
        && !local.startsWith('.') && !local.endsWith('.') && !domain.startsWith('.') && !domain.endsWith('.');
    }
    return ['https:', 'http:'].includes(url.protocol) && !!url.hostname && !url.username && !url.password;
  } catch { return false; }
}

function sorted(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sorted);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([k, v]) => [k, sorted(v)]));
  }
  return value;
}

// The restricted adapter canonicalizes editor output. Rust validates untrusted input.
export function snapshotFromEditor(body: unknown): WnsDocument {
  const result = structuredClone(body) as WnsDocument['body'];
  for (const block of result.content) {
    if (block.type === 'sceneBreak') continue;
    const inline: Inline[] = [];
    for (const node of block.content ?? []) {
      if (node.type === 'text') {
        node.marks?.sort((a, b) => ['bold', 'italic', 'link'].indexOf(a.type) - ['bold', 'italic', 'link'].indexOf(b.type));
        if (!node.marks?.length) delete node.marks;
        const previous = inline.at(-1);
        if (previous?.type === 'text' && JSON.stringify(previous.marks) === JSON.stringify(node.marks)) {
          previous.text += node.text;
          continue;
        }
      }
      inline.push(node);
    }
    if (inline.length) block.content = inline;
    else delete block.content;
  }
  return { schemaVersion: 1, body: result };
}

export function canonicalJson(snapshot: unknown): string { return JSON.stringify(sorted(snapshot)); }
export async function bodyHash(json: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(json));
  return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
}

export const sample: WnsDocument['body'] = {
  type: 'doc',
  content: [
    { type: 'paragraph', attrs: { id: 'sample-opening' }, content: [
      { type: 'text', text: 'By dusk, every lantern in the harbour had gone dark. All but one.' },
    ] },
    { type: 'paragraph', attrs: { id: 'sample-choice' }, content: [
      { type: 'text', text: 'Mei stood at the end of the pier, turning a brass key between her fingers. ' },
      { type: 'text', text: 'Wait for me.', marks: [{ type: 'italic' }] },
      { type: 'text', text: ' Her brother had said it that morning. He had said it every morning for eleven years.' },
    ] },
    { type: 'paragraph', attrs: { id: 'sample-lantern' }, content: [
      { type: 'text', text: 'Beyond the harbour, the sect’s lantern still burned. Mei steadied her breathing. The qi beneath her ribs would not settle.' },
    ] },
    { type: 'sceneBreak', attrs: { id: 'sample-break' } },
    { type: 'paragraph', attrs: { id: 'sample-ending' }, content: [
      { type: 'text', text: 'The last lantern moved. Against the tide.' },
    ] },
  ],
};

/** One line per block, for comparing two revisions of the same document. */
function inlineText(value: { type: string; text?: string }): string {
  return value.type === 'hardBreak' ? '\n' : value.text ?? '';
}

/**
 * The text of each block, in order, with a scene break as an em dash.
 *
 * It lived in `chat/DraftReviewDiff` and was imported from there by
 * `assistant/SourceVersionComparison`, which made the two features mutually
 * dependent for the sake of one pure function over a document. It is a
 * document helper, so it belongs with the document model.
 */
export function documentBlocks(document: WnsDocument | null): string[] {
  if (!document) return [];
  return document.body.content.map(block => block.type === 'sceneBreak' ? '—' : (block.content ?? []).map(inlineText).join(''));
}
