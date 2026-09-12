import type { WnsDocument } from '../editor';

export function plainText(document: WnsDocument): string {
  return document.body.content.map(block => 'content' in block ? block.content?.map(node => node.type === 'text' ? node.text : '\n').join('') ?? '' : '').join('\n\n');
}
/** New paragraphs receive stable IDs when the preview is prepared, not on rerender. */
export function textDocument(text: string): WnsDocument {
  return { schemaVersion: 1, body: { type: 'doc', content: text.split(/\n\s*\n/).map(paragraph => ({ type: 'paragraph' as const, attrs: { id: crypto.randomUUID() }, ...(paragraph ? { content: paragraph.split('\n').flatMap((line, index) => [...(index ? [{ type: 'hardBreak' as const }] : []), ...(line ? [{ type: 'text' as const, text: line }] : [])]) } : {}) })) } };
}
export function appendText(document: WnsDocument, text: string): WnsDocument {
  return { ...document, body: { ...document.body, content: [...document.body.content, ...textDocument(text).body.content] } };
}
