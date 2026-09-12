/**
 * The single-flight, lost-acknowledgment-tolerant save loop.
 *
 * `WorkshopStore` and `ComposerSession` each grew this independently, and the
 * shape repeats again in the document session. The subtle part is not the loop
 * — it is the retention rule:
 *
 * > A write that fails **without a definitive answer** must keep the exact
 * > payload and operation id it sent. A retry under a fresh operation id is a
 * > second mutation of the same intent, and this product's whole safety story
 * > is that a lost acknowledgment is reconciled rather than repeated.
 *
 * That rule should exist once. Getting it wrong is not a style problem: the
 * failure mode is an author's work being sent twice or dropped.
 */

/** One captured write, and what to do with it afterwards. */
export interface SaveAttempt {
  /** Perform the write. Throws on failure. */
  send(): Promise<void>;
  /** Runs after a successful write, before the payload is released. */
  commit(): void;
  /** Runs after a failed write, before the error propagates. */
  fail?(error: unknown): void;
  /**
   * Return `true` to release the captured payload on this error.
   *
   * Only a refusal that provably happened *before* the transaction qualifies —
   * a validation rejection, say. Anything else may have committed, and
   * releasing it would let the author's next edit overwrite work that landed.
   */
  discardOn?(error: unknown): boolean;
}

export interface SaveLoop {
  /** Run or join one drain. Callers requiring a clean store must recheck dirtiness after awaiting. */
  flush(): Promise<void>;
  /** True while a drain is in progress. */
  readonly saving: boolean;
  /** True while a failed write is being retained for reconciliation. */
  readonly hasPending: boolean;
}

/**
 * @param isDirty whether there is anything to write right now
 * @param capture  build the payload, or reuse the retained one; `null` when a
 *                 dirty flag has no corresponding payload
 */
export function createSaveLoop(options: {
  isDirty: () => boolean;
  capture: () => SaveAttempt | null;
}): SaveLoop {
  let flight: Promise<void> | null = null;
  let pending: SaveAttempt | null = null;

  const drain = async (): Promise<void> => {
    while (options.isDirty() || pending) {
      pending ??= options.capture();
      if (!pending) return;
      const attempt = pending;
      try {
        await attempt.send();
        attempt.commit();
        pending = null;
      } catch (error) {
        if (attempt.discardOn?.(error)) pending = null;
        attempt.fail?.(error);
        throw error;
      }
    }
  };

  const flush = async (): Promise<void> => {
    // A second caller joins the in-flight drain rather than starting a rival
    // one. It does not re-run afterwards: `drain` already re-checks `isDirty`
    // between writes, but an edit can arrive after the drain resolves and
    // before `flight` clears. A caller requiring a clean store must re-check.
    // Re-running here would livelock whenever a
    // `capture` legitimately returns `null` for a still-dirty store.
    if (flight) {
      await flight;
      return;
    }
    flight = drain();
    try {
      await flight;
    } finally {
      flight = null;
    }
  };

  return {
    flush,
    get saving() {
      return flight !== null;
    },
    get hasPending() {
      return pending !== null;
    },
  };
}
