import { errorTextFor } from '../kernel';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  v2Import,
  v2ImportListProjects,
  v2ImportPreview,
  type V2ChapterBodyChoice,
  type V2ChapterPreview,
  type V2ImportPreview,
  type V2ProjectSummary,
  type V2SourceProjectList,
} from '../ipc/v2Import';
import type { OpenedProject } from '../ipc/projects';

const errorText = errorTextFor('Could not read this V2 database.');
function uncertainImport(error: unknown): boolean {
  if (!error || typeof error !== 'object' || !('code' in error)) return true;
  return ['UncertainOutcome', 'ReconciliationRequired', 'PersistenceUnavailable'].includes(String(error.code));
}
function excerpt(text: string): string {
  const compact = text.replace(/\s+/g, ' ').trim();
  return compact.length > 72 ? `${compact.slice(0, 72)}…` : compact;
}
function missingChapters(preview: V2ImportPreview): V2ChapterPreview[] {
  return preview.chapters.filter(chapter => chapter.workingProse.state === 'missing');
}
function selectedChoiceValue(choice: V2ChapterBodyChoice | undefined): string {
  if (choice === 'empty') return 'empty';
  return choice?.draft.sourceDraftId ?? '';
}

export function V2ImportDialog({ session, onImported, onClose }: {
  session: string;
  onImported(opened: OpenedProject): void;
  onClose(): void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const operationId = useRef(crypto.randomUUID());
  const [source, setSource] = useState<V2SourceProjectList | null>(null);
  const [selectedProjectId, setSelectedProjectId] = useState('');
  const [preview, setPreview] = useState<V2ImportPreview | null>(null);
  const [choices, setChoices] = useState<Record<string, V2ChapterBodyChoice>>({});
  const [title, setTitle] = useState('');
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [retryRequired, setRetryRequired] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  useEffect(() => {
    dialog.current?.showModal();
    let active = true;
    void v2ImportListProjects().then(value => {
      if (!active) return;
      if (!value) { onCloseRef.current(); return; }
      setSource(value);
    }).catch(reason => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; dialog.current?.close(); };
  }, []);

  const selectedProject = useMemo<V2ProjectSummary | null>(() =>
    source?.projects.find(project => project.sourceProjectId === selectedProjectId) ?? null,
  [source, selectedProjectId]);
  const missing = preview ? missingChapters(preview) : [];
  const allChoicesSelected = missing.every(chapter => choices[chapter.sourceId] !== undefined);

  function close() {
    if (working) return;
    dialog.current?.close();
    onClose();
  }
  function chooseProject(projectId: string) {
    setSelectedProjectId(projectId);
    setPreview(null); setChoices({}); setRetryRequired(false); setError(''); setNotice(''); setTitle('');
    operationId.current = crypto.randomUUID();
  }
  async function review() {
    if (!source || !selectedProject) return;
    operationId.current = crypto.randomUUID();
    setWorking(true); setRetryRequired(false); setError(''); setNotice(''); setPreview(null); setChoices({});
    try {
      const result = await v2ImportPreview(source.sourcePath, selectedProject.sourceProjectId);
      setPreview(result.preview);
      setTitle(`Imported ${result.preview.project.title}`);
    } catch (reason) { setError(errorText(reason)); }
    finally { setWorking(false); }
  }
  function setChoice(chapterId: string, value: string) {
    setChoices(current => {
      const next = { ...current };
      if (!value) delete next[chapterId];
      else if (value === 'empty') next[chapterId] = 'empty';
      else next[chapterId] = { draft: { sourceDraftId: value } };
      return next;
    });
  }
  async function importProject() {
    if (!source || !preview || !title.trim() || !allChoicesSelected || working) return;
    setWorking(true); setError(''); setNotice('');
    try {
      const request = {
        operationId: operationId.current,
        sourcePath: source.sourcePath,
        sourceProjectId: preview.project.sourceProjectId,
        title: title.trim(),
        expectedSourceSha256: preview.source.sourceSha256,
        choices: missing.map(chapter => ({ sourceChapterId: chapter.sourceId, choice: choices[chapter.sourceId] })),
      };
      const opened = await v2Import(request, session);
      onImported(opened);
    } catch (reason) {
      setError(errorText(reason));
      if (uncertainImport(reason)) {
        setRetryRequired(true);
        setNotice('The import may already exist. Check import to recover the result before changing your choices.');
      } else {
        setNotice('The import could not finish. Review the project again before retrying.');
      }
    }
    finally { setWorking(false); }
  }

  return <dialog className="export-dialog v2-import-dialog" ref={dialog} aria-labelledby="v2-import-heading" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="export-heading"><div><p className="export-kicker">Library</p><h2 id="v2-import-heading">Import a V2 project</h2></div><button disabled={working} onClick={close}>Close</button></div>
    <p className="export-format-note">Choose a V2 project and review its writing. Where chapter text is missing, choose a saved draft or start with an empty chapter.</p>
    {loading && <p role="status">Reading available V2 projects…</p>}
    {source && <>
      <p className="v2-import-source"><strong>Source database</strong><br /><code>{source.sourcePath}</code></p>
      <label htmlFor="v2-import-project">V2 project</label>
      <select id="v2-import-project" value={selectedProjectId} disabled={working || retryRequired} onChange={event => chooseProject(event.target.value)}>
        <option value="">Choose a project</option>
        {source.projects.map(project => <option key={project.sourceProjectId} value={project.sourceProjectId}>{project.title} · {project.chapterCount} chapters</option>)}
      </select>
      <div className="export-actions"><button disabled={working || retryRequired || !selectedProject} onClick={() => void review()}>Review project</button></div>
    </>}
    {preview && <>
      <div className="v2-import-summary"><strong>{preview.project.title}</strong><span>{preview.chapters.length} chapters</span></div>
      <label htmlFor="v2-import-title">New project title</label>
      <input id="v2-import-title" value={title} maxLength={512} disabled={working || retryRequired} onChange={event => setTitle(event.target.value)} />
      <section className="v2-import-chapters" aria-label="V2 chapter text choices">
        <h3>Chapter text</h3><p className="v2-import-help">Stored working text is copied exactly. Saved drafts and approvals remain available as history.</p>
        {preview.chapters.map(chapter => chapter.workingProse.state === 'present'
          ? <div className="v2-import-row" key={chapter.sourceId}><strong>Chapter {chapter.chapterNumber}: {chapter.title}</strong><span>Stored working text · {chapter.workingProse.text.length} characters</span></div>
          : <label className="v2-import-row" key={chapter.sourceId}><strong>Chapter {chapter.chapterNumber}: {chapter.title}</strong><span>Working text is missing; choose a saved draft or empty text</span><select value={selectedChoiceValue(choices[chapter.sourceId])} disabled={working || retryRequired} onChange={event => setChoice(chapter.sourceId, event.target.value)}><option value="">Choose chapter text</option><option value="empty">Import empty text</option>{chapter.drafts.map(draft => <option key={draft.sourceId} value={draft.sourceId}>Draft v{draft.version}{draft.isApproved ? ' · approved' : ''}{draft.prose ? ` · ${excerpt(draft.prose)}` : ''}</option>)}</select></label>)}
      </section>
      {notice && <p className="export-notice" role="status">{notice}</p>}
      <footer className="export-actions"><button disabled={working} onClick={close}>Cancel</button><button className="primary-button" disabled={working || !title.trim() || !allChoicesSelected} onClick={() => void importProject()}>{working ? 'Importing…' : retryRequired ? 'Check import' : 'Import project'}</button></footer>
    </>}
    {error && <p className="export-error" role="alert">{error}</p>}
  </dialog>;
}
