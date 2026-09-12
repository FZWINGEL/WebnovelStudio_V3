//! Write the generated bindings. Run from the workspace root:
//!
//! ```text
//! cargo run -p wns-bindings
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = wns_bindings::workspace_root().join(wns_bindings::OUTPUT_DIR);
    std::fs::create_dir_all(&directory)?;
    for (name, text) in wns_bindings::render_all()? {
        let path = directory.join(&name);
        std::fs::write(&path, text)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
