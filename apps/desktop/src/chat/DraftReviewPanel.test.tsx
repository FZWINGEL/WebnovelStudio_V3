// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { WnsDocument } from '../editor/document';
import type { AssistantDraft } from '../ipc/projectChat';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';
import { DocumentSession } from '../editor/session';
vi.mock('../assistant/ContextInspector', () => ({ ContextInspector: () => null }));
import { DraftReviewPanel, type DraftReviewPanelHandle, type StalePreviewComparison } from './DraftReviewPanel';
import type { ChatAdoptionPreview } from '../ipc/projectChat';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'A draft that should remain mounted.' }] }] } };
const beforeBody: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'A draft before review.' }] }] } };
const afterBody: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'A revised draft after review.' }] }] } };
const head = { documentId: 'draft-1', version: '1', bodyHash: 'a'.repeat(64) };
const draftDocument: DocumentRecord = { head, title: 'World draft', kind: 'world', metadataVersion: '1', body, lastCheckpointId: null, role: 'assistantDraft' };
const draft: AssistantDraft = { document: draftDocument, conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-1', initialRevisionId: 'revision-1', target: null, disposition: 'pending', dispositionVersion: '1', stale: false };
const project: OpenedProject = { project: { projectId: 'project-1', operationNamespace: 'namespace-1', title: 'Test project', formatVersion: 1 }, access, documents: [draftDocument], metadataVersion: '1', viewState: null, libraryWarning: null };

let host: HTMLDivElement;
let root: Root;
function Harness({ tick }: { tick: number }) {
  return <div data-tick={tick}><DraftReviewPanel project={project} drafts={[draft]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} /></div>;
}

beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('DraftReviewPanel editor lifecycle', () => {
  it('focuses the exact closed draft without opening an editable session', async () => {
    let handle: DraftReviewPanelHandle | null = null;
    await act(async () => root.render(<DraftReviewPanel ref={value => { handle = value; }} project={project} drafts={[{ ...draft, disposition: 'rejected' }]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    await act(async () => handle?.openDraft('draft-1'));
    expect(document.activeElement).toBe(host.querySelector('.coauthor-review-tabpanel[data-draft-id="draft-1"]'));
    expect(host.querySelector('.ProseMirror')).toBeNull();
  });

  it('can refresh a stale draft explicitly while its old adoption remains disabled', async () => {
    const refresh = vi.fn();
    const stale = { ...draft, stale: true };
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[stale]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} onRevise={refresh} />));
    const buttons = [...host.querySelectorAll('button')];
    expect(buttons.find(button => button.textContent === 'Prepare adoption preview')?.disabled).toBe(true);
    const refreshButton = buttons.find(button => button.textContent === 'Refresh with assistant');
    expect(refreshButton?.disabled).toBe(false);
    await act(async () => refreshButton!.click());
    expect(refresh).toHaveBeenCalledExactlyOnceWith(stale);
  });

  it('keeps the mounted editor alive when the parent rerenders during review polling', async () => {
    await act(async () => root.render(<Harness tick={0} />));
    const edit = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Edit this draft') as HTMLButtonElement;
    await act(async () => { edit.click(); });
    const editor = host.querySelector('.ProseMirror');
    expect(editor).not.toBeNull();
    await act(async () => root.render(<Harness tick={1} />));
    expect(host.querySelector('.ProseMirror')).toBe(editor);
    expect(editor?.isConnected).toBe(true);
  });

  it('renders draft formatting and provenance instead of flattening the document to plain text', async () => {
    const formatted: AssistantDraft = { ...draft, document: { ...draft.document, body: { schemaVersion: 1, body: { type: 'doc', content: [
      { type: 'heading', attrs: { id: 'heading-1', level: 2 }, content: [{ type: 'text', text: 'A heading', marks: [{ type: 'bold' }] }] },
      { type: 'paragraph', attrs: { id: 'paragraph-2' }, content: [{ type: 'text', text: 'A promise', marks: [{ type: 'italic' }] }] },
      { type: 'sceneBreak', attrs: { id: 'break-1' } },
    ] } } } };
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[formatted]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    expect(host.querySelector('.chat-rendered-document strong')?.textContent).toBe('A heading');
    expect(host.querySelector('.chat-rendered-document em')?.textContent).toBe('A promise');
    expect(host.querySelector('.chat-rendered-scene-break')?.textContent).toBe('* * *');
    expect(host.textContent).toContain('packet-1');
  });

  it('exposes explicit revise and reconsider actions without dispatching a request', async () => {
    const revise = vi.fn(); const reconsider = vi.fn();
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft, { ...draft, disposition: 'rejected', document: { ...draft.document, head: { ...draft.document.head, documentId: 'draft-2' } } }]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} onRevise={revise} onReconsider={reconsider} />));
    await act(async () => { (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Revise with assistant') as HTMLButtonElement).click(); });
    await act(async () => { (host.querySelectorAll('[role="tab"]')[1] as HTMLButtonElement).click(); });
    await act(async () => { (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Reconsider in a new review') as HTMLButtonElement).click(); });
    expect(revise).toHaveBeenCalledTimes(1);
    expect(reconsider).toHaveBeenCalledTimes(1);
  });

  it('opens the exact requested draft and focuses its editor through the review handle', async () => {
    let handle: DraftReviewPanelHandle | null = null;
    // This checks the focus request. Real selection geometry is covered by
    // WebView2; JSDOM does not implement Range.getClientRects.
    const focus = vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(() => {});
    await act(async () => root.render(<DraftReviewPanel ref={value => { handle = value; }} project={project} drafts={[draft]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    await act(async () => { await handle?.openDraft('draft-1'); });
    expect(host.querySelector('.ProseMirror')).not.toBeNull();
    expect(focus).toHaveBeenCalled();
    focus.mockRestore();
  });

  it('provides named draft tabs and flushes the active editor before switching', async () => {
    const second: AssistantDraft = {
      ...draft,
      document: { ...draft.document, title: 'Character sketch', kind: 'character', head: { ...draft.document.head, documentId: 'draft-2' } },
      initialRevisionId: 'revision-2',
    };
    const flush = vi.spyOn(DocumentSession.prototype, 'flush').mockResolvedValue();
    await act(async () => root.render(<DraftReviewPanel project={project} viewKey="tabs-flush-test" drafts={[draft, second]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    const tabs = [...host.querySelectorAll('[role="tab"]')];
    expect(tabs.map(tab => tab.textContent)).toEqual(expect.arrayContaining(['World draftworld · v1 · Not adopted', 'Character sketchcharacter · v1 · Not adopted']));
    await act(async () => { (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Edit this draft') as HTMLButtonElement).click(); });
    await act(async () => { (tabs[1] as HTMLButtonElement).click(); });
    expect(flush).toHaveBeenCalled();
    expect((tabs[1] as HTMLButtonElement).getAttribute('aria-selected')).toBe('true');
    expect(host.querySelector('.coauthor-review-document-header h3')?.textContent).toBe('Character sketch');
    flush.mockRestore();
  });

  it('opens a second draft in its own editor session rather than reusing the previous draft buffer', async () => {
    let handle: DraftReviewPanelHandle | null = null;
    const second: AssistantDraft = { ...draft, document: { ...draft.document, title: 'Another draft', head: { ...head, documentId: 'draft-2' }, body: afterBody } };
    const flush = vi.spyOn(DocumentSession.prototype, 'flush').mockResolvedValue();
    const focus = vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(() => {});
    try {
      await act(async () => root.render(<DraftReviewPanel ref={value => { handle = value; }} project={project} viewKey="distinct-editor-sessions" drafts={[draft, second]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
      await act(async () => { await handle?.openDraft('draft-1'); });
      const firstEditor = host.querySelector('.ProseMirror');
      await act(async () => { await handle?.openDraft('draft-2'); });
      expect(flush).toHaveBeenCalled();
      expect(host.querySelectorAll('.ProseMirror')).toHaveLength(1);
      expect(host.querySelector('.ProseMirror')).not.toBe(firstEditor);
      expect(host.querySelector('.ProseMirror')?.textContent).toContain('A revised draft after review.');
    } finally { flush.mockRestore(); focus.mockRestore(); }
  });

  it('keeps the complete exact preview available from Changes and pins its adoption action', async () => {
    const preview = {
      id: 'preview-tabs', version: '1', digest: 'p'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      targets: [{ draft: { head: { ...head, version: '4' }, dispositionVersion: '1' }, draftRevisionId: 'revision-4', documentId: 'target-1', title: 'World draft', kind: 'world', before: { ...draftDocument, head: { ...head, documentId: 'target-1' }, body: beforeBody }, body: afterBody }],
    } as ChatAdoptionPreview;
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    expect(host.querySelector('.coauthor-review-changes')?.hasAttribute('hidden')).toBe(true);
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} preview={preview} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    expect(host.querySelector('.coauthor-review-changes')?.hasAttribute('hidden')).toBe(false);
    const changes = Array.from(host.querySelectorAll('[role="tab"]')).find(tab => tab.textContent === 'Changes') as HTMLButtonElement;
    await act(async () => changes.click());
    expect(host.querySelector('.coauthor-review-changes')?.hasAttribute('hidden')).toBe(false);
    expect(host.querySelector('[aria-label="Before paragraph/block 1"] p')?.textContent).toContain('A draft before review.');
    expect(host.querySelector('[aria-label="After paragraph/block 1"] p')?.textContent).toContain('A revised draft after review.');
    expect(host.querySelector('.coauthor-review-footer .coauthor-review-primary')?.textContent).toBe('Adopt World draft draft v4');
  });

  it('labels the adoption action with the exact draft title and version', async () => {
    const preview = {
      id: 'preview-1', version: '1', digest: 'd'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      targets: [{ draft: { head: { ...head, version: '7' }, dispositionVersion: '1' }, draftRevisionId: 'revision-7', documentId: 'target-1', title: 'World sketch', kind: 'world', before: null, body }],
    } as ChatAdoptionPreview;
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} preview={preview} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} />));
    expect(Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Adopt World sketch draft v7')).not.toBeNull();
  });

  it('shows the immutable effects manifest and sends that exact preview to Apply', async () => {
    const preview = {
      id: 'preview-effects', version: '1', digest: 'z'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      effects: {
        version: 'chat-adoption-effects.v1', sourceOutputHash: 'output-hash', relationshipDependencies: [],
        protectedContent: [{ targetDocumentId: 'target-1', sourceHead: { ...head, documentId: 'target-1', version: '3' }, text: 'Preserve this metadata.', textHash: 'protected-hash' }],
        proposedRelationships: [], impacts: [{ targetDocumentId: 'target-1', kind: 'possibleTension', reason: 'Review this consequence.', relationshipKey: null }], supersessions: [], placements: [],
      },
      targets: [{ draft: { head: { ...head, version: '7' }, dispositionVersion: '1' }, draftRevisionId: 'revision-7', documentId: 'target-1', title: 'World sketch', kind: 'world', before: null, body }],
    } as ChatAdoptionPreview;
    const apply = vi.fn();
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} preview={preview} onPrepareAdoption={() => {}} onApplyPreview={apply} onReject={() => {}} />));
    expect(host.textContent).toContain('Complete effects manifest');
    expect(host.textContent).toContain('Manifest versionchat-adoption-effects.v1');
    expect(host.textContent).toContain('Preserve this metadata.');
    expect(host.textContent).toContain('Editable scope: Whole document');
    expect(host.textContent).toContain('New target: no existing metadata/order to replace.');
    const button = Array.from(host.querySelectorAll('button')).find(candidate => candidate.textContent === 'Adopt World sketch draft v7') as HTMLButtonElement;
    await act(async () => button.click());
    expect(apply).toHaveBeenCalledExactlyOnceWith(preview);
  });

  it('shows an exact immutable preview diff and preserves the originating request when current inputs change', async () => {
    const preview = {
      id: 'preview-diff', version: '1', digest: 'e'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      targets: [{ draft: { head: { ...head, version: '2' }, dispositionVersion: '1' }, draftRevisionId: 'revision-2', documentId: 'destination-1', title: 'World draft', kind: 'world', before: { ...draftDocument, head: { ...draftDocument.head, documentId: 'destination-1' }, body: beforeBody }, body: afterBody }],
    } as ChatAdoptionPreview;
    const reviewContext = { 'run-1': { instruction: 'Build a quiet harbor mystery.', assumptions: ['Keep the existing narrator voice.'] } } as const;
    const renderPanel = (currentInput: string) => <div data-current-input={currentInput}><DraftReviewPanel project={project} drafts={[draft]} preview={preview} reviewContext={reviewContext} onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} /></div>;
    await act(async () => root.render(renderPanel('first current composer text')));
    expect(host.textContent).toContain('Build a quiet harbor mystery.');
    expect(host.textContent).toContain('Keep the existing narrator voice.');
    expect(host.querySelector('.chat-draft-provenance dd')?.textContent).toBe('preview-diff');
    expect([...host.querySelectorAll('.chat-draft-provenance dd')].some(element => element.textContent === 'run-1')).toBe(true);
    expect(host.textContent).toContain('1 paragraph/block changed');
    expect(host.querySelector('del')?.textContent).toContain('before');
    expect(host.querySelector('ins')?.textContent).toContain('revised');
    expect(host.querySelector('[aria-label="Before paragraph/block 1"] p')?.textContent).toBe('A draft before review.');
    expect(host.querySelector('[aria-label="After paragraph/block 1"] p')?.textContent).toBe('A revised draft after review.');
    const reviewHeadings = [...host.querySelectorAll('.chat-adoption-preview article h4')].map(element => element.textContent);
    expect(reviewHeadings.slice(0, 7)).toEqual(['World draft', 'What changed', 'Author request', 'Working assumptions', 'Affected documents', 'Before', 'After · World draft']);
    await act(async () => root.render(renderPanel('a different current composer text')));
    expect(host.textContent).toContain('Build a quiet harbor mystery.');
    expect(host.textContent).not.toContain('a different current composer text');
  });

  it('keeps a stale preview immutable until current sources are compared and explicitly prepared', async () => {
    const prepare = vi.fn();
    const currentSource: DocumentRecord = { ...draftDocument, head: { ...draftDocument.head, documentId: 'destination-1', version: '3', bodyHash: 'b'.repeat(64) }, body: { ...afterBody, body: { ...afterBody.body, content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'The saved source changed after the preview.' }] }] } } };
    const compare = vi.fn(async (): Promise<StalePreviewComparison> => ({
      previewId: 'preview-stale',
      targets: [{ documentId: 'destination-1', current: currentSource }],
    }));
    const preview = {
      id: 'preview-stale', version: '1', digest: 'f'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      targets: [{ draft: { head: { ...head, version: '2' }, dispositionVersion: '1' }, draftRevisionId: 'revision-2', documentId: 'destination-1', title: 'World draft', kind: 'world', before: { ...draftDocument, head: { ...draftDocument.head, documentId: 'destination-1' }, body: beforeBody }, body: afterBody }],
    } as ChatAdoptionPreview;
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} preview={preview} previewError="The preview is stale: the source changed." onPrepareAdoption={prepare} onApplyPreview={() => {}} onReject={() => {}} onCompareStalePreview={compare} onPrepareAgainstCurrent={prepare} />));
    const summary = host.querySelector('.chat-draft-diff-details > summary') as HTMLElement;
    expect(summary).not.toBeNull();
    expect(summary.getAttribute('tabindex')).toBe('0');
    expect(host.querySelector('[aria-label="Paragraph changes"]')).not.toBeNull();
    expect((host.querySelector('[aria-label^="Preview is stale"]') as HTMLButtonElement).disabled).toBe(true);
    expect(Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Prepare against current versions')).toBeUndefined();
    const compareButton = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Compare sources/current heads') as HTMLButtonElement;
    expect(compareButton).not.toBeNull();
    await act(async () => compareButton.click());
    expect(compare).toHaveBeenCalledExactlyOnceWith(preview);
    expect(host.textContent).toContain('current v3');
    expect(host.textContent).toContain('The saved source changed after the preview.');
    const reprepare = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Prepare against current versions') as HTMLButtonElement;
    expect(reprepare).not.toBeNull();
    await act(async () => reprepare.click());
    expect(prepare).toHaveBeenCalledExactlyOnceWith([draft], expect.objectContaining({ previewId: 'preview-stale' }));
    expect(host.textContent).toContain('World draft');
    expect(host.querySelector('.chat-adoption-preview .chat-draft-provenance dd')?.textContent).toBe('preview-stale');
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[{ ...draft, stale: true }]} preview={preview} previewError="The preview is stale: the source changed." onPrepareAdoption={prepare} onApplyPreview={() => {}} onReject={() => {}} onCompareStalePreview={compare} onPrepareAgainstCurrent={prepare} />));
    expect(Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Prepare against current versions')).toBeUndefined();
  });

  it('allows an explicit reprepare for a new target with no existing working body', async () => {
    const compare = vi.fn(async (): Promise<StalePreviewComparison> => ({ previewId: 'preview-new', targets: [{ documentId: 'new-destination', current: null }] }));
    const prepare = vi.fn();
    const preview = {
      id: 'preview-new', version: '1', digest: 'n'.repeat(64), projectId: access.projectId,
      operationNamespace: access.operationNamespace, conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1',
      targets: [{ draft: { head: { ...head, version: '4' }, dispositionVersion: '1' }, draftRevisionId: 'revision-4', documentId: 'new-destination', title: 'New chapter', kind: 'chapter', before: null, body: afterBody }],
    } as ChatAdoptionPreview;
    await act(async () => root.render(<DraftReviewPanel project={project} drafts={[draft]} preview={preview} previewError="The preview is stale: the project changed." onPrepareAdoption={() => {}} onApplyPreview={() => {}} onReject={() => {}} onCompareStalePreview={compare} onPrepareAgainstCurrent={prepare} />));
    const compareButton = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Compare sources/current heads') as HTMLButtonElement;
    await act(async () => compareButton.click());
    expect(host.textContent).toContain('New document: no existing working body.');
    const reprepare = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Prepare against current versions') as HTMLButtonElement;
    expect(reprepare.disabled).toBe(false);
    await act(async () => reprepare.click());
    expect(prepare).toHaveBeenCalledExactlyOnceWith([draft], expect.objectContaining({ previewId: 'preview-new' }));
  });
});
