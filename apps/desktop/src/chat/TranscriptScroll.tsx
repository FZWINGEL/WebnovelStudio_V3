import { useCallback, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import './TranscriptScroll.css';

export interface TranscriptScrollOptions {
  containerRef: RefObject<HTMLDivElement | null>;
  itemCount: number;
  replyKeys: string[];
  latestReplyId?: string | null;
  olderLoading: boolean;
}

export interface TranscriptScrollState {
  unseenReplyCount: number;
  jumpToLatest(): void;
}

interface PrependMeasurement {
  itemCount: number;
  scrollHeight: number;
  scrollTop: number;
}

function visibleInContainer(container: HTMLDivElement, replyId: string | null | undefined): boolean {
  if (!replyId) return false;
  const target = [...container.querySelectorAll<HTMLElement>('[data-assistant-reply-id]')]
    .find(item => item.dataset.assistantReplyId === replyId);
  if (!target) return false;
  const containerRect = container.getBoundingClientRect();
  const targetRect = target.getBoundingClientRect();
  return targetRect.bottom > containerRect.top && targetRect.top < containerRect.bottom;
}

/**
 * Keeps the visible reading position stable while an older page is prepended
 * and reports replies that arrive outside the current viewport. It never
 * changes scroll position for a new reply; the author must choose the jump.
 */
export function useTranscriptScroll({ containerRef, itemCount, replyKeys, latestReplyId, olderLoading }: TranscriptScrollOptions): TranscriptScrollState {
  const [unseenReplyCount, setUnseenReplyCount] = useState(0);
  const seenReplies = useRef<Set<string> | null>(null);
  const prependMeasurement = useRef<PrependMeasurement | null>(null);
  const suppressReplyAnnouncement = useRef(false);
  const wasOlderLoading = useRef(false);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (olderLoading && !wasOlderLoading.current) {
      prependMeasurement.current = { itemCount, scrollHeight: container.scrollHeight, scrollTop: container.scrollTop };
    }
    if (!olderLoading && wasOlderLoading.current && prependMeasurement.current) {
      const measurement = prependMeasurement.current;
      if (itemCount > measurement.itemCount) {
        container.scrollTop = measurement.scrollTop + (container.scrollHeight - measurement.scrollHeight);
        suppressReplyAnnouncement.current = true;
      }
      prependMeasurement.current = null;
    }
    wasOlderLoading.current = olderLoading;
  }, [containerRef, itemCount, olderLoading]);

  useLayoutEffect(() => {
    const current = new Set(replyKeys);
    const previous = seenReplies.current;
    const known = previous ? new Set(previous) : null;
    if (known) for (const key of current) known.add(key);
    seenReplies.current = known ?? current;
    if (!previous || olderLoading) return;
    if (suppressReplyAnnouncement.current) {
      suppressReplyAnnouncement.current = false;
      return;
    }
    const newReplies = replyKeys.filter(key => !previous.has(key));
    if (!newReplies.length) return;
    const container = containerRef.current;
    if (!container || !visibleInContainer(container, latestReplyId)) setUnseenReplyCount(count => count + newReplies.length);
  }, [containerRef, latestReplyId, olderLoading, replyKeys]);

  const jumpToLatest = useCallback(() => {
    const container = containerRef.current;
    if (container) {
      container.scrollTop = container.scrollHeight;
      setUnseenReplyCount(0);
    }
  }, [containerRef]);

  return { unseenReplyCount, jumpToLatest };
}

export function NewReplyAffordance({ count, onJump }: { count: number; onJump: () => void }) {
  if (count < 1) return null;
  return <button type="button" className="chat-transcript-new-reply" aria-live="polite" aria-label={`${count} new ${count === 1 ? 'reply' : 'replies'}; jump to latest`} onMouseDown={event => event.preventDefault()} onClick={onJump}>{count === 1 ? 'New reply' : `${count} new replies`} · Jump to latest</button>;
}
