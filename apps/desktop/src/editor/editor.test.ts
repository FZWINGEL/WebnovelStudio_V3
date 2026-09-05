import { Editor, type JSONContent } from '@tiptap/core';
import { Fragment, Slice } from '@tiptap/pm/model';
import { redo, undo } from '@tiptap/pm/history';
import { NodeSelection, TextSelection } from '@tiptap/pm/state';
import { afterEach, describe, expect, it } from 'vitest';
import { editorExtensions } from './schema';
import { captureSelection, generation, prepareReplacement } from './selection';

const editors: Editor[] = [];
let fallbackId = 0;

if (!globalThis.crypto?.randomUUID) {
  Object.defineProperty(globalThis.crypto, 'randomUUID', {
    configurable: true,
    value: () => `test-paste-${++fallbackId}`,
  });
}

function createEditor(content: JSONContent[]): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const editor = new Editor({
    element,
    extensions: editorExtensions,
    content: { type: 'doc', content },
  });
  editors.push(editor);
  return editor;
}

function blockIds(editor: Editor): string[] {
  const ids: string[] = [];
  editor.state.doc.forEach(node => ids.push(String(node.attrs.id)));
  return ids;
}

function blockContentStart(editor: Editor, index: number): number {
  let position = 1;
  for (let current = 0; current < index; current += 1) position += editor.state.doc.child(current).nodeSize;
  return position;
}

function select(editor: Editor, from: number, to = from): void {
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, from, to)));
}

function transformedPaste(editor: Editor, slice: Slice): Slice {
  return editor.view.someProp('transformPasted', handler => handler(slice, editor.view, false)) ?? slice;
}

afterEach(() => {
  while (editors.length) editors.pop()?.destroy();
  document.body.replaceChildren();
});

describe('restricted editor identity and replacement behavior', () => {
  it.each([
    ['paragraph', 'beginning', 0],
    ['paragraph', 'middle', 5],
    ['paragraph', 'end', 10],
    ['heading', 'beginning', 0],
    ['heading', 'middle', 5],
    ['heading', 'end', 10],
  ] as const)('splitting a %s at the %s preserves the left ID and creates one fresh ID', (nodeType, _label, offset) => {
    const attrs = nodeType === 'heading' ? { id: 'split-source', level: 2 } : { id: 'split-source' };
    const editor = createEditor([
      { type: nodeType, attrs, content: [{ type: 'text', text: 'alpha beta' }] },
    ]);
    select(editor, blockContentStart(editor, 0) + offset);

    expect(editor.commands.splitBlock()).toBe(true);
    const ids = blockIds(editor);
    expect(ids).toHaveLength(2);
    expect(ids[0]).toBe('split-source');
    expect(new Set(ids).size).toBe(2);
    expect(ids.every(id => /^[A-Za-z0-9_-]{1,64}$/u.test(id))).toBe(true);

    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(blockIds(editor)).toEqual(['split-source']);
    expect(redo(editor.state, editor.view.dispatch)).toBe(true);
    const redoneIds = blockIds(editor);
    expect(redoneIds[0]).toBe('split-source');
    expect(new Set(redoneIds).size).toBe(2);
  });

  it('merging left keeps the left ID and undo restores both IDs', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'left' }, content: [{ type: 'text', text: 'left' }] },
      { type: 'paragraph', attrs: { id: 'right' }, content: [{ type: 'text', text: 'right' }] },
    ]);
    select(editor, blockContentStart(editor, 1));

    expect(editor.commands.joinBackward()).toBe(true);
    expect(blockIds(editor)).toEqual(['left']);
    expect(editor.state.doc.textContent).toBe('leftright');

    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(blockIds(editor)).toEqual(['left', 'right']);
  });

  it('assigns fresh IDs to every copied pasted block', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'existing' }, content: [{ type: 'text', text: 'existing' }] },
    ]);
    const paragraph = editor.schema.nodeFromJSON({
      type: 'paragraph', attrs: { id: 'copied-paragraph' }, content: [{ type: 'text', text: 'copy' }],
    });
    const heading = editor.schema.nodeFromJSON({
      type: 'heading', attrs: { id: 'copied-heading', level: 2 }, content: [{ type: 'text', text: 'heading' }],
    });
    const transformed = transformedPaste(editor, new Slice(Fragment.fromArray([paragraph, heading]), 0, 0));
    const ids: string[] = [];
    transformed.content.forEach(node => ids.push(String(node.attrs.id)));

    expect(ids).toHaveLength(2);
    expect(ids).not.toContain('copied-paragraph');
    expect(ids).not.toContain('copied-heading');
    expect(new Set(ids).size).toBe(2);
    expect(ids.every(id => /^[A-Za-z0-9_-]{1,64}$/u.test(id))).toBe(true);
  });

  it('inserts transformed pasted blocks and undo restores the original identity', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'existing' }, content: [{ type: 'text', text: 'existing' }] },
    ]);
    const paragraph = editor.schema.nodeFromJSON({
      type: 'paragraph', attrs: { id: 'copied-paragraph' }, content: [{ type: 'text', text: 'copy' }],
    });
    const heading = editor.schema.nodeFromJSON({
      type: 'heading', attrs: { id: 'copied-heading', level: 2 }, content: [{ type: 'text', text: 'heading' }],
    });
    const transformed = transformedPaste(editor, new Slice(Fragment.fromArray([paragraph, heading]), 0, 0));
    const pastedIds: string[] = [];
    transformed.content.forEach(node => pastedIds.push(String(node.attrs.id)));

    select(editor, 0);
    editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, 0)));
    editor.view.dispatch(editor.state.tr.replaceSelection(transformed));

    expect(blockIds(editor)).toEqual(pastedIds);
    expect(blockIds(editor)).not.toContain('copied-paragraph');
    expect(blockIds(editor)).not.toContain('copied-heading');
    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(blockIds(editor)).toEqual(['existing']);
  });

  it('replaces the selected repeated emoji using exact UTF-16 offsets', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'unicode' }, content: [{ type: 'text', text: 'A🙂B🙂C' }] },
    ]);
    const start = blockContentStart(editor, 0);
    select(editor, start + 4, start + 6);
    const scope = captureSelection(editor);

    expect(scope).not.toBeNull();
    expect(scope?.start.utf16Offset).toBe(4);
    expect(scope?.end.utf16Offset).toBe(6);
    expect(scope?.quote).toBe('🙂');

    const replacement = prepareReplacement(editor, scope!, 'Y');
    editor.view.dispatch(replacement);
    expect(editor.state.doc.textContent).toBe('A🙂BYC');
  });

  it.each([
    ['half-surrogate', 'A🙂B', 2, 3, '🙂'],
    ['combining-sequence', 'Ae\u0301B', 2, 3, 'e\u0301'],
    ['ZWJ sequence', 'A👩‍💻B', 2, 3, '👩‍💻'],
  ] as const)('snaps %s selections to whole graphemes and preserves block identity', (_label, text, fromOffset, toOffset, quote) => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'grapheme' }, content: [{ type: 'text', text }] },
    ]);
    const start = blockContentStart(editor, 0);
    select(editor, start + fromOffset, start + toOffset);
    const scope = captureSelection(editor);

    expect(scope).not.toBeNull();
    expect(scope?.quote).toBe(quote);
    expect(scope?.start.utf16Offset).toBe(text.indexOf(quote));
    expect(scope?.end.utf16Offset).toBe(text.indexOf(quote) + quote.length);
    editor.view.dispatch(prepareReplacement(editor, scope!, 'X'));
    expect(editor.state.doc.textContent).toBe(`AX${text.slice(1 + quote.length)}`);
    expect(blockIds(editor)).toEqual(['grapheme']);
  });

  it('merges a cross-paragraph replacement with left identity and uniform formatting', () => {
    const editor = createEditor([
      {
        type: 'paragraph', attrs: { id: 'cross-left' },
        content: [{ type: 'text', text: 'left', marks: [{ type: 'bold' }] }],
      },
      {
        type: 'paragraph', attrs: { id: 'cross-right' },
        content: [{ type: 'text', text: 'right', marks: [{ type: 'bold' }] }],
      },
    ]);
    const leftStart = blockContentStart(editor, 0);
    const rightStart = blockContentStart(editor, 1);
    select(editor, leftStart + 2, rightStart + 2);
    const scope = captureSelection(editor);

    expect(scope?.inlineOnly).toBe(false);
    expect(scope?.replacementAllowed).toBe(true);
    expect(scope?.uniformMarks.map(mark => mark.type.name)).toEqual(['bold']);
    const replacement = prepareReplacement(editor, scope!, 'X');
    editor.view.dispatch(replacement);

    expect(blockIds(editor)).toEqual(['cross-left']);
    expect(editor.state.doc.textContent).toBe('leXght');
    expect(editor.state.doc.firstChild?.firstChild?.marks.map(mark => mark.type.name)).toEqual(['bold']);
  });

  it('uses plain replacement formatting for mixed marks within one block', () => {
    const editor = createEditor([
      {
        type: 'paragraph', attrs: { id: 'mixed' },
        content: [
          { type: 'text', text: 'plain' },
          { type: 'text', text: 'bold', marks: [{ type: 'bold' }] },
          { type: 'text', text: 'tail', marks: [{ type: 'bold' }] },
        ],
      },
    ]);
    const start = blockContentStart(editor, 0);
    select(editor, start, start + 9);
    const scope = captureSelection(editor);

    expect(scope?.replacementAllowed).toBe(true);
    expect(scope?.uniformMarks).toEqual([]);
    expect(scope?.formattingNote).toMatch(/Mixed formatting/);
    editor.view.dispatch(prepareReplacement(editor, scope!, 'new'));
    expect(editor.state.doc.textContent).toBe('newtail');
    expect(blockIds(editor)).toEqual(['mixed']);
    expect(editor.state.doc.firstChild?.childCount).toBe(2);
    expect(editor.state.doc.firstChild?.firstChild?.marks).toEqual([]);
    expect(editor.state.doc.firstChild?.lastChild?.marks.map(mark => mark.type.name)).toEqual(['bold']);
  });

  it('keeps a scope stale after an edit even when undo returns the source document', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'stale' }, content: [{ type: 'text', text: 'hello world' }] },
    ]);
    const start = blockContentStart(editor, 0);
    select(editor, start + 6, start + 11);
    const scope = captureSelection(editor);
    const source = editor.state.doc;
    const capturedGeneration = generation(editor);

    editor.commands.insertContentAt(start + 11, '!');
    expect(generation(editor)).toBeGreaterThan(capturedGeneration);
    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(editor.state.doc.eq(source)).toBe(true);
    expect(() => prepareReplacement(editor, scope!, 'planet')).toThrow(/chapter changed/i);
  });

  it('rejects newline replacement text', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'newline' }, content: [{ type: 'text', text: 'text' }] },
    ]);
    const start = blockContentStart(editor, 0);
    select(editor, start, start + 4);
    const scope = captureSelection(editor);
    expect(() => prepareReplacement(editor, scope!, 'line\nbreak')).toThrow(/single line/i);
  });

  it('rejects selections spanning a scene break', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'before-scene' }, content: [{ type: 'text', text: 'before' }] },
      { type: 'sceneBreak', attrs: { id: 'scene' } },
      { type: 'paragraph', attrs: { id: 'after-scene' }, content: [{ type: 'text', text: 'after' }] },
    ]);
    const firstStart = blockContentStart(editor, 0);
    const lastStart = blockContentStart(editor, 2);
    select(editor, firstStart + 2, lastStart + 2);
    const scope = captureSelection(editor);

    expect(scope?.replacementAllowed).toBe(false);
    expect(() => prepareReplacement(editor, scope!, 'replacement')).toThrow(/scene breaks/i);
  });

  it('rejects selections spanning mixed heading and paragraph styles', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'paragraph' }, content: [{ type: 'text', text: 'plain' }] },
      { type: 'heading', attrs: { id: 'heading', level: 2 }, content: [{ type: 'text', text: 'title' }] },
    ]);
    const firstStart = blockContentStart(editor, 0);
    const secondStart = blockContentStart(editor, 1);
    select(editor, firstStart + 2, secondStart + 2);
    const scope = captureSelection(editor);

    expect(scope?.replacementAllowed).toBe(false);
    expect(() => prepareReplacement(editor, scope!, 'replacement')).toThrow(/matching paragraphs/i);
  });
});
