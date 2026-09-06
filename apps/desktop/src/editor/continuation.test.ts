import { Editor } from '@tiptap/core';
import { redo, undo } from '@tiptap/pm/history';
import { afterEach, describe, expect, it } from 'vitest';
import fixture from '../../../../tests/fixtures/continuation.json';
import { canonicalJson, snapshotFromEditor, type WnsDocument } from './document';
import { editorExtensions } from './schema';
import { prepareContinuation } from './continuation';
import type { ScopeGrant } from '../ipc/context';

type ContinuationCase = (typeof fixture.cases)[number];
const editors: Editor[] = [];

function createEditor(snapshot: WnsDocument): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const editor = new Editor({ element, extensions: editorExtensions, content: snapshot.body });
  editors.push(editor);
  return editor;
}

function snapshot(editor: Editor): WnsDocument {
  return snapshotFromEditor(editor.getJSON());
}

function validCases(): ContinuationCase[] {
  return fixture.cases.filter(item => item.valid);
}

function byName(name: string): ContinuationCase {
  const value = fixture.cases.find(item => item.name === name);
  if (!value) throw new Error(`Missing continuation fixture ${name}`);
  return value;
}

afterEach(() => {
  while (editors.length) editors.pop()?.destroy();
  document.body.replaceChildren();
});

describe('continuation append grammar through the real ProseMirror schema', () => {
  it.each(validCases())('preflights and dispatches the exact result for $name', testCase => {
    const source = testCase.sourceSnapshot as WnsDocument;
    const editor = createEditor(source);
    const scope = testCase.scope as ScopeGrant;
    const transaction = prepareContinuation(editor.state, scope, testCase.paragraphs, testCase.ids);
    const expected = editor.state.applyTransaction(transaction).state.doc;

    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(source));
    editor.view.dispatch(transaction);
    expect(editor.state.doc.eq(expected)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(testCase.resultSnapshot as WnsDocument));
  });

  it('preserves the empty placeholder ID, keeps trailing blanks, and keeps source formatting and IDs byte-for-byte', () => {
    const empty = byName('valid-empty-placeholder');
    const emptyEditor = createEditor(empty.sourceSnapshot as WnsDocument);
    const emptyBefore = snapshot(emptyEditor);
    emptyEditor.view.dispatch(prepareContinuation(emptyEditor.state, empty.scope as ScopeGrant, empty.paragraphs, empty.ids));
    const emptyAfter = snapshot(emptyEditor);
    expect(emptyAfter.body.content[0].attrs.id).toBe('empty');
    expect(emptyAfter.body.content[1].attrs.id).toBe('empty-next');
    expect(emptyAfter.body.content[0]).toMatchObject({ type: 'paragraph', attrs: { id: 'empty' }, content: [{ type: 'text', text: 'The lantern answered.' }] });
    expect(emptyBefore.body.content[0]).toEqual({ type: 'paragraph', attrs: { id: 'empty' } });

    const formatted = byName('valid-formatting-and-scene-break');
    const formattedEditor = createEditor(formatted.sourceSnapshot as WnsDocument);
    formattedEditor.view.dispatch(prepareContinuation(formattedEditor.state, formatted.scope as ScopeGrant, formatted.paragraphs, formatted.ids));
    const formattedAfter = snapshot(formattedEditor);
    expect(formattedAfter.body.content.slice(0, 4)).toEqual((formatted.sourceSnapshot as WnsDocument).body.content);

    const trailing = byName('valid-trailing-blank-remains-content');
    const trailingEditor = createEditor(trailing.sourceSnapshot as WnsDocument);
    trailingEditor.view.dispatch(prepareContinuation(trailingEditor.state, trailing.scope as ScopeGrant, trailing.paragraphs, trailing.ids));
    const trailingAfter = snapshot(trailingEditor);
    expect(trailingAfter.body.content[1]).toEqual({ type: 'paragraph', attrs: { id: 'blank' } });
    expect(trailingAfter.body.content[2].attrs.id).toBe('after-blank');
  });

  it.each(fixture.negativeMutations)('rejects $name before BlockIdentity can repair it', mutation => {
    const base = byName(mutation.base);
    const editor = createEditor(base.sourceSnapshot as WnsDocument);
    const scope = mutation.scope ? { ...(base.scope as ScopeGrant), end: mutation.scope.end } : base.scope as ScopeGrant;
    const paragraphs = mutation.paragraphs ?? base.paragraphs;
    const ids = mutation.ids ?? base.ids;
    expect(() => prepareContinuation(editor.state, scope, paragraphs, ids)).toThrow(mutation.errorContains);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(base.sourceSnapshot as WnsDocument));
  });

  it('isolates append as one history event and rejects reapplying the saved IDs', () => {
    const testCase = byName('valid-unicode');
    const editor = createEditor(testCase.sourceSnapshot as WnsDocument);
    const original = snapshot(editor);
    const transaction = prepareContinuation(editor.state, testCase.scope as ScopeGrant, testCase.paragraphs, testCase.ids);
    const expected = editor.state.applyTransaction(transaction).state.doc;
    editor.view.dispatch(transaction);
    expect(editor.state.doc.eq(expected)).toBe(true);
    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(original));
    expect(redo(editor.state, editor.view.dispatch)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(testCase.resultSnapshot as WnsDocument));
    expect(() => prepareContinuation(editor.state, testCase.scope as ScopeGrant, testCase.paragraphs, testCase.ids)).toThrow(/append scope/u);
  });

  it('allocates IDs once per prepared body and reuses supplied IDs without regeneration', () => {
    const testCase = byName('valid-formatting-and-scene-break');
    const source = testCase.sourceSnapshot as WnsDocument;
    const editor = createEditor(source);
    const first = prepareContinuation(editor.state, testCase.scope as ScopeGrant, testCase.paragraphs, testCase.ids);
    const expected = editor.state.applyTransaction(first).state.doc;
    expect(expected.child(expected.childCount - 1).attrs.id).toBe('continuation-two');

    const secondEditor = createEditor(source);
    const second = prepareContinuation(secondEditor.state, testCase.scope as ScopeGrant, testCase.paragraphs, testCase.ids);
    secondEditor.view.dispatch(second);
    expect(canonicalJson(snapshot(secondEditor))).toBe(canonicalJson(testCase.resultSnapshot as WnsDocument));
  });

  it('allocates valid fresh IDs when the prepared caller does not supply them', () => {
    const testCase = byName('valid-formatting-and-scene-break');
    const editor = createEditor(testCase.sourceSnapshot as WnsDocument);
    editor.view.dispatch(prepareContinuation(editor.state, testCase.scope as ScopeGrant, testCase.paragraphs));
    const ids = editor.state.doc.content.content.map(node => String(node.attrs.id));
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.slice(0, 4)).toEqual(['heading', 'prose', 'break', 'ending']);
    expect(ids.slice(4).every(id => /^[A-Za-z0-9_-]{1,64}$/u.test(id))).toBe(true);
  });
});
