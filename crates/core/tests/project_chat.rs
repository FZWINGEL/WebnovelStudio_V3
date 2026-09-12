use std::{fs,path::PathBuf};
use webnovel_core::projects::{ProjectAccess,ProjectSession};
use webnovel_core::projects::project_chat::*;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin,DiscussionFinish};

struct Temp(PathBuf);
impl Temp { fn new()->Self {let path=std::env::temp_dir().join(format!("wns-project-chat-{}",uuid::Uuid::new_v4()));fs::create_dir_all(&path).unwrap();Self(path)} }
impl Drop for Temp {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn setup()->(Temp,ProjectSession,ProjectAccess) {
    let temp=Temp::new();let project=ProjectSession::create(temp.0.join("project"),"Chat fixture").unwrap();let access=project.documents().attach("chat-test".into()).unwrap();(temp,project,access)
}

#[test]
fn manual_save_recap_uses_receipts_survives_reopen_and_does_not_create_chat_events() {
    use webnovel_core::projects::{CheckpointReason, CheckpointRequest, CreateDocument, SaveCause, SaveSnapshot};
    let (temp, project, access) = setup();
    let initial = read(&project, &access);
    let body = |text: &str| serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":text}]}]}});
    let document = project.documents().create(CreateDocument {
        access: access.clone(), operation_id: "recap-create".into(), document_id: "recap-note".into(),
        title: "Author note".into(), kind: "note".into(), body: body("First wording"),
    }).unwrap();
    let request = SaveSnapshot { access: access.clone(), operation_id: "recap-save".into(), expected: document.head, local_generation: "1".into(), body: body("Author revised wording"), cause: SaveCause::Typing };
    let ack = project.documents().save(request.clone()).unwrap();
    project.documents().save(request).unwrap();
    let checkpoint = project.documents().checkpoint(CheckpointRequest { access: access.clone(), expected: ack.head.clone(), reason: CheckpointReason::Manual }).unwrap();
    let epoch = project.context().source_epoch().unwrap();
    let view = read(&project, &access);
    assert_eq!(view.document_saves.len(), 1);
    assert_eq!(view.document_saves[0].head, ack.head);
    assert_eq!(view.document_saves[0].revision_id.as_deref(), Some(checkpoint.id.as_str()));
    assert_eq!(view.document_saves[0].operation_id, "recap-save");
    assert_eq!(view.items.len(), initial.items.len());
    assert_eq!(view.composer.version, initial.composer.version);
    assert_eq!(project.context().source_epoch().unwrap(), epoch);
    let mut wrong = access.clone(); wrong.operation_namespace = "wrong-namespace".into();
    assert!(project.read_project_conversation(ReadProjectConversation { access: wrong, before: None, limit: 40 }).is_err());
    let (_other_temp, other, other_access) = setup();
    assert!(read(&other, &other_access).document_saves.is_empty());
    drop(project);
    let reopened = ProjectSession::open(temp.0.join("project")).unwrap();
    let reopened_access = reopened.documents().attach("recap-reopen".into()).unwrap();
    let retained = read(&reopened, &reopened_access);
    assert_eq!(retained.document_saves.len(), 1);
    assert_eq!(retained.document_saves[0].head, ack.head);
    assert!(retained.items.is_empty());
}
fn read(project:&ProjectSession,access:&ProjectAccess)->ProjectConversation {
    project.read_project_conversation(ReadProjectConversation{access:access.clone(),before:None,limit:40}).unwrap()
}
fn save(project:&ProjectSession,access:&ProjectAccess,view:&ProjectConversation,text:&str)->ProjectComposerSnapshot {
    project.save_project_composer(SaveProjectComposer{access:access.clone(),operation_id:uuid::Uuid::new_v4().to_string(),conversation_id:view.id.clone(),expected_version:view.composer.version.clone(),body:ProjectComposer{text:text.into(),..Default::default()}}).unwrap()
}
fn start(access:&ProjectAccess,composer:ProjectComposerSnapshot)->StartProjectChat {
    StartProjectChat{access:access.clone(),operation_id:uuid::Uuid::new_v4().to_string(),conversation_id:composer.conversation_id,expected_composer_version:composer.version,composer:composer.body,budget:MockContextBudget::new("100000","8192","100"),provider_binding:None}
}

#[test]
fn activity_counts_pending_drafts_without_mutation_or_namespace_leak() {
    let (_temp,project,access)=setup();
    let initial=read(&project,&access);
    let epoch=project.context().source_epoch().unwrap();
    let composer=ProjectComposer{text:"Develop a provisional world note.".into(),..ProjectComposer::default()};
    let saved=project.save_project_composer(SaveProjectComposer{access:access.clone(),operation_id:"activity-save".into(),conversation_id:initial.id.clone(),expected_version:initial.composer.version,body:composer.clone()}).unwrap();
    let started=project.start_project_chat(StartProjectChat{access:access.clone(),operation_id:"activity-start".into(),conversation_id:initial.id.clone(),expected_composer_version:saved.version,composer,budget:MockContextBudget::new("100000","8192","100"),provider_binding:None}).unwrap();
    project.begin_discussion_run(DiscussionBegin{owner:started.run.owner.clone()}).unwrap();
    project.mark_discussion_delivered(started.run.owner.clone()).unwrap();
    let output=serde_json::json!({
        "schemaVersion":"project-assistant-output.v1",
        "answer":"A retained provisional note.",
        "questions":[],
        "assumptions":[],
        "drafts":[{"key":"world","title":"World note","kind":"world","changeSummary":"A pending setting note.","blocks":[{"type":"paragraph","content":[{"type":"text","text":"The harbor keeps its secrets."}]}]}]
    }).to_string();
    project.finish_discussion(DiscussionFinish{owner:started.run.owner.clone(),expected_sequence:"0".into(),event_id:"activity-finish".into(),assistant_text:output}).unwrap();
    project.materialize_chat_result(started.run.owner).unwrap().unwrap();

    let before=read(&project,&access);
    let snapshot=project.project_chat_activity().unwrap();
    assert_eq!(snapshot.pending_drafts,1);
    assert_eq!(project.context().source_epoch().unwrap(),epoch);
    let after=read(&project,&access);
    assert_eq!(after.composer.version,before.composer.version);
    assert_eq!(after.items.len(),before.items.len());
    assert_eq!(after.drafts.len(),before.drafts.len());

    let (_other_temp,other,_other_access)=setup();
    assert_eq!(other.project_chat_activity().unwrap().pending_drafts,0);
    assert_ne!(project.info.project_id,other.info.project_id);
    assert_ne!(project.info.operation_namespace,other.info.operation_namespace);
}

#[test]
fn blank_project_has_one_conversation_without_a_story_document_or_source_change() {
    let (_temp,project,access)=setup();let epoch=project.context().source_epoch().unwrap();
    let first=read(&project,&access);let second=read(&project,&access);
    assert_eq!(first.id,second.id);assert!(first.items.is_empty());assert!(first.composer.body.text.is_empty());
    assert!(project.documents().list(access).unwrap().is_empty());assert_eq!(project.context().source_epoch().unwrap(),epoch);
}

#[test]
fn composer_is_versioned_recoverable_and_does_not_change_story_context() {
    let (temp,project,access)=setup();let view=read(&project,&access);let epoch=project.context().source_epoch().unwrap();
    let request=SaveProjectComposer{access:access.clone(),operation_id:"save-one".into(),conversation_id:view.id.clone(),expected_version:"0".into(),body:ProjectComposer{text:"A city where people trade memories.".into(),..Default::default()}};
    let first=project.save_project_composer(request.clone()).unwrap();assert_eq!(first.version,"1");
    assert_eq!(project.save_project_composer(request.clone()).unwrap().version,first.version);
    let mut stale=request;stale.operation_id="save-two".into();stale.body.text="Unsent newer buffer".into();
    assert_eq!(project.save_project_composer(stale).unwrap_err().code,"VersionConflict");assert_eq!(project.context().source_epoch().unwrap(),epoch);
    drop(project);let reopened=ProjectSession::open(temp.0.join("project")).unwrap();let access=reopened.documents().attach("reopened".into()).unwrap();
    assert_eq!(read(&reopened,&access).composer.body.text,"A city where people trade memories.");
}

#[test]
fn first_idea_acceptance_links_run_and_clears_only_the_accepted_composer_atomically() {
    let (_temp,project,access)=setup();let view=read(&project,&access);let saved=save(&project,&access,&view,"Help me develop a travelling healer.");
    let request=start(&access,saved);let accepted=project.start_project_chat(request.clone()).unwrap();
    let retry=project.start_project_chat(request.clone()).unwrap();assert_eq!(accepted.run.id,retry.run.id);
    assert_eq!(accepted.run.intent,webnovel_core::projects::discussions::FeedbackIntent::Discuss);
    let next=read(&project,&access);assert!(next.composer.body.text.is_empty());assert_eq!(next.composer.version,"2");
    assert_eq!(next.items.iter().filter(|i|i.kind=="request").count(),1);assert!(project.documents().list(access.clone()).unwrap().is_empty());
    let envelope:serde_json::Value=serde_json::from_str(&accepted.packet.messages[1].content).unwrap();
    assert_eq!(envelope["projectChat"]["conversationId"],view.id);assert_eq!(accepted.packet.messages.last().unwrap().content,request.composer.text);
    let next_saved=save(&project,&access,&next,"A second idea while the first runs.");
    assert_eq!(project.start_project_chat(start(&access,next_saved)).unwrap_err().code,"ProjectChatBusy");
    assert_eq!(read(&project,&access).composer.body.text,"A second idea while the first runs.");
    let mut changed=request;changed.composer.text="Different input".into();
    assert_eq!(project.start_project_chat(changed).unwrap_err().code,"OperationIdReusedWithDifferentPayload");
}

#[test]
fn failed_packet_acceptance_keeps_the_composer_and_has_no_run_link() {
    let (_temp,project,access)=setup();let view=read(&project,&access);let saved=save(&project,&access,&view,"A request that cannot fit.");
    let mut request=start(&access,saved);request.budget=MockContextBudget::new("10","5","5");
    assert!(project.start_project_chat(request).is_err());let current=read(&project,&access);
    assert_eq!(current.composer.body.text,"A request that cannot fit.");assert!(!current.items.iter().any(|i|i.kind=="request"));assert!(current.active_run.is_none());
}

#[test]
fn another_project_cannot_read_or_send_into_the_conversation() {
    let (temp,project,access)=setup();let view=read(&project,&access);
    let other=ProjectSession::create(temp.0.join("other"),"Other project").unwrap();let other_access=other.documents().attach("other".into()).unwrap();
    assert_eq!(other.save_project_composer(SaveProjectComposer{access:other_access,operation_id:"wrong".into(),conversation_id:view.id,expected_version:"0".into(),body:ProjectComposer::default()}).unwrap_err().code,"WrongProjectConversation");
}
