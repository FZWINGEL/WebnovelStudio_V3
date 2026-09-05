import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { DocumentSession } from '../editor/session';
import { createDocument, reconcileProject, readDocument, projectTransport, projectMetadata, renameProject, renameDocument, type CreateDocumentIntent, type DocumentRecord, type OpenedProject, type ProjectAccess, type ViewState } from '../ipc/projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, runCreateIntent } from '../ipc/createIntent';
import { librarySnapshot, libraryCreate, libraryOpen, libraryArchive, libraryRecover, libraryDuplicate, projectBackup, projectExportDraft, type LibrarySnapshot } from '../ipc/library';
import { App as EditorTrial } from './App';
import { Writer } from './Writer';

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
  const activeRef = useRef(active); activeRef.current = active;
  const [search, setSearch] = useState('');
  const [archived, setArchived] = useState(false);
  const [newProject, setNewProject] = useState(false);
  const [title, setTitle] = useState('');
  const [newDocument, setNewDocument] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renamedTitle, setRenamedTitle] = useState('');
  const [renamingDocument, setRenamingDocument] = useState(false);
  const [renamedDocumentTitle, setRenamedDocumentTitle] = useState('');
  const [documentTitle, setDocumentTitle] = useState('');
  const [kind, setKind] = useState('chapter');
  const [trial, setTrial] = useState(false);
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
    setProject(opened);
    const next = document ?? opened.documents.find(document => document.head.documentId === opened.viewState?.documentId) ?? opened.documents[0];
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
      const access = activeRef.current?.session.projectAccess ?? project.access;
      const record = await navigate(() => readDocument(access, document.head.documentId));
      activate({ ...project, access }, record);
    });
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
    const settled = await settlePendingDocumentIntent();
    const current = activeRef.current; if (!current) return;
    await current.session.projectWrite(async () => {
      const result = await projectExportDraft(current.session.projectAccess, current.session.state.head);
      if (result) setNotice(`Draft exported as plain text: ${result}`);
    });
  }

  if (trial) return <><button className="trial-return" onClick={() => setTrial(false)}>Back to library</button><EditorTrial /></>;
  const entries = library.entries.filter(entry => entry.archived === archived && entry.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const documents = project?.documents.filter(document => document.title.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ?? [];
  return <div className="app persistent-workspace">
    <header className="app-header"><div className="brand"><strong>WebnovelStudio</strong><span className="trial-label">{project ? project.project.title : 'Library'}</span></div>
      {project ? <div className="header-actions"><button disabled={busy} onClick={backToLibrary}>All projects</button><button disabled={busy} onClick={() => { setRenamedTitle(project.project.title); setRenaming(!renaming); }}>Rename</button><button disabled={busy} onClick={duplicate}>Duplicate</button><button disabled={busy} onClick={() => void perform(backup)}>Backup</button><button disabled={busy || !active} onClick={() => void perform(exportDraft)}>Export draft</button></div>
        : <span className="session-notice">Desktop preview · English writing</span>}
    </header>
    {project && renaming && <form className="rename-project-form" onSubmit={rename}><label htmlFor="rename-project">Project title</label><input autoFocus id="rename-project" value={renamedTitle} maxLength={160} onChange={event => setRenamedTitle(event.target.value)} /><button type="button" onClick={() => setRenaming(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedTitle.trim()}>Save title</button></form>}
    {project && active && renamingDocument && <form className="rename-project-form" onSubmit={renameCurrentDocument}><label htmlFor="rename-document">Document title</label><input autoFocus id="rename-document" value={renamedDocumentTitle} maxLength={160} onChange={event => setRenamedDocumentTitle(event.target.value)} /><button type="button" onClick={() => setRenamingDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedDocumentTitle.trim()}>Save document title</button></form>}
    {!project ? <main className="library-page" aria-label="Project library">
      <div className="library-heading"><div><h1>Your stories</h1><p>Start wherever the idea begins.</p></div><div className="header-actions"><button className="secondary-button" disabled={busy} onClick={() => open(null)}>Open folder</button><button className="primary-button" disabled={busy} onClick={() => setNewProject(true)}>New project</button></div></div>
      {newProject && <form className="inline-form" onSubmit={event => { event.preventDefault(); create(title); }}><label htmlFor="project-title">Project title</label><input autoFocus id="project-title" value={title} maxLength={160} onChange={event => setTitle(event.target.value)} placeholder="Untitled project" disabled={busy} /><div><button type="button" disabled={busy} onClick={() => setNewProject(false)}>Cancel</button><button className="primary-button" disabled={busy}>{busy ? 'Creating…' : 'Create project'}</button></div></form>}
      <div className="library-filters"><label className="search-field"><span className="sr-only">Find a project</span><input type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Find a project" /></label><button aria-pressed={archived} onClick={() => setArchived(!archived)}>{archived ? 'Show active' : 'Archived'}</button></div>
      {loading ? <p role="status">Opening your library…</p> : entries.length ? <ul className="project-list">{entries.map(entry => <li key={entry.projectId}><button className="project-open" disabled={busy || entry.missing} onClick={() => open(entry.path)}><strong>{entry.title}</strong><span>{entry.missing ? 'Folder moved or unavailable' : `Last opened ${new Date(entry.lastOpened).toLocaleDateString()}`}</span></button>{entry.missing && <button disabled={busy} onClick={() => open(null)}>Locate</button>}<button disabled={busy} aria-label={`${entry.archived ? 'Unarchive' : 'Archive'} ${entry.title}`} onClick={() => void perform(async () => { await libraryArchive(entry.projectId, !entry.archived); await refreshLibrary(); })}>{entry.archived ? 'Unarchive' : 'Archive'}</button></li>)}</ul>
        : <div className="library-empty"><h2>{search ? 'No matching projects' : archived ? 'No archived projects' : 'A place for your next story'}</h2><p>{search ? 'Try a different title.' : archived ? 'Archived projects stay on your computer.' : 'Create a project, then add a character, a world, a chapter, or a simple note. There is no required order.'}</p></div>}
      {!!library.pending.length && <section className="pending-projects" aria-label="Unfinished project operations"><h2>Unfinished setup</h2>{library.pending.map(pending => <div key={pending.origin.operationId}><span>{pending.title}</span>{pending.kind === 'create' && <button disabled={busy} onClick={() => create(pending.title, pending.origin.operationId)}>Resume creation</button>}{pending.kind === 'duplicate' && <button disabled={busy} onClick={() => resumeDuplicate(pending.origin.operationId, pending.title)}>Resume copy</button>}{pending.kind === 'recover' && <button disabled={busy} onClick={() => recover(pending.origin.operationId, pending.title)}>Resume recovery</button>}</div>)}</section>}
      <footer className="library-footer"><span>Projects are saved on this computer.</span><div className="header-actions"><button disabled={busy} onClick={() => recover()}>Recover backup</button><button disabled={busy} onClick={() => setTrial(true)}>Open editor trial</button></div></footer>
    </main> : <div className="workspace">
      <aside className="document-sidebar" aria-label="Project documents"><div className="sidebar-heading"><h2>Writing & ideas</h2><button disabled={busy} onClick={() => setNewDocument(true)}>Add</button></div><input aria-label="Find a document" type="search" placeholder="Find a document" value={search} onChange={event => setSearch(event.target.value)} />
        {newDocument && <form className="inline-form document-form" onSubmit={addDocument}><label htmlFor="document-kind">Start with</label><select id="document-kind" value={kind} onChange={event => setKind(event.target.value)}>{kinds.map(kind => <option key={kind} value={kind}>{kind.charAt(0).toUpperCase() + kind.slice(1)}</option>)}</select><label htmlFor="document-title">Title</label><input id="document-title" autoFocus value={documentTitle} onChange={event => setDocumentTitle(event.target.value)} maxLength={160} /><div><button type="button" onClick={() => setNewDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy}>Create</button></div></form>}
        <nav aria-label="Documents">{documents.map(document => <button key={document.head.documentId} aria-current={active?.record.head.documentId === document.head.documentId ? 'page' : undefined} disabled={busy} onClick={() => selectDocument(document)}><span>{document.title}</span><small>{document.kind}</small></button>)}</nav>
        {!documents.length && <p className="small-copy">{search ? 'No matching documents.' : 'Your project is ready. Add whatever you want to explore first.'}</p>}
      </aside>
      {active ? <Writer key={`${project.project.projectId}:${active.record.head.documentId}`} active={active} onError={setError} onRename={() => { setRenamedDocumentTitle(active.record.title); setRenamingDocument(!renamingDocument); }} /> : <main className="empty-project"><h1>Where would you like to start?</h1><p>A character, a world, a chapter, or just a thought.</p><button className="primary-button" disabled={busy} onClick={() => setNewDocument(true)}>Add your first document</button></main>}
    </div>}
    {(error || notice || busy) && <footer className="workspace-notice" role={error ? 'alert' : 'status'}><span className={error ? 'error-status' : ''}>{error || notice || 'Working…'}</span>{error && <button onClick={() => setError('')}>Dismiss</button>}</footer>}
  </div>;
}
