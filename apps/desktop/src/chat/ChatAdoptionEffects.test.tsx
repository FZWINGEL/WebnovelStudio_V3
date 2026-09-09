// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { ChatAdoptionEffects as EffectsManifest, ChatAdoptionTarget } from '../ipc/projectChat';
import type { WnsDocument } from '../editor/document';
import { ChatAdoptionEffects } from './ChatAdoptionEffects';

const head = (documentId: string, version: string) => ({ documentId, version, bodyHash: `${documentId}-${version}-hash` });
const emptyBody: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [] } };
const target = (documentId: string, title: string, kind: string): ChatAdoptionTarget => ({
  draft: { head: head(documentId, '1'), dispositionVersion: '1' }, draftRevisionId: `revision-${documentId}`,
  documentId, title, kind, before: null, body: emptyBody,
});
const targets: ChatAdoptionTarget[] = [
  target('character-a', 'Character A', 'character'), target('character-b', 'Character B', 'character'),
  target('world-1', 'Alliance world', 'world'), target('plan-1', 'Current plan', 'plan'),
  target('plan-old', 'Abandoned plan', 'plan'), target('character-z', 'Character Z', 'character'),
];
const effects: EffectsManifest = {
  version: 'chat-adoption-effects.v1', sourceOutputHash: 'output-hash-1',
  relationshipDependencies: [{ relationshipId: 'relationship-1', fromDocumentId: 'character-a', toDocumentId: 'character-b', relationshipType: 'trusts', fromHead: head('character-a', '4'), toHead: head('character-b', '7') }],
  protectedContent: [{ targetDocumentId: 'character-a', sourceHead: head('character-a', '4'), text: 'Keep the original oath.', textHash: 'protected-hash-1' }],
  proposedRelationships: [{ key: 'edge-1', relationshipId: 'relationship-new', fromDocumentId: 'character-a', toDocumentId: 'character-b', type: 'owes', description: 'A debt changes their next decision.', uncertainty: 'The debt may be concealed.', fromHead: head('character-a', '4'), toHead: head('character-b', '7') }],
  impacts: [{ targetDocumentId: 'world-1', kind: 'possibleTension', reason: 'The new debt pressures the existing alliance.', relationshipKey: 'edge-1' }],
  supersessions: [{ targetDocumentId: 'plan-1', supersededDocumentId: 'plan-old', reason: 'The new plan replaces the abandoned route.' }],
  placements: [{ targetDocumentId: 'character-a', afterDocumentId: 'character-z' }],
};

let host: HTMLDivElement;
let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('ChatAdoptionEffects', () => {
  it('renders the complete immutable manifest, endpoints, heads, protected content, and every proposed effect', async () => {
    await act(async () => root.render(<ChatAdoptionEffects effects={effects} targets={targets} />));
    expect(host.textContent).toContain('Complete effects manifest');
    expect(host.textContent).not.toContain('Complete effects manifest · chat-adoption-effects.v1');
    expect(host.textContent).toContain('output-hash-1');
    expect(host.textContent).toContain('Character A · character');
    expect(host.textContent).toContain('Character B · character');
    expect(host.querySelectorAll('.chat-adoption-version-details')).toHaveLength(7);
    expect(host.textContent).toContain('Keep the original oath.');
    expect(host.textContent).toContain('protected-hash-1');
    expect(host.textContent).toContain('owes');
    expect(host.textContent).toContain('A debt changes their next decision.');
    expect(host.textContent).toContain('Alliance world · world · possibleTension');
    expect(host.textContent).toContain('Current plan · plan supersedes Abandoned plan · plan');
    expect(host.textContent).toContain('Character A · character · after Character Z · character');
    expect(host.querySelectorAll('.chat-adoption-version-details:not([open])')).toHaveLength(7);
    expect(host.querySelector('input,textarea,select,button')).toBeNull();
  });

  it('distinguishes a historical preview without a manifest from an explicit empty manifest', async () => {
    await act(async () => root.render(<ChatAdoptionEffects />));
    expect(host.textContent).toContain('does not include an effects manifest');
    await act(async () => root.render(<ChatAdoptionEffects effects={null} />));
    expect(host.textContent).toContain('No effects manifest is available for this preview.');
    expect(host.textContent).not.toContain('does not include an effects manifest');
  });
});
