import { useEffect, useRef } from 'react';
import type { AppCloseStatus } from '../ipc/appClose';

export type AppCloseDialogPhase = 'waiting' | 'stopping' | 'blocked' | 'error';

export function AppCloseDialog({ phase, message, onStop, onStayOpen }: {
  phase: AppCloseDialogPhase;
  status: AppCloseStatus | null;
  message: string;
  onStop(): void;
  onStayOpen(): void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
    return () => element?.close();
  }, []);
  const stopping = phase === 'stopping';
  return <dialog className="model-dialog app-close-dialog" ref={dialog} aria-labelledby="app-close-heading" onCancel={event => { event.preventDefault(); onStayOpen(); }}>
    <div className="provider-dialog-heading"><div><p className="export-kicker">Before closing</p><h2 id="app-close-heading">Finish closing WebnovelStudio?</h2></div></div>
    <p>{message}</p>
    <footer className="export-actions">
      {phase === 'waiting' && <button type="button" className="primary-button" autoFocus onClick={onStop}>Stop replies and close</button>}
      {stopping && <span role="status">Stopping local work…</span>}
      <button type="button" onClick={onStayOpen}>Stay open</button>
    </footer>
  </dialog>;
}
