import { useState, type ReactNode } from 'react';
import { BooksIcon } from '@phosphor-icons/react/dist/csr/Books';
import { BookOpenIcon } from '@phosphor-icons/react/dist/csr/BookOpen';
import { ChatCircleIcon } from '@phosphor-icons/react/dist/csr/ChatCircle';
import { GlobeHemisphereWestIcon } from '@phosphor-icons/react/dist/csr/GlobeHemisphereWest';
import { UsersIcon } from '@phosphor-icons/react/dist/csr/Users';
import { LightbulbIcon } from '@phosphor-icons/react/dist/csr/Lightbulb';
import { NotePencilIcon } from '@phosphor-icons/react/dist/csr/NotePencil';
import { PlusIcon } from '@phosphor-icons/react/dist/csr/Plus';
import { ListIcon } from '@phosphor-icons/react/dist/csr/List';
import { PROJECT_TABS, type ProjectTabId } from './projectTabs';
import type { RecentProjectPickerItem } from './RecentProjectPicker';

const materialIcons = { chapters: BookOpenIcon, worldbuilding: GlobeHemisphereWestIcon, characters: UsersIcon, plot: LightbulbIcon, notes: NotePencilIcon };

export function CoauthorSidebar({ current, recent, busy, counts, onLibrary, onNewProject, onOpen, onMaterial, creationForm }: {
  current: RecentProjectPickerItem;
  recent: RecentProjectPickerItem[];
  busy: boolean;
  counts: Record<ProjectTabId, number>;
  onLibrary(): void;
  onNewProject(): void;
  onOpen(path: string): void;
  onMaterial(tab: ProjectTabId): void;
  creationForm?: ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  const others = recent.filter(item => item.projectId !== current.projectId).slice(0, 4);
  return <aside className="coauthor-sidebar" aria-label="Story navigation">
    <button type="button" className="coauthor-sidebar-toggle" aria-expanded={expanded} aria-controls="coauthor-navigation" onClick={() => setExpanded(value => !value)}><ListIcon aria-hidden size={20} /> Projects & story material</button>
    <div id="coauthor-navigation" className={`coauthor-sidebar-body${expanded ? ' is-expanded' : ''}`}>
      <button className="coauthor-library" disabled={busy} onClick={() => { setExpanded(false); onLibrary(); }}><BooksIcon aria-hidden size={20} />Your library</button>
      <button className="coauthor-new-project" disabled={busy} onClick={onNewProject}><PlusIcon aria-hidden size={18} />New project</button>
      {creationForm}
      <nav className="coauthor-projects" aria-label="Your projects">
        <div className="coauthor-current-project" aria-current="page"><BookOpenIcon aria-hidden size={22} /><div><strong>{current.title}</strong><small>Current project</small>{!!current.pendingDrafts && <small>{current.pendingDrafts} draft{current.pendingDrafts === 1 ? '' : 's'} to review</small>}{current.activityLabel && <small>{current.activityLabel}</small>}</div></div>
        {others.map(item => <button className="coauthor-project" key={item.projectId} disabled={busy || item.missing} onClick={() => { setExpanded(false); onOpen(item.path); }}><BookOpenIcon aria-hidden size={21} /><span><strong>{item.title}</strong>{item.missing ? <small>Folder unavailable</small> : <>{item.activityLabel && <small>{item.activityLabel}</small>}{!!item.pendingDrafts && <small>{item.pendingDrafts} draft{item.pendingDrafts === 1 ? '' : 's'} to review</small>}</>}</span></button>)}
      </nav>
      <nav className="coauthor-materials" aria-label="Story material">
        <p>IN THIS PROJECT</p>
        <span className="coauthor-conversation-current" aria-current="page"><ChatCircleIcon aria-hidden size={19} />Conversation</span>
        {[...PROJECT_TABS].sort((a, b) => ['worldbuilding', 'characters', 'chapters', 'plot', 'notes'].indexOf(a.id) - ['worldbuilding', 'characters', 'chapters', 'plot', 'notes'].indexOf(b.id)).map(tab => {
          const Icon = materialIcons[tab.id];
          return <button key={tab.id} disabled={busy} onClick={() => { setExpanded(false); onMaterial(tab.id); }}><Icon aria-hidden size={19} /><span>{tab.label}</span><small aria-label={`${counts[tab.id]} documents`}>{counts[tab.id] || ''}</small></button>;
        })}
      </nav>
      <p className="coauthor-sidebar-note">Build your story in any order.</p>
    </div>
  </aside>;
}
