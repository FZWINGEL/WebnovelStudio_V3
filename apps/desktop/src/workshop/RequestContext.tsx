import { useEffect, useState } from 'react';
import { preparedStoryContext } from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';
import { describeWorkshopError } from './store';

type Envelope = Record<string, unknown>;
export function workshopEnvelope(messages: Array<{ content: string }>): Envelope | null {
  for (const message of messages) {
    try {
      const value: unknown = JSON.parse(message.content);
      if (value && typeof value === 'object' && 'workshop' in value && value.workshop && typeof value.workshop === 'object' && !Array.isArray(value.workshop)) return value.workshop as Envelope;
    } catch { /* Ordinary system instructions are not JSON context envelopes. */ }
  }
  return null;
}
const string = (value: unknown) => typeof value === 'string' ? value : '';
function texts(value: unknown): string[] { return Array.isArray(value) ? value.flatMap(item => typeof item === 'string' ? [item] : item && typeof item === 'object' && 'text' in item && typeof item.text === 'string' ? [item.text] : []) : []; }

/** Reads the immutable saved packet, never reconstructs it from current preferences. */
export function RequestContext({ access, packetId }: { access: ProjectAccess; packetId: string }) {
  const [context, setContext] = useState<Envelope | null>(null); const [error, setError] = useState(''); const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let disposed = false; setLoaded(false); setContext(null); setError('');
    void preparedStoryContext(access, packetId).then(packet => { if (!disposed) setContext(workshopEnvelope(packet.messages)); }).catch(reason => { if (!disposed) setError(describeWorkshopError(reason)); }).finally(() => { if (!disposed) setLoaded(true); });
    return () => { disposed = true; };
  }, [access.projectId, access.operationNamespace, access.writerLease, access.session, packetId]);
  const exploration = context?.exploration && typeof context.exploration === 'object' ? context.exploration as Envelope : null;
  const voice = context?.voiceGuidance && typeof context.voiceGuidance === 'object' ? context.voiceGuidance as Envelope : null;
  return <details className="workshop-frozen-context"><summary>Creative direction in this saved request</summary>{!loaded && <p role="status">Opening the saved request…</p>}{error && <p role="alert">{error}</p>}{loaded && !error && !context && <p>This saved packet has no Workshop context envelope.</p>}{context && <>
    {exploration && <><h4>Instruction and editable scope</h4><p>{string(exploration.instruction)}</p><p>{string(exploration.selectedScope)}</p>{string(exploration.selectedText) && <blockquote>{string(exploration.selectedText)}</blockquote>}</>}
    <h4>Current element</h4><p className="workshop-preserve-lines">{string(context.currentElement)}</p>
    {voice && <section><h4>Voice sample in this request</h4><blockquote className="workshop-preserve-lines">{string(voice.sample)}</blockquote><p>{string(voice.authorInstruction)}</p><ul>{texts(voice.dimensions).map(dimension => <li key={dimension}>{dimension}</li>)}</ul><p>Source for style guidance only. The sample’s events are not adopted by this request.</p></section>}
    <p>Lens: {string(context.lens)} · Depth: {string(context.depth)}</p>
    {string(context.originalNotes) && <details><summary>Original notes in this request</summary><p className="workshop-preserve-lines">{string(context.originalNotes)}</p></details>}
    <h4>Direction</h4><p>{context.outsideDirection === true ? 'This request permits alternatives outside the current direction while keeping hard constraints.' : string(context.direction) || 'No direction was chosen.'}</p>
    <h4>Question and reason</h4><p>{string(context.focusQuestion)}</p><p>{string(context.focusReason)}</p>
    {Array.isArray(context.questions) && context.questions.length > 0 && <><h4>Saved question choices</h4><ul>{context.questions.map((question, index) => { const item = question as Envelope; return <li key={index}>{string(item.text)} · {string(item.status)} · unknown to {string(item.unknownTo)}</li>; })}</ul></>}
    {([['preferences', 'Preferences'], ['hardConstraints', 'Must / Never constraints'], ['fixedDetails', 'Keep fixed decisions'], ['selectedDetails', 'Selected details'], ['chosenDetails', 'Chosen related material'], ['includedAlternatives', 'Explicitly included alternatives'], ['rejectedRationales', 'Local rejection reasons']] as const).map(([key, label]) => <div key={key}><h4>{label}</h4>{texts(context[key]).length ? <ul>{texts(context[key]).map((text, index) => <li key={index}>{text}</li>)}</ul> : <p className="small-copy">None included.</p>}</div>)}
  </>}</details>;
}
