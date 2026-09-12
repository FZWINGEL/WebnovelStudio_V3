/**
 * L0 — foundation utilities shared by every feature.
 *
 * Nothing here may import from `features/`, `shell/` or a sibling. This is the
 * frontend half of the same layering rule `crates/architecture` enforces on the
 * Rust workspace; `docs/V3_ARCHITECTURE_MODULAR.md` §4 records the rest of the
 * frontend decomposition.
 */
export { sameHead, sameDocumentHead, type RevisionIdentity } from './heads';
export { errorCode, errorText, errorTextFor } from './errors';
export { createSaveLoop, type SaveAttempt, type SaveLoop } from './saveLoop';
/**
 * The document model itself. It sat in `editor/` while `ipc/*.ts` imported it
 * from there to narrow a wire body to `WnsDocument`, which made `ipc` and
 * `editor` mutually dependent. It is vocabulary every layer shares rather than
 * editing behaviour, so it belongs at the foundation.
 */
export { bodyHash, canonicalJson, documentBlocks, safeHref, sample, snapshotFromEditor } from './document';
export type { Block, Inline, Mark, SnapshotReceipt, WnsDocument } from './document';
