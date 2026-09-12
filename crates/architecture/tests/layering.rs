//! Validate the production graph using one Cargo metadata snapshot per workspace.

use cargo_metadata::{DependencyKind, Metadata};
use std::path::Path;
use std::sync::OnceLock;
use wns_architecture::{LAYERS, layering_violations, workspace_metadata, workspace_root};

fn metadata() -> &'static Metadata {
    static METADATA: OnceLock<Metadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        workspace_metadata(&workspace_root().join("Cargo.toml")).expect("read workspace metadata")
    })
}

fn violating_metadata() -> &'static Metadata {
    static METADATA: OnceLock<Metadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        workspace_metadata(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/violating/Cargo.toml"),
        )
        .expect("read isolated violating workspace metadata")
    })
}

#[test]
fn every_layered_package_has_its_declared_identity_and_workspace_membership() {
    let packages = metadata().workspace_packages();
    for (name, path, _) in LAYERS {
        let package = packages
            .iter()
            .find(|package| package.name.as_str() == *name)
            .unwrap_or_else(|| panic!("{name} must be a workspace member"));
        assert_eq!(
            package.manifest_path.as_std_path().canonicalize().unwrap(),
            workspace_root()
                .join(path)
                .join("Cargo.toml")
                .canonicalize()
                .unwrap(),
            "{name} must be the package declared at {path}",
        );
    }
    for package in packages {
        assert!(
            matches!(
                package.name.as_str(),
                "contracts"
                    | "wns-architecture"
                    | "wns-bindings"
                    | "webnovel-core"
                    | "webnovel-desktop"
            ) || LAYERS
                .iter()
                .any(|(name, _, _)| *name == package.name.as_str()),
            "{} must be assigned a layer before joining the workspace",
            package.name,
        );
    }
}

#[test]
fn production_dependencies_point_only_to_strictly_lower_layers() {
    let violations = layering_violations(metadata());
    assert!(
        violations.is_empty(),
        "layering violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn aliases_inheritance_optional_and_target_dependencies_cannot_hide_violations() {
    let violations = layering_violations(violating_metadata());
    assert_eq!(violations.len(), 6, "{violations:#?}");
    for edge in [
        "wns-context (L3) -> wns-story", // optional, aliased dependency subtable
        "wns-context (L3) -> webnovel-core", // Windows-only alias
        "wns-context (L3) -> wns-workshop", // build dependency
        "wns-storage (L1) -> wns-providers", // same-layer alias
        "wns-kernel (L0) -> wns-story",  // workspace-inherited alias
        "wns-providers (L1) -> wns-story", // Linux-only alias
    ] {
        assert!(
            violations
                .iter()
                .any(|violation| violation.starts_with(edge)),
            "missing {edge}: {violations:#?}"
        );
    }
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("for cfg(windows)"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("for cfg(target_os = \"linux\")"))
    );
}

#[test]
fn development_dependencies_are_explicitly_outside_the_production_graph() {
    let fixture = violating_metadata();
    let story = fixture
        .packages
        .iter()
        .find(|package| package.name == "wns-story")
        .unwrap();
    assert!(story.dependencies.iter().any(|dependency| {
        dependency.name == "webnovel-core" && dependency.kind == DependencyKind::Development
    }));
    assert!(
        !layering_violations(fixture)
            .iter()
            .any(|violation| violation.starts_with("wns-story "))
    );
}

#[test]
fn ubuntu_contracts_select_the_workspace_including_extracted_unit_tests() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    let workflow = workflow.replace("\r\n", "\n");
    let contracts = workflow
        .split("  contracts:\n")
        .nth(1)
        .unwrap()
        .split("  windows-native:\n")
        .next()
        .unwrap();
    for command in ["test", "clippy"] {
        let prefix = format!("- run: cargo {command} ");
        let commands: Vec<_> = contracts
            .lines()
            .filter_map(|line| line.trim().strip_prefix(&prefix))
            .collect();
        assert_eq!(
            commands.len(),
            1,
            "expected one Ubuntu cargo {command} command"
        );
        let flags: Vec<_> = commands[0].split_whitespace().collect();
        assert!(
            flags.contains(&"--workspace"),
            "Ubuntu cargo {command} must select dependency packages' tests too"
        );
        assert!(
            !flags
                .iter()
                .any(|flag| *flag == "-p" || flag.starts_with("--package"))
        );
        let exclusions: Vec<_> = flags
            .windows(2)
            .filter_map(|pair| (pair[0] == "--exclude").then_some(pair[1]))
            .collect();
        assert_eq!(
            exclusions,
            ["webnovel-desktop"],
            "only the native Tauri package is excluded on Ubuntu"
        );
        if command == "clippy" {
            assert!(flags.contains(&"--all-targets"));
        }
    }
    assert!(
        metadata()
            .workspace_packages()
            .iter()
            .any(|package| package.name == "webnovel-desktop")
    );
}
