import { useEffect, useRef, useState } from 'react';
import type { ProjectAccess } from '../ipc/projects';
import { listProjectChatHistory, readHistoricalProjectChat, type HistoricalConversation, type HistoricalConversationSummary } from '../ipc/chatHistory';
import { HistoricalConversation as Transcript } from './HistoricalConversation';

function message(reason: unknown): string {
  return reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail)
    : reason instanceof Error ? reason.message : 'Saved conversation history could not be read.';
}

export function ConversationHistoryPanel({ access, onClose }: { access: ProjectAccess; onClose(): void }) {
  const [conversations, setConversations] = useState<HistoricalConversationSummary[]>([]);
  const [selection, setSelection] = useState<HistoricalConversationSummary | null>(null);
  const [history, setHistory] = useState<HistoricalConversation | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const generation = useRef(0);
  const dialog = useRef<HTMLElement>(null);
  useEffect(() => {
    const origin = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLButtonElement>('button')?.focus();
    return () => { origin?.focus(); generation.current += 1; };
  }, []);
  useEffect(() => {
    const request = ++generation.current;
    setBusy(true); setError('');
    void listProjectChatHistory(access).then(items => {
      if (request === generation.current) setConversations(items);
    }).catch(reason => { if (request === generation.current) setError(message(reason)); })
      .finally(() => { if (request === generation.current) setBusy(false); });
    return () => { generation.current += 1; };
  }, [access.projectId, access.operationNamespace, access.session, access.writerLease]);
  async function open(summary: HistoricalConversationSummary, before: string | null = null) {
    const request = ++generation.current;
    setBusy(true); setError('');
    try {
      const page = await readHistoricalProjectChat(access, summary.conversation, before);
      if (request !== generation.current) return;
      setSelection(summary);
      setHistory(previous => {
        if (!before || previous?.conversation.conversationId !== page.conversation.conversationId) return page;
        const items = new Map([...page.items, ...previous.items].map(item => [item.item.id, item]));
        return { ...page, items: [...items.values()].sort((a, b) => Number(a.item.sequence) - Number(b.item.sequence)) };
      });
    } catch (reason) { if (request === generation.current) setError(message(reason)); }
    finally { if (request === generation.current) setBusy(false); }
  }
  return <div className="chat-history-overlay"><section ref={dialog} className="chat-history-dialog" role="dialog" aria-modal="true" aria-label="Saved conversations" onKeyDown={event => {
    if (event.key === 'Escape') { event.preventDefault(); onClose(); }
    if (event.key === 'Tab') {
      const controls = [...(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], summary, [tabindex="0"]') ?? [])];
      const index = controls.indexOf(document.activeElement as HTMLElement);
      if (event.shiftKey && index <= 0) { event.preventDefault(); controls.at(-1)?.focus(); }
      else if (!event.shiftKey && index === controls.length - 1) { event.preventDefault(); controls[0]?.focus(); }
    }
  }}>
    <header><div><h2>Saved conversations</h2><p>Read original messages and revisions. Recovered history stays separate from current writing.</p></div><button type="button" onClick={onClose}>Return to workspace</button></header>
    {error && <p role="alert">{error}</p>}
    <nav aria-label="Saved conversation selection">{conversations.map(item => <button key={`${item.conversation.projectId}:${item.conversation.conversationId}`} disabled={busy} aria-pressed={selection?.conversation.conversationId === item.conversation.conversationId} onClick={() => void open(item)}>{item.current ? 'Current conversation' : 'Recovered conversation'} · {item.itemCount} items</button>)}</nav>
    {busy && <p role="status">Reading saved history…</p>}
    {!busy && !conversations.length && <p>No project conversations have been saved yet.</p>}
    {history && <Transcript history={history} summary={selection ?? undefined} onLoadOlder={before => !busy && selection ? open(selection, before) : undefined} />}
  </section></div>;
}
