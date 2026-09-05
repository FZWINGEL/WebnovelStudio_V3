import { useEffect, useRef, useState } from 'react';
import { bodyHash } from '../editor/document';
import type { DraftExportPreview, DraftFormat } from '../ipc/exports';
import type { ProjectAccess } from '../ipc/projects';

function errorText(error: unknown): string {
  return error && typeof error === 'object' && 'detail' in error ? String(error.detail)
    : error instanceof Error ? error.message : 'Could not prepare this export. Try again.';
}
function definitelyNotWritten(error: unknown): boolean {
  return !!error && typeof error === 'object' && 'code' in error
    && ['TargetExists', 'InvalidRequest', 'InvalidExport', 'ExportPreviewMismatch', 'ExportSourceMismatch', 'InvalidDocument', 'DocumentNotFound', 'InvalidPath', 'ExportAlreadyRecorded', 'RevisionNotFound', 'RevisionMismatch', 'RevisionDocumentMismatch', 'WrongProjectSession', 'OperationIdReusedWithDifferentPayload', 'VersionConflict'].includes(String(error.code));
}
function errorCode(error: unknown): string { return error && typeof error === 'object' && 'code' in error ? String(error.code) : ''; }

/** Preview exact frozen output; no destination is written until the author chooses one. */
export function ExportDialog({ access, documentId, title, onPrepare, onExport, onClose }: {
  access: ProjectAccess; documentId: string; title: string;
  onPrepare(format: DraftFormat): Promise<DraftExportPreview>;
  onExport(preview: DraftExportPreview): Promise<string | null>;
  onClose(): void;
}) {
  const [format, setFormat] = useState<DraftFormat>('markdown');
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
    void callbacks.current.onPrepare(format).then(async value => {
      if (!owns()) return;
      if (value.projectId !== access.projectId || value.operationNamespace !== access.operationNamespace || value.sourceHead.documentId !== documentId
        || value.format !== format || value.formatVersion !== 1 || !value.id || !value.revisionId
        || new TextEncoder().encode(value.previewText).length !== value.utf8Bytes || await bodyHash(value.previewText) !== value.sha256) {
        throw new Error('The export preview did not match the selected document and format. Prepare it again.');
      }
      if (owns()) setPreview(value);
    }).catch(reason => { if (owns()) setError(errorText(reason)); })
      .finally(() => { if (owns()) setLoading(false); });
    return () => { ++sequence.current; };
  }, [owner, format, refresh]);
  async function save() {
    if (!preview || saving || (saveFlight.current?.owner === owner && saveFlight.current.sequence === sequence.current) || loading || exported) return;
    const flight = { owner, sequence: sequence.current }; saveFlight.current = flight;
    const current = sequence.current; const owns = () => current === sequence.current && liveOwner.current === owner;
    setSaving(true); setError(''); setNotice('');
    try {
      const path = await callbacks.current.onExport(preview);
      if (!owns()) return;
      if (path) { setExported(true); setNotice(`Draft exported: ${path}`); }
      else setNotice('No destination chosen. Your preview is still ready.');
    } catch (reason) {
      if (!owns()) return;
      setError(errorText(reason));
      if (!['TargetExists', 'InvalidPath'].includes(errorCode(reason))) {
        setPreview(null);
        if (errorCode(reason) === 'ExportAlreadyRecorded') setNotice('This preview was already exported. Prepare another preview if you want an additional copy.');
        else if (!definitelyNotWritten(reason)) setNotice('A file may already have been created. Check the chosen destination before preparing another export.');
      }
    } finally { if (saveFlight.current === flight) saveFlight.current = null; if (owns()) setSaving(false); }
  }
  return <dialog className="export-dialog" ref={dialog} aria-labelledby="export-heading" onCancel={event => {
    event.preventDefault(); if (!saving) callbacks.current.onClose();
  }}>
    <div className="export-heading"><div><p className="export-kicker">Working draft</p><h2 id="export-heading">Export draft</h2></div><button disabled={saving} onClick={onClose} aria-label="Close export">Close</button></div>
    <p className="export-document">{title}</p><p className="export-format-note">Export this document to a new file. Existing files are kept.</p>
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
      {exported ? <button className="primary-button" onClick={onClose}>Done</button>
        : <button className="primary-button" disabled={loading || saving || !preview} onClick={() => void save()}>{saving ? 'Saving draft…' : 'Choose destination…'}</button>}</footer>
  </dialog>;
}
