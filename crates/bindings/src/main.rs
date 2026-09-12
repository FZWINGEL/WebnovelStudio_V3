//! Write the generated bindings. Run from the workspace root:
//!
//! ```text
//! cargo run -p wns-bindings
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = wns_bindings::workspace_root().join(wns_bindings::OUTPUT_DIR);
    std::fs::create_dir_all(&directory)?;
    for group in wns_bindings::groups()? {
        let path = directory.join(format!("{}.ts", group.file));
        std::fs::write(&path, wns_bindings::render(&group))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
