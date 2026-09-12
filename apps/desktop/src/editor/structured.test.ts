// @vitest-environment jsdom
import { Editor } from '@tiptap/core';
import { redo, undo } from '@tiptap/pm/history';
import { afterEach, describe, expect, it } from 'vitest';
import type { ScopeGrant } from '../ipc/context';
import type { StructuredBlock } from '../ipc/proposals';
import golden from '../../../../contracts/fixtures/structured_proposals_golden.json';
import { canonicalJson, snapshotFromEditor, type WnsDocument } from '../kernel';
import { captureRevisionScope, blocksQuote } from './revisionScope';
import { editorExtensions } from './schema';
import {
  prepareStructuredReplacement,
  structuredEditorBlocks,
  structuredFromEditor,
  structuredRange,
  validateStructuredBlocks,
} from './structured';

const editors: Editor[] = [];

function createEditor(content: WnsDocument['body']['content']): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const editor = new Editor({ element, extensions: editorExtensions, content: { type: 'doc', content } });
  editors.push(editor);
  return editor;
}

function snapshot(editor: Editor): WnsDocument {
  return snapshotFromEditor(editor.getJSON());
}

function scope(kind: ScopeGrant['kind'], quote: string, start: ScopeGrant['start'] = null, end: ScopeGrant['end'] = null): ScopeGrant {
  return { kind, start, end, quote, sourceHash: 'a'.repeat(64), quoteHash: 'b'.repeat(64), prefix: null, suffix: null };
}

afterEach(() => {
  while (editors.length) editors.pop()?.destroy();
  document.body.replaceChildren();
});

describe('structured suggestion editor contracts', () => {
  it('validates and round-trips paragraphs, headings, scene breaks, marks, hard breaks, and Unicode without provider IDs', () => {
    const blocks: StructuredBlock[] = [
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: '霜火🙂', marks: [{ type: 'bold' }] }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'First line', marks: [{ type: 'italic' }] }, { type: 'hardBreak' }, { type: 'text', text: 'Second line', marks: [{ type: 'link', attrs: { href: 'https://example.com/story' } }] }] },
      { type: 'sceneBreak' },
    ];
    validateStructuredBlocks(blocks);
    expect(() => structuredEditorBlocks(blocks, ['heading-1', 'paragraph-1', 'break-1'])).not.toThrow();
    expect(JSON.stringify(blocks)).not.toContain('"id"');
    const editor = createEditor(structuredEditorBlocks(blocks, ['heading-1', 'paragraph-1', 'break-1']));
    expect(structuredFromEditor(editor.state.doc)).toEqual(blocks);
  });

  it('captures exact block and whole-chapter quotes, including scene breaks, trailing empty blocks, hard breaks, and Unicode', () => {
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [
      { type: 'paragraph', attrs: { id: 'before' }, content: [{ type: 'text', text: '霜🙂' }, { type: 'hardBreak' }, { type: 'text', text: 'line two' }] },
      { type: 'sceneBreak', attrs: { id: 'break' } },
      { type: 'paragraph', attrs: { id: 'selected' }, content: [{ type: 'text', text: 'Selected paragraph.' }] },
      { type: 'paragraph', attrs: { id: 'trailing' } },
    ] } };
    const hash = 'f'.repeat(64);
    const selected = { kind: 'passage' as const, start: { blockId: 'selected', utf16Offset: 0 }, end: { blockId: 'selected', utf16Offset: 19 }, quote: 'Selected paragraph.', sourceBodyHash: hash };
    const selectedBlocks = body.body.content.slice(2, 3);
    const widened = captureRevisionScope(body, hash, 'blocks', selected);
    expect(widened).toEqual({ kind: 'blocks', start: { blockId: 'selected', utf16Offset: 0 }, end: { blockId: 'selected', utf16Offset: 19 }, quote: blocksQuote(selectedBlocks), sourceBodyHash: hash });
    expect(captureRevisionScope(body, hash, 'wholeDocument')).toEqual({ kind: 'wholeDocument', start: null, end: null, quote: blocksQuote(body.body.content), sourceBodyHash: hash });
    expect(() => captureRevisionScope(body, hash, 'blocks', null)).toThrow(/Select the passage again/u);

    const editor = createEditor(body.body.content);
    const widenedGrant: ScopeGrant = { ...widened, sourceHash: hash, quoteHash: 'b'.repeat(64), prefix: null, suffix: null };
    expect(structuredRange(editor.state.doc, widenedGrant)).toMatchObject({ first: 2, last: 2 });
  });

  it('only widens a partial selection after the author chooses block scope', () => {
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [
      { type: 'paragraph', attrs: { id: 'p1' }, content: [{ type: 'text', text: 'before selection' }] },
      { type: 'paragraph', attrs: { id: 'p2' }, content: [{ type: 'text', text: 'another paragraph' }] },
    ] } };
    const hash = 'a'.repeat(64);
    const passage = { kind: 'passage' as const, start: { blockId: 'p1', utf16Offset: 7 }, end: { blockId: 'p1', utf16Offset: 16 }, quote: 'selection', sourceBodyHash: hash };
    expect(captureRevisionScope(body, hash, 'blocks', passage)).toEqual({ kind: 'blocks', start: { blockId: 'p1', utf16Offset: 0 }, end: { blockId: 'p1', utf16Offset: 16 }, quote: 'before selection', sourceBodyHash: hash });
    expect(() => captureRevisionScope(body, hash, 'blocks', null)).toThrow();
  });

  it('rejects passage and append authority, reused IDs, duplicate IDs, and empty whole-document results', () => {
    const source = [
      { type: 'paragraph' as const, attrs: { id: 'left' }, content: [{ type: 'text' as const, text: 'left' }] },
      { type: 'paragraph' as const, attrs: { id: 'selected' }, content: [{ type: 'text' as const, text: 'selected' }] },
      { type: 'paragraph' as const, attrs: { id: 'right' }, content: [{ type: 'text' as const, text: 'right' }] },
    ];
    const editor = createEditor(source);
    const replacement: StructuredBlock[] = [{ type: 'paragraph', content: [{ type: 'text', text: 'changed' }] }];
    expect(() => prepareStructuredReplacement(editor.state, scope('passage', 'selected', { blockId: 'selected', utf16Offset: 0 }, { blockId: 'selected', utf16Offset: 8 }), replacement, ['new'])).toThrow(/explicit paragraph or whole-chapter/u);
    expect(() => prepareStructuredReplacement(editor.state, scope('append', 'selected', null, { blockId: 'selected', utf16Offset: 8 }), replacement, ['new'])).toThrow(/explicit paragraph or whole-chapter/u);
    expect(() => prepareStructuredReplacement(editor.state, scope('blocks', 'selected', { blockId: 'selected', utf16Offset: 0 }, { blockId: 'selected', utf16Offset: 8 }), replacement, ['right'])).toThrow(/reuse source/u);
    expect(() => prepareStructuredReplacement(editor.state, scope('blocks', 'selected', { blockId: 'selected', utf16Offset: 0 }, { blockId: 'selected', utf16Offset: 8 }), [{ type: 'paragraph', content: [] }, { type: 'paragraph', content: [] }], ['new', 'new'])).toThrow(/unique identity/u);
    expect(() => prepareStructuredReplacement(editor.state, scope('wholeDocument', 'left\n\nselected\n\nright'), [], [])).toThrow(/at least one paragraph/u);
  });

  it('preserves unselected IDs and marks, uses supplied fresh IDs, and makes Apply one undoable change', () => {
    const editor = createEditor([
      { type: 'paragraph', attrs: { id: 'left' }, content: [{ type: 'text', text: 'left', marks: [{ type: 'bold' }] }] },
      { type: 'paragraph', attrs: { id: 'selected' }, content: [{ type: 'text', text: 'selected' }] },
      { type: 'paragraph', attrs: { id: 'right' }, content: [{ type: 'text', text: 'right', marks: [{ type: 'italic' }] }] },
    ]);
    const original = canonicalJson(snapshot(editor));
    const replacement: StructuredBlock[] = [{ type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'new title', marks: [{ type: 'bold' }] }] }, { type: 'sceneBreak' }];
    const tr = prepareStructuredReplacement(editor.state, scope('blocks', 'selected', { blockId: 'selected', utf16Offset: 0 }, { blockId: 'selected', utf16Offset: 8 }), replacement, ['fresh-heading', 'fresh-break']);
    editor.view.dispatch(tr);
    const changed = snapshot(editor);
    expect(changed.body.content.map(block => block.attrs.id)).toEqual(['left', 'fresh-heading', 'fresh-break', 'right']);
    expect(changed.body.content[0]).toMatchObject({ content: [{ type: 'text', text: 'left', marks: [{ type: 'bold' }] }] });
    expect(changed.body.content[3]).toMatchObject({ content: [{ type: 'text', text: 'right', marks: [{ type: 'italic' }] }] });
    expect(changed.body.content[1]).toMatchObject({ type: 'heading', attrs: { id: 'fresh-heading', level: 2 } });
    expect(undo(editor.state, editor.view.dispatch)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(original);
    expect(redo(editor.state, editor.view.dispatch)).toBe(true);
    expect(canonicalJson(snapshot(editor))).toBe(canonicalJson(changed));
  });

  it('keeps stable supplied IDs across retries and permits deletion only when a valid block remains', () => {
    const replacement: StructuredBlock[] = [{ type: 'paragraph', content: [{ type: 'text', text: 'stable' }] }];
    const create = () => createEditor([
      { type: 'paragraph', attrs: { id: 'left' }, content: [{ type: 'text', text: 'left' }] },
      { type: 'paragraph', attrs: { id: 'selected' }, content: [{ type: 'text', text: 'selected' }] },
      { type: 'paragraph', attrs: { id: 'right' }, content: [{ type: 'text', text: 'right' }] },
    ]);
    const first = create();
    const second = create();
    const grant = scope('blocks', 'selected', { blockId: 'selected', utf16Offset: 0 }, { blockId: 'selected', utf16Offset: 8 });
    first.view.dispatch(prepareStructuredReplacement(first.state, grant, replacement, ['retry-stable']));
    second.view.dispatch(prepareStructuredReplacement(second.state, grant, replacement, ['retry-stable']));
    expect(canonicalJson(snapshot(first))).toBe(canonicalJson(snapshot(second)));
    const deletion = create();
    deletion.view.dispatch(prepareStructuredReplacement(deletion.state, grant, [], []));
    expect(deletion.state.doc.childCount).toBe(2);
    expect(deletion.state.doc.childCount).toBeGreaterThan(0);
  });

  it.each(golden.cases)('matches the shared Rust/JavaScript structured fixture: $name', testCase => {
    const source = createEditor((testCase.sourceSnapshot as WnsDocument).body.content);
    const grant = testCase.scope as ScopeGrant;
    const range = structuredRange(source.state.doc, grant);
    const sourceBlocks = (testCase.sourceSnapshot as WnsDocument).body.content;
    expect(blocksQuote(sourceBlocks.slice(range.first, range.last + 1))).toBe(testCase.expectedQuote);
    source.view.dispatch(prepareStructuredReplacement(source.state, grant, testCase.replacementBlocks as StructuredBlock[], testCase.replacementIds));
    expect(canonicalJson(snapshot(source))).toBe(canonicalJson(testCase.resultSnapshot));
  });
});
