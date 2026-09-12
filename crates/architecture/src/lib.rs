//! Enforces the layering rule documented in `docs/ARCHITECTURE.md`.
//!
//! The rule this crate exists to hold:
//!
//! * A crate may depend only on crates at a **strictly lower layer**.
//! * Siblings at the same layer must not depend on each other, so no dependency
//!   edge ever points sideways.
//! * No layered crate may depend on `webnovel-core`, the crate being decomposed.
//!   A backward edge there would mean the split is leaking.
//!
//! Cargo metadata checks production package edges. Source-level ownership
//! checks supplement that graph; neither proves arbitrary SQL access is safe.

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// `(package name, path relative to the workspace root, layer)`.
pub const LAYERS: &[(&str, &str, u32)] = &[
    ("wns-kernel", "crates/kernel", 0),
    ("wns-storage", "crates/storage", 1),
    ("wns-providers", "crates/providers", 1),
    // Document history consumes storage primitives.
    ("wns-documents", "crates/documents", 2),
    ("wns-context", "crates/context", 3),
    ("wns-story", "crates/story", 4),
    ("wns-conversation", "crates/conversation", 5),
    ("wns-workshop", "crates/workshop", 5),
    // Transfer consumes the domain storage validators, including both L5 crates.
    ("wns-transfer", "crates/transfer", 6),
    // The library drives transfer's project-installation workflow.
    ("wns-library", "crates/library", 7),
];

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

/// Cargo resolves package aliases, workspace inheritance and every dependency
/// table. `--no-deps` avoids resolving external packages; the workspace package
/// declarations still include optional and target-specific dependencies. Do not
/// filter by the current platform: a Windows edge must also be checked on Linux.
pub fn workspace_metadata(manifest: &Path) -> Result<Metadata, cargo_metadata::Error> {
    MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .other_options(vec!["--locked".into(), "--offline".into()])
        .exec()
}

/// Normal and build dependencies define the production graph. Development
/// dependencies are deliberately excluded so test fixtures can exercise several
/// layers without introducing a production edge.
pub fn layering_violations(metadata: &Metadata) -> Vec<String> {
    let packages = metadata.workspace_packages();
    let internal_names: BTreeSet<_> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut violations = Vec::new();
    for package in packages {
        let Some(layer) = layer_of(package.name.as_str()) else {
            continue;
        };
        for dependency in &package.dependencies {
            if dependency.kind == DependencyKind::Development {
                continue;
            }
            // Cargo's `name` is the actual package; `rename` is only the local
            // import alias and must never decide which layer a dependency owns.
            let violation = match layer_of(&dependency.name) {
                Some(dependency_layer) if dependency_layer >= layer => {
                    Some(format!("L{dependency_layer} is not below L{layer}"))
                }
                None if internal_names.contains(dependency.name.as_str()) => {
                    Some("an unlayered workspace package is not a lower layer".into())
                }
                _ => None,
            };
            if let Some(reason) = violation {
                let target = dependency
                    .target
                    .as_ref()
                    .map(|target| format!(" for {target}"))
                    .unwrap_or_default();
                violations.push(format!(
                    "{} (L{layer}) -> {} ({} dependency{target}): {reason}",
                    package.name, dependency.name, dependency.kind,
                ));
            }
        }
    }
    violations.sort();
    violations
}
