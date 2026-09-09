import { useEffect, useRef, useState, type CSSProperties, type PointerEvent, type ReactNode, type KeyboardEvent } from 'react';
import './ChatSplitPane.css';

const MIN_WIDTH = 30;
const MAX_WIDTH = 70;
const DEFAULT_WIDTH = 56;
const STORAGE_PREFIX = 'webnovelstudio.chat-split-pane.v1:';

export interface ChatSplitPaneProps {
  /** A stable project/session identity. It is used only for local view preferences. */
  projectKey: string;
  left: ReactNode;
  right: ReactNode;
  initialWidthPercent?: number;
  className?: string;
}

interface SplitPaneStyle extends CSSProperties {
  '--chat-split-left': string;
}

interface DragState {
  pointerId: number;
  startX: number;
  startWidth: number;
}

function clampWidth(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_WIDTH;
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, value));
}

export function chatSplitPanePreferenceKey(projectKey: string): string {
  return `${STORAGE_PREFIX}${encodeURIComponent(projectKey)}`;
}

function readStoredWidth(projectKey: string): number | null {
  try {
    if (typeof localStorage === 'undefined') return null;
    const raw = localStorage.getItem(chatSplitPanePreferenceKey(projectKey));
    if (raw === null) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? clampWidth(value) : null;
  } catch {
    return null;
  }
}

function storeWidth(projectKey: string, value: number): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(chatSplitPanePreferenceKey(projectKey), String(Math.round(value * 100) / 100));
    }
  } catch {
    // Local layout preferences are optional and must not affect the writing surface.
  }
}

function defaultWidth(initialWidthPercent: number | undefined): number {
  return clampWidth(initialWidthPercent ?? DEFAULT_WIDTH);
}

export function ChatSplitPane({ projectKey, left, right, initialWidthPercent, className }: ChatSplitPaneProps) {
  const [width, setWidth] = useState(() => readStoredWidth(projectKey) ?? defaultWidth(initialWidthPercent));
  const widthRef = useRef(width);
  const containerRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragState | null>(null);

  useEffect(() => {
    dragRef.current = null;
    const next = readStoredWidth(projectKey) ?? defaultWidth(initialWidthPercent);
    widthRef.current = next;
    setWidth(next);
  }, [initialWidthPercent, projectKey]);

  const commitWidth = (next: number, persist = true): void => {
    const clamped = clampWidth(next);
    widthRef.current = clamped;
    setWidth(clamped);
    if (persist) storeWidth(projectKey, clamped);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    let next: number | null = null;
    switch (event.key) {
      case 'ArrowLeft':
        next = widthRef.current - 2;
        break;
      case 'ArrowRight':
        next = widthRef.current + 2;
        break;
      case 'Home':
        next = MIN_WIDTH;
        break;
      case 'End':
        next = MAX_WIDTH;
        break;
      default:
        return;
    }
    event.preventDefault();
    commitWidth(next);
  };

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>): void => {
    if (event.button !== 0) return;
    event.preventDefault();
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startWidth: widthRef.current };
    event.currentTarget.focus();
    if (event.currentTarget.setPointerCapture) event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>): void => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const containerWidth = containerRef.current?.getBoundingClientRect().width ?? 0;
    if (!containerWidth) return;
    commitWidth(drag.startWidth + ((event.clientX - drag.startX) / containerWidth) * 100);
  };

  const finishPointerDrag = (event: PointerEvent<HTMLDivElement>): void => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  const style: SplitPaneStyle = { '--chat-split-left': `${width}%` };
  const classes = ['chat-workspace', 'chat-split-pane', className].filter(Boolean).join(' ');

  return <div ref={containerRef} className={classes} style={style} data-chat-split-pane="true">
    {left}
    <div
      className="chat-split-separator"
      role="separator"
      aria-label="Resize conversation and document panels"
      aria-orientation="vertical"
      aria-valuemin={MIN_WIDTH}
      aria-valuemax={MAX_WIDTH}
      aria-valuenow={Math.round(width)}
      aria-valuetext={`${Math.round(width)}% conversation panel`}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={finishPointerDrag}
      onPointerCancel={finishPointerDrag}
    />
    {right}
  </div>;
}
