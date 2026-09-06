import type { DocumentRecord } from '../ipc/projects';

export type ProjectTabId = 'chapters' | 'worldbuilding' | 'characters' | 'plot' | 'notes';

export interface ProjectTab {
  id: ProjectTabId;
  label: string;
  initialKind: string;
}

export const PROJECT_TABS: readonly ProjectTab[] = [
  { id: 'chapters', label: 'Chapters', initialKind: 'chapter' },
  { id: 'worldbuilding', label: 'Worldbuilding', initialKind: 'world' },
  { id: 'characters', label: 'Characters', initialKind: 'character' },
  { id: 'plot', label: 'Plot & themes', initialKind: 'theme' },
  { id: 'notes', label: 'Notes', initialKind: 'note' },
];

const TAB_IDS = new Set<ProjectTabId>(PROJECT_TABS.map(tab => tab.id));
const PREFERENCE_VERSION = 1;
const PREFERENCE_PREFIX = 'webnovelstudio.project-tabs.v1:';

export interface ProjectTabsPreferences {
  activeTab: ProjectTabId | null;
  lastDocumentByTab: Partial<Record<ProjectTabId, string>>;
}

/** Backwards-compatible descriptive alias for callers that prefer the longer name. */
export type ProjectTabPreferences = ProjectTabsPreferences;

export interface ProjectTabStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function tabForKind(kind: string): ProjectTabId {
  switch (kind) {
    case 'chapter': return 'chapters';
    case 'world': return 'worldbuilding';
    case 'character': return 'characters';
    case 'theme':
    case 'hook':
    case 'scene': return 'plot';
    case 'note':
    default: return 'notes';
  }
}

export function documentsForTab(documents: readonly DocumentRecord[], tab: ProjectTabId): DocumentRecord[] {
  return documents.filter(document => tabForKind(document.kind) === tab);
}

/** The first document kind is the initial category; a blank project starts in Chapters. */
export function initialKind(documents: readonly Pick<DocumentRecord, 'kind'>[]): string {
  return documents[0]?.kind ?? 'chapter';
}

export function initialTab(documents: readonly Pick<DocumentRecord, 'kind'>[]): ProjectTabId {
  return tabForKind(initialKind(documents));
}

export function projectTabPreferenceKey(projectId: string): string {
  return `${PREFERENCE_PREFIX}${projectId}`;
}

export function emptyProjectTabs(): ProjectTabsPreferences {
  return { activeTab: null, lastDocumentByTab: {} };
}

export const emptyProjectTabPreferences = emptyProjectTabs;

function browserStorage(): ProjectTabStorage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function validTab(value: unknown): value is ProjectTabId {
  return typeof value === 'string' && TAB_IDS.has(value as ProjectTabId);
}

function validDocumentId(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

export function readProjectTabPreferences(
  projectId: string,
  storage: ProjectTabStorage | null = browserStorage(),
): ProjectTabsPreferences {
  const empty = emptyProjectTabs();
  if (!storage || !projectId) return empty;
  try {
    const raw = storage.getItem(projectTabPreferenceKey(projectId));
    if (!raw) return empty;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return empty;
    const value = parsed as { version?: unknown; activeTab?: unknown; lastDocumentByTab?: unknown };
    if (value.version !== PREFERENCE_VERSION || !value.lastDocumentByTab || typeof value.lastDocumentByTab !== 'object') return empty;
    const lastDocumentByTab: Partial<Record<ProjectTabId, string>> = {};
    for (const [tab, documentId] of Object.entries(value.lastDocumentByTab)) {
      if (validTab(tab) && validDocumentId(documentId)) lastDocumentByTab[tab] = documentId;
    }
    return { activeTab: validTab(value.activeTab) ? value.activeTab : null, lastDocumentByTab };
  } catch {
    return empty;
  }
}

export function writeProjectTabPreferences(
  projectId: string,
  preferences: ProjectTabsPreferences,
  storage: ProjectTabStorage | null = browserStorage(),
): void {
  if (!storage || !projectId) return;
  const lastDocumentByTab: Partial<Record<ProjectTabId, string>> = {};
  for (const [tab, documentId] of Object.entries(preferences.lastDocumentByTab)) {
    if (validTab(tab) && validDocumentId(documentId)) lastDocumentByTab[tab] = documentId;
  }
  const payload = JSON.stringify({
    version: PREFERENCE_VERSION,
    activeTab: validTab(preferences.activeTab) ? preferences.activeTab : null,
    lastDocumentByTab,
  });
  try {
    storage.setItem(projectTabPreferenceKey(projectId), payload);
  } catch {
    // Browser storage can be unavailable or quota-limited; tabs remain usable in memory.
  }
}

export function readProjectTabs(projectId: string, storage?: ProjectTabStorage | null): ProjectTabsPreferences {
  return readProjectTabPreferences(projectId, storage);
}

export function writeProjectTabs(projectId: string, preferences: ProjectTabsPreferences, storage?: ProjectTabStorage | null): void {
  writeProjectTabPreferences(projectId, preferences, storage);
}
