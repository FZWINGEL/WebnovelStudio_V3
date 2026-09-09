import { useRef, type KeyboardEvent } from 'react';
import type { LibraryEntry } from '../ipc/library';

export interface RecentProjectPickerItem extends Pick<LibraryEntry, 'projectId' | 'title' | 'path' | 'lastOpened' | 'missing' | 'archived'> {
  current?: boolean;
  /** Optional read-only status supplied by an already authenticated owner. */
  activityLabel?: string;
  /** Optional pending-draft count from an authenticated conversation projection. */
  pendingDrafts?: number;
}

export interface RecentProjectPickerProps {
  current?: RecentProjectPickerItem | null;
  recent: RecentProjectPickerItem[];
  disabled?: boolean;
  onOpen(path: string): void;
  onVisibilityChange?(visible: boolean): void;
}

function ProjectBadges({ item }: { item: RecentProjectPickerItem }) {
  return <span className="project-picker-badges" aria-label="Project status">
    {item.current && <span className="project-picker-badge">Current</span>}
    {item.activityLabel && <span className="project-picker-badge">{item.activityLabel}</span>}
    {!!item.pendingDrafts && <span className="project-picker-badge">{item.pendingDrafts} draft{item.pendingDrafts === 1 ? '' : 's'} to review</span>}
  </span>;
}

function ProjectButton({ item, disabled, onOpen }: { item: RecentProjectPickerItem; disabled: boolean; onOpen: (path: string) => void }) {
  return <button type="button" className="project-picker-item" disabled={disabled || item.missing} onClick={() => onOpen(item.path)}>
    <span className="project-picker-item-copy"><strong>{item.title}</strong><small>{item.missing ? 'Folder moved or unavailable' : item.archived ? 'Archived' : 'Open project'}</small></span>
    <ProjectBadges item={item} />
  </button>;
}

/**
 * A read-only projection of the known library catalog. It never opens a
 * project to discover badges; an explicit author click is the only operation
 * that calls the Workspace open callback. Optional activity/draft badges must
 * come from an already authenticated projection and are otherwise omitted.
 */
export function RecentProjectPicker({ current = null, recent, disabled = false, onOpen, onVisibilityChange }: RecentProjectPickerProps) {
  const pickerRef = useRef<HTMLDetailsElement>(null);
  const summaryRef = useRef<HTMLElement>(null);
  const otherProjects = recent.filter(item => item.projectId !== current?.projectId).slice(0, 8);
  const closePicker = () => {
    const picker = pickerRef.current;
    if (!picker) return;
    picker.open = false;
    onVisibilityChange?.(false);
    summaryRef.current?.focus();
  };
  const selectProject = (path: string) => {
    // Fence the menu before starting navigation. If opening fails, Workspace's
    // global error remains visible while the author keeps a stable focus target.
    closePicker();
    onOpen(path);
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLDetailsElement>) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    closePicker();
  };
  return <details ref={pickerRef} className="project-tools recent-project-picker" onKeyDown={handleKeyDown} onToggle={event => onVisibilityChange?.(event.currentTarget.open)}>
    <summary ref={summaryRef} aria-label={current ? `Current project: ${current.title}. Open project switcher` : 'Choose a project'}>
      <span>Projects</span><small>{current ? 'Switch project' : 'Recent projects'}</small>
    </summary>
    <div className="project-tools-menu recent-project-picker-menu">
      {current && <div className="project-picker-current" aria-label="Current project">
        <strong>{current.title}</strong>
        <ProjectBadges item={{ ...current, current: true }} />
      </div>}
      {otherProjects.length ? <nav aria-label="Recent projects">{otherProjects.map(item => <ProjectButton key={item.projectId} item={item} disabled={disabled} onOpen={selectProject} />)}</nav> : <p className="small-copy">No other projects are available yet.</p>}
    </div>
  </details>;
}
