import { Editor, type JSONContent } from '@tiptap/core';
import { TextSelection } from '@tiptap/pm/state';
import { closeHistory, undo, redo } from '@tiptap/pm/history';
import { afterEach, describe, expect, it } from 'vitest';
import golden from '../../../../contracts/fixtures/w1_scope_golden.json';
import { canonicalJson, snapshotFromEditor, type WnsDocument } from '../kernel';
import { editorExtensions } from './schema';
import { captureSelection, prepareReplacement, prepareScopedReplacement } from './selection';
import type { ScopeGrant } from '../ipc/context';

type GoldenCase = (typeof golden.cases)[number];
const editors: Editor[] = [];

function createEditor(snapshot: WnsDocument): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const editor = new Editor({ element, extensions: editorExtensions, content: snapshot.body });
  editors.push(editor);
  return editor;
}

function blockContentStart(editor: Editor, index: number): number {
  let position = 1;
  for (let current = 0; current < index; current += 1) position += editor.state.doc.child(current).nodeSize;
  return position;
}

function select(editor: Editor, from: number, to: number): void {
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, from, to)));
}

function snapshot(editor: Editor): WnsDocument {
  return snapshotFromEditor(editor.getJSON());
}

afterEach(() => {
  while (editors.length) editors.pop()?.destroy();
  document.body.replaceChildren();
});

describe('W1 scope golden snapshots through the real ProseMirror schema', () => {
  it('preflights stored endpoints without changing selection, then isolates Apply from later typing in history', () => {
    const testCase = golden.cases.find(item => item.name === 'repeated-quote-targets-second-occurrence')!;
    const editor = createEditor(testCase.request.sourceSnapshot as WnsDocument);
    const original = editor.state.doc; const selection = editor.state.selection;
    const tr = prepareScopedReplacement(editor.state, testCase.request.scope as ScopeGrant, 'changed').setMeta('durableApply', true);
    const expected = editor.state.applyTransaction(tr).state.doc;
    expect(editor.state.doc.eq(original)).toBe(true); expect(editor.state.selection.eq(selection)).toBe(true);
    editor.view.dispatch(tr); editor.view.dispatch(closeHistory(editor.state.tr));
    expect(editor.state.doc.eq(expected)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(testCase.request.resultSnapshot));
    editor.view.dispatch(editor.state.tr.insertText(' later'));
    const later = editor.state.doc;
    undo(editor.state, editor.view.dispatch); expect(editor.state.doc.eq(expected)).toBe(true);
    undo(editor.state, editor.view.dispatch); expect(editor.state.doc.eq(original)).toBe(true);
    redo(editor.state, editor.view.dispatch); expect(editor.state.doc.eq(expected)).toBe(true);
    redo(editor.state, editor.view.dispatch); expect(editor.state.doc.eq(later)).toBe(true);
  });

  it('refuses malformed stored endpoints rather than widening or searching for a quote', () => {
    const testCase = golden.cases.find(item => item.name === 'repeated-quote-targets-second-occurrence')!;
    const editor = createEditor(testCase.request.sourceSnapshot as WnsDocument);
    const grant = testCase.request.scope as ScopeGrant;
    expect(() => prepareScopedReplacement(editor.state, { ...grant, start: { blockId: 'missing', utf16Offset: 0 } }, 'changed')).toThrow(/unavailable/u);
    expect(() => prepareScopedReplacement(editor.state, { ...grant, quote: 'different words' }, 'changed')).toThrow(/exact source/u);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(testCase.request.sourceSnapshot));
  });
  it.each(golden.cases)('round-trips the literal source and result for $name', (testCase: GoldenCase) => {
    const source = createEditor(testCase.request.sourceSnapshot as WnsDocument);
    const result = createEditor(testCase.request.resultSnapshot as WnsDocument);
    expect(canonicalJson(snapshot(source))).toBe(canonicalJson(testCase.request.sourceSnapshot as WnsDocument));
    expect(canonicalJson(snapshot(result))).toBe(canonicalJson(testCase.request.resultSnapshot as WnsDocument));
  });

  it.each([
    ['valid-inline-with-marks-and-link', 'changed'],
    ['repeated-quote-targets-second-occurrence', 'changed'],
  ] as const)('applies %s at its exact UTF-16 endpoints', (name, replacement) => {
    const testCase = golden.cases.find(item => item.name === name)!;
    const source = createEditor(testCase.request.sourceSnapshot as WnsDocument);
    const scope = testCase.request.scope;
    const startBlock = source.state.doc.content.content.findIndex(node => node.attrs.id === scope.start?.blockId);
    const endBlock = source.state.doc.content.content.findIndex(node => node.attrs.id === scope.end?.blockId);
    expect(startBlock).toBeGreaterThanOrEqual(0);
    expect(endBlock).toBe(startBlock);
    select(source, blockContentStart(source, startBlock) + scope.start!.utf16Offset, blockContentStart(source, endBlock) + scope.end!.utf16Offset);
    const captured = captureSelection(source);
    expect(captured?.quote).toBe(scope.quote);
    expect(captured?.start).toEqual(scope.start);
    expect(captured?.end).toEqual(scope.end);
    source.view.dispatch(prepareReplacement(source, captured!, replacement));
    expect(canonicalJson(snapshot(source))).toBe(canonicalJson(testCase.request.resultSnapshot as WnsDocument));
  });

  it('merges a complete cross paragraph selection while preserving the left PM identity', () => {
    const testCase = golden.cases.find(item => item.name === 'valid-cross-paragraph-merge')!;
    const source = createEditor(testCase.request.sourceSnapshot as WnsDocument);
    select(source, blockContentStart(source, 0), blockContentStart(source, 1) + testCase.request.scope.end!.utf16Offset);
    const scope = captureSelection(source);
    expect(scope?.inlineOnly).toBe(false);
    source.view.dispatch(prepareReplacement(source, scope!, 'left right'));
    expect(canonicalJson(snapshot(source))).toBe(canonicalJson(testCase.request.resultSnapshot as WnsDocument));
    expect(source.state.doc.firstChild?.attrs.id).toBe('left');
  });

  it('keeps every invalid fixture as an explicit source/result pair for Rust validation', () => {
    const invalid = golden.cases.filter(testCase => !testCase.valid);
    expect(invalid).toHaveLength(13);
    for (const testCase of invalid) {
      const source = createEditor(testCase.request.sourceSnapshot as WnsDocument);
      const result = createEditor(testCase.request.resultSnapshot as WnsDocument);
      expect(snapshot(source)).toEqual(testCase.request.sourceSnapshot);
      expect(snapshot(result)).toEqual(testCase.request.resultSnapshot);
      expect((testCase.errorContains ?? '').length).toBeGreaterThan(0);
    }
  });
});
