import type { HistoricalConversation, HistoricalConversationItem, HistoricalConversationSummary } from '../ipc/chatHistory';

export interface HistoricalConversationProps {
  history: HistoricalConversation;
  summary?: HistoricalConversationSummary;
  onLoadOlder?: (before: string) => Promise<void> | void;
}

function itemLabel(item: HistoricalConversationItem): string {
  switch (item.item.kind) {
    case 'request': return 'Project discussion';
    case 'chapterRequest': return 'Chapter request';
    case 'materializeChatResult': return 'Saved assistant result';
    case 'saveAssistantDraft': return 'Draft revision';
    default: return item.item.kind;
  }
}

function readableBody(value: unknown): string {
  const text: string[] = [];
  const visit = (node: unknown) => {
    if (!node || typeof node !== 'object') return;
    const record = node as Record<string, unknown>;
    if (typeof record.text === 'string') text.push(record.text);
    if (Array.isArray(record.content)) record.content.forEach(visit);
  };
  const body = value && typeof value === 'object' && 'body' in value ? (value as { body: unknown }).body : value;
  if (body && typeof body === 'object' && 'content' in body && Array.isArray((body as { content: unknown }).content)) {
    for (const block of (body as { content: unknown[] }).content) { visit(block); text.push('\n\n'); }
  } else visit(body);
  return text.length ? text.join('') : JSON.stringify(value, null, 2);
}

function messageText(content: string, role: 'user' | 'assistant'): string {
  if (role !== 'assistant') return content;
  try {
    const value: unknown = JSON.parse(content);
    if (value && typeof value === 'object') {
      const record = value as Record<string, unknown>;
      if (typeof record.answer === 'string') return record.answer;
      if (typeof record.text === 'string') return record.text;
    }
  } catch { /* Plain provider output. */ }
  return content;
}

/**
 * Read-only view for retained project-chat evidence. It intentionally has no
 * composer, Apply, adoption, retry, or resume affordance: historical rows are
 * evidence, while new work goes through the current conversation.
 */
export function HistoricalConversation({ history, summary, onLoadOlder }: HistoricalConversationProps) {
  const current = summary?.current ?? false;
  return <section className="historical-conversation" aria-label="Historical conversation">
    <header className="historical-conversation__header">
      <div>
        <p className="chat-muted">{current ? 'Current conversation' : 'Retained conversation'}</p>
        <h2>{current ? 'Project chat' : 'Recovered project chat'}</h2>
      </div>
      <p className="chat-muted">{summary ? `${summary.itemCount} retained items` : 'Read-only retained evidence'}</p>
    </header>
    {history.olderBefore && onLoadOlder && <button type="button" onClick={() => void onLoadOlder(history.olderBefore!)}>
      Load earlier messages
    </button>}
    <div className="historical-conversation__items">
      {history.items.length === 0 && <p className="chat-muted">No retained items in this page.</p>}
      {history.items.map(({ item, run, messages = [], sourceRevisions = [], draftRevisions = [] }) => <article key={item.id} className="historical-conversation__item">
        <header>
          <strong>{itemLabel({ item, run, messages, sourceRevisions, draftRevisions })}</strong>
          <span className="chat-muted">Sequence {item.sequence}</span>
        </header>
        {messages.map(message => <div key={message.id} className={`historical-conversation__message historical-conversation__message--${message.role}`}>
          <strong>{message.role === 'user' ? 'You' : 'Assistant'}</strong>
          <p>{messageText(message.content, message.role)}</p>
        </div>)}
        {run && <p className="chat-muted">Request {run.status}</p>}
        {sourceRevisions.length > 0 && <details><summary>Original source revisions ({sourceRevisions.length})</summary><ul>{sourceRevisions.map(source => <li key={`${source.handle}:${source.revision.id}`}><strong>{source.handle}</strong> · {source.revision.head.version}<pre>{readableBody(source.revision.body)}</pre></li>)}</ul></details>}
        {draftRevisions.length > 0 && <details><summary>Original draft revisions ({draftRevisions.length})</summary><ul>{draftRevisions.map(draft => <li key={`${draft.documentId}:${draft.revision.id}`}><strong>{draft.documentId}</strong> · {draft.initial ? 'original generated draft' : 'saved checkpoint'} · {draft.revision.head.version}<pre>{readableBody(draft.revision.body)}</pre></li>)}</ul></details>}
      </article>)}
    </div>
  </section>;
}
