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
