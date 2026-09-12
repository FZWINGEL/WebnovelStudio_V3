//! Write the generated bindings. Run from the workspace root:
//!
//! ```text
//! cargo run -p wns-bindings
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = wns_bindings::workspace_root();
    // Derive and validate the command contract before touching any generated
    // output.  This makes a stale or ambiguous registration fail closed.
    let command_manifest = wns_bindings::commands::manifest()?;
    let directory = root.join(wns_bindings::OUTPUT_DIR);
    wns_bindings::write_groups(&directory, &wns_bindings::groups()?)?;
    wns_bindings::commands::write_manifest(
        &root.join("apps/desktop/src/ipc/tauriCommands.generated.json"),
        &command_manifest,
    )?;
    println!("generated bindings in {}", directory.display());
    Ok(())
}
