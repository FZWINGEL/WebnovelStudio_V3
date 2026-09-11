import { errorCode, errorTextFor } from '../kernel';
import { useEffect, useRef, useState } from 'react';
import { bodyHash } from '../editor/document';
import type { DraftExportPreview, DraftFormat } from '../ipc/exports';
import type { ProjectAccess } from '../ipc/projects';

export type ExportBasis = 'working' | 'reviewed';

const errorText = errorTextFor('Could not prepare this export. Try again.');
function definitelyNotWritten(error: unknown): boolean {
  if (!error || typeof error !== 'object' || !('code' in error)) return false;
  const code = String(error.code);
  return ['TargetExists', 'InvalidRequest', 'InvalidExport', 'ExportPreviewMismatch', 'ExportSourceMismatch', 'InvalidDocument', 'DocumentNotFound', 'InvalidPath', 'ExportAlreadyRecorded', 'RevisionNotFound', 'RevisionMismatch', 'RevisionDocumentMismatch', 'WrongProjectSession', 'OperationIdReusedWithDifferentPayload', 'VersionConflict', 'ReviewRequired', 'ReviewStale', 'ReviewSourceMismatch', 'ReviewBundleNotFound', 'ReviewBasisUnavailable', 'ReviewedExportStale'].includes(code);
}
// The shared `errorCode` returns `string | null`; the `?? ''` below is where this
// surface's former local copy used to coerce the absence itself.


/** Preview exact frozen output; no destination is written until the author chooses one. */
export function ExportDialog({ access, documentId, title, isChapter = false, onPrepare, onExport, onClose }: {
  access: ProjectAccess; documentId: string; title: string;
  isChapter?: boolean;
  onPrepare(format: DraftFormat, basis: ExportBasis): Promise<DraftExportPreview>;
  onExport(preview: DraftExportPreview): Promise<string | null>;
  onClose(): void;
}) {
  const [format, setFormat] = useState<DraftFormat>('markdown');
  const [basis, setBasis] = useState<ExportBasis>('working');
  const [preview, setPreview] = useState<DraftExportPreview | null>(null);
  const [loading, setLoading] = useState(true); const [saving, setSaving] = useState(false);
  const [error, setError] = useState(''); const [notice, setNotice] = useState(''); const [exported, setExported] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const dialog = useRef<HTMLDialogElement>(null); const sequence = useRef(0);
  const saveFlight = useRef<{ owner: string; sequence: number } | null>(null);
  const owner = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}/${documentId}`;
  const liveOwner = useRef(owner); liveOwner.current = owner;
  const callbacks = useRef({ onPrepare, onExport, onClose }); callbacks.current = { onPrepare, onExport, onClose };
  useEffect(() => { const element = dialog.current; element?.showModal(); return () => element?.close(); }, []);
  useEffect(() => {
    const current = ++sequence.current;
    setPreview(null); setLoading(true); setSaving(false); setError(''); setNotice(''); setExported(false);
    const owns = () => sequence.current === current && liveOwner.current === owner;
    const requestedBasis: ExportBasis = isChapter && basis === 'reviewed' ? 'reviewed' : 'working';
    void callbacks.current.onPrepare(format, requestedBasis).then(async value => {
      if (!owns()) return;
      if (value.projectId !== access.projectId || value.operationNamespace !== access.operationNamespace || value.sourceHead.documentId !== documentId
        || value.format !== format || value.formatVersion !== 1 || !value.id || !value.revisionId
        || new TextEncoder().encode(value.previewText).length !== value.utf8Bytes || await bodyHash(value.previewText) !== value.sha256) {
        throw new Error('The export preview did not match the selected document and format. Prepare it again.');
      }
      const hasReviewBundle = typeof value.reviewBundleId === 'string' && value.reviewBundleId.trim().length > 0;
      if ((requestedBasis === 'reviewed' && !hasReviewBundle) || (requestedBasis === 'working' && value.reviewBundleId !== undefined)) {
        throw new Error(requestedBasis === 'reviewed'
          ? 'The author-reviewed preview did not include its review bundle. Prepare it again.'
          : 'The working-draft preview unexpectedly included review authority. Prepare it again.');
      }
      if (owns()) setPreview(value);
    }).catch(reason => { if (owns()) setError(errorText(reason)); })
      .finally(() => { if (owns()) setLoading(false); });
    return () => { ++sequence.current; };
  }, [owner, format, basis, isChapter, refresh]);
  function close() {
    if (saving) return;
    // Release the modal's inert background before the parent restores focus.
    dialog.current?.close();
    callbacks.current.onClose();
  }
  async function save() {
    if (!preview || saving || (saveFlight.current?.owner === owner && saveFlight.current.sequence === sequence.current) || loading || exported) return;
    const flight = { owner, sequence: sequence.current }; saveFlight.current = flight;
    const current = sequence.current; const owns = () => current === sequence.current && liveOwner.current === owner;
    setSaving(true); setError(''); setNotice('');
    try {
      const path = await callbacks.current.onExport(preview);
      if (!owns()) return;
      if (path) { setExported(true); setNotice(`${preview.reviewBundleId ? 'Author-reviewed snapshot exported' : 'Draft exported'}: ${path}`); }
      else setNotice('No destination chosen. Your preview is still ready.');
    } catch (reason) {
      if (!owns()) return;
      setError(errorText(reason));
      if (!['TargetExists', 'InvalidPath'].includes(errorCode(reason) ?? '')) {
        setPreview(null);
        if (errorCode(reason) === 'ExportAlreadyRecorded') setNotice('This preview was already exported. Prepare another preview if you want an additional copy.');
        else if (!definitelyNotWritten(reason)) setNotice('A file may already have been created. Check the chosen destination before preparing another export.');
      }
    } finally { if (saveFlight.current === flight) saveFlight.current = null; if (owns()) setSaving(false); }
  }
  const reviewed = isChapter && basis === 'reviewed';
  return <dialog className="export-dialog" ref={dialog} aria-labelledby="export-heading" onCancel={event => {
    event.preventDefault(); close();
  }}>
    <div className="export-heading"><div><p className="export-kicker">{reviewed ? 'Author-reviewed snapshot' : 'Working draft'}</p><h2 id="export-heading">{reviewed ? 'Export author-reviewed chapter' : 'Export draft'}</h2></div><button disabled={saving} onClick={close} aria-label="Close export">Close</button></div>
    <p className="export-document">{title}</p><p className="export-format-note">{reviewed ? 'Save the exact author-reviewed chapter snapshot. This does not publish or change the manuscript.' : 'Export this document to a new file. Existing files are kept.'}</p>
    {isChapter && <fieldset className="export-basis" disabled={saving}><legend>Export basis</legend><label><input type="radio" name="export-basis" value="working" checked={basis === 'working'} onChange={() => setBasis('working')} /> Working draft</label><label><input type="radio" name="export-basis" value="reviewed" checked={basis === 'reviewed'} onChange={() => setBasis('reviewed')} /> Author-reviewed snapshot</label></fieldset>}
    <label htmlFor="export-format">File format</label>
    <select autoFocus id="export-format" value={format} disabled={saving} onChange={event => setFormat(event.target.value as DraftFormat)}>
      <option value="markdown">Markdown (.md)</option><option value="plainText">Plain text (.txt)</option>
    </select>
    {preview && <><p className="export-format-note">{preview.formatLoss}</p><div className="export-preview-heading"><h3>File preview</h3><span>Saved version {preview.sourceHead.version} · {new Intl.NumberFormat().format(preview.utf8Bytes)} bytes</span></div>
      <pre className="export-preview" tabIndex={0} aria-label="Exported file preview">{preview.previewText}</pre>{!preview.previewText && <p className="export-format-note">The exported file will be empty.</p>}</>}
    {loading && <p role="status">Preparing saved writing…</p>}
    {error && <p className="export-error" role="alert">{error}</p>}
    {notice && <p className="export-notice" role="status">{notice}</p>}
    <footer className="export-actions"><button disabled={loading || saving} onClick={() => setRefresh(value => value + 1)}>{preview ? 'Refresh preview' : 'Prepare export again'}</button>
      {exported ? <button className="primary-button" onClick={close}>Done</button>
        : <button className="primary-button" disabled={loading || saving || !preview} onClick={() => void save()}>{saving ? (reviewed ? 'Saving snapshot…' : 'Saving draft…') : 'Choose destination…'}</button>}</footer>
  </dialog>;
}
