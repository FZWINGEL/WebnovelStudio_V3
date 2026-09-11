#!/usr/bin/env bash
# Move the provider delivery vocabulary from
# crates/core/src/projects/discussions.rs to wns-providers::vocabulary.
#
# Source layout (verified by reading): lines 244-352 are one contiguous block —
# a doc comment and ProviderOutcomeStatus, its impl, ProviderCleanup, its impl,
# HttpDeliverySubmission, HttpProviderUsage, ProviderDeliveryReceipt, and
# ProviderUsage. All six are leaves: serde plus CoreError/CoreResult, nothing
# else. Line 243 and line 353 are blank.
set -euo pipefail

SRC=crates/core/src/projects/discussions.rs
DST=crates/providers/src/vocabulary.rs

cat >> "$DST" <<'HEADER'

// ---------------------------------------------------------------------------
// Provider delivery vocabulary.
//
// Moved down from `webnovel-core::projects::discussions`, where they sat in a
// 4,619-line module bound for `wns-conversation` (L5). Three modules use them
// and they land in *different* crates: `memory` goes to `wns-story` (L4), and
// `discussions` and `discussion_lookup` to `wns-conversation` (L5). Vocabulary
// that two future siblings both need has to sit below both, so `memory`'s
// `use crate::projects::discussions::{...}` was an L4->L5 upward edge that no
// amount of moving either module could resolve.
//
// This is the second time this exact shape has been fixed here, and the
// argument is the one in this file's own header: these are provider contract
// types — an outcome, a cleanup state, a delivery receipt — not discussion
// logic, and they lived in `discussions` only because that is where the first
// adapter needing them happened to be written.
//
// Byte-compatibility is unaffected: a serialized receipt is identical whichever
// crate owns the type, and `discussions` re-exports every item at its
// historical path.
// ---------------------------------------------------------------------------

use wns_kernel::{CoreError, CoreResult};
HEADER

sed -n '244,352p' "$SRC" >> "$DST"
sed -i '244,353d' "$SRC"
