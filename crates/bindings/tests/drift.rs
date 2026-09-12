//! Fails when a Rust type changed without the bindings being regenerated.
//!
//! This is the test the hand-written mirrors never had. A renamed Rust field
//! used to surface as a runtime `undefined` in the renderer; it is a build
//! failure now, and the message says how to fix it.

#[test]
fn generated_bindings_match_the_rust_types() {
    let directory = wns_bindings::workspace_root().join(wns_bindings::OUTPUT_DIR);
    let mut stale = Vec::new();
    for group in wns_bindings::groups().expect("generate bindings") {
        let path = directory.join(format!("{}.ts", group.file));
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed != wns_bindings::render(&group) {
            stale.push(path.display().to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "the generated bindings are stale: {}\nRust is the source of truth. \
         Run `cargo run -p wns-bindings` and commit the result.",
        stale.join(", ")
    );
}
