use serde_json::{Value, json};
use webnovel_core::providers::codex_exec::{
    CodexEvent, CodexFailureCode, CodexJsonlParser, MAX_EVENTS, MAX_LINE_BYTES, MAX_TEXT_BYTES,
    MAX_TOTAL_BYTES, MAX_WARNINGS,
};

fn line(value: Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&value).expect("JSON");
    bytes.push(b'\n');
    bytes
}

fn usage() -> Value {
    json!({
        "input_tokens": 10,
        "cached_input_tokens": 2,
        "output_tokens": 5,
        "reasoning_output_tokens": 3
    })
}

fn valid_stream(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(line(
        json!({"type":"thread.started","thread_id":"thread-1"}),
    ));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed",
        "item":{"id":"message-1","type":"agent_message","text":text}
    })));
    bytes.extend(line(json!({"type":"turn.completed","usage":usage()})));
    bytes
}

fn feed(parser: &mut CodexJsonlParser, bytes: &[u8]) -> Vec<CodexEvent> {
    parser.feed(bytes).expect("feed")
}

#[test]
fn accepts_split_utf8_and_emits_only_assistant_text() {
    let bytes = valid_stream("海🙂");
    let mut parser = CodexJsonlParser::new();
    let mut events = Vec::new();
    for byte in bytes {
        events.extend(feed(&mut parser, &[byte]));
    }
    assert_eq!(events, vec![CodexEvent::AssistantDelta("海🙂".to_owned())]);
    let outcome = parser.finish(0).expect("success");
    assert_eq!(outcome.assistant_text, "海🙂");
    assert_eq!(outcome.usage.cache_write_input_tokens, 0);
}

#[test]
fn accepts_crlf_and_final_record_without_newline() {
    let mut bytes = Vec::new();
    for value in [
        json!({"type":"thread.started","thread_id":"thread-1"}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"id":"m","type":"agent_message","text":"done"}}),
    ] {
        let mut part = serde_json::to_vec(&value).expect("JSON");
        part.extend_from_slice(b"\r\n");
        bytes.extend(part);
    }
    bytes.extend(
        serde_json::to_vec(&json!({"type":"turn.completed","usage":usage()})).expect("JSON"),
    );
    let mut parser = CodexJsonlParser::new();
    let events = feed(&mut parser, &bytes);
    assert_eq!(events, vec![CodexEvent::AssistantDelta("done".to_owned())]);
    assert_eq!(parser.finish(0).expect("success").assistant_text, "done");
}

#[test]
fn snapshots_emit_only_append_only_suffixes_and_exact_repeats_are_noops() {
    let mut parser = CodexJsonlParser::new();
    let mut events = Vec::new();
    events.extend(feed(
        &mut parser,
        &line(json!({"type":"thread.started","thread_id":"t"})),
    ));
    events.extend(feed(&mut parser, &line(json!({"type":"turn.started"}))));
    events.extend(feed(
        &mut parser,
        &line(json!({
            "type":"item.started","item":{"id":"m","type":"agent_message","text":"Hel"}
        })),
    ));
    events.extend(feed(
        &mut parser,
        &line(json!({
            "type":"item.updated","item":{"id":"m","type":"agent_message","text":"Hello"}
        })),
    ));
    events.extend(feed(
        &mut parser,
        &line(json!({
            "type":"item.updated","item":{"id":"m","type":"agent_message","text":"Hello"}
        })),
    ));
    events.extend(feed(
        &mut parser,
        &line(json!({
            "type":"item.completed","item":{"id":"m","type":"agent_message","text":"Hello"}
        })),
    ));
    events.extend(feed(
        &mut parser,
        &line(json!({"type":"turn.completed","usage":usage()})),
    ));
    assert_eq!(
        events,
        vec![
            CodexEvent::AssistantDelta("Hel".to_owned()),
            CodexEvent::AssistantDelta("lo".to_owned())
        ]
    );
    assert_eq!(parser.finish(0).expect("success").assistant_text, "Hello");
}

#[test]
fn accepts_completed_only_items_and_does_not_display_reasoning() {
    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"r","type":"reasoning","text":"hidden thought"}
    })));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"m","type":"agent_message","text":"visible"}
    })));
    bytes.extend(line(json!({"type":"turn.completed","usage":usage()})));
    let events = feed(&mut parser, &bytes);
    assert_eq!(
        events,
        vec![CodexEvent::AssistantDelta("visible".to_owned())]
    );
    assert_eq!(parser.finish(0).expect("success").assistant_text, "visible");
}

#[test]
fn item_error_warning_is_allowed_before_turn_and_is_bounded() {
    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"w","type":"error","message":"SECRET_WARNING C:\\Users\\alice\\private.txt"}
    })));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"m","type":"agent_message","text":"answer"}
    })));
    bytes.extend(line(json!({"type":"turn.completed","usage":usage()})));
    let events = feed(&mut parser, &bytes);
    assert_eq!(
        events,
        vec![
            CodexEvent::Warning("provider warning".to_owned()),
            CodexEvent::AssistantDelta("answer".to_owned())
        ]
    );
    assert_eq!(
        parser.finish(0).expect("success").warnings,
        vec!["provider warning".to_owned()]
    );
    assert!(!events.iter().any(|event| {
        matches!(event, CodexEvent::Warning(warning) if warning.contains("SECRET_WARNING") || warning.contains("private.txt"))
    }));
}

#[test]
fn refuses_tool_action_and_unknown_item_types() {
    let mut parser = CodexJsonlParser::new();
    let bytes = [
        line(json!({"type":"thread.started","thread_id":"t"})),
        line(json!({"type":"turn.started"})),
        line(json!({"type":"item.completed","item":{"id":"x","type":"command_execution","text":"run"}})),
    ]
    .concat();
    let failure = parser.feed(&bytes).expect_err("unsupported item");
    assert_eq!(failure.code, CodexFailureCode::UnsupportedItem);
    assert!(parser.feed(b"not used").is_err(), "failure must be sticky");
}

#[test]
fn refuses_out_of_order_frames_and_terminal_conflicts() {
    let mut parser = CodexJsonlParser::new();
    let failure = parser
        .feed(&line(json!({"type":"turn.started"})))
        .expect_err("turn before thread");
    assert_eq!(failure.code, CodexFailureCode::Protocol);

    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"m","type":"agent_message","text":"one"}
    })));
    let _ = parser.feed(&bytes).expect("prefix");
    let failure = parser
        .feed(&line(json!({
            "type":"item.completed","item":{"id":"m","type":"agent_message","text":"two"}
        })))
        .expect_err("changed terminal snapshot");
    assert_eq!(failure.code, CodexFailureCode::Protocol);
    assert_eq!(failure.partial_output, "one");
}

#[test]
fn preserves_partial_output_for_provider_failure_and_nonzero_exit() {
    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"m","type":"agent_message","text":"partial"}
    })));
    bytes.extend(line(json!({
        "type":"error",
        "message":"SECRET_TOKEN C:\\Users\\alice\\private-output.txt"
    })));
    let failure = parser.feed(&bytes).expect_err("provider failure");
    assert_eq!(failure.code, CodexFailureCode::ProviderFailure);
    assert_eq!(failure.partial_output, "partial");
    assert!(!failure.detail.contains("SECRET_TOKEN"));
    assert!(!failure.detail.contains("private-output.txt"));
    assert!(parser.finish(0).is_err(), "provider failure is sticky");

    let mut parser = CodexJsonlParser::new();
    let mut events = Vec::new();
    events.extend(feed(&mut parser, &valid_stream("partial")));
    assert_eq!(
        events,
        vec![CodexEvent::AssistantDelta("partial".to_owned())]
    );
    let failure = parser.finish(7).expect_err("nonzero exit");
    assert_eq!(failure.code, CodexFailureCode::NonZeroExit);
    assert_eq!(failure.partial_output, "partial");
}

#[test]
fn validates_usage_and_requires_one_completed_nonempty_assistant_message() {
    for field in [
        "input_tokens",
        "cached_input_tokens",
        "cache_write_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
    ] {
        let mut parser = CodexJsonlParser::new();
        let mut bytes = valid_stream("answer");
        let mut usage_value = usage();
        usage_value[field] = json!(-1);
        let replacement = json!({
            "type":"turn.completed",
            "usage": usage_value
        });
        let start = bytes.len() - line(json!({"type":"turn.completed","usage":usage()})).len();
        bytes.truncate(start);
        bytes.extend(line(replacement));
        let failure = parser.feed(&bytes).expect_err("negative usage");
        assert_eq!(failure.code, CodexFailureCode::InvalidUsage, "{field}");
    }

    for field in [
        "input_tokens",
        "cached_input_tokens",
        "cache_write_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
    ] {
        let mut parser = CodexJsonlParser::new();
        let mut bytes = valid_stream("answer");
        let mut usage_value = usage();
        usage_value[field] = json!(9223372036854775808u64);
        let replacement = json!({
            "type":"turn.completed",
            "usage": usage_value
        });
        let start = bytes.len() - line(json!({"type":"turn.completed","usage":usage()})).len();
        bytes.truncate(start);
        bytes.extend(line(replacement));
        let failure = parser.feed(&bytes).expect_err("out-of-range usage");
        assert_eq!(failure.code, CodexFailureCode::InvalidUsage, "{field}");
    }

    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"a","type":"agent_message","text":"a"}
    })));
    bytes.extend(line(json!({
        "type":"item.completed","item":{"id":"b","type":"agent_message","text":"b"}
    })));
    bytes.extend(line(json!({"type":"turn.completed","usage":usage()})));
    feed(&mut parser, &bytes);
    let failure = parser.finish(0).expect_err("multiple messages");
    assert_eq!(failure.code, CodexFailureCode::Protocol);
}

#[test]
fn does_not_infer_refusal_from_assistant_prose() {
    let mut parser = CodexJsonlParser::new();
    feed(
        &mut parser,
        &valid_stream("I cannot comply with that request."),
    );
    let outcome = parser.finish(0).expect("protocol success");
    assert_eq!(outcome.assistant_text, "I cannot comply with that request.");
}

#[test]
fn rejects_invalid_utf8_and_does_not_echo_input() {
    let mut parser = CodexJsonlParser::new();
    let failure = parser
        .feed(b"{\"type\":\"thread.started\",\"thread_id\":\"SECRET\"}\xff\n")
        .expect_err("invalid UTF-8");
    assert_eq!(failure.code, CodexFailureCode::InvalidUtf8);
    assert!(!failure.detail.contains("SECRET"));
}

#[test]
fn enforces_line_text_and_event_ceilings() {
    let mut parser = CodexJsonlParser::new();
    let oversized_line = vec![b'x'; MAX_LINE_BYTES + 1];
    let failure = parser.feed(&oversized_line).expect_err("line limit");
    assert_eq!(failure.code, CodexFailureCode::LimitExceeded);

    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    bytes.extend(line(json!({
        "type":"item.updated",
        "item":{"id":"m","type":"agent_message","text":"x".repeat(MAX_TEXT_BYTES / 8 + 1)}
    })));
    for chunk in 2..=8 {
        bytes.extend(line(json!({
            "type":"item.updated",
            "item":{"id":"m","type":"agent_message","text":"x".repeat((MAX_TEXT_BYTES / 8 + 1) * chunk)}
        })));
    }
    let failure = parser.feed(&bytes).expect_err("text limit");
    assert_eq!(failure.code, CodexFailureCode::LimitExceeded);

    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    bytes.extend(line(json!({"type":"turn.started"})));
    for _ in 0..MAX_EVENTS {
        bytes.extend(line(json!({
            "type":"item.completed",
            "item":{"id":"warning","type":"error","message":"warn"}
        })));
    }
    let failure = parser.feed(&bytes).expect_err("event limit");
    assert_eq!(failure.code, CodexFailureCode::LimitExceeded);

    let mut parser = CodexJsonlParser::new();
    let failure = parser
        .feed(&vec![b'\n'; MAX_TOTAL_BYTES + 1])
        .expect_err("total byte limit");
    assert_eq!(failure.code, CodexFailureCode::LimitExceeded);

    let mut parser = CodexJsonlParser::new();
    let mut bytes = Vec::new();
    bytes.extend(line(json!({"type":"thread.started","thread_id":"t"})));
    for warning in 0..=MAX_WARNINGS {
        bytes.extend(line(json!({
            "type":"item.completed",
            "item":{"id":format!("warning-{warning}"),"type":"error","message":"SECRET_WARNING"}
        })));
    }
    let failure = parser.feed(&bytes).expect_err("warning limit");
    assert_eq!(failure.code, CodexFailureCode::LimitExceeded);
}

#[test]
fn rejects_incomplete_eof_and_events_after_terminal() {
    let mut parser = CodexJsonlParser::new();
    feed(&mut parser, &valid_stream("ok"));
    let failure = parser
        .feed(&line(json!({"type":"turn.started"})))
        .expect_err("event after terminal");
    assert_eq!(failure.code, CodexFailureCode::Protocol);

    let mut parser = CodexJsonlParser::new();
    feed(
        &mut parser,
        &[
            line(json!({"type":"thread.started","thread_id":"t"})),
            line(json!({"type":"turn.started"})),
            line(json!({"type":"item.started","item":{"id":"m","type":"agent_message","text":"partial"}})),
        ]
        .concat(),
    );
    let failure = parser.finish(0).expect_err("incomplete item");
    assert_eq!(failure.code, CodexFailureCode::Incomplete);
    assert_eq!(failure.partial_output, "partial");
}
