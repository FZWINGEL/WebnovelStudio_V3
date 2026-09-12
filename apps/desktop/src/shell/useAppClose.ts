//! The app-close state machine.
//!
//! Split out of `Workspace`, which was 1,206 lines and held routing, layout,
//! library CRUD, document CRUD, chat adoption, export and this — a ~215-line
//! async protocol with its own fencing rules. It is the one piece of the
//! workspace that is a *protocol* rather than a view: begin, poll, ask, stop,
//! confirm, finish, destroy, with every step able to fall back to "stay open".
//!
//! The rules it keeps are load-bearing and are why it is a hook rather than
//! inline code:
//!
//! * A `CloseAttempt` is fenced by identity. Every await re-checks
//!   `currentCloseAttempt` before acting, so an acknowledgment that arrives
//!   after the author chose Stay open can never turn the original close back
//!   into a destroy.
//! * A failed cancellation keeps the attempt fenced rather than clearing it.
//!   A late status or finish acknowledgment must not be treated as permission.
//! * The final read before `finish` fences the stop decision from work that
//!   started while the dialog was open. Destroy is deliberately inside
//!   `detachAfter`'s callback: if it fails, the mounted editor session is never
//!   disposed.

import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { DocumentSession } from '../editor';
import {
  appCloseStatus, beginAppClose, cancelAppClose, finishAppClose, stopAppJobs,
  type AppCloseStatus,
} from '../ipc/appClose';
import type { AppCloseDialogPhase } from './AppCloseDialog';

type CloseDecision = 'stop' | 'stay';
type CloseAttempt = {
  closeId: string;
  gateActive: boolean;
  cancelRequired: boolean;
  invalidated: boolean;
  keepPrompt: boolean;
  decision: CloseDecision | null;
  resolveDecision: ((decision: CloseDecision) => void) | null;
  stopFlight: Promise<void> | null;
  cancelFlight: Promise<boolean> | null;
};
export type ClosePrompt = { phase: AppCloseDialogPhase; status: AppCloseStatus | null; message: string };

const CLOSE_POLL_MS = 500;
const CLOSE_MAX_POLLS = 120;
const waitForClosePoll = () => new Promise<void>(resolve => setTimeout(resolve, CLOSE_POLL_MS));
const closeHasActiveWork = (status: AppCloseStatus) => status.startingRequests > 0 || status.activeJobs > 0 || status.activeWorkers > 0;
const closeHasPendingOnly = (status: AppCloseStatus) => status.pendingResults > 0 && status.startingRequests === 0 && status.activeWorkers === 0;
const closeStatusMessage = (status: AppCloseStatus) => closeHasActiveWork(status)
  ? 'Some AI replies or story memory refreshes are still in progress. Stop them before closing, or stay open.'
  : status.pendingResults > 0
    ? 'Some replies or story memory still need saving. Stop local work and close, or stay open while they finish.'
    : 'The app is still finishing local work. Stay open and try closing again.';
const closeBlockedMessage = (status: AppCloseStatus) => closeHasActiveWork(status)
  ? 'Some AI replies or story memory refreshes are still finishing. Stay open and try closing again once they finish.'
  : status.pendingResults > 0
    ? 'Some replies or story memory still need saving. Stay open and retry saving them.'
    : 'The app is still finishing local work. Stay open and try closing again.';

function errorText(error: unknown): string {
  if (error && typeof error === 'object' && 'detail' in error) return String(error.detail);
  return error instanceof Error ? error.message : String(error);
}

/** Thrown to unwind the close protocol when the author chose to stay open. */
class StayOpenSignal extends Error {
  constructor() { super('The editor remains open.'); }
}

/** What the protocol needs from the workspace that owns it. */
export type AppCloseDeps = {
  /** The busy guard: true while another operation is running. */
  isRunning: () => boolean;
  /** A transient message for the header, e.g. "finish the current operation". */
  notice: (message: string) => void;
  /** Run work under the workspace's busy/error envelope. */
  run: (work: () => Promise<void>) => Promise<void>;
  /** Flush the chat and workshop handles before the editor detaches. */
  save: () => Promise<void>;
  /** The mounted editor session, or null when none is active. */
  session: () => DocumentSession | null;
};

export function useAppClose(deps: AppCloseDeps) {
  const [prompt, setPrompt] = useState<ClosePrompt | null>(null);
  const attempt = useRef<CloseAttempt | null>(null);
  const depsRef = useRef(deps);
  depsRef.current = deps;

  function current(probe: CloseAttempt): boolean {
    return attempt.current === probe && !probe.invalidated;
  }

  async function releaseCloseGate(probe: CloseAttempt): Promise<boolean> {
    if (!probe.cancelRequired && !probe.gateActive) return true;
    if (probe.cancelFlight) return probe.cancelFlight;
    probe.invalidated = true;
    const flight = cancelAppClose(probe.closeId).then(() => {
      probe.gateActive = false;
      probe.cancelRequired = false;
      return true;
    }).catch(() => {
      // Keep this attempt fenced after a failed cancellation. A late status
      // or finish acknowledgment must never turn the original close back
      // into a destroy; Stay open can retry the same cancellation token.
      return false;
    }).finally(() => {
      if (probe.cancelFlight === flight) probe.cancelFlight = null;
    });
    probe.cancelFlight = flight;
    return flight;
  }

  async function blockClose(probe: CloseAttempt, status: AppCloseStatus | null, message: string, resolveWaitingDecision: boolean): Promise<boolean> {
    probe.keepPrompt = true;
    const released = await releaseCloseGate(probe);
    setPrompt({ phase: released ? 'blocked' : 'error', status, message: released ? message : `${message} The close request could not be released. Try Stay open again.` });
    if (released && resolveWaitingDecision && probe.resolveDecision) {
      const resolve = probe.resolveDecision;
      probe.resolveDecision = null;
      probe.decision = 'stay';
      resolve('stay');
    }
    return released;
  }

  function askToClose(probe: CloseAttempt, status: AppCloseStatus, message: string): Promise<CloseDecision> {
    return new Promise(resolve => {
      probe.resolveDecision = resolve;
      setPrompt({ phase: 'waiting', status, message });
    });
  }

  async function stayOpen(): Promise<void> {
    const probe = attempt.current;
    if (!probe) return;
    if (probe.decision === 'stay' && !probe.cancelRequired && !probe.gateActive) {
      attempt.current = null;
      setPrompt(null);
      return;
    }
    probe.keepPrompt = true;
    const released = await releaseCloseGate(probe);
    if (!released) {
      setPrompt(previous => ({
        phase: 'error',
        status: previous?.status ?? null,
        message: 'The close request is still active. Stay open and try again to release it safely.',
      }));
      return;
    }
    probe.keepPrompt = false;
    probe.decision = 'stay';
    const resolve = probe.resolveDecision;
    probe.resolveDecision = null;
    if (resolve) {
      resolve('stay');
      setPrompt(null);
      if (attempt.current === probe) attempt.current = null;
      return;
    }
    attempt.current = null;
    setPrompt(null);
  }

  async function stopAndClose(): Promise<void> {
    const probe = attempt.current;
    if (!probe || probe.invalidated || probe.stopFlight) return;
    probe.stopFlight = (async () => {
      try {
        let status = await appCloseStatus(probe.closeId);
        for (let poll = 0; status.startingRequests > 0 && poll < CLOSE_MAX_POLLS; poll += 1) {
          if (!current(probe)) return;
          setPrompt({ phase: 'stopping', status, message: 'Waiting for a request to finish starting before stopping local work…' });
          await waitForClosePoll();
          status = await appCloseStatus(probe.closeId);
        }
        if (!current(probe)) return;
        if (status.startingRequests > 0) {
          await blockClose(probe, status, 'A reply is still starting and could not be stopped yet. Stay open and try again.', true);
          return;
        }
        status = await stopAppJobs(probe.closeId);
        for (let poll = 0; !status.ready && !closeHasPendingOnly(status) && poll < CLOSE_MAX_POLLS; poll += 1) {
          if (!current(probe)) return;
          setPrompt({ phase: 'stopping', status, message: 'Stopping local work…' });
          await waitForClosePoll();
          status = await appCloseStatus(probe.closeId);
        }
        if (!current(probe)) return;
        if (!status.ready) {
          await blockClose(probe, status, closeBlockedMessage(status), true);
          return;
        }
        const resolve = probe.resolveDecision;
        probe.resolveDecision = null;
        probe.decision = 'stop';
        setPrompt({ phase: 'stopping', status, message: 'Finishing the close safely…' });
        resolve?.('stop');
      } catch (reason) {
        if (current(probe)) await blockClose(probe, null, `Could not stop local work safely: ${errorText(reason)}`, true);
      }
    })().finally(() => { probe.stopFlight = null; });
    await probe.stopFlight;
  }

  async function closeApplication(): Promise<void> {
    if (attempt.current) return;
    const probe: CloseAttempt = {
      closeId: crypto.randomUUID(), gateActive: false, cancelRequired: false, invalidated: false,
      keepPrompt: false, decision: null, resolveDecision: null, stopFlight: null, cancelFlight: null,
    };
    attempt.current = probe;
    try {
      try {
        probe.cancelRequired = true;
        await beginAppClose(probe.closeId);
        probe.gateActive = true;
      } catch (reason) {
        await blockClose(probe, null, `Could not prepare to close safely: ${errorText(reason)}`, false);
        return;
      }
      const prepare = async (): Promise<void> => {
        let status: AppCloseStatus;
        try {
          status = await appCloseStatus(probe.closeId);
        } catch (reason) {
          await blockClose(probe, null, `Could not check whether it is safe to close: ${errorText(reason)}`, false);
          throw new StayOpenSignal();
        }
        if (!current(probe)) throw new StayOpenSignal();
        if (!status.ready) {
          if (closeHasPendingOnly(status)) {
            await blockClose(probe, status, closeBlockedMessage(status), false);
            throw new StayOpenSignal();
          }
          const decision = await askToClose(probe, status, closeStatusMessage(status));
          if (decision === 'stay' || !current(probe)) throw new StayOpenSignal();
          try {
            status = await appCloseStatus(probe.closeId);
          } catch (reason) {
            await blockClose(probe, null, `Could not confirm that local work stopped: ${errorText(reason)}`, false);
            throw new StayOpenSignal();
          }
          if (!status.ready) {
            await blockClose(probe, status, closeBlockedMessage(status), false);
            throw new StayOpenSignal();
          }
        }
        if (!current(probe)) throw new StayOpenSignal();
        // A final read fences the stop decision from any work that started while
        // the dialog was open. Never finish or destroy on a stale status.
        try {
          status = await appCloseStatus(probe.closeId);
        } catch (reason) {
          await blockClose(probe, null, `Could not confirm that it is safe to close: ${errorText(reason)}`, false);
          throw new StayOpenSignal();
        }
        if (!current(probe)) throw new StayOpenSignal();
        if (!status.ready) {
          await blockClose(probe, status, closeBlockedMessage(status), false);
          throw new StayOpenSignal();
        }
        try {
          await finishAppClose(probe.closeId);
          if (!current(probe)) throw new StayOpenSignal();
        } catch (reason) {
          if (reason instanceof StayOpenSignal) throw reason;
          await blockClose(probe, status, `Could not finish closing safely: ${errorText(reason)}`, false);
          throw new StayOpenSignal();
        }
        // Destroy is deliberately inside detachAfter's prepare callback. If it
        // fails, detachAfter never disposes the mounted editor session.
        await getCurrentWindow().destroy();
        probe.gateActive = false;
        probe.cancelRequired = false;
      };
      try {
        await depsRef.current.save();
        const session = depsRef.current.session();
        if (session) await session.detachAfter(prepare, 'close');
        else await prepare();
      } catch (reason) {
        if (reason instanceof StayOpenSignal) return;
        // A flush/checkpoint can fail before the native close callback runs.
        // Release the native admission gate, but keep the editor mounted.
        await blockClose(probe, null, `Could not save the current writing before closing: ${errorText(reason)}`, false);
        throw reason;
      }
    } finally {
      if (attempt.current === probe && !probe.keepPrompt) attempt.current = null;
    }
  }

  useEffect(() => {
    if (!isTauri()) return;
    const attached = getCurrentWindow().onCloseRequested(event => {
      event.preventDefault();
      if (depsRef.current.isRunning() || attempt.current) {
        depsRef.current.notice('Finish the current operation before closing.');
        return;
      }
      void depsRef.current.run(closeApplication);
    });
    return () => { void attached.then(unlisten => unlisten()); };
    // Registered once: the handler reads its dependencies through `depsRef` so
    // it never closes over a stale render, and re-registering would leave two
    // native listeners racing for the same close event.
  }, []);

  return { prompt, close: closeApplication, stayOpen, stopAndClose };
}
