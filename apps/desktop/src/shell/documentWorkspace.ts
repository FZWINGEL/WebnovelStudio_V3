/** Owns the opened project, mounted editor, and their fenced transitions. */
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { DocumentSession, canonicalJson } from '../editor';
import { createDocument, reconcileProject, readDocument, projectTransport, projectMetadata, renameProject, renameDocument, type CreateDocumentIntent, type DocumentRecord, type OpenedProject, type ProjectAccess, type ViewState } from '../ipc/projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, runCreateIntent } from '../ipc/createIntent';
import { libraryOpen, projectBackup } from '../ipc/library';
import { prepareDraftExport, prepareReviewedDraftExport, exportPreparedDraft, type DraftExportPreview, type DraftFormat } from '../ipc/exports';
import type { DiscussionRun } from '../ipc/discussions';
import type { ChatAdoptionTarget } from '../ipc/projectChat';
import { PROJECT_TABS, documentsForTab, tabForKind, readProjectTabs, writeProjectTabs, type ProjectTabId } from './projectTabs';
import { readWorkspaceMode, writeWorkspaceMode, CHAT_FIRST_TRIAL_ENABLED, type WorkspaceMode } from './workspaceModes';
import { projectIdentity, sameWorkspaceIdentity, sameAccessIdentity, workspaceErrorText as errorText, type ProjectNavigation, type WorkspaceIdentity, type WorkspaceOperations, type WorkspacePresentation } from './workspaceContracts';

type ActiveDocument = Readonly<{ record: DocumentRecord; session: DocumentSession; viewState: ViewState | null }>;
const kinds = ['chapter', 'character', 'world', 'theme', 'hook', 'scene', 'note'] as const;
export interface DocumentWorkspaceDeps {
  rendererId: string;
  operations: WorkspaceOperations;
  participants: { flush(): Promise<void>; refreshConversation(): void };
  present(event: WorkspacePresentation): void;
}

export function useDocumentWorkspace(deps: DocumentWorkspaceDeps) {
  const depsRef = useRef(deps); depsRef.current = deps;
  const rendererId = deps.rendererId;
  const perform = (work: () => Promise<void>) => depsRef.current.operations.perform(work);
  const setNotice = (message: string) => depsRef.current.operations.notice(message);
  const setError = (message: string) => depsRef.current.operations.error(message);
  const present = (event: WorkspacePresentation) => depsRef.current.present(event);
  const focusAfterNavigation = (selector: string, projectId: string | null) => present({ kind: 'focus', selector, projectId });
  const [project, setProject] = useState<OpenedProject | null>(null);
  const [active, setActive] = useState<ActiveDocument | null>(null);
  const [exporting, setExporting] = useState<ActiveDocument | null>(null);
  const activeRef = useRef(active); activeRef.current = active;
  const projectRef = useRef(project); projectRef.current = project;
  const [newDocument, setNewDocument] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renamedTitle, setRenamedTitle] = useState('');
  const [renamingDocument, setRenamingDocument] = useState(false);
  const [renamedDocumentTitle, setRenamedDocumentTitle] = useState('');
  const [documentTitle, setDocumentTitle] = useState('');
  const [kind, setKind] = useState('chapter');
  const [projectTab, setProjectTab] = useState<ProjectTabId>('chapters');
  const [workspaceMode, setWorkspaceMode] = useState<WorkspaceMode | null>(null);
  const [chatReviewRun, setChatReviewRun] = useState<DiscussionRun | null>(null);
  const documentIntent = useRef<CreateDocumentIntent | null>(null);
  const chatAdoptionTarget = useRef<{
    projectId: string;
    documentId: string;
    access: ProjectAccess;
    record: DocumentRecord;
    viewState: ViewState | null;
  } | null>(null);
  async function acceptChatAccess(access: ProjectAccess): Promise<void> {
    const currentProject = projectRef.current;
    if (!currentProject || !sameAccessIdentity(access, projectIdentity(currentProject))) return;
    const identity = projectIdentity(currentProject);
    const current = activeRef.current;
    if (current && current.session.projectAccess.projectId === access.projectId) {
      await current.session.acceptProjectAccess(access);
    }
    if (!sameWorkspaceIdentity(projectRef.current, identity)) return;
    if (current && activeRef.current?.session !== current.session) return;
    setProject(value => value && sameWorkspaceIdentity(value, identity)
      ? { ...value, access } : value);
  }
  useEffect(() => {
    if (!active) return;
    let previous = active.session.projectAccess.writerLease;
    let previousHead = canonicalJson(active.session.state.head);
    let previousEditable = active.session.state.editable;
    return active.session.subscribe(() => {
      const access = active.session.projectAccess;
      const head = canonicalJson(active.session.state.head);
      const editable = active.session.state.editable;
      if (head !== previousHead || (!previousEditable && editable)) {
        previousHead = head;
        if (projectRef.current && sameAccessIdentity(access, projectIdentity(projectRef.current))) {
          // A successful ordinary save can invalidate a reply immediately,
          // including after it finished. Refresh counts/context locally;
          // this does not dispatch or replace the mounted editor. Refresh
          // again when a lifecycle guard settles so the return recap can
          // resolve a checkpoint written after the save acknowledgment.
          depsRef.current.participants.refreshConversation();
        }
      }
      previousEditable = editable;
      if (access.writerLease === previous) return;
      previous = access.writerLease;
      setProject(value => value && value.access.projectId === access.projectId && value.access.session === access.session
        ? { ...value, access } : value);
    });
  }, [active?.session]);

  async function flushWorkshop(): Promise<void> {
    await depsRef.current.participants.flush();
  }

  function activate(opened: OpenedProject, document?: DocumentRecord) {
    if (document) opened = { ...opened, documents: opened.documents.map(item => item.head.documentId === document.head.documentId ? document : item) };
    // A project activation fences callbacks from the previous conversation or
    // adoption. Their acknowledgments must never hydrate the newly mounted
    // project or attach an old chapter review to it.
    chatAdoptionTarget.current = null;
    setChatReviewRun(null);
    setExporting(null);
    setProject(opened);
    const preferences = readProjectTabs(opened.project.projectId);
    const savedMode = readWorkspaceMode(opened.project.projectId);
    const mode = (savedMode === 'chat' && !CHAT_FIRST_TRIAL_ENABLED ? null : savedMode) ?? (opened.documents.length ? 'write' : null);
    const previousDocument = opened.documents.find(item => item.head.documentId === opened.viewState?.documentId) ?? opened.documents[0];
    const tab = document ? tabForKind(document.kind) : preferences.activeTab ?? tabForKind(previousDocument?.kind ?? 'chapter');
    const eligible = documentsForTab(opened.documents, tab);
    const next = document ?? eligible.find(item => item.head.documentId === preferences.lastDocumentByTab[tab]) ?? eligible.find(item => item.head.documentId === opened.viewState?.documentId) ?? eligible[0];
    setWorkspaceMode(mode);
    setProjectTab(tab);
    writeProjectTabs(opened.project.projectId, { ...preferences, activeTab: tab, lastDocumentByTab: { ...preferences.lastDocumentByTab, ...(next ? { [tab]: next.head.documentId } : {}) } });
    setActive((mode === 'write' || mode === 'chat') && next ? { record: next, session: new DocumentSession(opened.access, next, projectTransport), viewState: opened.viewState } : null);
    setNewDocument(false); setRenaming(false); setRenamingDocument(false);
    present({ kind: 'activated' });
    if (opened.libraryWarning) setNotice(`Project opened. Library update needs attention: ${opened.libraryWarning}`);
  }

  function mergeWorkshopDocuments(projectId: string, documents: DocumentRecord[]): void {
    setProject(current => {
      if (!current || current.project.projectId !== projectId) return current;
      const mountedId = activeRef.current?.record.head.documentId;
      const merged = new Map(current.documents.map(document => [document.head.documentId, document]));
      for (const document of documents) {
        // A mounted writer owns its live buffer. Workshop adoption can add or
        // update other records, but it must never replace that editor's body.
        if (document.head.documentId !== mountedId) merged.set(document.head.documentId, document);
      }
      return { ...current, documents: [...merged.values()] };
    });
  }

  async function restoreWriteDocument(base: OpenedProject, preferredTab: ProjectTabId): Promise<void> {
    const preferences = readProjectTabs(base.project.projectId);
    const tab = preferredTab;
    const eligible = documentsForTab(base.documents, tab);
    const destination = eligible.find(item => item.head.documentId === preferences.lastDocumentByTab[tab]) ?? eligible[0];
    const record = destination ? await readDocument(base.access, destination.head.documentId) : null;
    const projectBase = record
      ? { ...base, documents: base.documents.map(item => item.head.documentId === record.head.documentId ? record : item) }
      : base;
    writeProjectTabs(base.project.projectId, { ...preferences, activeTab: tab, lastDocumentByTab: { ...preferences.lastDocumentByTab, ...(record ? { [tab]: record.head.documentId } : {}) } });
    setProject(projectBase);
    setProjectTab(tab);
    setActive(record ? { record, session: new DocumentSession(base.access, record, projectTransport), viewState: null } : null);
    setNewDocument(false); setRenamingDocument(false); setExporting(null); present({ kind: 'selectionChanged' });
  }

  async function transitionMode(next: WorkspaceMode): Promise<void> {
    if (!project || workspaceMode === next) return;
    if (next === 'chat' && !CHAT_FIRST_TRIAL_ENABLED) return;
    await flushWorkshop();
    const projectId = project.project.projectId;
    if (next === 'develop') {
      await settlePendingDocumentIntent();
      const current = activeRef.current;
      // The existing editor owns the write barrier. Only clear it after the
      // checkpoint and detach have completed, so a failed save leaves Write
      // mounted and editable.
      if (current) {
        const saved = await current.session.detachAfter(() => readDocument(current.session.projectAccess, current.record.head.documentId), 'switch');
        setProject(base => base?.project.projectId === projectId ? {
          ...base, access: current.session.projectAccess,
          documents: base.documents.map(document => document.head.documentId === saved.head.documentId ? saved : document),
        } : base);
      }
      setActive(null);
      writeWorkspaceMode(projectId, 'develop');
      setWorkspaceMode('develop');
      present({ kind: 'hideStoryBible' });
      return;
    }

    // Workshop has its own local working version and save boundary. Read the
    // destination only after that boundary succeeds, keeping Develop mounted
    // if either operation fails.
    await flushWorkshop();
    let base = project;
    const current = activeRef.current;
    if (current) {
      const record = await current.session.detachAfter(() => readDocument(current.session.projectAccess, current.record.head.documentId), 'switch');
      base = { ...base, access: current.session.projectAccess, documents: base.documents.map(document => document.head.documentId === record.head.documentId ? record : document) };
    }
    if (base.documents.length) await restoreWriteDocument(base, projectTab);
    else {
      setProject(base);
      setProjectTab(projectTab);
      setActive(null);
    }
    writeWorkspaceMode(projectId, next);
    setWorkspaceMode(next);
    present({ kind: 'hideStoryBible' });
  }

  function selectWorkspaceMode(next: WorkspaceMode): void {
    if (!project || workspaceMode === next) return;
    void perform(() => transitionMode(next));
  }

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

  function openDocumentFromWorkshop(projectId: string, documentId: string): void {
    void perform(async () => {
      if (!project || project.project.projectId !== projectId) return;
      const base = project;
      const access = activeRef.current?.session.projectAccess ?? base.access;
      const record = await navigate(() => readDocument(access, documentId));
      const opened = { ...base, access, documents: base.documents.map(item => item.head.documentId === documentId ? record : item) };
      writeWorkspaceMode(projectId, 'write');
      activate(opened, record);
    });
  }

  function openStoryBible(): void {
    if (!project) return;
    void perform(async () => {
      await settlePendingDocumentIntent();
      await activeRef.current?.session.flush();
      await flushWorkshop();
      const access = activeRef.current?.session.projectAccess;
      if (access) setProject(current => current ? { ...current, access } : current);
      present({ kind: 'showStoryBible' });
    });
  }

  /** Keep a reconciled lease attached to the mounted editor before using its snapshot. */
  async function adoptCreateSnapshot(snapshot: OpenedProject): Promise<ProjectAccess> {
    const current = activeRef.current;
    if (!current) {
      setProject(snapshot);
      return snapshot.access;
    }
    await current.session.reconcile();
    const access = current.session.projectAccess;
    const currentRecord = snapshot.documents.find(document => document.head.documentId === current.record.head.documentId) ?? current.record;
    setActive({ ...current, record: currentRecord, viewState: snapshot.viewState });
    setProject({ ...snapshot, access });
    return access;
  }

  /**
   * Resolve the pending logical create before navigation or another create.
   * The intent stays in the ref until this returns, so a lost renderer ACK
   * cannot turn a retry into a second document.
   */
  async function settlePendingDocumentIntent(): Promise<{ record: DocumentRecord; access: ProjectAccess; opened: OpenedProject } | null> {
    const intent = documentIntent.current;
    const currentProject = project;
    if (!intent || !currentProject) return null;
    const current = activeRef.current;
    if (current && current.session.state.phase === 'reconciling') await current.session.reconcile();
    if (current && ['editing', 'saveFailed'].includes(current.session.state.phase)) await current.session.flush();
    let result;
    try {
      result = await runCreateIntent({
        projectId: currentProject.project.projectId,
        session: rendererId,
        access: current?.session.projectAccess ?? currentProject.access,
        intent,
        transport: { createDocument, reconcileProject },
        onReconciled: adoptCreateSnapshot,
      });
    } catch (reason) {
      if (!(reason instanceof CreateIntentRecoveryError) && !(reason instanceof CreateIntentUnresolvedError)) documentIntent.current = null;
      throw reason;
    }
    documentIntent.current = null;
    const base = result.snapshot ?? currentProject;
    const opened = { ...base, access: result.access, documents: base.documents.some(document => document.head.documentId === result.record.head.documentId)
      ? base.documents.map(document => document.head.documentId === result.record.head.documentId ? result.record : document)
      : [...base.documents, result.record] };
    setProject(opened);
    return { record: result.record, access: result.access, opened };
  }
  async function navigate<T>(prepare: () => Promise<T>): Promise<T> {
    await flushWorkshop();
    await settlePendingDocumentIntent();
    return activeRef.current ? activeRef.current.session.detachAfter(prepare) : prepare();
  }
  function rename(event: FormEvent, reopenPath: string | null) {
    event.preventDefault(); if (!project) return;
    void perform(async () => {
      const settled = await settlePendingDocumentIntent();
      const projectBase = settled?.opened ?? project;
      const write = () => renameProject(activeRef.current?.session.projectAccess ?? settled?.access ?? projectBase.access, projectBase.metadataVersion, renamedTitle.trim());
      try {
        const metadata = activeRef.current ? await activeRef.current.session.projectWrite(write) : await write();
        setProject({ ...projectBase, project: metadata.project, metadataVersion: metadata.metadataVersion }); setRenaming(false);
        if (metadata.libraryWarning) setNotice(`Title saved. ${metadata.libraryWarning}`);
      } catch (error) {
        const metadata = await projectMetadata(project.project.projectId);
        setProject({ ...projectBase, project: metadata.project, metadataVersion: metadata.metadataVersion });
        if (!activeRef.current) {
          const refreshed = await libraryOpen(reopenPath, rendererId);
          if (refreshed) setProject(refreshed);
        }
        throw error;
      }
    });
  }
  function selectDocument(document: DocumentRecord) {
    if (!project || document.head.documentId === active?.record.head.documentId) return;
    void perform(async () => {
      const settled = await settlePendingDocumentIntent();
      const projectBase = settled?.opened ?? project;
      const access = activeRef.current?.session.projectAccess ?? projectBase.access;
      const record = await navigate(() => readDocument(access, document.head.documentId));
      activate({ ...projectBase, access }, record);
    });
  }
  function selectTab(tab: ProjectTabId, openWriting = false) {
    if (!project || (tab === projectTab && (!openWriting || workspaceMode === 'write'))) return;
    void perform(async () => {
      const settled = await settlePendingDocumentIntent();
      const projectBase = settled?.opened ?? project;
      const access = activeRef.current?.session.projectAccess ?? projectBase.access;
      const preferences = readProjectTabs(project.project.projectId);
      const eligible = documentsForTab(projectBase.documents, tab);
      const destination = eligible.find(item => item.head.documentId === preferences.lastDocumentByTab[tab]) ?? eligible[0];
      const record = await navigate(() => destination ? readDocument(access, destination.head.documentId) : Promise.resolve(null));
      const updated = { ...preferences, activeTab: tab, lastDocumentByTab: { ...preferences.lastDocumentByTab, ...(record ? { [tab]: record.head.documentId } : {}) } };
      writeProjectTabs(project.project.projectId, updated);
      setProject({ ...projectBase, access, documents: record ? projectBase.documents.map(item => item.head.documentId === record.head.documentId ? record : item) : projectBase.documents });
      setProjectTab(tab);
      if (openWriting) {
        writeWorkspaceMode(project.project.projectId, 'write');
        setWorkspaceMode('write');
        focusAfterNavigation(`#project-tab-${tab}`, project.project.projectId);
      }
      setActive(record ? { record, session: new DocumentSession(access, record, projectTransport), viewState: null } : null);
      setNewDocument(false); setRenamingDocument(false); setExporting(null); present({ kind: 'selectionChanged' });
    });
  }
  function beginDocument(documentKind = PROJECT_TABS.find(tab => tab.id === projectTab)!.initialKind) {
    setKind(documentKind); setDocumentTitle(''); setNewDocument(true);
  }
  function renameCurrentDocument(event: FormEvent) {
    event.preventDefault(); const current = activeRef.current; if (!project || !current) return;
    void perform(async () => {
      const settled = await settlePendingDocumentIntent();
      const projectBase = settled?.opened ?? project;
      const updateMetadata = (record: DocumentRecord) => {
        setActive({ ...current, record });
        setProject({ ...projectBase, documents: projectBase.documents.map(item => item.head.documentId === record.head.documentId ? record : item) });
      };
      try {
        const record = await current.session.projectWrite(() => renameDocument(current.session.projectAccess, current.record.head.documentId, current.record.metadataVersion, renamedDocumentTitle.trim()));
        updateMetadata(record); setRenamingDocument(false);
      } catch (error) {
        // Reconcile before reading metadata when the commit acknowledgment is
        // unknown. The live editor remains mounted and retains its own body.
        if (current.session.state.phase === 'reconciling') await current.session.reconcile();
        updateMetadata(await readDocument(current.session.projectAccess, current.record.head.documentId));
        throw error;
      }
    });
  }
  function addDocument(event: FormEvent) {
    event.preventDefault(); if (!project) return;
    void perform(async () => {
      const name = documentTitle.trim() || `Untitled ${kind}`;
      const pending = documentIntent.current;
      let settledAccess: ProjectAccess | null = null;
      let projectBase = project;
      if (pending && (pending.title !== name || pending.kind !== kind)) {
        // A changed form is a new logical intent only after the previous
        // uncertain operation has been reconciled and settled.
        const settled = await settlePendingDocumentIntent();
        settledAccess = settled?.access ?? null;
        projectBase = settled?.opened ?? projectBase;
      }
      const intent = documentIntent.current ?? {
        operationId: crypto.randomUUID(),
        documentId: crypto.randomUUID(),
        title: name,
        kind,
        body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: crypto.randomUUID() } }] } },
      } satisfies CreateDocumentIntent;
      documentIntent.current = intent;
      const current = activeRef.current;
      if (current && current.session.state.phase === 'reconciling') await current.session.reconcile();
      if (current && ['editing', 'saveFailed'].includes(current.session.state.phase)) await current.session.flush();
      let result;
      try {
        result = await runCreateIntent({
          projectId: project.project.projectId,
          session: rendererId,
          access: current?.session.projectAccess ?? settledAccess ?? project.access,
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
      const opened: OpenedProject = { ...base, access: result.access, documents: base.documents.some(document => document.head.documentId === result.record.head.documentId)
        ? base.documents.map(document => document.head.documentId === result.record.head.documentId ? result.record : document)
        : [...base.documents, result.record] };
      setDocumentTitle('');
      await navigate(() => Promise.resolve());
      activate(opened, result.record);
    });
  }
  async function backup() {
    if (!project) return;
    await flushWorkshop();
    const settled = await settlePendingDocumentIntent();
    const work = async () => {
      await activeRef.current?.session.flush();
      const result = await projectBackup(activeRef.current?.session.projectAccess ?? settled?.access ?? project.access);
      if (result) setNotice(`Backup saved: ${result}`);
    };
    if (activeRef.current) await activeRef.current.session.withLifecycleGuard(work); else await work();
  }
  async function exportDraft() {
    await settlePendingDocumentIntent();
    const current = activeRef.current; if (!current) return;
    setExporting(current);
  }
  async function prepareExport(current: ActiveDocument, format: DraftFormat, basis: 'working' | 'reviewed'): Promise<DraftExportPreview> {
    return current.session.withLifecycleGuard(async () => {
      if (activeRef.current?.session !== current.session) throw new Error('Open this document again to export it.');
      await current.session.flush();
      const head = current.session.state.head;
      const preview = basis === 'reviewed'
        ? await prepareReviewedDraftExport(current.session.projectAccess, head, format)
        : await prepareDraftExport(current.session.projectAccess, head, format);
      if (canonicalJson(preview.sourceHead) !== canonicalJson(head)) throw new Error('The export preview does not match the saved writing. Prepare it again.');
      if (basis === 'reviewed' && (!preview.reviewBundleId || !preview.reviewBundleId.trim())) {
        throw new Error('The author-reviewed preview did not include its review bundle. Prepare it again.');
      }
      if (basis === 'working' && preview.reviewBundleId !== undefined) {
        throw new Error('The working-draft preview unexpectedly included review authority. Prepare it again.');
      }
      return preview;
    });
  }
  async function writeExport(current: ActiveDocument, preview: DraftExportPreview): Promise<string | null> {
    if (activeRef.current?.session !== current.session) throw new Error('Finish the current operation before exporting.');
    return depsRef.current.operations.exclusive('Finish the current operation before exporting.', async () => {
      const result = await exportPreparedDraft(current.session.projectAccess, preview);
      if (!result) return null;
      if (result.previewId !== preview.id || result.sha256 !== preview.sha256 || result.utf8Bytes !== preview.utf8Bytes || !result.path) {
        throw new Error('Could not verify the exported file. Check the chosen destination before trying again.');
      }
      if (activeRef.current?.session === current.session) setNotice(`${preview.reviewBundleId ? 'Author-reviewed snapshot exported' : 'Draft exported'}: ${result.path}`);
      return result.path;
    });
  }


  function currentProject() {
    const current = projectRef.current;
    return current ? { projectId: current.project.projectId, title: current.project.title, access: activeRef.current?.session.projectAccess ?? current.access } : null;
  }
  function ownsNavigation(identity: WorkspaceIdentity | null, session: DocumentSession | null) {
    return (identity ? sameWorkspaceIdentity(projectRef.current, identity) : projectRef.current === null)
      // No mounted session is also an identity: an adoption acknowledgment
      // may restore an editor while destination preparation is still pending.
      && (activeRef.current?.session ?? null) === session;
  }
  const navigation: ProjectNavigation = {
    currentProject,
    replaceProject: async prepare => {
      const identity = projectRef.current ? projectIdentity(projectRef.current) : null;
      const session = activeRef.current?.session ?? null;
      const opened = await navigate(() => prepare(currentProject()));
      if (!ownsNavigation(identity, session)) throw new Error('The project changed while the destination was opening. Open it again.');
      activate(opened);
      return opened;
    },
    returnToLibrary: async prepare => {
      const identity = projectRef.current ? projectIdentity(projectRef.current) : null;
      const session = activeRef.current?.session ?? null;
      await navigate(prepare);
      if (!ownsNavigation(identity, session)) return;
      setActive(null); setProject(null); present({ kind: 'library' });
    },
    acceptImported: opened => {
      // The import dialog belongs to the library. A callback from a dialog
      // that has since unmounted cannot replace a newly opened project.
      if (projectRef.current) return;
      activate(opened);
    },
  };

  return {
    view: { project, active, exporting, projectTab, workspaceMode, chatReviewRun, kinds,
      newDocument, renaming, renamedTitle, renamingDocument, renamedDocumentTitle, documentTitle, kind } as const,
    actions: {
      setNewDocument, setRenaming, setRenamedTitle, setRenamingDocument, setRenamedDocumentTitle, setDocumentTitle, setKind,
      closeExport: () => setExporting(null),
      acceptChatAccess, mergeWorkshopDocuments, selectWorkspaceMode, openChatChapterResult,
      prepareChatAdoption, acceptChatDocuments, restoreChatAdoption, createChatNote, createChatChapter,
      prepareChatChapter, prepareChatSource, openDocumentFromWorkshop, openStoryBible, rename,
      selectDocument, selectTab, beginDocument, renameCurrentDocument, addDocument,
      backup: () => { void perform(backup); }, exportDraft: () => { void perform(exportDraft); }, prepareExport, writeExport,
    },
    navigation,
    currentSession: () => activeRef.current?.session ?? null,
    isCurrent: (identity: WorkspaceIdentity | null) => identity ? sameWorkspaceIdentity(projectRef.current, identity) : projectRef.current === null,
  };
}
