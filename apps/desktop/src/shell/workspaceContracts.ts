import type { OpenedProject, ProjectAccess } from '../ipc/projects';

export type WorkspaceIdentity = Pick<ProjectAccess, 'projectId' | 'operationNamespace' | 'session'>;

export function projectIdentity(project: OpenedProject): WorkspaceIdentity {
  return { projectId: project.project.projectId, operationNamespace: project.access.operationNamespace, session: project.access.session };
}
export function sameAccessIdentity(access: ProjectAccess, identity: WorkspaceIdentity): boolean {
  return access.projectId === identity.projectId && access.operationNamespace === identity.operationNamespace && access.session === identity.session;
}
export function sameWorkspaceIdentity(project: OpenedProject | null, identity: WorkspaceIdentity): boolean {
  return !!project && project.project.projectId === identity.projectId && sameAccessIdentity(project.access, identity);
}
export function workspaceErrorText(error: unknown): string {
  if (error && typeof error === 'object' && 'detail' in error) return String(error.detail);
  return error instanceof Error ? error.message : String(error);
}

/** Ordinary commands display errors; dialog writes must propagate them. */
export interface WorkspaceOperations {
  perform(work: () => Promise<void>): Promise<void>;
  exclusive<T>(busyMessage: string, work: () => Promise<T>): Promise<T>;
  notice(message: string): void;
  error(message: string): void;
}

export type WorkspacePresentation =
  | { kind: 'activated' | 'selectionChanged' | 'library' | 'hideStoryBible' | 'showStoryBible' }
  | { kind: 'focus'; selector: string; projectId: string | null };

export type ProjectNavigationSource = Readonly<{ projectId: string; title: string; access: ProjectAccess }>;

/** Library commands cannot mutate a mounted session or reproduce its barriers. */
export interface ProjectNavigation {
  currentProject(): ProjectNavigationSource | null;
  replaceProject(prepare: (current: ProjectNavigationSource | null) => Promise<OpenedProject>): Promise<OpenedProject>;
  returnToLibrary(prepare: () => Promise<void>): Promise<void>;
  acceptImported(opened: OpenedProject): void;
}
