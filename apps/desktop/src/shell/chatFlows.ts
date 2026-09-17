/** Chat-surface document flows: chapter review opens, adoption lifecycle, and chat-created documents. */
import { useRef, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import { DocumentSession } from '../editor';
import { createDocument, reconcileProject, readDocument, projectTransport, type CreateDocumentIntent, type DocumentRecord, type OpenedProject, type ProjectAccess, type ViewState } from '../ipc/projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, runCreateIntent } from '../ipc/createIntent';
import type { DiscussionRun } from '../ipc/discussions';
import type { ChatAdoptionTarget } from '../ipc/projectChat';
import { projectIdentity, sameWorkspaceIdentity, sameAccessIdentity, workspaceErrorText as errorText } from './workspaceContracts';
import type { ActiveDocument } from './documentWorkspace';

/** Prepared adoption source kept locally so failure recovery never depends on a second IPC read. */
export type ChatAdoptionSource = Readonly<{
  projectId: string;
  documentId: string;
  access: ProjectAccess;
  record: DocumentRecord;
  viewState: ViewState | null;
}>;

export interface ChatFlowContext {
  rendererId: string;
  project: OpenedProject | null;
  projectRef: MutableRefObject<OpenedProject | null>;
  activeRef: MutableRefObject<ActiveDocument | null>;
  documentIntent: MutableRefObject<CreateDocumentIntent | null>;
  perform(work: () => Promise<void>): Promise<void>;
  setError(message: string): void;
  setProject: Dispatch<SetStateAction<OpenedProject | null>>;
  setActive: Dispatch<SetStateAction<ActiveDocument | null>>;
  setChatReviewRun: Dispatch<SetStateAction<DiscussionRun | null>>;
  activate(opened: OpenedProject, document?: DocumentRecord): void;
  navigate<T>(prepare: () => Promise<T>): Promise<T>;
  flushWorkshop(): Promise<void>;
  settlePendingDocumentIntent(): Promise<{ record: DocumentRecord; access: ProjectAccess; opened: OpenedProject } | null>;
  adoptCreateSnapshot(snapshot: OpenedProject): Promise<ProjectAccess>;
}

export function useChatFlows(context: ChatFlowContext) {
  const {
    rendererId, project, projectRef, activeRef, documentIntent,
    perform, setError, setProject, setActive, setChatReviewRun,
    activate, navigate, flushWorkshop, settlePendingDocumentIntent, adoptCreateSnapshot,
  } = context;
  const chatAdoptionTarget = useRef<ChatAdoptionSource | null>(null);

  async function openChatChapterResult(run: DiscussionRun): Promise<void> {
    const currentProject = projectRef.current;
    if (!currentProject || run.owner.projectId !== currentProject.project.projectId || run.owner.operationNamespace !== currentProject.access.operationNamespace) return;
    const identity = projectIdentity(currentProject);
    await perform(async () => {
      if (!sameWorkspaceIdentity(projectRef.current, identity)) return;
      const current = activeRef.current;
      if (current?.record.head.documentId !== run.target.documentId) {
        const sourceSession = current?.session ?? null;
        const access = current?.session.projectAccess ?? currentProject.access;
        const record = await navigate(() => readDocument(access, run.target.documentId));
        // Navigation/read can settle after the author has switched projects or
        // replaced the mounted session. Never let that old result activate a
        // document in the new workspace.
        if (!sameWorkspaceIdentity(projectRef.current, identity)) return;
        if (sourceSession && activeRef.current?.session !== sourceSession) return;
        const latestProject = projectRef.current;
        if (!latestProject) return;
        activate({ ...latestProject, access }, record);
      }
      if (!sameWorkspaceIdentity(projectRef.current, identity)) return;
      setChatReviewRun(run);
    });
  }

  async function prepareChatAdoption(targets: ChatAdoptionTarget[]): Promise<void> {
    const currentProject = projectRef.current;
    const current = activeRef.current;
    if (!currentProject || !current || !targets.some(target => target.documentId === current.record.head.documentId)) return;
    const identity = projectIdentity(currentProject);
    const sourceSession = current.session;
    const sourceAccess = current.session.projectAccess;
    await current.session.detachAfter(() => Promise.resolve(), 'switch');
    if (!sameWorkspaceIdentity(projectRef.current, identity) || activeRef.current?.session !== sourceSession) {
      throw new Error('The project changed while the adoption was being prepared. Prepare the preview again.');
    }
    chatAdoptionTarget.current = {
      projectId: sourceAccess.projectId,
      documentId: current.record.head.documentId,
      access: sourceAccess,
      // detachAfter has flushed the current editor. Keep that exact flushed
      // body/head locally so failure recovery never depends on a second IPC
      // read or risks replacing the author's text with an older project row.
      record: { ...current.record, head: sourceSession.state.head, body: sourceSession.body },
      viewState: current.viewState,
    };
    setActive(null);
  }

  async function acceptChatDocuments(documents: DocumentRecord[]): Promise<void> {
    const currentProject = projectRef.current;
    if (!currentProject) return;
    const identity = projectIdentity(currentProject);
    const previous = chatAdoptionTarget.current;
    if (previous && !sameAccessIdentity(previous.access, identity)) return;
    // Replayed adoption receipts deliberately contain historical after
    // revisions. Read the current Working heads before showing the result so
    // a lost acknowledgment cannot replace later author edits in the editor.
    const sourceSession = previous ? null : activeRef.current?.session ?? null;
    const access = previous?.access ?? activeRef.current?.session.projectAccess ?? currentProject.access;
    const refreshed = await Promise.all(documents.map(document => readDocument(access, document.head.documentId)));
    if (!sameWorkspaceIdentity(projectRef.current, identity)) return;
    // A prepared adoption intentionally detached the source session. For an
    // ordinary document update, however, a session replacement means the
    // acknowledgment belongs to an older editor and must be ignored.
    if (!previous && sourceSession && activeRef.current?.session !== sourceSession) return;
    if (previous && chatAdoptionTarget.current !== previous) return;
    const latestProject = projectRef.current;
    if (!latestProject) return;
    const replacement = previous && refreshed.find(document => document.head.documentId === previous.documentId);
    if (previous && replacement) {
      // The review action flushed this editor before Rust accepted the exact
      // source. Display only the document returned by that committed receipt.
      setActive({ record: replacement, session: new DocumentSession(previous.access, replacement, projectTransport), viewState: previous.viewState });
    }
    if (chatAdoptionTarget.current === previous) chatAdoptionTarget.current = null;
    setProject(base => {
      if (!base || !sameWorkspaceIdentity(base, identity)) return base;
      const merged = new Map(base.documents.map(document => [document.head.documentId, document]));
      for (const document of refreshed) merged.set(document.head.documentId, document);
      return { ...base, documents: [...merged.values()] };
    });
  }

  /*
   * ProjectConversation calls this after a failed or unresolved adoption.
   * The source session was deliberately disposed to fence edits while Rust
   * validated the exact preview, so restoring a fresh session from the
   * flushed source keeps the editor visible and usable for an explicit retry.
   */
  async function restoreChatAdoption(): Promise<void> {
    const previous = chatAdoptionTarget.current;
    const currentProject = projectRef.current;
    if (!previous || !currentProject || !sameAccessIdentity(previous.access, projectIdentity(currentProject))) return;
    const identity = projectIdentity(currentProject);
    if (!sameWorkspaceIdentity(projectRef.current, identity) || chatAdoptionTarget.current !== previous) return;
    // An adoption failure is not evidence that Rust did not commit. Reopen a
    // fenced session from the flushed source and run the normal empty-pending
    // reconciliation query. A committed adoption becomes a visible conflict;
    // an uncommitted one rotates the lease and returns the editor to editing.
    const session = new DocumentSession(previous.access, previous.record, projectTransport);
    setActive({ record: previous.record, session, viewState: previous.viewState });
    await session.reconcile();
    if (!sameWorkspaceIdentity(projectRef.current, identity) || chatAdoptionTarget.current !== previous) return;
    const access = session.projectAccess;
    setProject(base => base && sameWorkspaceIdentity(base, identity) ? { ...base, access } : base);
    chatAdoptionTarget.current = null;
  }

  /** Create an ordinary empty note through the same idempotent, local-only
   * document intent used by the writing workspace. This action never asks a
   * provider for content; the author can edit the note or attach it later. */
  async function createChatNote(): Promise<void> {
    try {
      await perform(async () => {
        const currentProject = projectRef.current;
        if (!currentProject) throw new Error('Open a project before creating a note.');
        await flushWorkshop();
        let projectBase = currentProject;
        let settledAccess: ProjectAccess | null = null;
        if (documentIntent.current) {
          const settled = await settlePendingDocumentIntent();
          projectBase = settled?.opened ?? projectBase;
          settledAccess = settled?.access ?? null;
        }
        const current = activeRef.current;
        if (current && current.session.state.phase === 'reconciling') await current.session.reconcile();
        if (current && ['editing', 'saveFailed'].includes(current.session.state.phase)) await current.session.flush();
        const intent = documentIntent.current ?? {
          operationId: crypto.randomUUID(),
          documentId: crypto.randomUUID(),
          title: 'Untitled note',
          kind: 'note',
          body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: crypto.randomUUID() } }] } },
        } satisfies CreateDocumentIntent;
        documentIntent.current = intent;
        let result;
        try {
          result = await runCreateIntent({
            projectId: projectBase.project.projectId,
            session: rendererId,
            access: current?.session.projectAccess ?? settledAccess ?? projectBase.access,
            intent,
            transport: { createDocument, reconcileProject },
            onReconciled: adoptCreateSnapshot,
          });
        } catch (reason) {
          if (!(reason instanceof CreateIntentRecoveryError) && !(reason instanceof CreateIntentUnresolvedError)) documentIntent.current = null;
          throw reason;
        }
        documentIntent.current = null;
        const base = result.snapshot ?? projectBase;
        const opened: OpenedProject = {
          ...base,
          access: result.access,
          documents: base.documents.some(document => document.head.documentId === result.record.head.documentId)
            ? base.documents.map(document => document.head.documentId === result.record.head.documentId ? result.record : document)
            : [...base.documents, result.record],
        };
        await navigate(() => Promise.resolve());
        activate(opened, result.record);
      });
    } catch (reason) { setError(errorText(reason)); }
  }

  async function createChatChapter(): Promise<void> {
    try { await prepareChatChapter(null, `Chapter ${(project?.documents.filter(document => document.kind === 'chapter').length ?? 0) + 1}`); }
    catch (reason) { setError(errorText(reason)); }
  }

  async function prepareChatChapter(targetId: string | null, proposedTitle: string): Promise<DocumentRecord> {
    if (!project) throw new Error('Open a project before preparing a chapter.');
    let prepared: DocumentRecord | undefined;
    await perform(async () => {
      await flushWorkshop();
      if (targetId) {
        const requested = project.documents.find(document => document.head.documentId === targetId && document.kind === 'chapter' && (document.role ?? 'ordinary') === 'ordinary');
        if (!requested) throw new Error('The selected chapter is no longer available.');
        const current = activeRef.current;
        if (current?.record.head.documentId === targetId) {
          await current.session.flush();
          prepared = { ...current.record, head: current.session.state.head };
        } else {
          const access = current?.session.projectAccess ?? project.access;
          prepared = await navigate(() => readDocument(access, targetId));
          activate({ ...project, access }, prepared);
        }
        return;
      }
      if (documentIntent.current && (documentIntent.current.kind !== 'chapter' || documentIntent.current.title !== proposedTitle)) {
        throw new Error('Finish the pending document creation before preparing another chapter.');
      }
      if (!documentIntent.current) documentIntent.current = {
        operationId: crypto.randomUUID(), documentId: crypto.randomUUID(),
        title: proposedTitle, kind: 'chapter',
        body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: crypto.randomUUID() } }] } },
      };
      const result = await settlePendingDocumentIntent();
      if (!result) return;
      await navigate(() => Promise.resolve());
      activate(result.opened, result.record);
      prepared = result.record;
    });
    if (!prepared) throw new Error('The chapter was not prepared. Resolve the project operation before trying again.');
    return prepared;
  }

  async function prepareChatSource(document: DocumentRecord) {
    const current = activeRef.current;
    if (current?.record.head.documentId === document.head.documentId) {
      await current.session.flush();
      return current.session.state.head;
    }
    return document.head;
  }

  return {
    chatAdoptionTarget,
    openChatChapterResult,
    prepareChatAdoption,
    acceptChatDocuments,
    restoreChatAdoption,
    createChatNote,
    createChatChapter,
    prepareChatChapter,
    prepareChatSource,
  };
}
