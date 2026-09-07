// @vitest-environment jsdom
import { it, expect } from 'vitest';
import { readProjectTabs, writeProjectTabs, projectTabPreferenceKey } from './projectTabs';

it('uses browser localStorage when no storage adapter is injected', () => {
  const project = 'browser-storage-fixture';
  try {
    const preferences = { activeTab: 'notes' as const, lastDocumentByTab: { notes: 'note-1' } };
    writeProjectTabs(project, preferences);
    expect(window.localStorage.getItem(projectTabPreferenceKey(project))).not.toBeNull();
    expect(readProjectTabs(project)).toEqual(preferences);
  } finally { window.localStorage.removeItem(projectTabPreferenceKey(project)); }
});
