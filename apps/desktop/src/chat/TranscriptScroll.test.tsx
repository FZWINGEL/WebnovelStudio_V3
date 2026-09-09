// @vitest-environment jsdom
import { act, useEffect, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { NewReplyAffordance, useTranscriptScroll } from './TranscriptScroll';

interface HarnessProps {
  ids: string[];
  replyKeys: string[];
  olderLoading: boolean;
  onState?: (state: { unseen: number }) => void;
}

function Harness({ ids, replyKeys, olderLoading, onState }: HarnessProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const state = useTranscriptScroll({ containerRef, itemCount: ids.length, replyKeys, latestReplyId: ids.at(-1), olderLoading });
  useEffect(() => { onState?.({ unseen: state.unseenReplyCount }); }, [onState, state.unseenReplyCount]);
  return <>
    <div className="chat-transcript-shell">
      <div ref={containerRef} className="chat-transcript" aria-label="Project conversation">
        {ids.map(id => <article key={id} data-conversation-item-id={id}><span data-assistant-reply-id={id}>{id}</span></article>)}
      </div>
      <NewReplyAffordance count={state.unseenReplyCount} onJump={state.jumpToLatest} />
    </div>
    <input aria-label="Composer" />
  </>;
}

function ActualReplyHarness({ replyKeys, onState }: { replyKeys: string[]; onState?: (state: { unseen: number }) => void }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const state = useTranscriptScroll({ containerRef, itemCount: 2, replyKeys, latestReplyId: 'reply-2', olderLoading: false });
  useEffect(() => { onState?.({ unseen: state.unseenReplyCount }); }, [onState, state.unseenReplyCount]);
  return <>
    <div className="chat-transcript-shell">
      <div ref={containerRef} className="chat-transcript" aria-label="Project conversation">
        <article data-conversation-item-id="turn-1"><p data-user-part="true">Earlier request</p><p data-assistant-reply-id="reply-1">Earlier answer</p></article>
        <article data-conversation-item-id="turn-2"><p data-user-part="true">New request</p><p data-assistant-reply-id="reply-2">New answer</p></article>
      </div>
      <NewReplyAffordance count={state.unseenReplyCount} onJump={state.jumpToLatest} />
    </div>
    <input aria-label="Composer" />
  </>;
}

let host: HTMLDivElement;
let root: Root;

function setMetrics(element: HTMLElement, metrics: { scrollHeight: number; clientHeight: number; top?: number; bottom?: number }) {
  Object.defineProperty(element, 'scrollHeight', { configurable: true, get: () => metrics.scrollHeight });
  Object.defineProperty(element, 'clientHeight', { configurable: true, get: () => metrics.clientHeight });
  element.getBoundingClientRect = () => ({ top: metrics.top ?? 0, bottom: metrics.bottom ?? metrics.clientHeight, left: 0, right: 100, width: 100, height: metrics.clientHeight, x: 0, y: metrics.top ?? 0, toJSON: () => ({}) });
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('transcript scroll behavior', () => {
  it('preserves the visible reading position when older content is prepended', async () => {
    await act(async () => root.render(<Harness ids={['current-1', 'current-2']} replyKeys={['current-2:completed']} olderLoading={false} />));
    const transcript = host.querySelector('.chat-transcript') as HTMLDivElement;
    const metrics = { scrollHeight: 200, clientHeight: 100 };
    setMetrics(transcript, metrics);
    transcript.scrollTop = 50;
    await act(async () => root.render(<Harness ids={['current-1', 'current-2']} replyKeys={['current-2:completed']} olderLoading />));
    metrics.scrollHeight = 420;
    await act(async () => root.render(<Harness ids={['old-1', 'old-2', 'current-1', 'current-2']} replyKeys={['old-1:completed', 'current-2:completed']} olderLoading={false} />));
    // The layout effect applies before the assertion; keep the metric getter
    // stable so the compensation is deterministic in jsdom.
    expect(transcript.scrollTop).toBe(270);
    expect(host.querySelector('.chat-transcript-new-reply')).toBeNull();
  });

  it('shows an accessible new-reply action without moving scroll or stealing composer focus', async () => {
    await act(async () => root.render(<Harness ids={['reply-1', 'reply-2']} replyKeys={['reply-1:completed']} olderLoading={false} />));
    const transcript = host.querySelector('.chat-transcript') as HTMLDivElement;
    setMetrics(transcript, { scrollHeight: 300, clientHeight: 100 });
    transcript.scrollTop = 20;
    const first = transcript.querySelector('[data-conversation-item-id="reply-1"]') as HTMLElement;
    first.getBoundingClientRect = () => ({ top: 10, bottom: 60, left: 0, right: 100, width: 100, height: 50, x: 0, y: 10, toJSON: () => ({}) });
    const latest = transcript.querySelector('[data-conversation-item-id="reply-2"]') as HTMLElement;
    latest.getBoundingClientRect = () => ({ top: 120, bottom: 180, left: 0, right: 100, width: 100, height: 60, x: 0, y: 120, toJSON: () => ({}) });
    const composer = host.querySelector('[aria-label="Composer"]') as HTMLInputElement;
    composer.focus();
    await act(async () => root.render(<Harness ids={['reply-1', 'reply-2']} replyKeys={['reply-1:completed', 'reply-2:completed']} olderLoading={false} />));
    const button = host.querySelector('.chat-transcript-new-reply') as HTMLButtonElement;
    expect(button).not.toBeNull();
    expect(transcript.querySelector('.chat-transcript-new-reply')).toBeNull();
    expect(button.parentElement?.classList.contains('chat-transcript-shell')).toBe(true);
    expect(button.getAttribute('aria-label')).toBe('1 new reply; jump to latest');
    expect(transcript.scrollTop).toBe(20);
    expect(document.activeElement).toBe(composer);
    await act(async () => button.click());
    expect(transcript.scrollTop).toBe(300);
    expect(host.querySelector('.chat-transcript-new-reply')).toBeNull();
  });

  it('checks the assistant marker instead of the visible user part of a turn', async () => {
    await act(async () => root.render(<ActualReplyHarness replyKeys={['reply-1:completed']} />));
    const transcript = host.querySelector('.chat-transcript') as HTMLDivElement;
    setMetrics(transcript, { scrollHeight: 300, clientHeight: 100 });
    transcript.scrollTop = 15;
    const userPart = transcript.querySelector('[data-user-part="true"]') as HTMLElement;
    userPart.getBoundingClientRect = () => ({ top: 10, bottom: 45, left: 0, right: 100, width: 100, height: 35, x: 0, y: 10, toJSON: () => ({}) });
    const assistant = transcript.querySelector('[data-assistant-reply-id="reply-2"]') as HTMLElement;
    assistant.getBoundingClientRect = () => ({ top: 130, bottom: 170, left: 0, right: 100, width: 100, height: 40, x: 0, y: 130, toJSON: () => ({}) });
    const composer = host.querySelector('[aria-label="Composer"]') as HTMLInputElement;
    composer.focus();
    await act(async () => root.render(<ActualReplyHarness replyKeys={['reply-1:completed', 'reply-2:completed']} />));
    expect(host.querySelector('.chat-transcript-new-reply')?.textContent).toContain('New reply');
    expect(document.activeElement).toBe(composer);
    expect(transcript.scrollTop).toBe(15);
  });
});
