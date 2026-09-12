// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentRecord } from '../ipc/projects';
import { AdoptionLinks, adoptionParticipants, validAdoptionLinks, type AdoptionLinkDraft, type AdoptionMaterialDraft } from './AdoptionLinks';

let host: HTMLDivElement;
let root: Root;

const target = (id: string, title: string, kind: 'character' | 'world'): AdoptionMaterialDraft => ({
  id, documentId: '', title, kind, mode: 'add', text: `${title} material`,
});

const relationship = (overrides: Partial<AdoptionLinkDraft> = {}): AdoptionLinkDraft => ({
  id: 'relationship-1', fromDocumentId: 'new-character', toDocumentId: 'new-world', type: 'trusts',
  description: 'The character trusts the place.', uncertainty: '', ...overrides,
});

const existingDocument = (documentId: string, title: string, kind: 'character' | 'world'): DocumentRecord => ({
  head: { documentId, version: '1', bodyHash: `hash-${documentId}` },
  title, kind, metadataVersion: '1',
  body: { schemaVersion: 1, body: { type: 'doc', content: [] } },
  lastCheckpointId: null,
});

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('Adoption relationship identity', () => {
  it('uses each new destination ID as the relationship participant identity', () => {
    const participants = adoptionParticipants([], [target('new-character', 'Mira', 'character'), target('new-world', 'The archive', 'world')]);
    expect(participants).toEqual([
      { id: 'new-character', title: 'Mira', isNew: true },
      { id: 'new-world', title: 'The archive', isNew: true },
    ]);
    expect(validAdoptionLinks([relationship()], participants)).toBe(true);
  });

  it('keeps a removed destination invalid instead of redirecting the link by list position', async () => {
    const originalParticipants = [
      { id: 'new-character', title: 'Mira', isNew: true },
      { id: 'new-world', title: 'The archive', isNew: true },
    ];
    const changedParticipants = [
      { id: 'new-character', title: 'Mira', isNew: true },
      { id: 'replacement-world', title: 'A different archive', isNew: true },
    ];
    let links = [relationship()];
    const onChange = vi.fn((next: AdoptionLinkDraft[]) => { links = next; });
    const render = async (participants: typeof originalParticipants) => {
      await act(async () => root.render(<AdoptionLinks participants={participants} links={links} onChange={onChange} />));
    };

    await render(originalParticipants);
    let selects = host.querySelectorAll<HTMLSelectElement>('select');
    expect(selects[1].value).toBe('new-world');
    expect(validAdoptionLinks(links, originalParticipants)).toBe(true);

    await render(changedParticipants);
    selects = host.querySelectorAll<HTMLSelectElement>('select');
    expect(selects[1].value).toBe('new-world');
    expect(selects[1].querySelector('option[value="new-world"]')?.textContent).toContain('Removed destination');
    expect(host.textContent).toContain('Each relationship needs two different available participants');
    expect(validAdoptionLinks(links, changedParticipants)).toBe(false);

    await act(async () => {
      selects[1].value = 'replacement-world';
      selects[1].dispatchEvent(new Event('change', { bubbles: true }));
    });
    await render(changedParticipants);
    expect(links[0].toDocumentId).toBe('replacement-world');
    expect(validAdoptionLinks(links, changedParticipants)).toBe(true);
  });

  it('keeps an existing relationship endpoint bound to its document when a target slot changes', async () => {
    const documents = [
      existingDocument('existing-a', 'A', 'character'),
      existingDocument('existing-b', 'B', 'world'),
      existingDocument('existing-other', 'Other', 'world'),
    ];
    const links = [relationship({
      fromDocumentId: 'existing-a', toDocumentId: 'existing-other',
    })];
    const targetBefore: AdoptionMaterialDraft = {
      id: 'target-slot', documentId: 'existing-a', title: 'A', kind: 'character', mode: 'add', text: 'A update',
    };
    const targetAfter = { ...targetBefore, documentId: 'existing-b', title: 'B', kind: 'world' };
    const before = adoptionParticipants(documents, [targetBefore]);
    const after = adoptionParticipants(documents, [targetAfter]);

    await act(async () => root.render(<AdoptionLinks participants={after} links={links} onChange={vi.fn()} />));
    const selects = host.querySelectorAll<HTMLSelectElement>('select');
    expect(selects[0].value).toBe('existing-a');
    expect(links[0].fromDocumentId).toBe('existing-a');
    expect(validAdoptionLinks(links, before)).toBe(true);
    expect(validAdoptionLinks(links, after)).toBe(true);
  });
});
