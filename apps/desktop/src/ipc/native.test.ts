// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}));

import { bodyHash, canonicalJson, type WnsDocument } from '../kernel';
import { validateSnapshot } from './native';

const snapshot: WnsDocument = {
  schemaVersion: 1,
  body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'block-1' }, content: [{ type: 'text', text: 'Draft' }] }] },
};

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauri.mockReturnValue(true);
});

/**
 * Rust validates the snapshot independently, and the renderer is the half that
 * would silently accept a disagreement: this is the only place the two
 * canonical forms are compared, and it is why the editor's serialization can
 * be trusted to mean the same thing as Rust's.
 */
describe('validateSnapshot', () => {
  it('sends the editor canonical form and accepts the matching receipt', async () => {
    const json = canonicalJson(snapshot);
    const receipt = { hash: await bodyHash(json), canonicalJson: json };
    mocks.invoke.mockResolvedValue(receipt);
    await expect(validateSnapshot(snapshot)).resolves.toEqual(receipt);
    expect(mocks.invoke).toHaveBeenCalledWith('validate_snapshot', { snapshotJson: json });
  });

  it('refuses a snapshot Rust canonicalized differently', async () => {
    const json = canonicalJson(snapshot);
    mocks.invoke.mockResolvedValue({ hash: await bodyHash(json), canonicalJson: '{"body":{"type":"doc"}}' });
    await expect(validateSnapshot(snapshot)).rejects.toThrow(/disagree about this snapshot/);
  });

  it('refuses a snapshot Rust hashed differently, even when the text matches', async () => {
    const json = canonicalJson(snapshot);
    mocks.invoke.mockResolvedValue({ hash: 'f'.repeat(64), canonicalJson: json });
    await expect(validateSnapshot(snapshot)).rejects.toThrow(/disagree about this snapshot/);
  });

  it('keys the canonical form, so key order in the caller object cannot matter', async () => {
    const reordered = { body: snapshot.body, schemaVersion: snapshot.schemaVersion } as WnsDocument;
    expect(canonicalJson(reordered)).toBe(canonicalJson(snapshot));
  });

  it('refuses outside the desktop app rather than falling back to the editor', async () => {
    mocks.isTauri.mockReturnValue(false);
    await expect(validateSnapshot(snapshot)).rejects.toThrow(/desktop app/);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
