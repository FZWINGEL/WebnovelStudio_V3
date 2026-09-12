import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { readProjectConversation, type AssistantDraft, type ConversationItem, type ProjectConversationView } from '../ipc/projectChat';
import type { ProjectAccess } from '../ipc/projects';

export interface DraftReviewContext {
  instruction: string;
  assumptions: readonly string[];
}

export interface DraftReviewContextRequest {
  access: ProjectAccess;
  view: ProjectConversationView | null;
  enabled: boolean;
}

export interface DraftReviewContextState {
  context: Record<string, DraftReviewContext>;
  loading: boolean;
  error: string | null;
  retry(): void;
}

type RunLike = Record<string, unknown>;

function runFromItem(item: ConversationItem): RunLike | null {
  return item.payload.run && typeof item.payload.run === 'object' ? item.payload.run as RunLike : null;
}

function assumptionsFromRun(run: RunLike): string[] {
  if (typeof run.outputText !== 'string' || !run.outputText.trim()) return [];
  try {
    const value: unknown = JSON.parse(run.outputText);
    if (!value || typeof value !== 'object') return [];
    const output = value as Record<string, unknown>;
    if (output.schemaVersion !== 'project-assistant-output.v1' || !Array.isArray(output.assumptions)) return [];
    return output.assumptions.flatMap(entry => {
      if (!entry || typeof entry !== 'object') return [];
      const text = (entry as Record<string, unknown>).text;
      return typeof text === 'string' ? [text] : [];
    });
  } catch { return []; }
}

function contextFromItem(item: ConversationItem): [string, DraftReviewContext] | null {
  if (item.kind !== 'request' && item.kind !== 'chapterRequest') return null;
  const run = runFromItem(item);
  const runId = typeof run?.id === 'string' ? run.id : item.referenceId;
  const instruction = item.payload.instruction;
  if (!runId || typeof instruction !== 'string') return null;
  return [runId, { instruction, assumptions: assumptionsFromRun(run ?? {}) }];
}

function currentContext(items: ConversationItem[]): Map<string, DraftReviewContext> {
  const result = new Map<string, DraftReviewContext>();
  for (const item of items) {
    const entry = contextFromItem(item);
    if (entry) result.set(entry[0], entry[1]);
  }
  return result;
}

function errorText(reason: unknown): string {
  if (reason && typeof reason === 'object') {
    const value = reason as Record<string, unknown>;
    if (typeof value.detail === 'string' && value.detail.trim()) return value.detail;
    if (typeof value.message === 'string' && value.message.trim()) return value.message;
  }
  return reason instanceof Error && reason.message ? reason.message : 'The older conversation context could not be loaded.';
}

function draftOrigins(drafts: AssistantDraft[]): string[] {
  return [...new Set(drafts.map(draft => draft.originRunId).filter(Boolean))];
}

interface ContextCache {
  projectKey: string;
  values: Map<string, DraftReviewContext>;
}

/**
 * Keeps review provenance independent from the visible conversation timeline.
 * The conversation page is intentionally bounded, while draft inventory is
 * complete; missing origin runs are read through the existing cursor API only
 * when the review surface is enabled.
 */
export function useDraftReviewContext({ access, view, enabled }: DraftReviewContextRequest): DraftReviewContextState {
  const projectKey = `${access.projectId}|${access.operationNamespace}|${access.session}`;
  const accessKey = `${projectKey}|${access.writerLease}`;
  const cacheRef = useRef<ContextCache>({ projectKey, values: new Map() });
  if (cacheRef.current.projectKey !== projectKey) cacheRef.current = { projectKey, values: new Map() };
  const [revision, setRevision] = useState(0);
  const [retryToken, setRetryToken] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = useMemo(() => currentContext(view?.items ?? []), [view?.items]);
  const originIds = useMemo(() => draftOrigins(view?.drafts ?? []), [view?.drafts]);
  const missing = useMemo(() => originIds.filter(id => !current.has(id) && !cacheRef.current.values.has(id)), [current, originIds, revision]);
  const cursor = view?.olderBefore ?? null;
  const requestKey = `${accessKey}|${enabled ? 'enabled' : 'disabled'}|${cursor ?? ''}|${missing.join('\u0000')}`;

  // Durable items currently in the visible page are authoritative. Keep their
  // exact request context so a later refresh that evicts that page does not
  // discard provenance already recovered during this session.
  useEffect(() => {
    for (const [runId, value] of current) cacheRef.current.values.set(runId, value);
  }, [accessKey, current]);

  useEffect(() => {
    if (!enabled || !view || missing.length === 0) {
      setLoading(false);
      if (missing.length === 0 || !enabled) setError(null);
      return;
    }
    let cancelled = false;
    const unresolved = new Set(missing);
    const seenCursors = new Set<string>();
    setLoading(true);
    setError(null);
    const load = async () => {
      let before = cursor;
      try {
        while (unresolved.size > 0 && before) {
          if (seenCursors.has(before)) {
            throw new Error('The conversation returned the same older-page cursor repeatedly.');
          }
          seenCursors.add(before);
          const page = await readProjectConversation(access, before, 100);
          if (cancelled) return;
          for (const item of page.items) {
            const entry = contextFromItem(item);
            if (!entry || !unresolved.has(entry[0])) continue;
            cacheRef.current.values.set(entry[0], entry[1]);
            unresolved.delete(entry[0]);
          }
          if (page.olderBefore === before) throw new Error('The conversation returned the same older-page cursor repeatedly.');
          before = page.olderBefore;
        }
        if (cancelled) return;
        if (unresolved.size > 0) {
          throw new Error(`The originating request for ${[...unresolved].join(', ')} was not found in the conversation history.`);
        }
        setRevision(value => value + 1);
        setLoading(false);
      } catch (reason) {
        if (cancelled) return;
        setLoading(false);
        setError(errorText(reason));
      }
    };
    void load();
    return () => { cancelled = true; };
  }, [requestKey, retryToken]);

  const values = useMemo(() => {
    const result = new Map(cacheRef.current.values);
    for (const [runId, value] of current) result.set(runId, value);
    return Object.fromEntries(result);
  }, [accessKey, current, revision]);
  const retry = useCallback(() => setRetryToken(value => value + 1), []);
  return { context: values, loading, error, retry };
}
