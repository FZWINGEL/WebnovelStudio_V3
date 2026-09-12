import { lazy, Suspense } from 'react';
import { Writer, ProjectConversation, ConversationHistoryPanel } from '../chat';
import { ModelSelector, ModelSettings } from '../providers';
import type { ProjectActivitySnapshot } from '../ipc/projectActivity';
import { ExportDialog } from './ExportDialog';
import { V2ImportDialog } from './V2ImportDialog';
import { PROJECT_TABS, documentsForTab, type ProjectTabId } from './projectTabs';
import { CHAT_FIRST_TRIAL_ENABLED, type WorkspaceMode } from './workspaceModes';
import { RecentProjectPicker, type RecentProjectPickerItem } from './RecentProjectPicker';
import { CoauthorSidebar } from './CoauthorSidebar';
import { Workshop } from './Workshop';
import { StoryBible } from './StoryBible';
import { AppCloseDialog } from './AppCloseDialog';
import { useWorkspaceModel } from './workspaceModel';
import './workspaceModes.css';
import './CoauthorWorkspace.css';

const EditorTrial = typeof __WNS_EDITOR_TRIAL__ !== 'undefined' && __WNS_EDITOR_TRIAL__
  ? lazy(() => import('./App').then(module => ({ default: module.App })))
  : null;

export function Workspace() {
  const model = useWorkspaceModel();
  const { snapshot: library, loading, archived, newProject, title } = model.library.view;
  const { open, create, backToLibrary, recover, duplicate, resumeDuplicate, resumeImport, archive,
    setArchived, setNewProject, setTitle, openImport } = model.library.actions;
  const { importDialog } = model.library;
  const { project, active, exporting, projectTab, workspaceMode, chatReviewRun, kinds,
    newDocument, renaming, renamedTitle, renamingDocument, renamedDocumentTitle, documentTitle, kind } = model.workspace.view;
  const { setNewDocument, setRenaming, setRenamedTitle, setRenamingDocument, setRenamedDocumentTitle, setDocumentTitle, setKind,
    closeExport, acceptChatAccess, mergeWorkshopDocuments, selectWorkspaceMode, openChatChapterResult,
    prepareChatAdoption, acceptChatDocuments, restoreChatAdoption, createChatNote, createChatChapter,
    prepareChatChapter, prepareChatSource, openDocumentFromWorkshop, openStoryBible, rename,
    selectDocument, selectTab, beginDocument, renameCurrentDocument, addDocument,
    backup, exportDraft, prepareExport, writeExport } = model.workspace.actions;
  const { busy, notice, error, setError } = model.status;
  const { search, setSearch, chatHistoryOpen, setChatHistoryOpen, storyBibleOpen, setStoryBibleOpen,
    trial, setTrial, trialAvailable, projectActivity, setProjectPickerOpen, closeStoryBible } = model.presentation;
  const { exportButton, storyBibleButton, workshopRef, projectChatRef } = model.refs;
  const { appClose } = model;

  if (trial && trialAvailable && EditorTrial) return <><button className="trial-return" onClick={() => setTrial(false)}>Back to library</button><Suspense fallback={<p role="status">Opening editor trial…</p>}><EditorTrial /></Suspense></>;
  const entries = library.entries.filter(entry => entry.archived === archived && entry.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const currentEntry = project ? library.entries.find(entry => entry.projectId === project.project.projectId) : undefined;
  const activityBadges = (activity: ProjectActivitySnapshot | undefined): Pick<RecentProjectPickerItem, 'activityLabel' | 'pendingDrafts'> => ({
    activityLabel: activity && activity.activeWorkCount > 0
      ? `${activity.activeWorkCount} active task${activity.activeWorkCount === 1 ? '' : 's'}`
      : undefined,
    pendingDrafts: activity && activity.pendingDrafts > 0 ? activity.pendingDrafts : undefined,
  });
  const pickerCurrent: RecentProjectPickerItem | null = project ? {
    projectId: project.project.projectId,
    title: project.project.title,
    path: currentEntry?.path ?? '',
    lastOpened: currentEntry?.lastOpened ?? '',
    missing: false,
    archived: false,
    current: true,
    ...activityBadges(projectActivity[project.project.projectId]),
  } : null;
  const pickerRecent: RecentProjectPickerItem[] = library.entries.filter(entry => !entry.archived).slice().sort((left, right) => {
    const leftOpened = Date.parse(left.lastOpened);
    const rightOpened = Date.parse(right.lastOpened);
    if (Number.isNaN(leftOpened) && Number.isNaN(rightOpened)) return left.projectId.localeCompare(right.projectId);
    if (Number.isNaN(leftOpened)) return 1;
    if (Number.isNaN(rightOpened)) return -1;
    return rightOpened - leftOpened || left.projectId.localeCompare(right.projectId);
  }).map(entry => ({
    ...entry,
    ...activityBadges(projectActivity[entry.projectId]),
  }));
  const currentTab = PROJECT_TABS.find(tab => tab.id === projectTab)!;
  const tabDocuments = documentsForTab(project?.documents ?? [], projectTab);
  const documents = tabDocuments.filter(document => document.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const activeIndex = tabDocuments.findIndex(document => document.head.documentId === active?.record.head.documentId);
  const coauthorLayout = !!project && workspaceMode === 'chat';
  return <div className={`app persistent-workspace${coauthorLayout ? ' coauthor-workspace' : ''}`}>
    {coauthorLayout && pickerCurrent && <CoauthorSidebar current={pickerCurrent} recent={pickerRecent} busy={busy} counts={Object.fromEntries(PROJECT_TABS.map(tab => [tab.id, documentsForTab(project.documents, tab.id).length])) as Record<ProjectTabId, number>} onLibrary={backToLibrary} onNewProject={() => setNewProject(true)} onOpen={path => open(path)} onMaterial={tab => selectTab(tab, true)} creationForm={newProject ? <form className="inline-form" onSubmit={event => { event.preventDefault(); create(title); }}><label htmlFor="coauthor-project-title">Project title</label><input autoFocus id="coauthor-project-title" value={title} maxLength={160} onChange={event => setTitle(event.target.value)} placeholder="Untitled project" disabled={busy} /><div><button type="button" disabled={busy} onClick={() => setNewProject(false)}>Cancel</button><button className="primary-button" disabled={busy}>{busy ? 'Creating…' : 'Create project'}</button></div></form> : undefined} />}
    <header className="app-header"><div className="brand">{project && !coauthorLayout && <button className="library-back" disabled={busy} onClick={backToLibrary}>All projects</button>}<strong tabIndex={-1}>{project ? project.project.title : 'WebnovelStudio'}</strong>{!project && <span className="trial-label">Your library</span>}{!coauthorLayout && <RecentProjectPicker current={pickerCurrent} recent={pickerRecent} disabled={busy} onVisibilityChange={setProjectPickerOpen} onOpen={path => open(path)} />}</div>
      <div className="assistant-controls"><ModelSelector /><ModelSettings /></div>
    </header>
    {project && <div className="project-navigation">
      <div className="workspace-mode-row">
        <div className="workspace-mode-switch" role="tablist" aria-label="Primary workspace" onKeyDown={event => {
          const tabs = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
          const index = tabs.indexOf(document.activeElement as HTMLButtonElement);
          const next = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1;
          if (next >= 0) { event.preventDefault(); tabs[next]?.focus(); }
        }}>
          {([...(CHAT_FIRST_TRIAL_ENABLED ? ['chat' as const] : []), 'develop', 'write'] as WorkspaceMode[]).map(mode => <button key={mode} id={`workspace-mode-${mode}`} role="tab" aria-selected={workspaceMode === mode} aria-controls="workspace-mode-panel" tabIndex={workspaceMode === mode ? 0 : -1} disabled={busy} onClick={() => selectWorkspaceMode(mode)}>{mode === 'chat' ? 'Project chat · trial' : mode === 'develop' ? 'Develop' : 'Write'}</button>)}
        </div>
        <button ref={storyBibleButton} className="story-bible-action" disabled={busy} onClick={openStoryBible}>Story Bible</button>
        <details className="project-tools"><summary>Project options</summary><div className="project-tools-menu"><button disabled={busy} onClick={() => { setRenamedTitle(project.project.title); setRenaming(!renaming); }}>Rename</button><button disabled={busy} onClick={() => setChatHistoryOpen(true)}>Conversation history</button><button disabled={busy} onClick={duplicate}>Duplicate</button><button disabled={busy} onClick={backup}>Backup</button><button ref={exportButton} disabled={busy || !active} onClick={exportDraft}>Export draft</button></div></details>
      </div>
      {workspaceMode === 'write' && <div className="project-tabs" role="tablist" aria-label="Project workspace" onKeyDown={event => {
        const tabs = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
        const index = tabs.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1;
        if (next >= 0) { event.preventDefault(); tabs[next]?.focus(); }
      }}>{PROJECT_TABS.map(tab => <button key={tab.id} id={`project-tab-${tab.id}`} role="tab" aria-selected={projectTab === tab.id} aria-controls="project-workspace-panel" tabIndex={projectTab === tab.id ? 0 : -1} disabled={busy} onClick={() => selectTab(tab.id)}>{tab.label}<span className="tab-count">{documentsForTab(project.documents, tab.id).length}</span></button>)}</div>}
    </div>}
    {project && renaming && <form className="rename-project-form" onSubmit={rename}><label htmlFor="rename-project">Project title</label><input autoFocus id="rename-project" value={renamedTitle} maxLength={160} onChange={event => setRenamedTitle(event.target.value)} /><button type="button" onClick={() => setRenaming(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedTitle.trim()}>Save title</button></form>}
    {project && active && renamingDocument && <form className="rename-project-form" onSubmit={renameCurrentDocument}><label htmlFor="rename-document">Document title</label><input autoFocus id="rename-document" value={renamedDocumentTitle} maxLength={160} onChange={event => setRenamedDocumentTitle(event.target.value)} /><button type="button" onClick={() => setRenamingDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy || !renamedDocumentTitle.trim()}>Save document title</button></form>}
    {!project ? <main className="library-page" aria-label="Project library">
      <div className="library-heading"><div><h1 tabIndex={-1}>Your stories</h1><p>Start wherever the idea begins.</p></div><div className="header-actions"><button className="secondary-button" disabled={busy} onClick={() => open(null)}>Open folder</button><button className="secondary-button" disabled={busy} onClick={openImport}>Import V2 project</button><button className="primary-button" disabled={busy} onClick={() => setNewProject(true)}>New project</button></div></div>
      {newProject && <form className="inline-form" onSubmit={event => { event.preventDefault(); create(title); }}><label htmlFor="project-title">Project title</label><input autoFocus id="project-title" value={title} maxLength={160} onChange={event => setTitle(event.target.value)} placeholder="Untitled project" disabled={busy} /><div><button type="button" disabled={busy} onClick={() => setNewProject(false)}>Cancel</button><button className="primary-button" disabled={busy}>{busy ? 'Creating…' : 'Create project'}</button></div></form>}
      <div className="library-filters"><label className="search-field"><span className="sr-only">Find a project</span><input type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Find a project" /></label><button aria-pressed={archived} onClick={() => setArchived(!archived)}>{archived ? 'Show active' : 'Archived'}</button></div>
      {loading ? <p role="status">Opening your library…</p> : entries.length ? <ul className="project-list">{entries.map(entry => <li key={entry.projectId}><button className="project-open" disabled={busy || entry.missing} onClick={() => open(entry.path)}><strong>{entry.title}</strong><span>{entry.missing ? 'Folder moved or unavailable' : `Last opened ${new Date(entry.lastOpened).toLocaleDateString()}`}</span></button>{entry.missing && <button disabled={busy} onClick={() => open(null)}>Locate</button>}<button disabled={busy} aria-label={`${entry.archived ? 'Unarchive' : 'Archive'} ${entry.title}`} onClick={() => archive(entry.projectId, !entry.archived)}>{entry.archived ? 'Unarchive' : 'Archive'}</button></li>)}</ul>
        : <div className="library-empty"><h2>{search ? 'No matching projects' : archived ? 'No archived projects' : 'A place for your next story'}</h2><p>{search ? 'Try a different title.' : archived ? 'Archived projects stay on your computer.' : 'Create a project, then add a character, a world, a chapter, or a simple note. There is no required order.'}</p></div>}
      {!!library.pending.length && <section className="pending-projects" aria-label="Unfinished project operations"><h2>Unfinished setup</h2>{library.pending.map(pending => <div key={pending.origin.operationId}><span>{pending.title}</span>{pending.kind === 'create' && <button disabled={busy} onClick={() => create(pending.title, pending.origin.operationId)}>Resume creation</button>}{pending.kind === 'duplicate' && <button disabled={busy} onClick={() => resumeDuplicate(pending.origin.operationId, pending.title)}>Resume copy</button>}{pending.kind === 'recover' && <button disabled={busy} onClick={() => recover(pending.origin.operationId, pending.title)}>Resume recovery</button>}{pending.kind === 'import' && <button disabled={busy} onClick={() => resumeImport(pending.origin.operationId)}>Check import</button>}</div>)}</section>}
      <footer className="library-footer"><span>Projects are saved on this computer.</span><div className="header-actions"><button disabled={busy} onClick={() => recover()}>Recover backup</button>{EditorTrial && trialAvailable && <button disabled={busy} onClick={() => setTrial(true)}>Open editor trial</button>}</div></footer>
    </main> : workspaceMode === null ? <main className="workspace-mode-choice" id="workspace-mode-panel" aria-labelledby="workspace-mode-choice-title">
      <div className="workspace-mode-choice-card">
        <p className="workspace-mode-kicker">A new story can begin anywhere.</p>
        <h1 id="workspace-mode-choice-title">How do you want to begin?</h1>
        <p>Explore the story first, or open the writing desk and create a chapter when you are ready.</p>
        <div className="workspace-mode-choice-actions">
          {CHAT_FIRST_TRIAL_ENABLED && <button className="primary-button" disabled={busy} onClick={() => selectWorkspaceMode('chat')}>Start a conversation · trial</button>}
          <button className="primary-button" disabled={busy} onClick={() => selectWorkspaceMode('develop')}>Develop a story</button>
          <button className="secondary-button" disabled={busy} onClick={() => selectWorkspaceMode('write')}>Start writing</button>
        </div>
      </div>
    </main> : workspaceMode === 'chat' ? <main className="workspace-chat" id="workspace-mode-panel" aria-labelledby="workspace-mode-chat">
      <ProjectConversation ref={projectChatRef} key={`${project.project.projectId}:${project.access.operationNamespace}:${project.access.session}`} project={project} activeDocument={active?.record} onOpenDocument={selectDocument} onDocumentsChanged={acceptChatDocuments} onPrepareSource={prepareChatSource} onPrepareChapter={prepareChatChapter} onEarlierWorkshop={() => selectWorkspaceMode('develop')} onBeforeAdoption={prepareChatAdoption} onAdoptionFailure={restoreChatAdoption} onAccessChanged={acceptChatAccess} onCreateChapter={createChatChapter} onCreateNote={createChatNote} onOpenChapterResult={openChatChapterResult} editor={active ? { active, sources: project.documents.map(document => ({ id: document.head.documentId, title: document.title })), onError: setError, onRename: () => { setRenamedDocumentTitle(active.record.title); setRenamingDocument(!renamingDocument); } } : undefined} reviewRunId={chatReviewRun && active && chatReviewRun.target.documentId === active.record.head.documentId ? chatReviewRun.id : null} />
    </main> : workspaceMode === 'develop' ? <main className="workspace-develop" id="workspace-mode-panel" aria-labelledby="workspace-mode-develop-title">
      <h1 id="workspace-mode-develop-title" className="sr-only">Develop your story</h1>
      <Workshop ref={workshopRef} project={project} navigationBusy={busy} onOpenDocument={(documentId: string) => openDocumentFromWorkshop(project.project.projectId, documentId)} onDocumentsChanged={documents => mergeWorkshopDocuments(project.project.projectId, documents)} onError={setError} />
    </main> : <div className="workspace" id="project-workspace-panel" role="tabpanel" aria-labelledby={`project-tab-${projectTab}`}>
      <aside className="document-sidebar" aria-label="Project documents"><div className="sidebar-heading"><h2>{currentTab.label}</h2><button className="primary-button" disabled={busy} onClick={() => beginDocument()}>Add</button></div><input aria-label="Find a document" type="search" placeholder={`Find ${currentTab.label.toLocaleLowerCase()}`} value={search} onChange={event => setSearch(event.target.value)} />
        {newDocument && <form className="inline-form document-form" onSubmit={addDocument}><label htmlFor="document-kind">Start with</label><select id="document-kind" value={kind} onChange={event => setKind(event.target.value)}>{kinds.map(kind => <option key={kind} value={kind}>{kind.charAt(0).toUpperCase() + kind.slice(1)}</option>)}</select><label htmlFor="document-title">Title</label><input id="document-title" autoFocus value={documentTitle} onChange={event => setDocumentTitle(event.target.value)} maxLength={160} /><div><button type="button" onClick={() => setNewDocument(false)} disabled={busy}>Cancel</button><button className="primary-button" disabled={busy}>Create</button></div></form>}
        <nav aria-label="Documents">{documents.map(document => <button key={document.head.documentId} aria-current={active?.record.head.documentId === document.head.documentId ? 'page' : undefined} disabled={busy} onClick={() => selectDocument(document)}>{projectTab === 'chapters' && <small>Chapter {tabDocuments.indexOf(document) + 1}</small>}<span>{document.title}</span>{projectTab !== 'chapters' && <small>{document.kind}</small>}</button>)}</nav>
        {!documents.length && <p className="small-copy">{search ? 'No matching documents.' : `Your ${currentTab.label.toLocaleLowerCase()} will appear here.`}</p>}
        <p className="sidebar-footnote">Build your story in any order.</p>
      </aside>
      {active ? <Writer key={`${project.project.projectId}:${active.record.head.documentId}`} active={active} sources={project.documents.map(document => ({ id: document.head.documentId, title: document.title }))} onError={setError} onRename={() => { setRenamedDocumentTitle(active.record.title); setRenamingDocument(!renamingDocument); }} navigation={projectTab === 'chapters' ? { index: activeIndex, total: tabDocuments.length, previous: activeIndex > 0 ? () => selectDocument(tabDocuments[activeIndex - 1]) : undefined, next: activeIndex < tabDocuments.length - 1 ? () => selectDocument(tabDocuments[activeIndex + 1]) : undefined, disabled: busy } : undefined} /> : <main className="empty-project"><div className="project-start"><h1>{projectTab === 'chapters' ? 'Give your next chapter a direction.' : projectTab === 'worldbuilding' ? 'Create the world your story needs.' : projectTab === 'characters' ? 'Find the people at the heart of it.' : projectTab === 'plot' ? 'Shape what happens next.' : 'Keep the ideas worth returning to.'}</h1><p>{projectTab === 'chapters' ? 'Start with a brief. Let the AI draft, then read, revise, and decide what belongs in your story.' : 'Bring an idea, ask the AI to develop it, and choose what to keep. You can always write and edit directly.'}</p><button className="primary-button" disabled={busy} onClick={() => beginDocument()}>{projectTab === 'chapters' ? 'Create a chapter' : projectTab === 'characters' ? 'Create a character' : projectTab === 'worldbuilding' ? 'Create worldbuilding' : 'Create an idea'}</button><p className="start-alternative">You can start in any tab. No setup checklist is required.</p></div></main>}
    </div>}
    {appClose.prompt && <AppCloseDialog phase={appClose.prompt.phase} status={appClose.prompt.status} message={appClose.prompt.message} onStop={() => { void appClose.stopAndClose(); }} onStayOpen={() => { void appClose.stayOpen(); }} />}
    {exporting && active?.session === exporting.session && <ExportDialog access={exporting.session.projectAccess} documentId={exporting.record.head.documentId} title={exporting.record.title} isChapter={exporting.record.kind === 'chapter'}
      onPrepare={(format, basis) => prepareExport(exporting, format, basis)} onExport={preview => writeExport(exporting, preview)}
      onClose={closeExport} />}
    {importDialog && !project && <V2ImportDialog {...importDialog} />}
    {storyBibleOpen && project && <StoryBible project={project} onClose={closeStoryBible} onOpenDocument={(documentId: string) => { setStoryBibleOpen(false); openDocumentFromWorkshop(project.project.projectId, documentId); }} />}
    {chatHistoryOpen && project && <ConversationHistoryPanel access={active?.session.projectAccess ?? project.access} onClose={() => setChatHistoryOpen(false)} />}
    {(error || notice || busy) && <footer className="workspace-notice" role={error ? 'alert' : 'status'}><span className={error ? 'error-status' : ''}>{error || notice || 'Working…'}</span>{error && <button onClick={() => setError('')}>Dismiss</button>}</footer>}
  </div>;
}

