import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { DocumentSession } from '../editor/session';
import { canonicalJson } from '../editor/document';
import { createDocument, reconcileProject, readDocument, projectTransport, projectMetadata, renameProject, renameDocument, type CreateDocumentIntent, type DocumentRecord, type OpenedProject, type ProjectAccess, type ViewState } from '../ipc/projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, runCreateIntent } from '../ipc/createIntent';
import { librarySnapshot, libraryCreate, libraryOpen, libraryArchive, libraryRecover, libraryDuplicate, libraryResumeImport, projectBackup, type LibrarySnapshot } from '../ipc/library';
import { prepareDraftExport, prepareReviewedDraftExport, exportPreparedDraft, type DraftExportPreview, type DraftFormat } from '../ipc/exports';
import { Writer } from './Writer';
import { ExportDialog, type ExportBasis } from './ExportDialog';
import { V2ImportDialog } from './V2ImportDialog';
import { runtimeInfo } from '../ipc/native';
import { ModelSelector } from '../providers/ModelSelector';
import { ModelSettings } from '../providers/ModelSettings';
import { PROJECT_TABS, documentsForTab, tabForKind, readProjectTabs, writeProjectTabs, type ProjectTabId } from './projectTabs';

const EditorTrial = typeof __WNS_EDITOR_TRIAL__ !== 'undefined' && __WNS_EDITOR_TRIAL__
  ? lazy(() => import('./App').then(module => ({ default: module.App })))
  : null;

type ActiveDocument = { record: DocumentRecord; session: DocumentSession; viewState: ViewState | null };
const emptyLibrary: LibrarySnapshot = { entries: [], pending: [] };
const kinds = ['chapter', 'character', 'world', 'theme', 'hook', 'scene', 'note'];
function errorText(error: unknown): string {
  if (error && typeof error === 'object' && 'detail' in error) return String(error.detail);
  return error instanceof Error ? error.message : String(error);
}

export function Workspace() {
  const [library, setLibrary] = useState(emptyLibrary);
  const [project, setProject] = useState<OpenedProject | null>(null);
  const [active, setActive] = useState<ActiveDocument | null>(null);
  const [exporting, setExporting] = useState<ActiveDocument | null>(null);
  const exportButton = useRef<HTMLButtonElement>(null);
  const activeRef = useRef(active); activeRef.current = active;
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
  const [trial, setTrial] = useState(false);
  const [trialAvailable, setTrialAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const renderer = useRef(crypto.randomUUID());
  const creation = useRef({ id: crypto.randomUUID(), title: '' });
  const documentIntent = useRef<CreateDocumentIntent | null>(null);
  const recovery = useRef(crypto.randomUUID());
  const duplication = useRef({ id: crypto.randomUUID(), source: '' });
  const [loading, setLoading] = useState(true);

  async function refreshLibrary() { setLibrary(await librarySnapshot()); }
  useEffect(() => { void refreshLibrary().catch(reason => setError(errorText(reason))).finally(() => setLoading(false)); }, []);
  useEffect(() => {
    let disposed = false;
    if (isTauri()) void runtimeInfo().then(info => { if (!disposed) setTrialAvailable(info.editorTrial === true); }).catch(() => {});
    return () => { disposed = true; };
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    const attached = getCurrentWindow().onCloseRequested(event => {
      event.preventDefault();
      if (running.current) { setNotice('Finish the current operation before closing.'); return; }
      void perform(async () => { await activeRef.current?.session.detach('close'); await getCurrentWindow().destroy(); });
    });
    return () => { void attached.then(unlisten => unlisten()); };
  }, []);

  async function perform(work: () => Promise<void>) {
    if (running.current) return;
    running.current = true; setBusy(true); setError(''); setNotice('');
    try { await work(); }
    catch (reason) { setError(errorText(reason)); }
    finally { running.current = false; setBusy(false); }
  }
  function activate(opened: OpenedProject, document?: DocumentRecord) {
    if (document) opened = { ...opened, documents: opened.documents.map(item => item.head.documentId === document.head.documentId ? document : item) };
    setExporting(null);
    setProject(opened);
    const preferences = readProjectTabs(opened.project.projectId);
    const previousDocument = opened.documents.find(item => item.head.documentId === opened.viewState?.documentId) ?? opened.documents[0];
    const tab = document ? tabForKind(document.kind) : preferences.activeTab ?? tabForKind(previousDocument?.kind ?? 'chapter');
    const eligible = documentsForTab(opened.documents, tab);
    const next = document ?? eligible.find(item => item.head.documentId === preferences.lastDocumentByTab[tab]) ?? eligible.find(item => item.head.documentId === opened.viewState?.documentId) ?? eligible[0];
    setProjectTab(tab);
    writeProjectTabs(opened.project.projectId, { ...preferences, activeTab: tab, lastDocumentByTab: { ...preferences.lastDocumentByTab, ...(next ? { [tab]: next.head.documentId } : {}) } });
    setActive(next ? { record: next, session: new DocumentSession(opened.access, next, projectTransport), viewState: opened.viewState } : null);
    setSearch(''); setNewProject(false); setNewDocument(false); setRenaming(false); setRenamingDocument(false);
    if (opened.libraryWarning) setNotice(`Project opened. Library update needs attention: ${opened.libraryWarning}`);
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
    });
  }
  function create(title: string, operationId?: string) {
    void perform(async () => {
      const name = title.trim() || 'Untitled project';
      if (creation.current.title !== name) creation.current = { id: crypto.randomUUID(), title: name };
      const opened = await navigate(() => libraryCreate(operationId ?? creation.current.id, name, renderer.current));
      activate(opened); await refreshLibrary(); setTitle(''); creation.current = { id: crypto.randomUUID(), title: '' };
    });
  }
  function backToLibrary() {
    void perform(async () => { await navigate(refreshLibrary); setActive(null); setProject(null); setSearch(''); });
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
  function selectTab(tab: ProjectTabId) {
    if (!project || tab === projectTab) return;
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

  if (trial && trialAvailable && EditorTrial) return <><button className="trial-return" onClick={() => setTrial(false)}>Back to library</button><Suspense fallback={<p role="status">Opening editor trial…</p>}><EditorTrial /></Suspense></>;
  const entries = library.entries.filter(entry => entry.archived === archived && entry.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const currentTab = PROJECT_TABS.find(tab => tab.id === projectTab)!;
  const tabDocuments = documentsForTab(project?.documents ?? [], projectTab);
  const documents = tabDocuments.filter(document => document.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const activeIndex = tabDocuments.findIndex(document => document.head.documentId === active?.record.head.documentId);
  return <div className="app persistent-workspace">
    <header className="app-header"><div className="brand">{project && <button className="library-back" disabled={busy} onClick={backToLibrary}>All projects</button>}<strong>{project ? project.project.title : 'WebnovelStudio'}</strong>{!project && <span className="trial-label">Your library</span>}</div>
      <div className="assistant-controls"><ModelSelector /><ModelSettings /></div>
    </header>
    {project && <div className="project-navigation"><div className="project-tabs" role="tablist" aria-label="Project workspace" onKeyDown={event => {
      const tabs = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
      const index = tabs.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1;
      if (next >= 0) { event.preventDefault(); tabs[next]?.focus(); }
    }}>{PROJECT_TABS.map(tab => <button key={tab.id} id={`project-tab-${tab.id}`} role="tab" aria-selected={projectTab === tab.id} aria-controls="project-workspace-panel" tabIndex={projectTab === tab.id ? 0 : -1} disabled={busy} onClick={() => selectTab(tab.id)}>{tab.label}<span className="tab-count">{documentsForTab(project.documents, tab.id).length}</span></button>)}</div>
      <details className="project-tools"><summary>Project options</summary><div className="project-tools-menu"><button disabled={busy} onClick={() => { setRenamedTitle(project.project.title); setRenaming(!renaming); }}>Rename</button><button disabled={busy} onClick={duplicate}>Duplicate</button><button disabled={busy} onClick={() => void perform(backup)}>Backup</button><button ref={exportButton} disabled={busy || !active} onClick={() => void perform(exportDraft)}>Export draft</button></div></details>
    </div>}
    {project && renaming && <form className="rename-project-form" onSubmit={rename}><label htmlFor="rename-project">Project title</label><input autoFocus id="rename-project" value={renamedTitle} maxLength={160} onChange={event => setRenamedTitle(event.target.value)} /><button type="button" onClick={() => setRenaming(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedTitle.trim()}>Save title</button></form>}
    {project && active && renamingDocument && <form className="rename-project-form" onSubmit={renameCurrentDocument}><label htmlFor="rename-document">Document title</label><input autoFocus id="rename-document" value={renamedDocumentTitle} maxLength={160} onChange={event => setRenamedDocumentTitle(event.target.value)} /><button type="button" onClick={() => setRenamingDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedDocumentTitle.trim()}>Save document title</button></form>}
    {!project ? <main className="library-page" aria-label="Project library">
      <div className="library-heading"><div><h1>Your stories</h1><p>Start wherever the idea begins.</p></div><div className="header-actions"><button className="secondary-button" disabled={busy} onClick={() => open(null)}>Open folder</button><button className="secondary-button" disabled={busy} onClick={() => setImportingV2(true)}>Import V2 project</button><button className="primary-button" disabled={busy} onClick={() => setNewProject(true)}>New project</button></div></div>
      {newProject && <form className="inline-form" onSubmit={event => { event.preventDefault(); create(title); }}><label htmlFor="project-title">Project title</label><input autoFocus id="project-title" value={title} maxLength={160} onChange={event => setTitle(event.target.value)} placeholder="Untitled project" disabled={busy} /><div><button type="button" disabled={busy} onClick={() => setNewProject(false)}>Cancel</button><button className="primary-button" disabled={busy}>{busy ? 'Creating…' : 'Create project'}</button></div></form>}
      <div className="library-filters"><label className="search-field"><span className="sr-only">Find a project</span><input type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Find a project" /></label><button aria-pressed={archived} onClick={() => setArchived(!archived)}>{archived ? 'Show active' : 'Archived'}</button></div>
      {loading ? <p role="status">Opening your library…</p> : entries.length ? <ul className="project-list">{entries.map(entry => <li key={entry.projectId}><button className="project-open" disabled={busy || entry.missing} onClick={() => open(entry.path)}><strong>{entry.title}</strong><span>{entry.missing ? 'Folder moved or unavailable' : `Last opened ${new Date(entry.lastOpened).toLocaleDateString()}`}</span></button>{entry.missing && <button disabled={busy} onClick={() => open(null)}>Locate</button>}<button disabled={busy} aria-label={`${entry.archived ? 'Unarchive' : 'Archive'} ${entry.title}`} onClick={() => void perform(async () => { await libraryArchive(entry.projectId, !entry.archived); await refreshLibrary(); })}>{entry.archived ? 'Unarchive' : 'Archive'}</button></li>)}</ul>
        : <div className="library-empty"><h2>{search ? 'No matching projects' : archived ? 'No archived projects' : 'A place for your next story'}</h2><p>{search ? 'Try a different title.' : archived ? 'Archived projects stay on your computer.' : 'Create a project, then add a character, a world, a chapter, or a simple note. There is no required order.'}</p></div>}
      {!!library.pending.length && <section className="pending-projects" aria-label="Unfinished project operations"><h2>Unfinished setup</h2>{library.pending.map(pending => <div key={pending.origin.operationId}><span>{pending.title}</span>{pending.kind === 'create' && <button disabled={busy} onClick={() => create(pending.title, pending.origin.operationId)}>Resume creation</button>}{pending.kind === 'duplicate' && <button disabled={busy} onClick={() => resumeDuplicate(pending.origin.operationId, pending.title)}>Resume copy</button>}{pending.kind === 'recover' && <button disabled={busy} onClick={() => recover(pending.origin.operationId, pending.title)}>Resume recovery</button>}{pending.kind === 'import' && <button disabled={busy} onClick={() => resumeImport(pending.origin.operationId)}>Check import</button>}</div>)}</section>}
      <footer className="library-footer"><span>Projects are saved on this computer.</span><div className="header-actions"><button disabled={busy} onClick={() => recover()}>Recover backup</button>{EditorTrial && trialAvailable && <button disabled={busy} onClick={() => setTrial(true)}>Open editor trial</button>}</div></footer>
    </main> : <div className="workspace" id="project-workspace-panel" role="tabpanel" aria-labelledby={`project-tab-${projectTab}`}>
      <aside className="document-sidebar" aria-label="Project documents"><div className="sidebar-heading"><h2>{currentTab.label}</h2><button className="primary-button" disabled={busy} onClick={() => beginDocument()}>Add</button></div><input aria-label="Find a document" type="search" placeholder={`Find ${currentTab.label.toLocaleLowerCase()}`} value={search} onChange={event => setSearch(event.target.value)} />
        {newDocument && <form className="inline-form document-form" onSubmit={addDocument}><label htmlFor="document-kind">Start with</label><select id="document-kind" value={kind} onChange={event => setKind(event.target.value)}>{kinds.map(kind => <option key={kind} value={kind}>{kind.charAt(0).toUpperCase() + kind.slice(1)}</option>)}</select><label htmlFor="document-title">Title</label><input id="document-title" autoFocus value={documentTitle} onChange={event => setDocumentTitle(event.target.value)} maxLength={160} /><div><button type="button" onClick={() => setNewDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy}>Create</button></div></form>}
        <nav aria-label="Documents">{documents.map(document => <button key={document.head.documentId} aria-current={active?.record.head.documentId === document.head.documentId ? 'page' : undefined} disabled={busy} onClick={() => selectDocument(document)}>{projectTab === 'chapters' && <small>Chapter {tabDocuments.indexOf(document) + 1}</small>}<span>{document.title}</span>{projectTab !== 'chapters' && <small>{document.kind}</small>}</button>)}</nav>
        {!documents.length && <p className="small-copy">{search ? 'No matching documents.' : `Your ${currentTab.label.toLocaleLowerCase()} will appear here.`}</p>}
        <p className="sidebar-footnote">Build your story in any order.</p>
      </aside>
      {active ? <Writer key={`${project.project.projectId}:${active.record.head.documentId}`} active={active} sources={project.documents.map(document => ({ id: document.head.documentId, title: document.title }))} onError={setError} onRename={() => { setRenamedDocumentTitle(active.record.title); setRenamingDocument(!renamingDocument); }} navigation={projectTab === 'chapters' ? { index: activeIndex, total: tabDocuments.length, previous: activeIndex > 0 ? () => selectDocument(tabDocuments[activeIndex - 1]) : undefined, next: activeIndex < tabDocuments.length - 1 ? () => selectDocument(tabDocuments[activeIndex + 1]) : undefined, disabled: busy } : undefined} /> : <main className="empty-project"><div className="project-start"><h1>{projectTab === 'chapters' ? 'Give your next chapter a direction.' : projectTab === 'worldbuilding' ? 'Create the world your story needs.' : projectTab === 'characters' ? 'Find the people at the heart of it.' : projectTab === 'plot' ? 'Shape what happens next.' : 'Keep the ideas worth returning to.'}</h1><p>{projectTab === 'chapters' ? 'Start with a brief. Let the AI draft, then read, revise, and decide what belongs in your story.' : 'Bring an idea, ask the AI to develop it, and choose what to keep. You can always write and edit directly.'}</p><button className="primary-button" disabled={busy} onClick={() => beginDocument()}>{projectTab === 'chapters' ? 'Create a chapter' : projectTab === 'characters' ? 'Create a character' : projectTab === 'worldbuilding' ? 'Create worldbuilding' : 'Create an idea'}</button><p className="start-alternative">You can start in any tab. No setup checklist is required.</p></div></main>}
    </div>}
    {exporting && active?.session === exporting.session && <ExportDialog access={exporting.session.projectAccess} documentId={exporting.record.head.documentId} title={exporting.record.title} isChapter={exporting.record.kind === 'chapter'}
      onPrepare={(format, basis) => prepareExport(exporting, format, basis)} onExport={preview => writeExport(exporting, preview)}
      onClose={() => { setExporting(null); exportButton.current?.focus(); }} />}
    {importingV2 && !project && <V2ImportDialog session={renderer.current}
      onImported={opened => { setImportingV2(false); activate(opened); void refreshLibrary(); }}
      onClose={() => { setImportingV2(false); void refreshLibrary(); }} />}
    {(error || notice || busy) && <footer className="workspace-notice" role={error ? 'alert' : 'status'}><span className={error ? 'error-status' : ''}>{error || notice || 'Working…'}</span>{error && <button onClick={() => setError('')}>Dismiss</button>}</footer>}
  </div>;
}
