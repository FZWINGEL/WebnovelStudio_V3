import { invoke } from '@tauri-apps/api/core';
import type { OpenedProject, ProjectAccess, Head } from './projects';
export interface LibraryEntry { projectId: string; title: string; path: string; archived: boolean; lastOpened: string; missing: boolean }
export interface PendingProject { origin: { operationNamespace: string; operationId: string }; kind: string; title: string; finalPath: string; stagingPath: string; completed: boolean }
export interface LibrarySnapshot { entries: LibraryEntry[]; pending: PendingProject[] }
export const librarySnapshot = (): Promise<LibrarySnapshot> => invoke('library_snapshot');
export const libraryCreate = (operationId: string, title: string, session: string): Promise<OpenedProject> => invoke('library_create', { operationId, title, session });
export const libraryOpen = (path: string | null, session: string): Promise<OpenedProject | null> => invoke('library_open', { path, session });
export const libraryArchive = (projectId: string, archived: boolean): Promise<void> => invoke('library_archive', { projectId, archived });
export const libraryRecover = (operationId: string, title: string, session: string): Promise<OpenedProject | null> => invoke('library_recover', { operationId, title, session });
export const libraryDuplicate = (operationId: string, access: ProjectAccess | null, title: string, session: string): Promise<OpenedProject> => invoke('library_duplicate', { operationId, access, title, session });
export const projectBackup = (access: ProjectAccess): Promise<string | null> => invoke('project_backup', { access });
export const projectExportDraft = (access: ProjectAccess, expected: Head): Promise<string | null> => invoke('project_export_draft', { access, expected });
