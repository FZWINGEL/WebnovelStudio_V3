use super::*;

#[test]
fn preview_digest_excludes_digest_field() {
    let preview = ChatAdoptionPreview {
        id: "preview".into(),
        version: "1".into(),
        digest: String::new(),
        project_id: "project".into(),
        operation_namespace: "namespace".into(),
        conversation_id: "conversation".into(),
        source_epoch: "0".into(),
        policy_epoch: "0".into(),
        workshop_version: "0".into(),
        targets: vec![],
        effects: None,
    };
    let digest = preview_digest(&preview).expect("digest");
    let mut with_digest = preview.clone();
    with_digest.digest = "other".into();
    assert_eq!(digest, preview_digest(&with_digest).expect("digest"));
}
