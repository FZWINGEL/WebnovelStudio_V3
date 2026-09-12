import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Editor } from '@tiptap/core';
import { EditorContent } from '@tiptap/react';
import type { Inline } from '../editor';
import { canonicalJson, safeHref } from '../editor';
import { editorExtensions } from '../editor';
import { structuredEditorBlocks, structuredFromEditor } from '../editor';
import type { StructuredBlock } from '../ipc/proposals';

function content(blocks: StructuredBlock[]) {
  return { type: 'doc', content: structuredEditorBlocks(blocks.length ? blocks : [{ type: 'paragraph', content: [] }], (blocks.length ? blocks : [null]).map(() => crypto.randomUUID())) };
}

export function StructuredSuggestionEditor({ blocks, disabled, label, onChange }: { blocks: StructuredBlock[]; disabled: boolean; label: string; onChange: (blocks: StructuredBlock[]) => void }) {
  const change = useRef(onChange); change.current = onChange;
  const lastValue = useRef(canonicalJson(blocks));
  const [, redraw] = useState(0);
  const [editor] = useState(() => new Editor({
    extensions: editorExtensions,
    content: content(blocks), editable: !disabled,
    editorProps: { attributes: { role: 'textbox', 'aria-label': label, 'aria-multiline': 'true', spellcheck: 'true' }, handleClick: (_view, _position, event) => { if ((event.target as HTMLElement).closest('a')) { event.preventDefault(); return true; } return false; } },
    onUpdate: ({ editor }) => {
      const value = structuredFromEditor(editor.state.doc);
      lastValue.current = canonicalJson(value); change.current(value);
    },
  }));
  useEffect(() => {
    const update = () => redraw(value => value + 1);
    editor.on('transaction', update);
    return () => { editor.off('transaction', update); editor.destroy(); };
  }, [editor]);
  useEffect(() => { editor.setEditable(!disabled, false); }, [disabled, editor]);
  useEffect(() => {
    const next = canonicalJson(blocks);
    if (next !== lastValue.current) { lastValue.current = next; editor.commands.setContent(content(blocks), { emitUpdate: false }); }
  }, [blocks, editor]);
  return <div className="structured-suggestion-editor">
    <div className="suggestion-formatbar" role="toolbar" aria-label="Suggestion formatting">
      <select aria-label="Suggestion paragraph style" disabled={disabled} value={editor.isActive('heading') ? `h${editor.getAttributes('heading').level}` : 'p'} onChange={event => { if (event.target.value === 'p') editor.chain().focus().setNode('paragraph').run(); else editor.chain().focus().setNode('heading', { level: Number(event.target.value.slice(1)) }).run(); }}><option value="p">Paragraph</option><option value="h1">Heading 1</option><option value="h2">Heading 2</option><option value="h3">Heading 3</option></select>
      <button type="button" aria-label="Suggestion bold" aria-pressed={editor.isActive('bold')} disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('bold').run()}><strong>B</strong></button>
      <button type="button" aria-label="Suggestion italic" aria-pressed={editor.isActive('italic')} disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().toggleMark('italic').run()}><em>I</em></button>
      <button type="button" disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={() => editor.chain().focus().insertContent({ type: 'sceneBreak', attrs: { id: crypto.randomUUID() } }).run()}>Scene break</button>
    </div>
    <EditorContent editor={editor} />
    {!blocks.length && <p className="small-copy">The selected paragraphs will be removed. Type here to replace them instead.</p>}
  </div>;
}

function formatted(inline: Inline, key: number): ReactNode {
  if (inline.type === 'hardBreak') return <br key={key} />;
  let child: ReactNode = inline.text;
  for (const mark of inline.marks ?? []) {
    if (mark.type === 'bold') child = <strong>{child}</strong>;
    else if (mark.type === 'italic') child = <em>{child}</em>;
    else if (mark.type === 'link' && safeHref(mark.attrs.href)) child = <a href={mark.attrs.href} tabIndex={-1} onClick={event => event.preventDefault()}>{child}</a>;
  }
  return <span key={key}>{child}</span>;
}

export function StructuredProse({ blocks }: { blocks: StructuredBlock[] }) {
  return <div className="structured-prose">{!blocks.length ? <em>Remove the selected paragraphs</em> : blocks.map((block, index) => {
    if (block.type === 'sceneBreak') return <hr key={index} aria-label="Scene break" />;
    const children = block.content.map(formatted);
    if (block.type === 'paragraph') return <p key={index}>{children.length ? children : <br />}</p>;
    const Heading = `h${block.attrs.level}` as 'h1' | 'h2' | 'h3';
    return <Heading key={index}>{children.length ? children : <br />}</Heading>;
  })}</div>;
}
