import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

/** Keep the desktop reference panel alongside the work; protect focus when it overlays it. */
export function WorkshopContextPanel({ children, onClose }: { children: ReactNode; onClose(): void }) {
  const [drawer, setDrawer] = useState(() => window.innerWidth <= 1190);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const resize = () => setDrawer(window.innerWidth <= 1190);
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);
  useEffect(() => {
    if (!drawer) return;
    const element = dialog.current!;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    element.showModal();
    return () => {
      element.close();
      if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
    };
  }, [drawer]);
  function keepTabWithinDrawer(event: KeyboardEvent<HTMLDialogElement>) {
    if (event.key !== 'Tab') return;
    const controls = [...event.currentTarget.querySelectorAll<HTMLElement>('button, input, textarea, select, summary, a[href], [tabindex]')]
      .filter(element => element.tabIndex >= 0 && !element.matches(':disabled') && element.getClientRects().length > 0);
    const first = controls[0], last = controls.at(-1);
    if (!first || !last) { event.preventDefault(); event.currentTarget.focus(); return; }
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }
  const content = <>
    <button className="workshop-context-close" onClick={onClose}>Close working story</button>
    {children}
  </>;
  return drawer
    ? <dialog ref={dialog} className="workshop-context workshop-context-drawer" aria-label="Working story and exploration context"
      onClick={event => { if (event.target === event.currentTarget) onClose(); }}
      onKeyDown={keepTabWithinDrawer} onCancel={event => { event.preventDefault(); onClose(); }}>{content}</dialog>
    : <aside className="workshop-context" aria-label="Working story and exploration context">{content}</aside>;
}
