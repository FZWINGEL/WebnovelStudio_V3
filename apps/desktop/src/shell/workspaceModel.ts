/** Composes lifecycle owners with shell presentation, feature handles and close. */
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import type { ProjectConversationHandle } from '../chat';
import type { WorkshopHandle } from './Workshop';
import { runtimeInfo } from '../ipc/native';
import { readProjectActivity, type ProjectActivitySnapshot } from '../ipc/projectActivity';
import { useDocumentWorkspace } from './documentWorkspace';
import { useLibraryModel } from './libraryModel';
import { projectIdentity, workspaceErrorText, type WorkspaceOperations, type WorkspacePresentation } from './workspaceContracts';
import { useAppClose } from './useAppClose';

export function useWorkspaceModel() {
  const [search, setSearch] = useState('');
  const [chatHistoryOpen, setChatHistoryOpen] = useState(false);
  const [storyBibleOpen, setStoryBibleOpen] = useState(false);
  const [trial, setTrial] = useState(false);
  const [trialAvailable, setTrialAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
  const [projectActivity, setProjectActivity] = useState<Record<string, ProjectActivitySnapshot>>({});
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const running = useRef(false);
  const renderer = useRef(crypto.randomUUID());
  const exportButton = useRef<HTMLButtonElement>(null);
  const storyBibleButton = useRef<HTMLButtonElement>(null);
  const workshopRef = useRef<WorkshopHandle>(null);
  const projectChatRef = useRef<ProjectConversationHandle>(null);

  async function perform(work: () => Promise<void>) {
    if (running.current) return;
    running.current = true; setBusy(true); setError(''); setNotice('');
    try { await work(); }
    catch (reason) { setError(workspaceErrorText(reason)); }
    finally { running.current = false; setBusy(false); }
  }
  async function exclusive<T>(busyMessage: string, work: () => Promise<T>): Promise<T> {
    if (running.current) throw new Error(busyMessage);
    running.current = true; setBusy(true);
    try { return await work(); }
    finally { running.current = false; setBusy(false); }
  }
  const operations: WorkspaceOperations = { perform, exclusive, notice: setNotice, error: setError };
  async function flushParticipants() {
    await projectChatRef.current?.flush();
    await workshopRef.current?.flush();
  }
  function present(event: WorkspacePresentation) {
    switch (event.kind) {
      case 'activated': setSearch(''); library.actions.closeCreateForm(); setStoryBibleOpen(false); break;
      case 'selectionChanged': case 'library': setSearch(''); break;
      case 'hideStoryBible': setStoryBibleOpen(false); break;
      case 'showStoryBible': setStoryBibleOpen(true); break;
      case 'focus': requestAnimationFrame(() => {
        if ((workspace.navigation.currentProject()?.projectId ?? null) === event.projectId) document.querySelector<HTMLElement>(event.selector)?.focus();
      }); break;
    }
  }
  const workspace = useDocumentWorkspace({ rendererId: renderer.current, operations, present,
    participants: { flush: flushParticipants, refreshConversation: () => { void projectChatRef.current?.refresh?.().catch(() => {}); } } });
  const library = useLibraryModel({ rendererId: renderer.current, navigation: workspace.navigation, operations, present });
  const { project, workspaceMode } = workspace.view;
  const entries = library.view.snapshot.entries;
  const appClose = useAppClose({ isRunning: () => running.current, notice: setNotice, run: perform, save: flushParticipants, session: workspace.currentSession });

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
    const refresh = async () => {
      if (disposed || inFlight) return;
      inFlight = true;
      try {
        const snapshots = await readProjectActivity();
        if (disposed || !workspace.isCurrent(identity)) return;
        const knownProjectIds = new Set(entries.map(entry => entry.projectId));
        const next: Record<string, ProjectActivitySnapshot> = {};
        for (const snapshot of snapshots) {
          if (!knownProjectIds.has(snapshot.projectId)
            && !(snapshot.projectId === identity?.projectId && snapshot.operationNamespace === identity.operationNamespace)) continue;
          if (identity && snapshot.projectId === identity.projectId && snapshot.operationNamespace !== identity.operationNamespace) continue;
          next[snapshot.projectId] = snapshot;
        }
        setProjectActivity(next);
      } catch {
        if (!disposed && workspace.isCurrent(identity)) setProjectActivity({});
      } finally { inFlight = false; }
    };
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 2000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [projectPickerOpen, workspaceMode, project?.project.projectId, project?.access.operationNamespace, project?.access.session, entries]);

  const { closeCreateForm: _closeCreateForm, ...libraryActions } = library.actions;
  return {
    library: { view: library.view, actions: libraryActions, importDialog: library.importDialog },
    workspace: { view: workspace.view, actions: { ...workspace.actions,
      rename: (event: FormEvent) => workspace.actions.rename(event, project ? library.projectPath(project.project.projectId) : null),
      closeExport: () => { workspace.actions.closeExport(); exportButton.current?.focus(); },
    } },
    status: { busy, notice, error, setError },
    presentation: { search, setSearch, chatHistoryOpen, setChatHistoryOpen, storyBibleOpen, setStoryBibleOpen,
      trial, setTrial, trialAvailable, projectActivity, setProjectPickerOpen,
      closeStoryBible: () => { setStoryBibleOpen(false); requestAnimationFrame(() => storyBibleButton.current?.focus()); } },
    refs: { exportButton, storyBibleButton, workshopRef, projectChatRef },
    appClose,
  };
}
