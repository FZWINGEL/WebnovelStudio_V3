//! The workspace's state and behaviour.
//!
//! Split out of `Workspace.tsx`, which was 1,206 lines and held this plus the
//! JSX. The component is 236 lines of layout now and binds what it renders.
//!
//! This is a *model*, not a helper: it owns `project`, `active`, the dialogs,
//! the tabs, the busy/notice/error envelope, and the document-session
//! operations over them. That ownership is why it is one hook rather than the
//! several smaller ones it looks like it should be.
//!
//! An earlier attempt cut the document-session functions out as their own hook
//! and gave up: `activate` alone resets the search box, the create, rename and
//! import dialogs, the project tab, the notice and the workspace mode, and its
//! siblings between them reach twenty more of these cells. A hook with a
//! twenty-five-argument interface that holds the workspace's state and hands it
//! back is not a seam — it is the same component with the parameters spelled
//! out. The boundary only exists once the state moves with the behaviour, which
//! is what this file is.
//!
//! The identity checks below are the fencing. A callback that settles after the
//! author switched project, replaced the session, or started a second adoption
//! must not hydrate the new editor; every operation re-checks
//! `sameWorkspaceIdentity` or `sameAccessIdentity` after each await, and the
//! comments at those points name the acknowledgment each one guards against.

import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { DocumentSession } from '../editor/session';
import { canonicalJson } from '../editor/document';
import { createDocument, reconcileProject, readDocument, projectTransport, projectMetadata, renameProject, renameDocument, type CreateDocumentIntent, type DocumentRecord, type OpenedProject, type ProjectAccess, type ViewState } from '../ipc/projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, runCreateIntent } from '../ipc/createIntent';
import { librarySnapshot, libraryCreate, libraryOpen, libraryArchive, libraryRecover, libraryDuplicate, libraryResumeImport, projectBackup, type LibrarySnapshot } from '../ipc/library';
import { prepareDraftExport, prepareReviewedDraftExport, exportPreparedDraft, type DraftExportPreview, type DraftFormat } from '../ipc/exports';
import { Writer } from '../editor/Writer';
import type { DiscussionRun } from '../ipc/discussions';
import { ExportDialog, type ExportBasis } from './ExportDialog';
import { V2ImportDialog } from './V2ImportDialog';
import { runtimeInfo } from '../ipc/native';
import { ModelSelector } from '../providers/ModelSelector';
import { ModelSettings } from '../providers/ModelSettings';
import { PROJECT_TABS, documentsForTab, tabForKind, readProjectTabs, writeProjectTabs, type ProjectTabId } from './projectTabs';
import { readWorkspaceMode, writeWorkspaceMode, CHAT_FIRST_TRIAL_ENABLED, type WorkspaceMode } from './workspaceModes';
import { RecentProjectPicker, type RecentProjectPickerItem } from './RecentProjectPicker';
import { CoauthorSidebar } from './CoauthorSidebar';
import { ProjectConversation, type ProjectConversationHandle } from '../chat/ProjectConversation';
import { ConversationHistoryPanel } from '../chat/ConversationHistoryPanel';
import type { ChatAdoptionTarget } from '../ipc/projectChat';
import { readProjectActivity, type ProjectActivitySnapshot } from '../ipc/projectActivity';
import { Workshop, type WorkshopHandle } from './Workshop';
import { StoryBible } from './StoryBible';
import { AppCloseDialog } from './AppCloseDialog';
import { useAppClose } from './useAppClose';
import { appCloseStatus, beginAppClose, cancelAppClose, finishAppClose, stopAppJobs, type AppCloseStatus } from '../ipc/appClose';
import './workspaceModes.css';
import './CoauthorWorkspace.css';

export const EditorTrial = typeof __WNS_EDITOR_TRIAL__ !== 'undefined' && __WNS_EDITOR_TRIAL__
  ? lazy(() => import('./App').then(module => ({ default: module.App })))
  : null;

type ActiveDocument = { record: DocumentRecord; session: DocumentSession; viewState: ViewState | null };
type WorkspaceIdentity = Pick<ProjectAccess, 'projectId' | 'operationNamespace' | 'session'>;
const emptyLibrary: LibrarySnapshot = { entries: [], pending: [] };
export const kinds = ['chapter', 'character', 'world', 'theme', 'hook', 'scene', 'note'];
function errorText(error: unknown): string {
  if (error && typeof error === 'object' && 'detail' in error) return String(error.detail);
  return error instanceof Error ? error.message : String(error);
}

function projectIdentity(project: OpenedProject): WorkspaceIdentity {
  return {
    projectId: project.project.projectId,
    operationNamespace: project.access.operationNamespace,
    session: project.access.session,
  };
}

function sameWorkspaceIdentity(project: OpenedProject | null, identity: WorkspaceIdentity): boolean {
  return !!project
    && project.project.projectId === identity.projectId
    && project.access.operationNamespace === identity.operationNamespace
    && project.access.session === identity.session;
}

function sameAccessIdentity(access: ProjectAccess, identity: WorkspaceIdentity): boolean {
  return access.projectId === identity.projectId
    && access.operationNamespace === identity.operationNamespace
    && access.session === identity.session;
}

export function useWorkspaceModel() {
  const [library, setLibrary] = useState(emptyLibrary);
  const [project, setProject] = useState<OpenedProject | null>(null);
  const [active, setActive] = useState<ActiveDocument | null>(null);
  const [exporting, setExporting] = useState<ActiveDocument | null>(null);
  const exportButton = useRef<HTMLButtonElement>(null);
  const activeRef = useRef(active); activeRef.current = active;
  const projectRef = useRef(project); projectRef.current = project;
  const [search, setSearch] = useState('');
  const [archived, setArchived] = useState(false);
  const [newProject, setNewProject] = useState(false);
  const [importingV2, setImportingV2] = useState(false);
  const [title, setTitle] = useState('');
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
  const [chatHistoryOpen, setChatHistoryOpen] = useState(false);
  const [storyBibleOpen, setStoryBibleOpen] = useState(false);
  const storyBibleButton = useRef<HTMLButtonElement>(null);
  const [trial, setTrial] = useState(false);
  const [trialAvailable, setTrialAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
  const [projectActivity, setProjectActivity] = useState<Record<string, ProjectActivitySnapshot>>({});
  const running = useRef(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const renderer = useRef(crypto.randomUUID());
  const creation = useRef({ id: crypto.randomUUID(), title: '' });
  const documentIntent = useRef<CreateDocumentIntent | null>(null);
  const recovery = useRef(crypto.randomUUID());
  const duplication = useRef({ id: crypto.randomUUID(), source: '' });
  const workshopRef = useRef<WorkshopHandle>(null);
  const projectChatRef = useRef<ProjectConversationHandle>(null);
  const chatAdoptionTarget = useRef<{
    projectId: string;
    documentId: string;
    access: ProjectAccess;
    record: DocumentRecord;
    viewState: ViewState | null;
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const appClose = useAppClose({
    isRunning: () => running.current,
    notice: setNotice,
    run: work => perform(work),
    save: () => flushWorkshop(),
    session: () => activeRef.current?.session ?? null,
  });

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
          void projectChatRef.current?.refresh?.().catch(() => {});
        }
      }
      previousEditable = editable;
      if (access.writerLease === previous) return;
      previous = access.writerLease;
      setProject(value => value && value.access.projectId === access.projectId && value.access.session === access.session
        ? { ...value, access } : value);
    });
  }, [active?.session]);

  async function refreshLibrary() { setLibrary(await librarySnapshot()); }
  function focusAfterNavigation(selector: string, projectId: string | null) {
    requestAnimationFrame(() => {
      if ((projectRef.current?.project.projectId ?? null) === projectId) document.querySelector<HTMLElement>(selector)?.focus();
    });
  }
  useEffect(() => { void refreshLibrary().catch(reason => setError(errorText(reason))).finally(() => setLoading(false)); }, []);
  useEffect(() => {
    let disposed = false;
    if (isTauri()) void runtimeInfo().then(info => { if (!disposed) setTrialAvailable(info.editorTrial === true); }).catch(() => {});
    return () => { disposed = true; };
  }, []);
  useEffect(() => {
    setProjectActivity({});
    if (!projectPickerOpen && workspaceMode !== 'chat') return;
    let disposed = false;
    let inFlight = false;
    const identity = project ? projectIdentity(project) : null;
    const stillCurrent = () => identity ? sameWorkspaceIdentity(projectRef.current, identity) : projectRef.current === null;
    const refresh = async () => {
      if (disposed || inFlight) return;
      inFlight = true;
      try {
        const snapshots = await readProjectActivity();
        if (disposed || !stillCurrent()) return;
        const knownProjectIds = new Set(library.entries.map(entry => entry.projectId));
        const next: Record<string, ProjectActivitySnapshot> = {};
        for (const snapshot of snapshots) {
          if (!knownProjectIds.has(snapshot.projectId)
            && !(snapshot.projectId === identity?.projectId && snapshot.operationNamespace === identity.operationNamespace)) continue;
          if (identity && snapshot.projectId === identity.projectId && snapshot.operationNamespace !== identity.operationNamespace) continue;
          next[snapshot.projectId] = snapshot;
        }
        setProjectActivity(next);
      } catch {
        if (!disposed && stillCurrent()) setProjectActivity({});
      } finally {
        inFlight = false;
      }
    };
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 2000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [projectPickerOpen, workspaceMode, project?.project.projectId, project?.access.operationNamespace, project?.access.session, library.entries]);

  async function perform(work: () => Promise<void>) {
    if (running.current) return;
    running.current = true; setBusy(true); setError(''); setNotice('');
    try { await work(); }
    catch (reason) { setError(errorText(reason)); }
    finally { running.current = false; setBusy(false); }
  }

  async function flushWorkshop(): Promise<void> {
    await projectChatRef.current?.flush();
    await workshopRef.current?.flush();
  }

  function activate(opened: OpenedProject, document?: DocumentRecord) {
    if (document) opened = { ...opened, documents: opened.documents.map(item => item.head.documentId === document.head.documentId ? document : item) };
    // A project activation fences callbacks from the previous conversation or
    // adoption. Their acknowledgments must never hydrate the newly mounted
    // project or attach an old chapter review to it.
    chatAdoptionTarget.current = null;
    setChatReviewRun(null);
    setExporting(null);
    setStoryBibleOpen(false);
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
    setSearch(''); setNewProject(false); setNewDocument(false); setRenaming(false); setRenamingDocument(false);
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
    setSearch(''); setNewDocument(false); setRenamingDocument(false); setExporting(null);
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
      setStoryBibleOpen(false);
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
    setStoryBibleOpen(false);
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
            session: renderer.current,
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
      setStoryBibleOpen(true);
    });
  }

  function closeStoryBible(): void {
    setStoryBibleOpen(false);
    requestAnimationFrame(() => storyBibleButton.current?.focus());
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
        session: renderer.current,
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
  function open(path: string | null) {
    void perform(async () => {
      // Cancelling a native picker keeps this editor attached and editable.
      const opened = await navigate(async () => {
        const result = await libraryOpen(path, renderer.current);
        if (!result) throw new Error('Open cancelled. Your current writing is still here.');
        return result;
      });
      activate(opened); await refreshLibrary();
      focusAfterNavigation('.app-header .brand strong', opened.project.projectId);
    });
  }
  function create(title: string, operationId?: string) {
    void perform(async () => {
      const name = title.trim() || 'Untitled project';
      if (creation.current.title !== name) creation.current = { id: crypto.randomUUID(), title: name };
      const opened = await navigate(() => libraryCreate(operationId ?? creation.current.id, name, renderer.current));
      activate(opened); await refreshLibrary(); setTitle(''); creation.current = { id: crypto.randomUUID(), title: '' };
      focusAfterNavigation('.app-header .brand strong', opened.project.projectId);
    });
  }
  function backToLibrary() {
    void perform(async () => { await navigate(refreshLibrary); setActive(null); setProject(null); setSearch(''); focusAfterNavigation('.library-heading h1', null); });
  }
  function recover(operationId?: string, title = 'Recovered project') {
    void perform(async () => {
      const opened = await navigate(async () => {
        const result = await libraryRecover(operationId ?? recovery.current, title, renderer.current);
        if (!result) throw new Error('Recovery cancelled. No project was replaced.');
        return result;
      });
      recovery.current = crypto.randomUUID(); activate(opened); await refreshLibrary();
    });
  }
  function duplicate() {
    if (!project) return;
    void perform(async () => {
      if (duplication.current.source !== project.project.projectId) duplication.current = { id: crypto.randomUUID(), source: project.project.projectId };
      const opened = await navigate(() => libraryDuplicate(duplication.current.id, activeRef.current?.session.projectAccess ?? project.access, `${project.project.title} copy`, renderer.current));
      duplication.current = { id: crypto.randomUUID(), source: '' }; activate(opened); await refreshLibrary();
    });
  }
  function resumeDuplicate(operationId: string, title: string) {
    void perform(async () => {
      const opened = await navigate(() => libraryDuplicate(operationId, null, title, renderer.current));
      activate(opened); await refreshLibrary();
    });
  }
  function resumeImport(operationId: string) {
    void perform(async () => {
      const opened = await navigate(() => libraryResumeImport(operationId, renderer.current));
      activate(opened); await refreshLibrary();
    });
  }
  function rename(event: React.FormEvent) {
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
          const refreshed = await libraryOpen(library.entries.find(entry => entry.projectId === project.project.projectId)?.path ?? null, renderer.current);
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
      setSearch(''); setNewDocument(false); setRenamingDocument(false); setExporting(null);
    });
  }
  function beginDocument(documentKind = PROJECT_TABS.find(tab => tab.id === projectTab)!.initialKind) {
    setKind(documentKind); setDocumentTitle(''); setNewDocument(true);
  }
  function renameCurrentDocument(event: React.FormEvent) {
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
  function addDocument(event: React.FormEvent) {
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
          session: renderer.current,
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
  async function prepareExport(current: ActiveDocument, format: DraftFormat, basis: ExportBasis): Promise<DraftExportPreview> {
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
    if (running.current || activeRef.current?.session !== current.session) throw new Error('Finish the current operation before exporting.');
    running.current = true; setBusy(true);
    try {
      const result = await exportPreparedDraft(current.session.projectAccess, preview);
      if (!result) return null;
      if (result.previewId !== preview.id || result.sha256 !== preview.sha256 || result.utf8Bytes !== preview.utf8Bytes || !result.path) {
        throw new Error('Could not verify the exported file. Check the chosen destination before trying again.');
      }
      if (activeRef.current?.session === current.session) setNotice(`${preview.reviewBundleId ? 'Author-reviewed snapshot exported' : 'Draft exported'}: ${result.path}`);
      return result.path;
    } finally { running.current = false; setBusy(false); }
  }

  return {
    library,
    setLibrary,
    project,
    setProject,
    active,
    setActive,
    exporting,
    setExporting,
    exportButton,
    activeRef,
    projectRef,
    search,
    setSearch,
    archived,
    setArchived,
    newProject,
    setNewProject,
    importingV2,
    setImportingV2,
    title,
    setTitle,
    newDocument,
    setNewDocument,
    renaming,
    setRenaming,
    renamedTitle,
    setRenamedTitle,
    renamingDocument,
    setRenamingDocument,
    renamedDocumentTitle,
    setRenamedDocumentTitle,
    documentTitle,
    setDocumentTitle,
    kind,
    setKind,
    projectTab,
    setProjectTab,
    workspaceMode,
    setWorkspaceMode,
    chatReviewRun,
    setChatReviewRun,
    chatHistoryOpen,
    setChatHistoryOpen,
    storyBibleOpen,
    setStoryBibleOpen,
    storyBibleButton,
    trial,
    setTrial,
    trialAvailable,
    setTrialAvailable,
    busy,
    setBusy,
    projectPickerOpen,
    setProjectPickerOpen,
    projectActivity,
    setProjectActivity,
    running,
    notice,
    setNotice,
    error,
    setError,
    renderer,
    creation,
    documentIntent,
    recovery,
    duplication,
    workshopRef,
    projectChatRef,
    chatAdoptionTarget,
    loading,
    setLoading,
    appClose,
    acceptChatAccess,
    refreshLibrary,
    focusAfterNavigation,
    perform,
    flushWorkshop,
    activate,
    mergeWorkshopDocuments,
    restoreWriteDocument,
    transitionMode,
    selectWorkspaceMode,
    openChatChapterResult,
    prepareChatAdoption,
    acceptChatDocuments,
    restoreChatAdoption,
    createChatNote,
    createChatChapter,
    prepareChatChapter,
    prepareChatSource,
    openDocumentFromWorkshop,
    openStoryBible,
    closeStoryBible,
    adoptCreateSnapshot,
    settlePendingDocumentIntent,
    navigate,
    open,
    create,
    backToLibrary,
    recover,
    duplicate,
    resumeDuplicate,
    resumeImport,
    rename,
    selectDocument,
    selectTab,
    beginDocument,
    renameCurrentDocument,
    addDocument,
    backup,
    exportDraft,
    prepareExport,
    writeExport,
  };
}
