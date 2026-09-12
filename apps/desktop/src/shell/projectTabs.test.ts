// @vitest-environment node
import { describe, expect, it } from 'vitest';
import type { DocumentRecord } from '../ipc/projects';
import {
  documentsForTab,
  initialKind,
  initialTab,
  projectTabPreferenceKey,
  readProjectTabs,
  tabForKind,
  writeProjectTabs,
  type ProjectTabStorage,
} from './projectTabs';

function document(documentId: string, kind: string, title = documentId): DocumentRecord {
  return { head: { documentId, version: '0', bodyHash: `${documentId}-hash` }, title, kind, metadataVersion: '0', body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, lastCheckpointId: null };
}

function storage(): ProjectTabStorage & { values: Map<string, string>; failRead?: boolean; failWrite?: boolean } {
  const values = new Map<string, string>();
  return {
    values,
    getItem(key) { if (this.failRead) throw new Error('storage unavailable'); return values.get(key) ?? null; },
    setItem(key, value) { if (this.failWrite) throw new Error('quota exceeded'); values.set(key, value); },
  };
}

describe('project tabs', () => {
  it('keeps private Workshop anchors out of author document tabs', () => {
    const note = document('author-note', 'note', 'Workshop notes');
    expect(documentsForTab([document('workshop-session-1', 'note'), note], 'notes')).toEqual([note]);
  });
  it('maps known kinds and sends unknown future kinds to Notes', () => {
    expect(tabForKind('chapter')).toBe('chapters');
    expect(tabForKind('world')).toBe('worldbuilding');
    expect(tabForKind('character')).toBe('characters');
    expect(tabForKind('theme')).toBe('plot');
    expect(tabForKind('hook')).toBe('plot');
    expect(tabForKind('scene')).toBe('plot');
    expect(tabForKind('note')).toBe('notes');
    expect(tabForKind('future-kind')).toBe('notes');
  });

  it('filters chapters without changing incoming order', () => {
    const documents = [document('w', 'world'), document('c2', 'chapter'), document('c1', 'chapter'), document('n', 'note')];
    expect(documentsForTab(documents, 'chapters').map(item => item.head.documentId)).toEqual(['c2', 'c1']);
  });

  it('uses the first kind for initial state and Chapters for an empty project', () => {
    expect(initialKind([document('w', 'world'), document('c', 'chapter')])).toBe('world');
    expect(initialTab([document('w', 'world')])).toBe('worldbuilding');
    expect(initialKind([])).toBe('chapter');
    expect(initialTab([])).toBe('chapters');
  });

  it('isolates preferences by project and stores only tab and document IDs', () => {
    const target = storage();
    writeProjectTabs('project-a', { activeTab: 'characters', lastDocumentByTab: { characters: 'char-a', chapters: 'chapter-a' } }, target);
    writeProjectTabs('project-b', { activeTab: 'chapters', lastDocumentByTab: { chapters: 'chapter-b' } }, target);
    expect(readProjectTabs('project-a', target)).toEqual({ activeTab: 'characters', lastDocumentByTab: { characters: 'char-a', chapters: 'chapter-a' } });
    expect(readProjectTabs('project-b', target)).toEqual({ activeTab: 'chapters', lastDocumentByTab: { chapters: 'chapter-b' } });
    expect(JSON.parse(target.values.get(projectTabPreferenceKey('project-a'))!)).toEqual({ version: 1, activeTab: 'characters', lastDocumentByTab: { characters: 'char-a', chapters: 'chapter-a' } });
  });

  it('ignores malformed or unavailable storage and invalid preference fields', () => {
    const target = storage();
    target.values.set(projectTabPreferenceKey('broken'), '{not-json');
    expect(readProjectTabs('broken', target)).toEqual({ activeTab: null, lastDocumentByTab: {} });
    target.values.set(projectTabPreferenceKey('invalid'), JSON.stringify({ version: 1, activeTab: 'chapters', lastDocumentByTab: { chapters: '', future: 'future-id', notes: 'note-id' } }));
    expect(readProjectTabs('invalid', target)).toEqual({ activeTab: 'chapters', lastDocumentByTab: { notes: 'note-id' } });
    target.failRead = true;
    expect(readProjectTabs('unavailable', target)).toEqual({ activeTab: null, lastDocumentByTab: {} });
    target.failRead = false; target.failWrite = true;
    expect(() => writeProjectTabs('unavailable', { activeTab: 'notes', lastDocumentByTab: { notes: 'note-id' } }, target)).not.toThrow();
  });
});
