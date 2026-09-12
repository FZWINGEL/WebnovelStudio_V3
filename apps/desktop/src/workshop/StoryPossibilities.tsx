import { useState } from 'react';
import type { StoryPossibility } from '../ipc/workshop';

export const POSSIBILITY_KINDS = [
  { kind: 'unresolvedQuestion', label: 'Unresolved question', plural: 'Unresolved questions', description: 'A question you may want the story to explore or answer.' },
  { kind: 'intendedPayoff', label: 'Intended payoff', plural: 'Intended payoffs', description: 'A hoped-for revelation or reward; it has not happened merely because it is planned.' },
  { kind: 'possibleArc', label: 'Possible arc', plural: 'Possible arc directions', description: 'One direction the story could take. It does not fix a chronology or ending.' },
] as const;

export function StoryPossibilities({ items, selectedText, disabled = false, onChange, onExplore }: {
  items: StoryPossibility[]; selectedText?: string; disabled?: boolean;
  onChange(items: StoryPossibility[]): void; onExplore(item: StoryPossibility): void;
}) {
  const [kind, setKind] = useState<StoryPossibility['kind']>('unresolvedQuestion');
  const [text, setText] = useState('');
  const setItem = (id: string, change: Partial<StoryPossibility>) => onChange(items.map(item => item.id === id ? { ...item, ...change } : item));
  return <section className="workshop-possibilities" aria-label="Open story possibilities">
    <h2>What the story could become</h2>
    <p className="small-copy">Keep questions, intended payoffs, and possible arcs separately. These are author intentions, not established events. You can leave the ending open.</p>
    {POSSIBILITY_KINDS.map(group => <section key={group.kind} aria-label={group.plural}>
      <h3>{group.plural}</h3><p className="small-copy">{group.description}</p>
      {items.filter(item => item.kind === group.kind && item.status === 'open').map((item, index) => <div key={item.id} className="workshop-possibility">
        <label>{group.label} {index + 1}<textarea value={item.text} maxLength={4000} disabled={disabled} onChange={event => setItem(item.id, { text: event.target.value })} /></label>
        <div className="workshop-actions"><button disabled={disabled || !item.text.trim()} onClick={() => onExplore(item)}>Prepare to explore</button><button disabled={disabled} onClick={() => setItem(item.id, { status: 'archived' })}>Set aside</button></div>
      </div>)}
      {!items.some(item => item.kind === group.kind && item.status === 'open') && <p className="small-copy">None kept yet.</p>}
    </section>)}
    <details><summary>Keep a story possibility</summary>
      <label>Kind<select value={kind} disabled={disabled} onChange={event => setKind(event.target.value as StoryPossibility['kind'])}>{POSSIBILITY_KINDS.map(group => <option key={group.kind} value={group.kind}>{group.label}</option>)}</select></label>
      <label>Possibility<textarea value={text} disabled={disabled} maxLength={4000} onChange={event => setText(event.target.value)} placeholder="For example, who left the broken compass—and why?" /></label>
      {selectedText && <button disabled={disabled || selectedText.length > 4000} onClick={() => setText(selectedText)}>Use selected working text</button>}
      <button disabled={disabled || !text.trim() || items.length >= 64} onClick={() => { onChange([...items, { id: crypto.randomUUID(), kind, text, status: 'open' }]); setText(''); }}>Keep possibility</button>
      {items.length >= 64 && <p className="small-copy">This exploration already holds 64 possibilities. Begin another exploration to keep more.</p>}
    </details>
    {items.some(item => item.status === 'archived') && <details><summary>Possibilities set aside</summary>{items.filter(item => item.status === 'archived').map(item => <div key={item.id}><p className="workshop-preserve-lines"><strong>{POSSIBILITY_KINDS.find(group => group.kind === item.kind)!.label}:</strong> {item.text || 'No wording retained.'}</p><button disabled={disabled} onClick={() => setItem(item.id, { status: 'open' })}>Reopen possibility</button></div>)}</details>}
  </section>;
}
