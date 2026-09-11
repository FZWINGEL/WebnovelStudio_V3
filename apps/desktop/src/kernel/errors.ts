/**
 * Rejection reading — the frontend's L0.
 *
 * The Rust side rejects with `CoreError { code, detail }`, and a rejection may
 * also arrive as a thrown `Error`. Every module grew its own reader for that
 * shape; these are the canonical ones.
 *
 * # What was deliberately NOT merged
 *
 * Two modules looked like duplicates and are not, so they keep their local
 * variants:
 *
 * * `chat/useDraftReviewContext.ts` also accepts a bare `{ message }` object
 *   that is not an `Error`. Folding that into the shared reader would silently
 *   change the fallback for every other site, so it stays where it is.
 * * `story/DocumentAliases.tsx` deliberately prefers `Error.message` **over**
 *   `detail` — the opposite precedence — because its call sites embed the text
 *   in a sentence about a save possibly having completed.
 *
 * A duplicate that is genuinely a different contract is not debt.
 */

/** The machine-readable code a `CoreError`-shaped rejection carries. */
export function errorCode(reason: unknown): string | null {
  if (!reason || typeof reason !== 'object' || !('code' in reason)) return null;
  const code = (reason as { code?: unknown }).code;
  return typeof code === 'string' ? code : null;
}

/**
 * Human-readable text for a rejection: the server's `detail`, else the
 * `Error`'s own message, else the caller's fallback.
 *
 * The fallback stays a parameter rather than a default because each surface
 * tells the author something specific — "Could not prepare this export" and
 * "Could not read this V2 database" are different sentences about different
 * failures, and a shared generic string would lose that.
 */
export function errorText(reason: unknown, fallback: string): string {
  if (reason && typeof reason === 'object' && 'detail' in reason) {
    return String((reason as { detail: unknown }).detail);
  }
  return reason instanceof Error ? reason.message : fallback;
}

/**
 * `errorText` bound to one module's fallback, so a module can replace its local
 * copy with a single line and leave every call site untouched. There are 47
 * `errorText` call sites across eight modules; rewriting all of them to thread
 * a constant through would be a large mechanical diff — and mechanical diffs at
 * that size are where behaviour quietly changes.
 */
export function errorTextFor(fallback: string): (reason: unknown) => string {
  return (reason: unknown) => errorText(reason, fallback);
}
