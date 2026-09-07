// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProjectAccess } from '../ipc/projects';
import { preparedStoryContext } from '../ipc/context';
import { RequestContext } from './RequestContext';

vi.mock('../ipc/context', () => ({ preparedStoryContext: vi.fn() }));

type Packet = Awaited<ReturnType<typeof preparedStoryContext>>;
const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const relationship = {
  id: 'historical-relationship-id', fromDocumentId: 'historical-from-id', toDocumentId: 'historical-to-id', type: 'trusts',
  description: 'Mira trusts the archive with the map.', uncertainty: 'The archive may be hiding its cost.', status: 'chosen',
  sourceHeads: [{ documentId: 'historical-from-id', version: '3', bodyHash: 'from-hash' }, { documentId: 'historical-to-id', version: '7', bodyHash: 'to-hash' }],
};

type PacketMessageOptions = { schemaVersion?: string; authorBrief?: string; stillOpen?: string };
function packetMessage(element: string, frozenRelationship?: typeof relationship, options: PacketMessageOptions = {}) {
  const value: Record<string, unknown> = {
    schemaVersion: options.schemaVersion ?? 'story-workshop-request.v1',
    workshop: { currentElement: element, stillOpen: options.stillOpen ?? '', relationship: frozenRelationship },
  };
  if (options.authorBrief !== undefined) value.authorBrief = options.authorBrief;
  return { role: 'user', content: JSON.stringify(value) };
}
function packet(element: string, frozenRelationship?: typeof relationship, options: PacketMessageOptions = {}): Packet {
  return {
    messages: [packetMessage(element, frozenRelationship, options)],
  } as Packet;
}
function packetWithMessages(messages: Array<{ role: string; content: string }>): Packet { return { messages } as Packet; }

let host: HTMLDivElement;
let root: Root;
const prepared = vi.mocked(preparedStoryContext);

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  prepared.mockReset();
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('RequestContext', () => {
  it('shows the relationship captured by the immutable packet without exposing IDs', async () => {
    prepared.mockResolvedValue(packet('Mira and the archive negotiate over the map.', relationship));
    act(() => root.render(<RequestContext access={access} packetId="packet-relationship" />));
    await settle();

    expect(host.textContent).toContain('Relationship in this request');
    expect(host.textContent).toContain('Relationship type: trusts');
    expect(host.textContent).toContain('Mira trusts the archive with the map.');
    expect(host.textContent).toContain('The archive may be hiding its cost.');
    expect(host.textContent).toContain('Status: Chosen author intention');
    expect(host.textContent).not.toContain('historical-relationship-id');
    expect(host.textContent).not.toContain('historical-from-id');
    expect(host.textContent).not.toContain('historical-to-id');
  });

  it('keeps legacy packets without a relationship in the normal context view', async () => {
    prepared.mockResolvedValue(packet('A legacy packet with no relationship projection.'));
    act(() => root.render(<RequestContext access={access} packetId="packet-legacy" />));
    await settle();

    expect(host.textContent).toContain('Current element');
    expect(host.textContent).toContain('A legacy packet with no relationship projection.');
    expect(host.textContent).not.toContain('Relationship in this request');
  });

  it('prefers the final recognized request envelope and keeps its author brief and still-open metadata separate', async () => {
    const earlier = packetMessage('Earlier context envelope', undefined, { schemaVersion: 'story-workshop-context.v1', authorBrief: 'Earlier brief', stillOpen: 'Earlier open question' });
    const final = packetMessage('Final request element', undefined, { authorBrief: 'Exact final author brief', stillOpen: 'Final open question' });
    const messages = [earlier, final];
    const original = structuredClone(messages);
    prepared.mockResolvedValue(packetWithMessages(messages));
    act(() => root.render(<RequestContext access={access} packetId="packet-final-request" />));
    await settle();

    expect(host.textContent).toContain('Exact final author brief');
    expect(host.textContent).toContain('Final request element');
    expect(host.textContent).toContain('Final open question');
    expect(host.textContent).not.toContain('Earlier context envelope');
    expect(host.textContent).not.toContain('Earlier brief');
    expect(host.textContent).not.toContain('Earlier open question');
    expect(messages).toEqual(original);
  });

  it('omits an explicitly empty author brief and identifies an absent legacy brief without rebuilding it', async () => {
    prepared.mockResolvedValue(packet('Current element with an empty brief', undefined, { authorBrief: '', stillOpen: 'Nothing else is decided.' }));
    act(() => root.render(<RequestContext access={access} packetId="packet-empty-brief" />));
    await settle();
    expect(host.textContent).toContain('Current element with an empty brief');
    expect(host.textContent).toContain('Nothing else is decided.');
    expect(host.textContent).not.toContain('Author brief');

    prepared.mockResolvedValue(packet('Legacy element without a brief field', undefined, { stillOpen: 'Legacy open question.' }));
    act(() => root.render(<RequestContext access={access} packetId="packet-absent-brief" />));
    await settle();
    expect(host.textContent).toContain('Author brief was not separately recorded in this request.');
    expect(host.textContent).toContain('Legacy element without a brief field');
    expect(host.textContent).toContain('Legacy open question.');
  });

  it('ignores a late immutable read after switching to another packet', async () => {
    let resolveFirst!: (value: Packet) => void;
    let resolveSecond!: (value: Packet) => void;
    prepared.mockImplementation((_access, packetId) => new Promise<Packet>(resolve => {
      if (packetId === 'packet-first') resolveFirst = resolve;
      else resolveSecond = resolve;
    }));

    act(() => root.render(<RequestContext access={access} packetId="packet-first" />));
    act(() => root.render(<RequestContext access={access} packetId="packet-second" />));
    await act(async () => resolveSecond(packet('The current packet has no relationship.', undefined, { authorBrief: 'Current packet brief', stillOpen: 'Current packet open question.' })));
    await settle();
    await act(async () => resolveFirst(packet('Stale packet', relationship, { authorBrief: 'Stale packet brief', stillOpen: 'Stale packet open question.' })));
    await settle();

    expect(host.textContent).toContain('The current packet has no relationship.');
    expect(host.textContent).toContain('Current packet brief');
    expect(host.textContent).toContain('Current packet open question.');
    expect(host.textContent).not.toContain('Relationship in this request');
    expect(host.textContent).not.toContain('Mira trusts the archive with the map.');
    expect(host.textContent).not.toContain('Stale packet brief');
    expect(host.textContent).not.toContain('Stale packet open question.');
  });
});
