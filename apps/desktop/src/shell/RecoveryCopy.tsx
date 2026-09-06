import { useEffect, useRef, useState } from 'react';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { saveRecoveryCopy } from '../ipc/recovery';

export function RecoveryCopy({ capture }: { capture: () => WnsDocument }) {
  const pending = useRef(false);
  const mounted = useRef(true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  async function save() {
    if (pending.current) return;
    pending.current = true; setBusy(true); setMessage('');
    try {
      // Capture before any await. Later typing is allowed but cannot change
      // the copy while the author chooses a destination in the native dialog.
      const body = structuredClone(capture());
      const expectedHash = await bodyHash(canonicalJson(body));
      const result = await saveRecoveryCopy(body);
      if (!mounted.current) return;
      if (!result) { setMessage('Recovery copy cancelled. Your text remains in the editor.'); return; }
      if (result.snapshotHash !== expectedHash || !result.path) throw new Error('The response did not match the captured text.');
      setMessage(`Recovery copy saved: ${result.path}. It contains the text captured when you clicked. This does not save the project.`);
    } catch (error) {
      if (!mounted.current) return;
      const detail = error && typeof error === 'object' && 'detail' in error ? String(error.detail) : error instanceof Error ? error.message : 'The result is unknown.';
      setMessage(`Could not confirm a recovery copy. ${detail} Check the chosen folder before saving another copy. Your text remains in the editor.`);
    } finally {
      pending.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  return <div className="recovery-copy"><button disabled={busy} onClick={() => void save()}>{busy ? 'Saving recovery copy…' : 'Save recovery copy…'}</button><p>Save this editor text as a new Markdown file in a folder you choose. Includes supported formatting; excludes other documents and discussion.</p>{message && <p role="status">{message}</p>}</div>;
}
