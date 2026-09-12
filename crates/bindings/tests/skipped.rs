//! `SKIPPED_WHEN_NONE` must be exactly what the Rust source says.
//!
//! The counterpart of `tests/omitted.rs`, for the other half of the same
//! problem. There, specta fails to mark a field optional that the wire leaves
//! out; here it marks a field nullable that the wire never sends as `null`.
//!
//! `skip_serializing_if = "Option::is_none"` means `None` is *absent*, so
//! `applied?: AppliedDecision | null` promises a value that cannot arrive — and
//! a frontend that believes it writes a `null` branch nothing can reach, or
//! passes `null` where the request can simply omit the field. specta reads
//! every `Option` as `| null` and cannot be told otherwise, so the suffix is
//! removed from a list, and this re-derives the list from the source.
//!
//! The same caveat as `omitted.rs` applies to scanning Rust with a regex: it is
//! acceptable because the pattern is one the compiler enforces, and because
//! getting it wrong fails this test loudly rather than generating a wrong type.

use std::path::{Path, PathBuf};

fn source_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
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

/// Every `(owner, field)` where an `Option` field's `None` is skipped.
fn from_source() -> Vec<(String, String)> {
    let root = wns_bindings::workspace_root();
    let mut pairs = Vec::new();
    for path in source_files(&root) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let mut owner: Option<String> = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("pub struct ") {
                owner = Some(rest.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or_default().to_owned());
            } else if trimmed.starts_with("pub enum ") || *line == "}" {
                // An enum variant's field is not `pub`, and the `Option` a
                // variant wraps is the variant itself rather than a field the
                // struct can omit. `omitted.rs` covers the empty-`Vec` case
                // there; this one is about `pub` fields.
                owner = None;
            }
            if !line.contains("skip_serializing_if") {
                continue;
            }
            let Some(owner) = owner.as_ref() else { continue };
            // The whole attribute run, so the predicate can be read from it —
            // `Option::is_none` here, `Vec::is_empty` and friends in
            // `omitted.rs`. They are disjoint, and a field belongs to exactly
            // one: absent-not-null, or absent-when-empty.
            let mut run = String::new();
            let mut j = i;
            while j < lines.len() && lines[j].trim_start().starts_with("#[") {
                run.push_str(lines[j]);
                j += 1;
            }
            if !run.contains("Option::is_none") {
                continue;
            }
            let Some(rest) = lines.get(j).and_then(|line| line.trim_start().strip_prefix("pub ")) else { continue };
            let Some((name, ty)) = rest.split_once(':') else { continue };
            if !ty.trim().starts_with("Option<") {
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
fn the_skipped_list_matches_the_rust_source() {
    let source: Vec<(String, String)> = from_source();
    let listed: Vec<(String, String)> = wns_bindings::SKIPPED_WHEN_NONE
        .iter()
        .map(|(t, f)| ((*t).to_owned(), (*f).to_owned()))
        .collect();

    let missing: Vec<&(String, String)> = source.iter().filter(|p| !listed.contains(p)).collect();
    let extra: Vec<&(String, String)> = listed.iter().filter(|p| !source.contains(p)).collect();

    assert!(
        missing.is_empty(),
        "these fields are absent from the wire when unset, so the generated \\
         TypeScript still offers a `| null` the wire never sends: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "SKIPPED_WHEN_NONE names fields that are not skipped when None, so the \\
         generated TypeScript has dropped a `| null` the wire really carries: {extra:?}"
    );
}
