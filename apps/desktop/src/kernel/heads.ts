/**
 * Revision identity — the frontend's L0.
 *
 * `sameHead` existed in six modules and `sameDocumentHead` in one. They are not
 * the same function: five copies compared two non-null heads, while
 * `ReviewPanel`'s and the exported `sameDocumentHead` tolerated a null on the
 * left. Two contracts under one name is how a null-safety regression gets
 * introduced by a "trivial" dedup, so both are kept explicitly separate here
 * rather than merged.
 *
 * Typed structurally rather than against `ipc/projects.ts`'s `Head`, so the
 * kernel has no dependency on the IPC layer. A `Head` satisfies
 * `RevisionIdentity` as-is.
 */

/** The three fields that identify one exact document revision. */
export interface RevisionIdentity {
  documentId: string;
  version: string;
  bodyHash: string;
}

/**
 * Exact identity of two revisions. Both must be present — this is the strict
 * contract, and it is the one that must be used when a mismatch is a protocol
 * error rather than an absent optional.
 */
export function sameHead(left: RevisionIdentity, right: RevisionIdentity): boolean {
  return left.documentId === right.documentId && left.version === right.version && left.bodyHash === right.bodyHash;
}

/**
 * The same comparison where either side may be absent. An absent side is never
 * a match, including two absents — "I have no revision" is not "we agree".
 */
export function sameDocumentHead(
  left: RevisionIdentity | null | undefined,
  right: RevisionIdentity | null | undefined,
): boolean {
  return !!left && !!right && sameHead(left, right);
}
