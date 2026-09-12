// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentAliasesRead } from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';
import { readDocumentAliases, setDocumentAliases } from '../ipc/context';
import { DocumentAliases, type DocumentAliasesProps, type DocumentAliasesGuard } from './DocumentAliases';

vi.mock('../ipc/context', () => ({ readDocumentAliases: vi.fn(), setDocumentAliases: vi.fn() }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const base = (documentId = 'character-a', aliases = ['Mira'], sourceEpoch = '1'): DocumentAliasesRead => ({ documentId, aliases, sourceEpoch });
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason?: unknown) => void; const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; }); return { promise, resolve, reject }; }

let host: HTMLDivElement;
let root: Root;
let guard: DocumentAliasesGuard | null;
let registerGuard: ReturnType<typeof vi.fn>;
let beforeSave: ReturnType<typeof vi.fn>;
let onSaved: ReturnType<typeof vi.fn>;

const props = (overrides: Partial<DocumentAliasesProps> = {}): DocumentAliasesProps => ({
  access, documentId: 'character-a', title: 'Mira', visible: true, onClose: vi.fn(),
  registerGuard: (value => { guard = value; }) as DocumentAliasesProps['registerGuard'],
  beforeSave: beforeSave as () => Promise<void>, onSaved: onSaved as () => void,
  ...overrides,
});

async function settle() {
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
}
async function render(overrides: Partial<DocumentAliasesProps> = {}) {
  await act(async () => root.render(<DocumentAliases {...props(overrides)} />));
}
function button(label: string): HTMLButtonElement { return [...host.querySelectorAll('button')].find(item => item.textContent === label) as HTMLButtonElement; }
function textarea(): HTMLTextAreaElement { return host.querySelector('textarea') as HTMLTextAreaElement; }
async function typeNames(value: string) {
  await act(async () => {
    const input = textarea();
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
async function click(label: string) { await act(async () => button(label).click()); await settle(); }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(readDocumentAliases).mockReset().mockResolvedValue(base());
  vi.mocked(setDocumentAliases).mockReset().mockResolvedValue({ source: '2', policy: '1' });
  guard = null; registerGuard = vi.fn(value => { guard = value; }); beforeSave = vi.fn(async () => {}); onSaved = vi.fn();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('DocumentAliases', () => {
  it('loads once and saves a normalized set against the fresh source epoch', async () => {
    await render(); await settle();
    expect(textarea().value).toBe('Mira');
    await typeNames(' Mira\nThe Lady Mira\nMira\n');
    await click('Save names');
    expect(beforeSave).toHaveBeenCalledOnce();
    expect(readDocumentAliases).toHaveBeenCalledTimes(2);
    expect(setDocumentAliases).toHaveBeenCalledWith(access, 'character-a', '1', ['Mira', 'The Lady Mira']);
    expect(onSaved).toHaveBeenCalledOnce();
    expect(host.textContent).toContain('Names saved.');
  });

  it('shows a concurrent alias change and requires explicit rebase before a new save', async () => {
    vi.mocked(readDocumentAliases).mockResolvedValueOnce(base()).mockResolvedValueOnce(base('character-a', ['Mira', 'The Archivist'], '2')).mockResolvedValueOnce(base('character-a', ['Mira', 'The Archivist'], '2'));
    await render(); await settle(); await typeNames('Mira\nThe Lady Mira'); await click('Save names');
    expect(setDocumentAliases).not.toHaveBeenCalled(); expect(host.textContent).toContain('changed elsewhere'); expect(button('Keep my names')).toBeDefined();
    expect(textarea().value).toBe('Mira\nThe Lady Mira');
    await click('Keep my names'); await click('Save names');
    expect(setDocumentAliases).toHaveBeenCalledWith(access, 'character-a', '2', ['Mira', 'The Lady Mira']);
  });

  it('retains one lost acknowledgment and confirms by read without replaying the write', async () => {
    vi.mocked(setDocumentAliases).mockRejectedValueOnce(new Error('acknowledgment lost'));
    vi.mocked(readDocumentAliases).mockResolvedValueOnce(base()).mockResolvedValueOnce(base()).mockResolvedValueOnce(base('character-a', ['Mira', 'The Lady Mira'], '2'));
    await render(); await settle(); await typeNames('Mira\nThe Lady Mira'); await click('Save names');
    expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(button('Check saved names')).toBeDefined(); expect(host.textContent).toContain('will not be replayed automatically');
    await click('Check saved names');
    expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(onSaved).toHaveBeenCalledOnce(); expect(host.textContent).toContain('current list matches');
  });

  it('keeps a differing reconciliation as a reviewable draft with explicit choices', async () => {
    vi.mocked(setDocumentAliases).mockRejectedValueOnce(new Error('lost')); vi.mocked(readDocumentAliases).mockResolvedValueOnce(base()).mockResolvedValueOnce(base()).mockResolvedValueOnce(base('character-a', ['Mira', 'Stored'], '3'));
    await render(); await settle(); await typeNames('Mira\nMine'); await click('Save names'); await click('Check saved names');
    expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(button('Keep my names')).toBeDefined(); expect(button('Discard changes').disabled).toBe(false);
    await click('Discard changes'); expect(textarea().value).toBe('Mira\nStored'); expect(button('Save names').disabled).toBe(true);
  });

  it('ignores hidden and late reads when the document owner changes', async () => {
    const first = deferred<DocumentAliasesRead>(); const second = deferred<DocumentAliasesRead>();
    vi.mocked(readDocumentAliases).mockImplementation((_access, documentId) => documentId === 'character-a' ? first.promise : second.promise);
    await render({ visible: false }); expect(readDocumentAliases).not.toHaveBeenCalled();
    await render({ visible: true }); expect(readDocumentAliases).toHaveBeenCalledOnce();
    await render({ documentId: 'character-b', title: 'The Archive' }); expect(readDocumentAliases).toHaveBeenCalledTimes(2);
    await act(async () => second.resolve(base('character-b', ['Archive name'], '7'))); await settle();
    await act(async () => first.resolve(base('character-a', ['Stale name'], '6'))); await settle();
    expect(textarea().value).toBe('Archive name'); expect(host.textContent).toContain('The Archive'); expect(host.textContent).not.toContain('Stale name');
  });

  it('restarts an initial read on a writer-lease change while preserving the current owner', async () => {
    const first = deferred<DocumentAliasesRead>(); const second = deferred<DocumentAliasesRead>();
    vi.mocked(readDocumentAliases).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    await render(); expect(readDocumentAliases).toHaveBeenCalledOnce();
    await render({ access: { ...access, writerLease: 'lease-2' } }); expect(readDocumentAliases).toHaveBeenCalledTimes(2);
    await act(async () => second.resolve(base('character-a', ['Fresh name'], '4'))); await settle();
    await act(async () => first.resolve(base('character-a', ['Stale name'], '3'))); await settle();
    expect(textarea().value).toBe('Fresh name');
  });

  it('treats a dispatched write with a changed lease as uncertain and checks with the current lease', async () => {
    const write = deferred<{ source: string; policy: string }>();
    vi.mocked(readDocumentAliases).mockResolvedValueOnce(base()).mockResolvedValueOnce(base()).mockResolvedValueOnce(base('character-a', ['Mira', 'Mine'], '3'));
    vi.mocked(setDocumentAliases).mockReturnValueOnce(write.promise);
    await render(); await settle(); await typeNames('Mira\nMine'); await click('Save names');
    await render({ access: { ...access, writerLease: 'lease-2' } });
    await act(async () => write.resolve({ source: '2', policy: '1' })); await settle();
    expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(button('Check saved names')).toBeDefined();
    await click('Check saved names'); expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(onSaved).toHaveBeenCalledOnce();
  });

  it('keeps a read-only reconciliation uncertain across a lease change and never opens a second write', async () => {
    const firstCheck = deferred<DocumentAliasesRead>(); const secondCheck = deferred<DocumentAliasesRead>();
    vi.mocked(readDocumentAliases).mockResolvedValueOnce(base()).mockResolvedValueOnce(base()).mockReturnValueOnce(firstCheck.promise).mockReturnValueOnce(secondCheck.promise);
    vi.mocked(setDocumentAliases).mockRejectedValueOnce(new Error('lost'));
    await render(); await settle(); await typeNames('Mira\nMine'); await click('Save names');
    await click('Check saved names');
    await render({ access: { ...access, writerLease: 'lease-2' } });
    await act(async () => firstCheck.resolve(base('character-a', ['Mira', 'Mine'], '3'))); await settle();
    expect(button('Save names').disabled).toBe(true); expect(button('Check saved names')).toBeDefined(); expect(setDocumentAliases).toHaveBeenCalledOnce();
    await act(async () => secondCheck.resolve(base('character-a', ['Mira', 'Mine'], '3'))); await click('Check saved names');
    expect(setDocumentAliases).toHaveBeenCalledOnce(); expect(onSaved).toHaveBeenCalledOnce();
  });

  it('registers a plain-error leave guard without saving, and preserves the draft while disabled', async () => {
    await render(); await settle(); await typeNames('Mira\nUncommitted');
    await render({ disabled: true });
    expect(textarea().disabled).toBe(true); expect(button('Save names').disabled).toBe(true);
    await expect(guard?.()).rejects.toEqual(new Error('Save or discard names before leaving.'));
    expect(setDocumentAliases).not.toHaveBeenCalled(); expect(beforeSave).not.toHaveBeenCalled();
  });

  it('refuses the close action while dirty and waits for an in-flight save before allowing leave', async () => {
    const onClose = vi.fn(); const write = deferred<{ source: string; policy: string }>();
    vi.mocked(setDocumentAliases).mockReturnValueOnce(write.promise);
    await render({ onClose }); await settle(); await typeNames('Mira\nUncommitted');
    await act(async () => button('Close names').click()); await settle();
    expect(onClose).not.toHaveBeenCalled(); expect(host.textContent).toContain('Save or discard names before closing.');
    await click('Save names');
    const leaving = guard!(); let settled = false; void leaving.then(() => { settled = true; });
    await Promise.resolve(); expect(settled).toBe(false);
    await act(async () => write.resolve({ source: '2', policy: '1' })); await expect(leaving).resolves.toBeUndefined();
  });

  it('rejects overlong names without truncating or dispatching a write', async () => {
    await render(); await settle(); await typeNames('x'.repeat(257));
    expect(button('Save names').disabled).toBe(true); expect(host.textContent).toContain('This name is too long'); expect(setDocumentAliases).not.toHaveBeenCalled();
  });
});
