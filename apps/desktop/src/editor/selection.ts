import type { Editor } from '@tiptap/core';
import { Fragment, Slice, type Mark, type Node as PMNode } from '@tiptap/pm/model';
import { TextSelection, type Transaction } from '@tiptap/pm/state';
import { ReplaceStep } from '@tiptap/pm/transform';
import { closeHistory } from '@tiptap/pm/history';

export interface Scope {
  generation: number;
  from: number;
  to: number;
  quote: string;
  source: PMNode;
  start: { blockId: string; utf16Offset: number };
  end: { blockId: string; utf16Offset: number };
  inlineOnly: boolean;
  replacementAllowed: boolean;
  uniformMarks: readonly Mark[];
  formattingNote: string;
}

function snap(text: string, offset: number, end: boolean): number {
  const segments = [...new Intl.Segmenter('und', { granularity: 'grapheme' }).segment(text)];
  for (const segment of segments) {
    const right = segment.index + segment.segment.length;
    if (offset > segment.index && offset < right) return end ? right : segment.index;
  }
  return offset;
}

export function captureSelection(editor: Editor): Scope | null {
  const { doc, selection } = editor.state;
  if (selection.empty || !selection.$from.parent.isTextblock || !selection.$to.parent.isTextblock) return null;
  const startText = selection.$from.parent.textBetween(0, selection.$from.parent.content.size, '', '\n');
  const endText = selection.$to.parent.textBetween(0, selection.$to.parent.content.size, '', '\n');
  const startOffset = snap(startText, selection.$from.parentOffset, false);
  const endOffset = snap(endText, selection.$to.parentOffset, true);
  const from = selection.$from.start() + startOffset;
  const to = selection.$to.start() + endOffset;
  const selectedMarks: (readonly Mark[])[] = [];
  let compatible = true;
  doc.nodesBetween(from, to, node => {
    if (node.isBlock && (!node.isTextblock || node.type !== selection.$from.parent.type || node.attrs.level !== selection.$from.parent.attrs.level)) compatible = false;
    if (node.isInline) selectedMarks.push(node.marks);
  });
  const first = selectedMarks[0] ?? [];
  const uniform = selectedMarks.every(marks => JSON.stringify(marks) === JSON.stringify(first));
  return {
    generation: generation(editor),
    from, to, quote: doc.textBetween(from, to, '\n\n', '\n'), source: doc,
    start: { blockId: selection.$from.parent.attrs.id, utf16Offset: startOffset },
    end: { blockId: selection.$to.parent.attrs.id, utf16Offset: endOffset },
    inlineOnly: selection.$from.sameParent(selection.$to), replacementAllowed: compatible,
    uniformMarks: uniform ? first : [],
    formattingNote: uniform ? (first.length ? 'Replacement keeps the selected formatting.' : 'Replacement uses plain text.') : 'Mixed formatting: replacement uses plain text; surrounding formatting stays.',
  };
}

export function prepareReplacement(editor: Editor, scope: Scope, text: string): Transaction {
  if (generation(editor) !== scope.generation || !editor.state.doc.eq(scope.source)) throw new Error('The chapter changed. Select the passage again before applying.');
  if (!scope.replacementAllowed) throw new Error('This trial cannot replace scene breaks or mixed block styles. Select text within matching paragraphs.');
  if (/[\r\n]/u.test(text)) throw new Error('Use a single line for this trial replacement. Paragraph restructuring comes in the next contract slice.');
  if (text.length > 100_000) throw new Error('The trial replacement is too large. Use a shorter passage.');
  const content = text ? Fragment.from(editor.schema.text(text, scope.uniformMarks)) : Fragment.empty;
  const tr = closeHistory(editor.state.tr);
  const result = tr.maybeStep(new ReplaceStep(scope.from, scope.to, new Slice(content, 0, 0)));
  if (result.failed) throw new Error(`This selection cannot be replaced exactly: ${result.failed}`);
  tr.setSelection(TextSelection.create(tr.doc, scope.from + text.length));
  tr.setMeta('localTrialApply', true);
  return tr;
}

export function generation(editor: Editor): number {
  return (editor.storage as unknown as { generation: { value: number } }).generation.value;
}
