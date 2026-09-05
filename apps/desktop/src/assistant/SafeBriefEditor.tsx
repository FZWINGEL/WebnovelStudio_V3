import { useEffect, useId, useRef } from 'react';
import type { SafeBriefInput } from '../ipc/discussions';

export function validBriefText(text: string): boolean { return !!text.trim() && new TextEncoder().encode(text).length <= 16_384; }

/** The author approves exact directions; the original planning conversation stays separate. */
export function SafeBriefEditor({ value, disabled, focusKey, onChange, onRemove }: {
  value: SafeBriefInput; disabled: boolean; focusKey: number;
  onChange(value: SafeBriefInput): void; onRemove(): void;
}) {
  const id = useId(); const input = useRef<HTMLTextAreaElement>(null); const section = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!focusKey) return;
    section.current?.scrollIntoView?.({ block: 'nearest' }); input.current?.focus({ preventScroll: true });
  }, [focusKey]);
  const valid = validBriefText(value.text);
  return <section ref={section} className="safe-brief-editor" aria-label="Writing brief editor">
    <div className="scope-title"><h3>Writing brief</h3><button className="text-button" disabled={disabled} onClick={onRemove}>Remove brief</button></div>
    <p className="small-copy">Write only what may guide this scene. Approving shares this exact text with the edit request, without adding the original discussion or private notes.</p>
    {value.originMessageId && <p className="small-copy">Adapted from this document’s discussion. Review the wording before approving.</p>}
    <label htmlFor={id}>Directions for this edit request</label>
    <textarea id={id} ref={input} value={value.text} disabled={disabled} maxLength={16_384} rows={5}
      placeholder="The mentor notices the pendant, then changes the subject. Mei reads the pause as grief…"
      onChange={event => onChange({ ...value, text: event.target.value, confirmed: false })} />
    {!valid && value.text.trim() && <p className="error-status" role="alert">This brief is too long. Shorten it before approving.</p>}
    {value.confirmed && valid ? <p className="small-copy" role="status">Approved for this edit request. Changing the brief or selection requires approval again.</p>
      : <button className="secondary-button" disabled={disabled || !valid} onClick={() => { onChange({ ...value, confirmed: true }); input.current?.focus(); }}>Approve this brief</button>}
  </section>;
}
