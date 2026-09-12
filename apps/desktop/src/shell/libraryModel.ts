/** Owns the catalog, library forms, and retry identities for project operations. */
import { useEffect, useRef, useState } from 'react';
import { librarySnapshot, libraryOpen, libraryCreate, libraryArchive, libraryRecover, libraryDuplicate, libraryResumeImport, type LibrarySnapshot } from '../ipc/library';
import type { OpenedProject } from '../ipc/projects';
import { sameAccessIdentity, workspaceErrorText, type ProjectNavigation, type WorkspaceOperations, type WorkspacePresentation } from './workspaceContracts';

export interface LibraryModelDeps {
  rendererId: string;
  navigation: ProjectNavigation;
  operations: WorkspaceOperations;
  present(event: WorkspacePresentation): void;
}

export function useLibraryModel(deps: LibraryModelDeps) {
  const depsRef = useRef(deps); depsRef.current = deps;
  const [snapshot, setSnapshot] = useState<LibrarySnapshot>({ entries: [], pending: [] });
  const [loading, setLoading] = useState(true);
  const [archived, setArchived] = useState(false);
  const [newProject, setNewProject] = useState(false);
  const [importingV2, setImportingV2] = useState(false);
  const [title, setTitle] = useState('');
  const creation = useRef({ id: crypto.randomUUID(), title: '' });
  const recovery = useRef(crypto.randomUUID());
  const duplication = useRef({ id: crypto.randomUUID(), source: '' });
  const perform = (work: () => Promise<void>) => depsRef.current.operations.perform(work);
  const focusProject = (projectId: string) => depsRef.current.present({ kind: 'focus', selector: '.app-header .brand strong', projectId });

  async function refresh() { setSnapshot(await librarySnapshot()); }
  useEffect(() => {
    void refresh().catch(reason => depsRef.current.operations.error(workspaceErrorText(reason))).finally(() => setLoading(false));
  }, []);

  function open(path: string | null) {
    void perform(async () => {
      const opened = await depsRef.current.navigation.replaceProject(async () => {
        const result = await libraryOpen(path, depsRef.current.rendererId);
        if (!result) throw new Error('Open cancelled. Your current writing is still here.');
        return result;
      });
      await refresh(); focusProject(opened.project.projectId);
    });
  }
  function create(title: string, operationId?: string) {
    void perform(async () => {
      const name = title.trim() || 'Untitled project';
      if (creation.current.title !== name) creation.current = { id: crypto.randomUUID(), title: name };
      const opened = await depsRef.current.navigation.replaceProject(() => libraryCreate(operationId ?? creation.current.id, name, depsRef.current.rendererId));
      await refresh(); setTitle(''); creation.current = { id: crypto.randomUUID(), title: '' };
      focusProject(opened.project.projectId);
    });
  }
  function backToLibrary() {
    void perform(async () => {
      await depsRef.current.navigation.returnToLibrary(refresh);
      depsRef.current.present({ kind: 'focus', selector: '.library-heading h1', projectId: null });
    });
  }
  function recover(operationId?: string, title = 'Recovered project') {
    void perform(async () => {
      await depsRef.current.navigation.replaceProject(async () => {
        const result = await libraryRecover(operationId ?? recovery.current, title, depsRef.current.rendererId);
        if (!result) throw new Error('Recovery cancelled. No project was replaced.');
        return result;
      });
      recovery.current = crypto.randomUUID(); await refresh();
    });
  }
  function duplicate() {
    const project = depsRef.current.navigation.currentProject();
    if (!project) return;
    void perform(async () => {
      if (duplication.current.source !== project.projectId) duplication.current = { id: crypto.randomUUID(), source: project.projectId };
      await depsRef.current.navigation.replaceProject(current => {
        if (!current || !sameAccessIdentity(current.access, project.access)) throw new Error('The project changed while the copy was being prepared. Try again.');
        return libraryDuplicate(duplication.current.id, current.access, `${project.title} copy`, depsRef.current.rendererId);
      });
      duplication.current = { id: crypto.randomUUID(), source: '' }; await refresh();
    });
  }
  function resumeDuplicate(operationId: string, title: string) {
    void perform(async () => {
      await depsRef.current.navigation.replaceProject(() => libraryDuplicate(operationId, null, title, depsRef.current.rendererId));
      await refresh();
    });
  }
  function resumeImport(operationId: string) {
    void perform(async () => {
      await depsRef.current.navigation.replaceProject(() => libraryResumeImport(operationId, depsRef.current.rendererId));
      await refresh();
    });
  }
  function archive(projectId: string, archived: boolean) {
    void perform(async () => { await libraryArchive(projectId, archived); await refresh(); });
  }
  function imported(opened: OpenedProject) {
    setImportingV2(false); depsRef.current.navigation.acceptImported(opened); void refresh();
  }
  function closeImport() { setImportingV2(false); void refresh(); }

  return {
    view: { snapshot, loading, archived, newProject, title } as const,
    actions: { open, create, backToLibrary, recover, duplicate, resumeDuplicate, resumeImport, archive,
      setArchived, setNewProject, setTitle, openImport: () => setImportingV2(true), closeCreateForm: () => setNewProject(false) },
    importDialog: importingV2 ? { session: deps.rendererId, onImported: imported, onClose: closeImport } : null,
    projectPath: (projectId: string) => snapshot.entries.find(entry => entry.projectId === projectId)?.path ?? null,
  };
}
