import { useEffect, useMemo, useState } from 'react';
import type { DocumentRecord, OpenedProject } from '../ipc/projects';
import type { AssistantDraft } from '../ipc/projectChat';

export type DocumentPanelTab = 'related' | 'drafts' | 'all';

export interface ProjectDocumentsPanelProps {
  project: OpenedProject;
  activeDocument?: DocumentRecord | null;
  drafts?: AssistantDraft[];
  relatedDocumentIds?: string[];
  onOpenDocument(document: DocumentRecord): Promise<void> | void;
  onOpenDraft?(draft: AssistantDraft): Promise<void> | void;
  onAttachSource?(document: DocumentRecord): Promise<void> | void;
  chapterTaskActive?: boolean;
  onCreateChapter?(): Promise<void> | void;
}

function label(document: DocumentRecord): string { return document.title.trim() || `${document.kind} ${document.head.version}`; }
function isOrdinary(document: DocumentRecord): boolean { return (document.role ?? 'ordinary') === 'ordinary'; }
function isChapter(document: DocumentRecord): boolean { return document.kind === 'chapter' && isOrdinary(document); }

export function ProjectDocumentsPanel(props: ProjectDocumentsPanelProps) {
  const preferenceKey = `webnovelstudio.chat-documents.v1:${props.project.project.projectId}:${props.project.access.operationNamespace}`;
  return <DocumentsPanel key={preferenceKey} {...props} preferenceKey={preferenceKey} />;
}

function preferences(key: string): { tab: DocumentPanelTab; query: string } {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? '{}');
    return { tab: value?.tab === 'drafts' || value?.tab === 'all' ? value.tab : 'related', query: typeof value?.query === 'string' ? value.query.slice(0, 200) : '' };
  } catch { return { tab: 'related', query: '' }; }
}

function DocumentsPanel({ project, activeDocument = null, drafts = [], relatedDocumentIds = [], onOpenDocument, onOpenDraft, onAttachSource, chapterTaskActive = false, onCreateChapter, preferenceKey }: ProjectDocumentsPanelProps & { preferenceKey: string }) {
  const [tab, setTab] = useState<DocumentPanelTab>(() => preferences(preferenceKey).tab);
  const [query, setQuery] = useState(() => preferences(preferenceKey).query);
  useEffect(() => {
    try { localStorage.setItem(preferenceKey, JSON.stringify({ tab, query })); } catch { /* Optional navigation preferences. */ }
  }, [preferenceKey, tab, query]);
  const [chapter, setChapter] = useState(activeDocument?.kind === 'chapter' ? activeDocument.head.documentId : '');
  const ordinary = useMemo(() => project.documents.filter(isOrdinary), [project.documents]);
  const all = useMemo(() => ordinary.filter(document => !query.trim() || `${document.title} ${document.kind}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())), [ordinary, query]);
  const related = useMemo(() => {
    const ids = new Set([activeDocument?.head.documentId, ...relatedDocumentIds].filter((value): value is string => !!value));
    const values = ordinary.filter(document => ids.has(document.head.documentId));
    return values.length ? values : activeDocument && isOrdinary(activeDocument) ? [activeDocument] : [];
  }, [activeDocument, ordinary, relatedDocumentIds]);
  const chapters = useMemo(() => ordinary.filter(isChapter), [ordinary]);
  const visible = tab === 'related' ? related : tab === 'drafts' ? drafts.map(draft => draft.document) : all;
  const open = (document: DocumentRecord) => { setChapter(document.kind === 'chapter' ? document.head.documentId : chapter); void onOpenDocument(document); };
  return <aside className="chat-documents-panel" aria-labelledby="chat-documents-heading">
    <div className="chat-documents-heading"><div><h2 id="chat-documents-heading">Documents</h2><p>Browse material without turning categories into steps.</p></div><span>{ordinary.length}</span></div>
    <div className="chat-documents-tabs" role="tablist" aria-label="Project documents">
      {(['related', 'drafts', 'all'] as const).map(value => <button key={value} type="button" role="tab" aria-selected={tab === value} onClick={() => setTab(value)}>{value === 'related' ? 'Related' : value === 'drafts' ? `Drafts to review${drafts.length ? ` (${drafts.length})` : ''}` : 'All documents'}</button>)}
    </div>
    {tab === 'all' && <label className="chat-documents-search">Find documents<input type="search" value={query} maxLength={200} onChange={event => setQuery(event.target.value)} placeholder="Title or category" /></label>}
    {tab === 'drafts' && <p className="chat-documents-note">Assistant drafts stay isolated until adoption.</p>}
    <details className="chat-documents-note"><summary>Chat and draft history</summary><p>Conversation and assistant drafts are author-room material, retained in this project and its backups. They are not automatically supplied to chapter writing or included in manuscript exports.</p><p>Sending a request delivers its permitted context to the selected provider. A fresh provider conversation does not guarantee deletion of data the provider received. Inspect each request to see what was supplied.</p></details>
    <nav className="chat-document-list" aria-label={`${tab} documents`}>
      {visible.length ? visible.map(document => <div className="chat-document-row" data-document-id={document.head.documentId} key={document.head.documentId}><button type="button" className={activeDocument?.head.documentId === document.head.documentId ? 'is-active' : ''} onClick={() => { const draft = drafts.find(item => item.document.head.documentId === document.head.documentId); if (draft && onOpenDraft) void onOpenDraft(draft); else open(document); }}>
        <span>{label(document)}</span><small>{document.kind}{document.role === 'assistantDraft' ? ' · Not adopted' : ` · Working · v${document.head.version}`}</small>
      </button>{isOrdinary(document) && onAttachSource && <button type="button" className="chat-attach-source" aria-label={`${chapterTaskActive ? 'Return to project conversation and use' : 'Use'} ${label(document)} as a source`} onClick={() => void onAttachSource(document)}>{chapterTaskActive ? 'Use in project conversation' : 'Use as source'}</button>}</div>) : <p className="chat-empty-copy">{tab === 'related' ? 'No related material is attached yet.' : tab === 'drafts' ? 'No assistant drafts are waiting.' : 'No documents match this search.'}</p>}
    </nav>
    {tab === 'all' && onCreateChapter && <button type="button" className="chat-create-chapter" onClick={() => void onCreateChapter()}>Create blank chapter</button>}
    <div className="chat-chapter-navigation">
      <div className="chat-panel-heading"><h3>Chapter order</h3><span>{chapters.length}</span></div>
      <label>Jump to chapter<select aria-label="Jump to chapter" value={chapter} onChange={event => { setChapter(event.target.value); const document = chapters.find(item => item.head.documentId === event.target.value); if (document) open(document); }}><option value="">Choose a chapter</option>{chapters.map(document => <option key={document.head.documentId} value={document.head.documentId}>{label(document)}</option>)}</select></label>
    </div>
  </aside>;
}
