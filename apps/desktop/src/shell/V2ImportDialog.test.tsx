// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { V2ImportDialog } from './V2ImportDialog';
import type { OpenedProject } from '../ipc/projects';
import type { V2ImportPreview, V2SourceProjectList } from '../ipc/v2Import';

const ipc = vi.hoisted(() => ({
  list: vi.fn(),
  preview: vi.fn(),
  importProject: vi.fn(),
}));
vi.mock('../ipc/v2Import', () => ({
  v2ImportListProjects: ipc.list,
  v2ImportPreview: ipc.preview,
  v2Import: ipc.importProject,
}));

const source: V2SourceProjectList = {
  sourcePath: 'C:/Users/author/V2/story.db',
  projects: [{ sourceProjectId: 'source-project', title: 'The Old Story', slug: 'the-old-story', chapterCount: 2 }],
};
const preview: V2ImportPreview = {
  importFormatVersion: 1,
  source: { schemaVersion: 8, sourceBytes: 128, sourceSha256: 'a'.repeat(64), migrationVersions: ['001', '008'], projectCount: 1 },
  project: { sourceProjectId: 'source-project', title: 'The Old Story', slug: 'the-old-story', chapterCount: 2 },
  chapters: [
    {
      sourceId: 'chapter-1', chapterNumber: 1, title: 'First', retiredAt: null,
      workingProse: { state: 'present', text: 'Stored first chapter.' }, bodySelection: { kind: 'workingProse' },
      workingProseBasedOnDraftId: 'draft-1', approvedDraftId: 'draft-1', draftCount: 1,
      drafts: [{ sourceId: 'draft-1', version: 1, prose: 'Stored first chapter.', isApproved: true, createdAt: '2026-09-06T00:00:00Z' }],
    },
    {
      sourceId: 'chapter-2', chapterNumber: 2, title: 'Second', retiredAt: null,
      workingProse: { state: 'missing' }, bodySelection: { kind: 'requiresAuthorChoice' },
      workingProseBasedOnDraftId: null, approvedDraftId: 'draft-2', draftCount: 1,
      drafts: [{ sourceId: 'draft-2', version: 3, prose: 'A saved draft.', isApproved: false, createdAt: '2026-09-06T00:00:00Z' }],
    },
  ],
  legacy: { recordCounts: { characters: 1 }, totalJsonBytes: 32 },
};
const opened = {
  project: { projectId: 'new-project', title: 'Imported story', slug: 'imported-story' },
  session: { projectId: 'new-project', operationNamespace: 'new-namespace', session: 'new-session', writerLease: 'new-lease' },
} as unknown as OpenedProject;

let host: HTMLDivElement;
let root: Root;
const onImported = vi.fn();
const onClose = vi.fn();

async function render() {
  await act(async () => root.render(<V2ImportDialog session="session" onImported={onImported} onClose={onClose} />));
}
async function settle() {
  await vi.waitFor(async () => {
    await act(async () => { await new Promise<void>(resolve => setTimeout(resolve, 0)); });
  }, { interval: 10, timeout: 1000 });
}
function button(label: string): HTMLButtonElement {
  return [...host.querySelectorAll('button')].find(item => item.textContent === label) as HTMLButtonElement;
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.setAttribute('open', ''); } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.removeAttribute('open'); } });
  ipc.list.mockResolvedValue(source);
  ipc.preview.mockResolvedValue({ sourcePath: source.sourcePath, preview });
  ipc.importProject.mockResolvedValue(opened);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.clearAllMocks();
});

describe('V2 import dialog', () => {
  it('lists source projects and requires an explicit choice for missing prose', async () => {
    await render();
    await settle();
    expect(host.textContent).toContain('The Old Story');
    const projectSelect = host.querySelector<HTMLSelectElement>('#v2-import-project')!;
    await act(async () => {
      projectSelect.value = 'source-project';
      projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Review project').click());
    await settle();
    expect(host.textContent).toContain('Working text is missing; choose a saved draft or empty text');
    expect(button('Import project').disabled).toBe(true);

    const bodySelect = [...host.querySelectorAll('select')].find(select => select !== projectSelect) as HTMLSelectElement;
    await act(async () => {
      bodySelect.value = 'empty';
      bodySelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(button('Import project').disabled).toBe(false);
  });

  it('sends the reviewed fingerprint and exact missing-body decisions', async () => {
    await render();
    await settle();
    const projectSelect = host.querySelector<HTMLSelectElement>('#v2-import-project')!;
    await act(async () => {
      projectSelect.value = 'source-project';
      projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Review project').click());
    await settle();
    const bodySelect = [...host.querySelectorAll('select')].find(select => select !== projectSelect) as HTMLSelectElement;
    await act(async () => {
      bodySelect.value = 'draft-2';
      bodySelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Import project').click());
    expect(ipc.importProject).toHaveBeenCalledOnce();
    expect(ipc.importProject.mock.calls[0][0]).toMatchObject({
      sourcePath: source.sourcePath,
      sourceProjectId: 'source-project',
      expectedSourceSha256: 'a'.repeat(64),
      choices: [{ sourceChapterId: 'chapter-2', choice: { draft: { sourceDraftId: 'draft-2' } } }],
    });
    expect(ipc.importProject.mock.calls[0][0].operationId).toMatch(/^[0-9a-f-]{36}$/);
    expect(ipc.importProject.mock.calls[0][1]).toBe('session');
    expect(onImported).toHaveBeenCalledWith(opened);
  });

  it('keeps an uncertain staged import immutable while a local retry reconciles it', async () => {
    ipc.importProject.mockRejectedValueOnce(new Error('The import acknowledgment was lost.')).mockResolvedValueOnce(opened);
    await render();
    await settle();
    const projectSelect = host.querySelector<HTMLSelectElement>('#v2-import-project')!;
    await act(async () => {
      projectSelect.value = 'source-project';
      projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Review project').click());
    await settle();
    const bodySelect = [...host.querySelectorAll('select')].find(select => select !== projectSelect) as HTMLSelectElement;
    await act(async () => {
      bodySelect.value = 'empty';
      bodySelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Import project').click());
    expect(button('Check import')).not.toBeNull();
    expect((host.querySelector('#v2-import-title') as HTMLInputElement).disabled).toBe(true);
    expect(bodySelect.disabled).toBe(true);
    expect(host.textContent).toContain('may already exist');
    const firstRequest = ipc.importProject.mock.calls[0][0];
    await act(async () => button('Check import').click());
    expect(ipc.importProject).toHaveBeenCalledTimes(2);
    expect(ipc.importProject.mock.calls[1][0]).toEqual(firstRequest);
    expect(onImported).toHaveBeenCalledWith(opened);
  });

  it('allows closing after an uncertain import so the library can recover it', async () => {
    ipc.importProject.mockRejectedValueOnce(new Error('The import acknowledgment was lost.'));
    await render();
    await settle();
    const projectSelect = host.querySelector<HTMLSelectElement>('#v2-import-project')!;
    await act(async () => {
      projectSelect.value = 'source-project';
      projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Review project').click());
    await settle();
    const bodySelect = [...host.querySelectorAll('select')].find(select => select !== projectSelect) as HTMLSelectElement;
    await act(async () => {
      bodySelect.value = 'empty';
      bodySelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => button('Import project').click());
    expect(button('Cancel').disabled).toBe(false);
    await act(async () => button('Cancel').click());
    expect(onClose).toHaveBeenCalledOnce();
  });
});
