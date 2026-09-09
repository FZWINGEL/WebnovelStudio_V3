// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { WnsDocument } from '../editor/document';
import type { AssistantDraft } from '../ipc/projectChat';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import { ProjectDocumentsPanel } from './ProjectDocumentsPanel';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p1' }, content: [{ type: 'text', text: 'Draft' }] }] } };
const draftDocument: DocumentRecord = { head: { documentId: 'draft-1', version: '2', bodyHash: 'a'.repeat(64) }, title: 'World sketch', kind: 'world', metadataVersion: '1', body, lastCheckpointId: null, role: 'assistantDraft' };
const draft: AssistantDraft = { document: draftDocument, conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-1', initialRevisionId: 'revision-1', target: null, disposition: 'pending', dispositionVersion: '1', stale: false };
const project: OpenedProject = { project: { projectId: access.projectId, operationNamespace: access.operationNamespace, title: 'Test project', formatVersion: 1 }, access, documents: [draftDocument], metadataVersion: '1', viewState: null, libraryWarning: null };

let host: HTMLDivElement;
let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); localStorage.clear(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('ProjectDocumentsPanel draft routing', () => {
  it('retains document search and tab on return without sharing them with another project', async () => {
    const ordinary = { ...draftDocument, role: 'ordinary' as const };
    const first = { ...project, documents: [ordinary] };
    const second = { ...first, project: { ...project.project, projectId: 'project-2' }, access: { ...access, projectId: 'project-2' } };
    const render = async (value: OpenedProject) => act(async () => root.render(<ProjectDocumentsPanel project={value} onOpenDocument={() => {}} />));
    await render(first);
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'All documents')!.click());
    const input = host.querySelector<HTMLInputElement>('input[type="search"]')!;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'World'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    await render(second);
    expect(host.querySelector('input[type="search"]')).toBeNull();
    await render(first);
    expect(host.querySelector<HTMLInputElement>('input[type="search"]')?.value).toBe('World');
    expect(host.textContent).toContain('Working · v2');
  });
  it('attaches an ordinary source without opening it or offering draft source authority', async () => {
    const ordinary = { ...draftDocument, role: 'ordinary' as const, head: { ...draftDocument.head, documentId: 'world-1' } };
    const onOpenDocument = vi.fn();
    const onAttachSource = vi.fn();
    await act(async () => root.render(<ProjectDocumentsPanel project={{ ...project, documents: [ordinary, draftDocument] }} activeDocument={ordinary} drafts={[draft]} onOpenDocument={onOpenDocument} onAttachSource={onAttachSource} />));
    const attach = host.querySelector<HTMLButtonElement>('[aria-label="Use World sketch as a source"]')!;
    await act(async () => attach.click());
    expect(onAttachSource).toHaveBeenCalledWith(ordinary);
    expect(onOpenDocument).not.toHaveBeenCalled();
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent?.startsWith('Drafts to review'))!.click());
    expect(host.querySelector('[aria-label="Use World sketch as a source"]')).toBeNull();
  });

  it('names the return to project context before replacing a staged chapter task', async () => {
    const ordinary = { ...draftDocument, role: 'ordinary' as const };
    await act(async () => root.render(<ProjectDocumentsPanel project={{ ...project, documents: [ordinary] }} activeDocument={ordinary} chapterTaskActive onOpenDocument={() => {}} onAttachSource={() => {}} />));
    expect(host.querySelector('[aria-label="Return to project conversation and use World sketch as a source"]')).not.toBeNull();
  });

  it('passes the selected draft identity to the review owner', async () => {
    const onOpenDraft = vi.fn();
    await act(async () => root.render(<ProjectDocumentsPanel project={project} drafts={[draft]} onOpenDocument={() => {}} onOpenDraft={onOpenDraft} />));
    await act(async () => { (Array.from(host.querySelectorAll('button')).find(button => button.textContent?.startsWith('Drafts to review')) as HTMLButtonElement).click(); });
    await act(async () => { (Array.from(host.querySelectorAll('nav button')).find(button => button.textContent?.includes('World sketch')) as HTMLButtonElement).click(); });
    expect(onOpenDraft).toHaveBeenCalledWith(draft);
  });
});
