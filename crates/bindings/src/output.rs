use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::{BindingResult, Group, render_groups, valid_file_stem};

/// Compare both the TypeScript filename inventory and each file's contents.
pub fn output_differences(directory: &Path, groups: &[Group]) -> BindingResult<Vec<String>> {
    let expected = render_groups(groups)?;
    let actual = read_outputs(directory)?;
    let mut differences = Vec::new();
    for (name, text) in &expected {
        match actual.get(name) {
            None => differences.push(format!("missing {name}")),
            Some(committed) if committed != text => differences.push(format!("changed {name}")),
            _ => {}
        }
    }
    for name in actual.keys() {
        if !expected.iter().any(|(expected, _)| expected == name) {
            differences.push(format!("unexpected {name}"));
        }
    }
    Ok(differences)
}

/// Reconcile only this generator's direct, marked TypeScript outputs.
/// All declarations, filenames and existing files are checked before any write.
pub fn write_groups(directory: &Path, groups: &[Group]) -> BindingResult<()> {
    let expected = render_groups(groups)?;
    let actual = read_outputs(directory)?;
    for (name, text) in &actual {
        if !is_generated(text) {
            return Err(format!(
                "refusing to overwrite or remove unowned binding file: {}",
                directory.join(name).display()
            )
            .into());
        }
    }

    fs::create_dir_all(directory)?;
    for (name, text) in &expected {
        fs::write(directory.join(name), text)?;
    }
    for name in actual.keys() {
        if !expected.iter().any(|(expected, _)| expected == name) {
            fs::remove_file(directory.join(name))?;
        }
    }
    Ok(())
}

fn is_generated(text: &str) -> bool {
    let mut lines = text.lines();
    lines.next().is_some_and(|line| {
        line.starts_with("// Generated from `")
            && line.ends_with("` by `crates/bindings`. Do not edit.")
    }) && lines.next() == Some("// Change the Rust type and run `cargo run -p wns-bindings`.")
}

fn read_outputs(directory: &Path) -> BindingResult<BTreeMap<String, String>> {
    match fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
            return Err(format!(
                "binding output root must be a directory, not a link: {}",
                directory.display()
            )
            .into());
        }
        Ok(_) => {}
    }
    let root = fs::canonicalize(directory)?;
    let mut outputs = BTreeMap::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
        {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "binding filename is not UTF-8")?;
        if !name.strip_suffix(".ts").is_some_and(valid_file_stem) {
            return Err(format!("invalid existing binding output filename: {name}").into());
        }
        // Never follow a file link or recurse: cleanup is confined to this root.
        if !entry.file_type()?.is_file()
            || fs::canonicalize(&path)?.parent() != Some(root.as_path())
        {
            return Err(format!(
                "binding output must be a regular file within {}: {name}",
                root.display()
            )
            .into());
        }
        outputs.insert(name, fs::read_to_string(path)?);
    }
    Ok(outputs)
}
