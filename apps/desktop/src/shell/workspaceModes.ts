export type WorkspaceMode = 'develop' | 'write';

const PREFERENCE_VERSION = 1;
const PREFERENCE_PREFIX = 'webnovelstudio.workspace-mode.v1:';

export interface WorkspaceModeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function browserStorage(): WorkspaceModeStorage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

export function workspaceModePreferenceKey(projectId: string): string {
  return `${PREFERENCE_PREFIX}${projectId}`;
}

function validMode(value: unknown): value is WorkspaceMode {
  return value === 'develop' || value === 'write';
}

/**
 * Read only the author's local shell choice. Story material and project data
 * never cross this UI preference boundary.
 */
export function readWorkspaceMode(
  projectId: string,
  storage: WorkspaceModeStorage | null = browserStorage(),
): WorkspaceMode | null {
  if (!storage || !projectId) return null;
  try {
    const raw = storage.getItem(workspaceModePreferenceKey(projectId));
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return null;
    const value = parsed as { version?: unknown; mode?: unknown };
    return value.version === PREFERENCE_VERSION && validMode(value.mode) ? value.mode : null;
  } catch {
    return null;
  }
}

export function writeWorkspaceMode(
  projectId: string,
  mode: WorkspaceMode,
  storage: WorkspaceModeStorage | null = browserStorage(),
): void {
  if (!storage || !projectId || !validMode(mode)) return;
  try {
    storage.setItem(workspaceModePreferenceKey(projectId), JSON.stringify({ version: PREFERENCE_VERSION, mode }));
  } catch {
    // A blocked or full browser store should not make the workspace unusable.
  }
}
