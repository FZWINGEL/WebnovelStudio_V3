use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use wns_bindings::{Group, group, output_differences, render_groups, write_groups};

const TEXT: &str = "export type Shared = { value: string }";
const CONFLICT: &str = "export type Shared = { value: number }";

fn binding(file: &'static str, crate_name: &'static str, text: &str) -> Group {
    group(file, crate_name, vec![("Example", Ok(text.into()))]).unwrap()
}

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wns-bindings-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn identical_declarations_are_deduplicated_within_a_group_and_allowed_across_groups() {
    let first = group(
        "first",
        "wns-first",
        vec![("One", Ok(TEXT.into())), ("Two", Ok(TEXT.into()))],
    )
    .unwrap();
    assert_eq!(first.types.len(), 1);
    let outputs = render_groups(&[first, binding("second", "wns-second", TEXT)]).unwrap();
    assert_eq!(outputs.len(), 2);
    assert!(
        outputs
            .iter()
            .all(|(_, text)| text.matches("export type Shared").count() == 1)
    );
}

#[test]
fn a_repeated_declaration_has_one_deterministic_import_owner() {
    let outputs = render_groups(&[
        binding("first", "wns-first", TEXT),
        binding("second", "wns-second", TEXT),
        binding(
            "consumer",
            "wns-consumer",
            "export type Uses = { value: Shared }",
        ),
    ])
    .unwrap();
    let consumer = &outputs[2].1;
    assert!(consumer.contains("import type { Shared } from './first';"));
    assert_eq!(consumer.matches("import type").count(), 1, "{consumer}");
    assert!(!consumer.contains("'./second'"));
}

#[test]
fn conflicting_declarations_name_both_exports_within_a_group() {
    let error = group(
        "first",
        "wns-first",
        vec![("One", Ok(TEXT.into())), ("Two", Ok(CONFLICT.into()))],
    )
    .unwrap_err()
    .to_string();
    for expected in ["Shared", "wns-first", "first.ts", "One", "Two"] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn conflicting_declarations_name_both_groups() {
    let error = render_groups(&[
        binding("first", "wns-first", TEXT),
        binding("second", "wns-second", CONFLICT),
    ])
    .unwrap_err()
    .to_string();
    for expected in ["Shared", "wns-first", "wns-second", "first.ts", "second.ts"] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn duplicate_output_filenames_are_rejected_even_with_identical_declarations() {
    let error = render_groups(&[
        binding("same", "wns-first", TEXT),
        binding("same", "wns-second", TEXT),
    ])
    .unwrap_err()
    .to_string();
    for expected in ["duplicate", "same.ts", "wns-first", "wns-second"] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn unsafe_output_filenames_are_rejected() {
    for file in [
        "../escape",
        "nested/file",
        "nested\\file",
        ".",
        "",
        "Alpha",
        "name.ts",
    ] {
        assert!(
            render_groups(&[binding(file, "wns-example", TEXT)]).is_err(),
            "{file}"
        );
    }
}

#[test]
fn drift_checks_missing_changed_and_extra_typescript_files() {
    let temp = TempDirectory::new();
    let groups = [
        binding("first", "wns-first", TEXT),
        binding("second", "wns-second", TEXT),
    ];
    write_groups(&temp.0, &groups).unwrap();
    assert!(output_differences(&temp.0, &groups).unwrap().is_empty());
    fs::remove_file(temp.0.join("first.ts")).unwrap();
    fs::write(temp.0.join("second.ts"), "changed").unwrap();
    fs::write(temp.0.join("obsolete.ts"), "unexpected module").unwrap();
    assert_eq!(
        output_differences(&temp.0, &groups).unwrap(),
        [
            "missing first.ts",
            "changed second.ts",
            "unexpected obsolete.ts"
        ]
    );
}

#[test]
fn writer_removes_only_obsolete_marked_typescript_in_its_output_root() {
    let temp = TempDirectory::new();
    let directory = temp.0.join("generated");
    let obsolete = binding("obsolete", "wns-old", TEXT);
    write_groups(&directory, &[obsolete]).unwrap();
    fs::write(directory.join("notes.txt"), "keep notes").unwrap();
    fs::write(temp.0.join("outside.ts"), "keep outside").unwrap();
    let groups = [binding("current", "wns-current", TEXT)];
    write_groups(&directory, &groups).unwrap();
    assert!(!directory.join("obsolete.ts").exists());
    assert!(output_differences(&directory, &groups).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(directory.join("notes.txt")).unwrap(),
        "keep notes"
    );
    assert_eq!(
        fs::read_to_string(temp.0.join("outside.ts")).unwrap(),
        "keep outside"
    );
}

#[test]
fn writer_refuses_foreign_files_before_changing_any_output() {
    for foreign in ["current.ts", "foreign.ts"] {
        let temp = TempDirectory::new();
        write_groups(&temp.0, &[binding("obsolete", "wns-old", TEXT)]).unwrap();
        let obsolete = fs::read(temp.0.join("obsolete.ts")).unwrap();
        fs::write(temp.0.join(foreign), "hand-written module").unwrap();
        let error = write_groups(&temp.0, &[binding("current", "wns-current", TEXT)])
            .unwrap_err()
            .to_string();
        assert!(error.contains("unowned"), "{error}");
        assert!(error.contains(foreign), "{error}");
        assert_eq!(
            fs::read_to_string(temp.0.join(foreign)).unwrap(),
            "hand-written module"
        );
        assert_eq!(fs::read(temp.0.join("obsolete.ts")).unwrap(), obsolete);
        if foreign != "current.ts" {
            assert!(!temp.0.join("current.ts").exists());
        }
    }
}

#[test]
fn declaration_conflicts_do_not_create_or_modify_the_output_directory() {
    let temp = TempDirectory::new();
    let directory = temp.0.join("generated");
    let groups = [
        binding("first", "wns-first", TEXT),
        binding("second", "wns-second", CONFLICT),
    ];
    assert!(write_groups(&directory, &groups).is_err());
    assert!(!directory.exists());
    write_groups(&directory, &[binding("first", "wns-first", TEXT)]).unwrap();
    let before = fs::read(directory.join("first.ts")).unwrap();
    assert!(write_groups(&directory, &groups).is_err());
    assert_eq!(fs::read(directory.join("first.ts")).unwrap(), before);
    assert!(!directory.join("second.ts").exists());
}

#[test]
fn writer_refuses_non_regular_and_case_ambiguous_typescript_outputs() {
    let temp = TempDirectory::new();
    fs::create_dir(temp.0.join("nested.ts")).unwrap();
    assert!(write_groups(&temp.0, &[binding("current", "wns-current", TEXT)]).is_err());
    assert!(!temp.0.join("current.ts").exists());
    fs::remove_dir(temp.0.join("nested.ts")).unwrap();
    let text = render_groups(&[binding("current", "wns-current", TEXT)])
        .unwrap()
        .remove(0)
        .1;
    fs::write(temp.0.join("Current.ts"), text).unwrap();
    assert!(write_groups(&temp.0, &[binding("current", "wns-current", TEXT)]).is_err());
}
