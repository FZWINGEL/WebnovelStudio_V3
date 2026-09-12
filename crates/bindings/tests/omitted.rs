//! `OMITTED_WHEN_EMPTY` must be exactly what the Rust source says.
//!
//! The generator is *told* which fields Rust omits when empty, because specta
//! cannot infer it (`tests/variant_fields.rs` pins the four attribute spellings
//! that do not work). A hand-maintained list drifts; this re-derives it from the
//! source and fails when the two disagree, in either direction.
//!
//! Scanning Rust with a regex is normally a bad idea. It is acceptable here
//! because the pattern is one the compiler enforces — `#[serde(...)]` sits
//! directly above a `pub field: Type` — and because getting it wrong fails this
//! test loudly rather than silently generating a wrong type. It is a check on a
//! list, never a source of truth.

use std::path::PathBuf;

fn source_files(root: &std::path::Path) -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for crate_dir in std::fs::read_dir(root.join("crates")).into_iter().flatten().flatten() {
        let src = crate_dir.path().join("src");
        if src.is_dir() {
            walk(&src, &mut out);
        }
    }
    out
}

/// Every `(owner, field)` where a non-`Option` field carries a
/// `skip_serializing_if`.
fn from_source() -> Vec<(String, String)> {
    let root = wns_bindings::workspace_root();
    let mut pairs = Vec::new();
    for path in source_files(&root) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let mut owner: Option<String> = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("pub struct ").or_else(|| trimmed.strip_prefix("pub enum ")) {
                owner = Some(
                    rest.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                );
            } else if *line == "}" {
                owner = None;
            }
            if !line.contains("skip_serializing_if") {
                continue;
            }
            let Some(owner) = owner.as_ref() else { continue };
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_start().starts_with("#[") {
                j += 1;
            }
            let Some(field_line) = lines.get(j) else { continue };
            let Some(rest) = field_line.trim_start().strip_prefix("pub ") else { continue };
            let Some((name, ty)) = rest.split_once(':') else { continue };
            let ty = ty.trim();
            // A field that is already an `Option` is optional for a different
            // reason and needs no entry here.
            if ty.starts_with("Option<") {
                continue;
            }
            pairs.push((owner.clone(), name.trim().to_owned()));
        }
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

#[test]
fn the_omission_list_matches_the_rust_source() {
    let source: Vec<(String, String)> = from_source();
    let listed: Vec<(String, String)> = wns_bindings::OMITTED_WHEN_EMPTY
        .iter()
        .map(|(t, f)| ((*t).to_owned(), (*f).to_owned()))
        .collect();

    let missing: Vec<&(String, String)> = source.iter().filter(|p| !listed.contains(p)).collect();
    let extra: Vec<&(String, String)> = listed.iter().filter(|p| !source.contains(p)).collect();

    assert!(
        missing.is_empty(),
        "these fields are omitted when empty but are not in OMITTED_WHEN_EMPTY, so the \\
         generated TypeScript promises a value the wire may not carry: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "OMITTED_WHEN_EMPTY names fields that no longer exist or are now `Option`, so the \\
         generated TypeScript marks them optional for no reason: {extra:?}"
    );
}
