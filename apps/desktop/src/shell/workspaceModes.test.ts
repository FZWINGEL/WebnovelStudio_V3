import { describe, expect, it } from 'vitest';
import {
  readWorkspaceMode,
  workspaceModePreferenceKey,
  writeWorkspaceMode,
  type WorkspaceModeStorage,
} from './workspaceModes';

function storage(): WorkspaceModeStorage & { values: Map<string, string>; failRead?: boolean; failWrite?: boolean } {
  const values = new Map<string, string>();
  return {
    values,
    getItem(key) { if (this.failRead) throw new Error('storage unavailable'); return values.get(key) ?? null; },
    setItem(key, value) { if (this.failWrite) throw new Error('quota exceeded'); values.set(key, value); },
  };
}

describe('workspace mode preferences', () => {
  it('stores develop/write choices per project without story data', () => {
    const target = storage();
    writeWorkspaceMode('project-a', 'develop', target);
    writeWorkspaceMode('project-b', 'write', target);

    expect(readWorkspaceMode('project-a', target)).toBe('develop');
    expect(readWorkspaceMode('project-b', target)).toBe('write');
    expect(JSON.parse(target.values.get(workspaceModePreferenceKey('project-a'))!)).toEqual({ version: 1, mode: 'develop' });
  });

  it('ignores malformed, stale, and unsupported preferences', () => {
    const target = storage();
    target.values.set(workspaceModePreferenceKey('broken'), '{not-json');
    target.values.set(workspaceModePreferenceKey('stale'), JSON.stringify({ version: 0, mode: 'develop' }));
    target.values.set(workspaceModePreferenceKey('invalid'), JSON.stringify({ version: 1, mode: 'preview' }));

    expect(readWorkspaceMode('broken', target)).toBeNull();
    expect(readWorkspaceMode('stale', target)).toBeNull();
    expect(readWorkspaceMode('invalid', target)).toBeNull();
  });

  it('keeps the shell usable when browser storage is unavailable', () => {
    const target = storage();
    target.failRead = true;
    expect(readWorkspaceMode('project', target)).toBeNull();
    target.failRead = false;
    target.failWrite = true;
    expect(() => writeWorkspaceMode('project', 'write', target)).not.toThrow();
  });
});
