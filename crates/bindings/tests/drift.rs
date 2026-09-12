//! Fails when a Rust type changed without the bindings being regenerated.
//!
//! This is the test the hand-written mirrors never had. A renamed Rust field
//! used to surface as a runtime `undefined` in the renderer; it is a build
//! failure now, and the message says how to fix it.

#[test]
fn generated_bindings_match_the_rust_types() {
    let directory = wns_bindings::workspace_root().join(wns_bindings::OUTPUT_DIR);
    let groups = wns_bindings::groups().expect("collect binding groups");
    let stale = wns_bindings::output_differences(&directory, &groups)
        .expect("compare generated binding inventory");
    assert!(
        stale.is_empty(),
        "the generated bindings are stale: {}\nRust is the source of truth. \
         Run `cargo run -p wns-bindings` and commit the result.",
        stale.join(", ")
    );
}

#[test]
fn generated_command_manifest_matches_independent_source_regeneration() {
    let root = wns_bindings::workspace_root();
    let expected = wns_bindings::commands::manifest().expect("derive command manifest");
    let path = root.join("apps/desktop/src/ipc/tauriCommands.generated.json");
    let stale = wns_bindings::commands::manifest_differences(&path, &expected)
        .expect("compare generated command manifest");
    assert!(
        stale.is_empty(),
        "the generated command manifest is stale: {}\nRun `cargo run -p wns-bindings` and commit the result.",
        stale.join(", ")
    );
}
