//! Enforces the layering rule declared in `docs/V3_ARCHITECTURE_MODULAR.md`.
//!
//! The rule this crate exists to hold:
//!
//! * A crate may depend only on crates at a **strictly lower layer**.
//! * Siblings at the same layer must not depend on each other, so no dependency
//!   edge ever points sideways.
//! * No layered crate may depend on `webnovel-core`, the crate being decomposed.
//!   A backward edge there would mean the split is leaking.
//!
//! Without this test the layering is a document. With it, a wrong `use` fails
//! the build — which is the only kind of boundary that has ever held here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// `(package name, path relative to the workspace root, layer)`.
pub const LAYERS: &[(&str, &str, u32)] = &[
    ("wns-kernel", "crates/kernel", 0),
    ("wns-storage", "crates/storage", 1),
    ("wns-providers", "crates/providers", 1),
    // Raised from L1 to L2 when document revision history moved here: history
    // reads and writes document rows, so it depends on the layer that owns the
    // schema. `wns-context` already depended on `wns-documents`, so this shifted
    // every layer above documents up one ordinal. The relative order is exactly
    // what it was: these are ordinals in a partial order, and no edge in the
    // graph changed direction except the one that was added.
    ("wns-documents", "crates/documents", 2),
    ("wns-context", "crates/context", 3),
    ("wns-story", "crates/story", 4),
    ("wns-conversation", "crates/conversation", 5),
    ("wns-workshop", "crates/workshop", 5),
    // Raised from L5 to L6. `transfer` is a pure CONSUMER: backup validation calls
    // every concern's own storage validator, so it sits above all of them rather
    // than beside them. Nothing depends on it except the app shell, so the raise
    // costs no other crate a renumber. Its old charter said "L5, may depend on
    // L0–L3" — that was aspirational; the module it was written for already
    // reached `story_context`, `reviewed_story` and `memory` at L4 and the
    // conversation and workshop validators at L5.
    ("wns-transfer", "crates/transfer", 6),
    // Raised from L5 to L7. The library's V2 import workflow drives the
    // installer in `wns-transfer` (L6), so it sits above the portability crate;
    // it also reaches `v2_import`, which moved up with the installer that
    // consumes it. Nothing depends on the library but the app shell, which the
    // layer table does not register.
    ("wns-library", "crates/library", 7),
];

/// The crate being decomposed. The rule above applies to `wns-*` only; this one
/// is expected to depend on lower layers while the split proceeds, and nothing
/// layered may depend on it.
pub const LEGACY_CRATE: &str = "webnovel-core";

pub fn workspace_root() -> PathBuf {
    // crates/architecture -> crates -> root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/architecture must sit two levels under the workspace root")
        .to_path_buf()
}

pub fn layer_of(package: &str) -> Option<u32> {
    LAYERS
        .iter()
        .find(|(name, _, _)| *name == package)
        .map(|(_, _, layer)| *layer)
}

/// Workspace-internal dependency names declared under `[dependencies]` in a
/// crate manifest. Parsed from the text rather than with a TOML crate so this
/// checker adds no dependency of its own.
pub fn internal_dependencies(manifest: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(manifest)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", manifest.display()));
    let mut found = BTreeMap::new();
    let mut in_dependencies = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            // Only the plain [dependencies] table; target- and dev-specific
            // tables are not the layer rule's business.
            in_dependencies = trimmed == "[dependencies]";
            continue;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.starts_with("wns-") || key == LEGACY_CRATE || key.starts_with("webnovel-") {
            found.insert(key.to_owned(), value.trim().to_owned());
        }
    }
    found
}
