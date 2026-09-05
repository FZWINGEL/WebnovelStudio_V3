// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ExportDialog } from './ExportDialog';
import { bodyHash } from '../editor/document';
import type { DraftExportPreview, DraftFormat } from '../ipc/exports';
import type { ProjectAccess } from '../ipc/projects';

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
let host: HTMLDivElement; let root: Root;
let prepare: ReturnType<typeof vi.fn<(format: DraftFormat) => Promise<DraftExportPreview>>>;
let save: ReturnType<typeof vi.fn<(preview: DraftExportPreview) => Promise<string | null>>>;
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: Error) => void; return { promise: new Promise<T>((accept, fail) => { resolve = accept; reject = fail; }), resolve: (value: T) => resolve(value), reject: (reason: Error) => reject(reason) }; }
async function preview(format: DraftFormat, id = 'preview', text = format === 'markdown' ? '**A promise** <script>inert</script>' : 'A promise') : Promise<DraftExportPreview> {
  return { id, projectId: access.projectId, operationNamespace: access.operationNamespace, sourceHead: { documentId: 'chapter', version: '4', bodyHash: 'source-hash' }, revisionId: 'revision',
    format, formatVersion: 1, previewText: text, utf8Bytes: new TextEncoder().encode(text).length, sha256: await bodyHash(text), formatLoss: 'The file contains this document only.' };
}
async function render(props: Partial<React.ComponentProps<typeof ExportDialog>> = {}) {
  await act(async () => root.render(<ExportDialog access={access} documentId="chapter" title="The promise" onPrepare={prepare} onExport={save} onClose={() => {}} {...props} />));
}
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === label)!; }
async function click(label: string) { await act(async () => button(label).click()); }
async function waitFor(assertion: () => void) {
  await vi.waitFor(async () => {
    await act(async () => { await new Promise<void>(resolve => setTimeout(resolve, 0)); });
    assertion();
  }, { interval: 10, timeout: 2000 });
  await act(async () => { await new Promise<void>(resolve => setTimeout(resolve, 0)); });
}
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.setAttribute('open', ''); } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.removeAttribute('open'); } });
  prepare = vi.fn(async format => preview(format)); save = vi.fn(async () => 'C:/Exports/Chapter.md');
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('exact draft export preview', () => {
  it('renders file content as inert text and writes only after an explicit destination action', async () => {
    const pending = deferred<string | null>(); const close = vi.fn(); save.mockReturnValue(pending.promise);
    await render({ onClose: close }); await waitFor(() => expect(host.querySelector('pre')?.textContent).toContain('<script>inert</script>'));
    expect(host.querySelector('script')).toBeNull(); expect(save).not.toHaveBeenCalled();
    await click('Choose destination…'); expect(save).toHaveBeenCalledOnce(); expect(button('Close').disabled).toBe(true);
    expect(host.querySelector('select')!.disabled).toBe(true); await click('Close'); expect(close).not.toHaveBeenCalled();
    await act(async () => host.querySelector('dialog')!.dispatchEvent(new Event('cancel', { cancelable: true }))); expect(close).not.toHaveBeenCalled();
    await act(async () => pending.resolve('C:/Exports/Chapter.md')); expect(host.textContent).toContain('Draft exported: C:/Exports/Chapter.md');
    await click('Done'); expect(close).toHaveBeenCalledOnce();
  });
  it('drops a late prior-format response and error after a newer selection', async () => {
    const first = deferred<DraftExportPreview>(); prepare.mockImplementationOnce(() => first.promise);
    await render(); await act(async () => { const select = host.querySelector('select')!; select.value = 'plainText'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    await waitFor(() => expect(host.querySelector('pre')?.textContent).toBe('A promise'));
    await act(async () => first.reject(new Error('Old Markdown request failed')));
    expect(host.textContent).not.toContain('Old Markdown request failed'); await click('Choose destination…');
    expect(save.mock.calls[0][0].format).toBe('plainText');
  });
  it('refuses a foreign or mismatched preview before offering a file write', async () => {
    prepare.mockResolvedValueOnce({ ...await preview('markdown'), projectId: 'other-project' });
    await render(); await waitFor(() => expect(host.querySelector('[role=alert]')).not.toBeNull());
    expect(button('Choose destination…').disabled).toBe(true); expect(host.querySelector('pre')).toBeNull();
    prepare.mockResolvedValueOnce({ ...await preview('markdown'), sha256: 'invalid' }); await click('Prepare export again');
    await waitFor(() => expect(host.querySelector('[role=alert]')).not.toBeNull()); expect(save).not.toHaveBeenCalled();
    await click('Prepare export again'); await waitFor(() => expect(host.querySelector('pre')).not.toBeNull());
  });
  it('keeps a cancelled destination ready without automatically writing or refreshing', async () => {
    save.mockResolvedValue(null); await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false));
    await click('Choose destination…'); expect(host.textContent).toContain('No destination chosen');
    expect(prepare).toHaveBeenCalledOnce(); expect(save).toHaveBeenCalledOnce(); expect(button('Choose destination…').disabled).toBe(false);
  });
  it('requires an explicit new preview after a possibly-written outcome and never replays it automatically', async () => {
    save.mockRejectedValueOnce({ code: 'ExportRecordUnavailable', detail: 'The file was written but its history could not be recorded.' });
    await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(host.textContent).toContain('Check the chosen destination'); expect(button('Choose destination…').disabled).toBe(true);
    expect(prepare).toHaveBeenCalledOnce(); expect(save).toHaveBeenCalledOnce();
    prepare.mockImplementationOnce(async format => preview(format, 'new-preview')); await click('Prepare export again');
    await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(save.mock.calls[1][0].id).toBe('new-preview');
  });
  it('permits another destination after a definite no-overwrite refusal', async () => {
    save.mockRejectedValueOnce({ code: 'TargetExists', detail: 'Choose a new filename.' });
    await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(host.textContent).toContain('Choose a new filename'); expect(button('Choose destination…').disabled).toBe(false);
    await click('Choose destination…'); expect(save.mock.calls[1][0]).toEqual(save.mock.calls[0][0]); expect(prepare).toHaveBeenCalledOnce();
  });
  it('requires a fresh author action for an already-recorded export', async () => {
    save.mockRejectedValueOnce({ code: 'ExportAlreadyRecorded', detail: 'This preview is already recorded.' });
    await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(button('Choose destination…').disabled).toBe(true); expect(host.textContent).toContain('This preview was already exported.');
    expect(save).toHaveBeenCalledOnce(); expect(prepare).toHaveBeenCalledOnce();
  });
  it('drops preview and export completions from a prior owner', async () => {
    const pending = deferred<string | null>(); save.mockReturnValueOnce(pending.promise);
    await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    await render({ access: { ...access, writerLease: 'new-lease' } });
    await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(save).toHaveBeenCalledTimes(2); expect(host.textContent).toContain('Draft exported: C:/Exports/Chapter.md');
    await act(async () => pending.resolve('Old result.md')); expect(host.textContent).not.toContain('Old result.md');
    expect(host.textContent).toContain('Draft exported: C:/Exports/Chapter.md');
  });
  it('asks for a new preview after a definite source refusal without implying a file was written', async () => {
    save.mockRejectedValueOnce({ code: 'WrongProjectSession', detail: 'The project session changed.' });
    await render(); await waitFor(() => expect(button('Choose destination…').disabled).toBe(false)); await click('Choose destination…');
    expect(host.textContent).toContain('The project session changed.'); expect(host.textContent).not.toContain('A file may');
    expect(button('Choose destination…').disabled).toBe(true); expect(button('Prepare export again').disabled).toBe(false);
  });
  it('shows an empty file preview without inserting placeholder text into its contents', async () => {
    prepare.mockResolvedValue(await preview('markdown', 'empty', '')); await render();
    await waitFor(() => expect(host.querySelector('pre')).not.toBeNull()); expect(host.querySelector('pre')!.textContent).toBe('');
    expect(host.textContent).toContain('The exported file will be empty.');
  });
});
