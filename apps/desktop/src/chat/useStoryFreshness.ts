import { useEffect, useRef, useState } from 'react';
import { preparedStoryContext, storyContextSnapshot } from '../ipc/context';
import type { DiscussionRun } from '../ipc/discussions';
import type { ProjectAccess } from '../ipc/projects';

export type StoryFreshnessStatus = 'idle' | 'checking' | 'current' | 'stale' | 'unknown';

export interface StoryFreshness {
  status: StoryFreshnessStatus;
  frozenSourceEpoch: string | null;
  frozenPolicyEpoch: string | null;
  detail: string | null;
}

export interface UseStoryFreshnessOptions {
  access: ProjectAccess;
  run: DiscussionRun | null;
  currentSourceEpoch?: string | null;
  currentPolicyEpoch?: string | null;
}

function identityFor(access: ProjectAccess, run: DiscussionRun | null): string {
  return [access.projectId, access.operationNamespace, access.session, access.writerLease, run?.id ?? '', run?.packetId ?? ''].join(':');
}

function differs(current: string | null | undefined, frozen: string | null): boolean {
  return typeof current === 'string' && current.length > 0 && frozen !== null && current !== frozen;
}

/**
 * Compares the live project epochs with the immutable context used by a run.
 * Reading a packet or snapshot never starts a model request and all responses
 * are discarded when the project lease or run identity changes.
 */
export function useStoryFreshness({ access, run, currentSourceEpoch = null, currentPolicyEpoch = null }: UseStoryFreshnessOptions): StoryFreshness {
  const identity = identityFor(access, run);
  const [state, setState] = useState<StoryFreshness>({ status: 'idle', frozenSourceEpoch: null, frozenPolicyEpoch: null, detail: null });
  const activeIdentity = useRef(identity);
  activeIdentity.current = identity;
  const [resolvedIdentity, setResolvedIdentity] = useState(identity);
  useEffect(() => {
    let cancelled = false;
    if (!run?.packetId) {
      setResolvedIdentity(identity);
      setState({ status: 'idle', frozenSourceEpoch: null, frozenPolicyEpoch: null, detail: null });
      return () => { cancelled = true; };
    }
    setResolvedIdentity(identity);
    setState({ status: 'checking', frozenSourceEpoch: null, frozenPolicyEpoch: null, detail: null });
    const capturedIdentity = identity;
    void (async () => {
      try {
        const packet = await preparedStoryContext(access, run.packetId);
        if (packet.receipt.packetId !== run.packetId) throw new Error('The prepared context belongs to another request.');
        const frozen = await storyContextSnapshot(access, packet.receipt.snapshotId);
        if (frozen.snapshot.projectId !== access.projectId || frozen.snapshot.snapshotId !== packet.receipt.snapshotId) throw new Error('The frozen context belongs to another project or packet.');
        if (cancelled || activeIdentity.current !== capturedIdentity) return;
        const frozenSourceEpoch = frozen.snapshot.contextSourceEpoch;
        const frozenPolicyEpoch = frozen.snapshot.disclosurePolicyVersion;
        const staleSource = differs(currentSourceEpoch, frozenSourceEpoch);
        const stalePolicy = differs(currentPolicyEpoch, frozenPolicyEpoch);
        setState({
          status: staleSource || stalePolicy ? 'stale' : 'current',
          frozenSourceEpoch,
          frozenPolicyEpoch,
          detail: staleSource && stalePolicy ? 'Story sources and disclosure policy changed after this request.' : staleSource ? 'Story sources changed after this request.' : stalePolicy ? 'Disclosure policy changed after this request.' : null,
        });
      } catch (reason) {
        if (cancelled || activeIdentity.current !== capturedIdentity) return;
        setState({ status: 'unknown', frozenSourceEpoch: null, frozenPolicyEpoch: null, detail: reason instanceof Error ? reason.message : 'The request context could not be compared.' });
      }
    })();
    return () => { cancelled = true; };
  }, [access.projectId, access.operationNamespace, access.session, access.writerLease, identity, run?.packetId]);

  if (resolvedIdentity !== identity) {
    return { status: run?.packetId ? 'checking' : 'idle', frozenSourceEpoch: null, frozenPolicyEpoch: null, detail: null };
  }
  const staleSource = differs(currentSourceEpoch, state.frozenSourceEpoch);
  const stalePolicy = differs(currentPolicyEpoch, state.frozenPolicyEpoch);
  if (staleSource || stalePolicy) {
    return {
      ...state,
      status: 'stale',
      detail: staleSource && stalePolicy ? 'Story sources and disclosure policy changed after this request.' : staleSource ? 'Story sources changed after this request.' : 'Disclosure policy changed after this request.',
    };
  }
  return state;
}
