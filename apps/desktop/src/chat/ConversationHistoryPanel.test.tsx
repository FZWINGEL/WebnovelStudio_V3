// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { HistoricalConversation, HistoricalConversationItem, HistoricalConversationSummary } from '../ipc/chatHistory';
import type { ProjectAccess } from '../ipc/projects';

const listProjectChatHistory = vi.hoisted(() => vi.fn());
const readHistoricalProjectChat = vi.hoisted(() => vi.fn());
vi.mock('../ipc/chatHistory', async () => {
  const actual = await vi.importActual<typeof import('../ipc/chatHistory')>('../ipc/chatHistory');
  return { ...actual, listProjectChatHistory, readHistoricalProjectChat };
});

import { ConversationHistoryPanel } from './ConversationHistoryPanel';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const currentRef = { projectId: 'project-1', operationNamespace: 'namespace-1', conversationId: 'conversation-current' };
const recoveredRef = { projectId: 'copied-project', operationNamespace: 'copied-namespace', conversationId: 'conversation-recovered' };
const summary: HistoricalConversationSummary = { conversation: recoveredRef, anchorDocumentId: 'anchor-1', itemCount: 3, current: false };

function item(id: string, sequence: string): HistoricalConversationItem {
  return {
    item: { id, sequence, kind: 'request', referenceId: `run-${id}`, payload: {}, createdAt: '2026-09-09T00:00:00.000Z' },
    messages: [{ id: `message-${id}`, threadId: 'thread-1', runId: `run-${id}`, role: 'user', content: `Message ${sequence}`, scope: null, packetId: null, createdAt: '2026-09-09T00:00:00.000Z' }],
    sourceRevisions: [],
    draftRevisions: [],
  };
}

function page(items: HistoricalConversationItem[], olderBefore: string | null): HistoricalConversation {
  return { conversation: recoveredRef, anchorDocumentId: 'anchor-1', items, olderBefore };
}

let host: HTMLDivElement;
let root: Root;
async function settle(): Promise<void> { await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); }); }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  listProjectChatHistory.mockResolvedValue([summary]);
  readHistoricalProjectChat.mockResolvedValue(page([item('item-2', '2'), item('item-3', '3')], 'cursor-older'));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.clearAllMocks(); });

describe('ConversationHistoryPanel', () => {
  it('reads the selected immutable conversation identity and merges an older page', async () => {
    await act(async () => root.render(<ConversationHistoryPanel access={access} onClose={() => {}} />));
    await settle();
    const recovered = Array.from(host.querySelectorAll('nav button')).find(button => button.textContent?.includes('Recovered conversation')) as HTMLButtonElement;
    await act(async () => recovered.click());
    await settle();
    expect(readHistoricalProjectChat).toHaveBeenCalledWith(access, recoveredRef, null);
    expect(host.textContent).toContain('Message 2');
    const older = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Load earlier messages') as HTMLButtonElement;
    readHistoricalProjectChat.mockResolvedValueOnce(page([item('item-1', '1'), item('item-2', '2')], null));
    await act(async () => older.click());
    await settle();
    expect(readHistoricalProjectChat).toHaveBeenLastCalledWith(access, recoveredRef, 'cursor-older');
    expect(host.textContent).toContain('Message 1');
    expect(host.textContent).toContain('Message 3');
    expect(host.textContent).not.toContain('Apply');
    expect(host.textContent).not.toContain('Send');
  });

  it('keeps read failures visible to the author', async () => {
    listProjectChatHistory.mockRejectedValueOnce({ code: 'HistoryUnavailable', detail: 'The copied conversation is unavailable.' });
    await act(async () => root.render(<ConversationHistoryPanel access={access} onClose={() => {}} />));
    await settle();
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('The copied conversation is unavailable.');
  });

  it('closes on Escape and restores focus to the opener', async () => {
    const opener = document.createElement('button'); opener.textContent = 'Open history'; document.body.append(opener); opener.focus();
    const onClose = vi.fn();
    await act(async () => root.render(<ConversationHistoryPanel access={access} onClose={onClose} />));
    await settle();
    expect(document.activeElement?.textContent).toContain('Return to workspace');
    const dialog = host.querySelector('[role="dialog"]') as HTMLElement;
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onClose).toHaveBeenCalledTimes(1);
    await act(async () => root.unmount());
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});
