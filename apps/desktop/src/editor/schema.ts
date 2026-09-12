import { Extension, Mark, Node, commands, mergeAttributes } from '@tiptap/core';
import { Fragment, Slice, type Node as PMNode } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { history, redo, undo } from '@tiptap/pm/history';
import { safeHref } from '../kernel';

const idAttributes = { id: {
  default: null,
  parseHTML: (element: HTMLElement) => element.getAttribute('data-block-id'),
  renderHTML: (attrs: Record<string, unknown>) => ({ 'data-block-id': attrs.id }),
} };
const freshId = () => crypto.randomUUID();

export const Document = Node.create({ name: 'doc', topNode: true, content: 'block+' });
export const Text = Node.create({ name: 'text', group: 'inline' });
export const Paragraph = Node.create({
  name: 'paragraph', group: 'block', content: 'inline*',
  addAttributes: () => idAttributes,
  parseHTML: () => [{ tag: 'p' }], renderHTML: ({ HTMLAttributes }) => ['p', HTMLAttributes, 0],
});
export const Heading = Node.create({
  name: 'heading', group: 'block', content: 'inline*', defining: true,
  addAttributes: () => ({ ...idAttributes, level: { default: 1, rendered: false } }),
  parseHTML: () => [1, 2, 3].map(level => ({ tag: `h${level}`, attrs: { level } })),
  renderHTML: ({ node, HTMLAttributes }) => [`h${node.attrs.level}`, HTMLAttributes, 0],
});
export const SceneBreak = Node.create({
  name: 'sceneBreak', group: 'block', atom: true, selectable: true,
  addAttributes: () => idAttributes,
  parseHTML: () => [{ tag: 'hr' }],
  renderHTML: ({ HTMLAttributes }) => ['hr', mergeAttributes(HTMLAttributes, { 'aria-label': 'Scene break' })],
});
export const HardBreak = Node.create({
  name: 'hardBreak', group: 'inline', inline: true, selectable: false,
  parseHTML: () => [{ tag: 'br' }], renderHTML: () => ['br'],
  addKeyboardShortcuts() { return { 'Shift-Enter': () => this.editor.commands.insertContent({ type: this.name }) }; },
});
export const Bold = Mark.create({
  name: 'bold', parseHTML: () => [{ tag: 'strong' }, { tag: 'b' }, { style: 'font-weight=bold' }],
  renderHTML: ({ HTMLAttributes }) => ['strong', HTMLAttributes, 0],
  addKeyboardShortcuts() { return { 'Mod-b': () => this.editor.commands.toggleMark(this.name) }; },
});
export const Italic = Mark.create({
  name: 'italic', parseHTML: () => [{ tag: 'em' }, { tag: 'i' }, { style: 'font-style=italic' }],
  renderHTML: ({ HTMLAttributes }) => ['em', HTMLAttributes, 0],
  addKeyboardShortcuts() { return { 'Mod-i': () => this.editor.commands.toggleMark(this.name) }; },
});
export const Link = Mark.create({
  name: 'link', inclusive: false,
  addAttributes: () => ({ href: { default: null } }),
  parseHTML: () => [{ tag: 'a[href]', getAttrs: element => safeHref(element.getAttribute('href') ?? '') ? { href: element.getAttribute('href') } : false }],
  renderHTML: ({ HTMLAttributes }) => ['a', mergeAttributes(HTMLAttributes, { rel: 'noopener noreferrer', tabindex: '-1' }), 0],
});
export const History = Extension.create({
  name: 'history',
  addProseMirrorPlugins: () => [history()],
  addKeyboardShortcuts() {
    return {
      'Mod-z': () => undo(this.editor.state, this.editor.view.dispatch),
      'Mod-Shift-z': () => redo(this.editor.state, this.editor.view.dispatch),
      'Mod-y': () => redo(this.editor.state, this.editor.view.dispatch),
    };
  },
});

export const Generation = Extension.create({
  name: 'generation',
  addStorage: () => ({ value: 0 }),
  onTransaction({ transaction }) { if (transaction.docChanged) this.storage.value += 1; },
});

function copied(fragment: Fragment): Fragment {
  const nodes: PMNode[] = [];
  fragment.forEach(node => nodes.push(node.isText ? node : node.type.create(
    node.isBlock ? { ...node.attrs, id: freshId() } : node.attrs,
    copied(node.content), node.marks,
  )));
  return Fragment.from(nodes);
}

export const BlockIdentity = Extension.create({
  name: 'blockIdentity',
  addCommands() {
    return {
      splitBlock: options => props => {
        const { tr, dispatch } = props;
        const { $from } = tr.selection;
        const sourceId = $from.parent.attrs.id;
        const position = $from.depth > 0 ? $from.before() : null;
        const mapStart = tr.mapping.maps.length;
        const stepStart = tr.steps.length;
        const result = commands.splitBlock(options)(props);
        // Tiptap changes an empty left heading to a paragraph with default attrs.
        // Restore that left identity inside the same transaction, before history.
        if (result && dispatch && position !== null && sourceId && tr.steps.length > stepStart) {
          const leftPosition = tr.mapping.slice(mapStart).map(position, -1);
          const left = tr.doc.nodeAt(leftPosition);
          if (left?.isTextblock && left.attrs.id !== sourceId) tr.setNodeMarkup(leftPosition, undefined, { ...left.attrs, id: sourceId });
        }
        return result;
      },
    };
  },
  addProseMirrorPlugins: () => [new Plugin({
    key: new PluginKey('blockIdentity'),
    props: { transformPasted: slice => new Slice(copied(slice.content), slice.openStart, slice.openEnd) },
    appendTransaction: (transactions, _old, state) => {
      if (!transactions.some(transaction => transaction.docChanged)) return null;
      const seen = new Set<string>();
      const tr = state.tr;
      state.doc.forEach((node, position) => {
        const id = node.attrs.id as string | null;
        if (!id || seen.has(id) || !/^[A-Za-z0-9_-]{1,64}$/u.test(id)) {
          const replacement = freshId();
          tr.setNodeMarkup(position, undefined, { ...node.attrs, id: replacement });
          seen.add(replacement);
        } else seen.add(id);
      });
      return tr.docChanged ? tr : null;
    },
  })],
});

export const editorExtensions = [Document, Paragraph, Heading, Text, SceneBreak, HardBreak, Bold, Italic, Link, History, BlockIdentity, Generation];
