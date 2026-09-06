use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use uuid::Uuid;
use webnovel_core::{transfer::save_recovery_copy, validate_snapshot_json};

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-recovery-copy-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn body() -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[
        {"type":"heading","attrs":{"id":"h","level":2},"content":[{"type":"text","text":"Unsaved chapter"}]},
        {"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":"Mara’s vow 🌙","marks":[{"type":"bold"}]},{"type":"hardBreak"},{"type":"text","text":"Still here."}]},
        {"type":"sceneBreak","attrs":{"id":"s"}},
        {"type":"paragraph","attrs":{"id":"end"}}
    ]}})
}

#[test]
fn recovery_preserves_captured_prose_without_any_project_storage() {
    let temp = TempDir::new();
    // This path contains no project database, registry, or identity marker.
    let original = body();
    let path = temp.0.join("recovery.md");
    let receipt = save_recovery_copy(&original, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "## Unsaved chapter\n\n**Mara’s vow 🌙**  \nStill here.\n\n---\n\n"
    );
    assert_eq!(receipt.utf8_bytes, text.len() as u64);
    assert_eq!(
        receipt.snapshot_hash,
        validate_snapshot_json(&original.to_string()).unwrap().hash
    );
    assert_eq!(body(), original);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn recovery_never_overwrites_an_existing_file() {
    let temp = TempDir::new();
    let path = temp.0.join("recovery.md");
    fs::write(&path, "Existing work").unwrap();
    assert_eq!(
        save_recovery_copy(&body(), &path).unwrap_err().code,
        "TargetExists"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), "Existing work");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn invalid_snapshot_leaves_no_copy_or_staging_files() {
    let temp = TempDir::new();
    let mut invalid = body();
    invalid["body"]["content"][1]["content"][0]["marks"] = json!([{"type":"unsafe"}]);
    assert_eq!(
        save_recovery_copy(&invalid, &temp.0.join("recovery.md"))
            .unwrap_err()
            .code,
        "InvalidDocument"
    );
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn unavailable_destination_does_not_prevent_a_later_explicit_copy() {
    let temp = TempDir::new();
    let snapshot = body();
    assert!(save_recovery_copy(&snapshot, &temp.0.join("missing/recovery.md")).is_err());
    assert!(!temp.0.join("missing").exists());
    assert_eq!(snapshot, body());
    save_recovery_copy(&snapshot, &temp.0.join("retry.md")).unwrap();
    assert!(temp.0.join("retry.md").is_file());
}

#[test]
fn recovery_rejects_device_stream_and_ambiguous_filenames() {
    let temp = TempDir::new();
    for name in ["CON.md", "chapter.md:stream", "chapter.md.", "chapter.md "] {
        assert!(
            save_recovery_copy(&body(), &temp.0.join(name)).is_err(),
            "accepted {name}"
        );
    }
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}
