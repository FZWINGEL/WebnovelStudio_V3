//! The layering rule, checked against the real manifests.
//!
//! Run with the rest of the suite: `cargo test -p wns-architecture`.

use std::path::PathBuf;
use wns_architecture::{
    LAYERS, LEGACY_CRATE, internal_dependencies, layer_of, workspace_root,
};

#[test]
fn every_layered_crate_exists_with_its_manifest() {
    let root = workspace_root();
    for (name, path, _) in LAYERS {
        let manifest = root.join(path).join("Cargo.toml");
        assert!(
            manifest.is_file(),
            "{name} is declared at layer but has no manifest at {}",
            manifest.display()
        );
    }
}

#[test]
fn the_workspace_declares_every_layered_crate_as_a_member() {
    let root = workspace_root();
    let workspace = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("workspace manifest must be readable");
    for (name, path, _) in LAYERS {
        assert!(
            workspace.contains(&format!("\"{path}\"")),
            "{name} lives at {path} but the workspace `members` list does not include it"
        );
    }
}

#[test]
fn a_crate_depends_only_on_strictly_lower_layers() {
    let root = workspace_root();
    let mut violations = Vec::new();
    for (name, path, layer) in LAYERS {
        let manifest = root.join(path).join("Cargo.toml");
        for (dependency, _) in internal_dependencies(&manifest) {
            if dependency == LEGACY_CRATE {
                violations.push(format!(
                    "{name} (L{layer}) depends on {LEGACY_CRATE} — a layered crate must not \
                     depend on the crate being decomposed"
                ));
                continue;
            }
            let Some(dependency_layer) = layer_of(&dependency) else {
                // A workspace-internal crate with no declared layer is either the
                // app shell or not yet registered; neither is a layering error.
                continue;
            };
            if dependency_layer >= *layer {
                let relation = if dependency_layer == *layer {
                    "a sibling at the same layer"
                } else {
                    "a higher layer"
                };
                violations.push(format!(
                    "{name} (L{layer}) depends on {dependency} (L{dependency_layer}) — {relation}. \
                     A boundary that needs a sideways edge is the wrong boundary."
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "layering violations:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn no_layered_crate_depends_on_the_legacy_crate() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    for (name, path, _) in LAYERS {
        let manifest = root.join(path).join("Cargo.toml");
        if internal_dependencies(&manifest).contains_key(LEGACY_CRATE) {
            offenders.push(format!("{name} -> {LEGACY_CRATE}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "backward edges into the crate under decomposition:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn the_rule_can_actually_fail() {
    // A check that cannot fail is not a check. This pins the parser's behaviour
    // against a manifest that violates the rule, so a future refactor that
    // neuters `internal_dependencies` breaks here rather than silently passing.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("violating.Cargo.toml");
    let parsed = internal_dependencies(&manifest);
    assert!(
        parsed.contains_key("wns-story"),
        "the fixture's wns-* dependency must be parsed, got {parsed:?}"
    );
    assert!(
        parsed.contains_key(LEGACY_CRATE),
        "the fixture's legacy dependency must be parsed, got {parsed:?}"
    );
    assert_eq!(layer_of("wns-story"), Some(3));
    assert_eq!(layer_of("wns-kernel"), Some(0));
    assert_eq!(layer_of("ends-with-kernel"), None);
}
