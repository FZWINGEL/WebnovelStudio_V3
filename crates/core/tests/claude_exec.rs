use serde_json::{Value, json};
use webnovel_core::providers::claude_exec::{
    ClaudeEvent, ClaudeFailureCode, ClaudeJsonlParser, MAX_EVENTS, MAX_LINE_BYTES, MAX_TEXT_BYTES,
    MAX_TOTAL_BYTES,
};

fn line(value: Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&value).expect("JSON");
    bytes.push(b'\n');
    bytes
}

fn init() -> Value {
    json!({
        "type": "system",
        "subtype": "init",
        "session_id": "session-1",
        "model": "claude-sonnet-4",
        "tools": [],
        "mcp_servers": [],
        "plugins": [],
        "agents": [],
        "skills": []
    })
}

fn stream_event(event: Value) -> Value {
    json!({"type":"stream_event","session_id":"session-1","event":event})
}

fn streamed_success(text: &str) -> Vec<u8> {
    [
        line(init()),
        line(stream_event(json!({
            "type":"message_start",
            "message":{"id":"message-1","type":"message","role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":4,"output_tokens":1}}
        }))),
        line(stream_event(json!({
            "type":"content_block_start",
            "index":0,
            "content_block":{"type":"text","text":""}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta",
            "index":0,
            "delta":{"type":"text_delta","text":text}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":0}))),
        line(stream_event(json!({
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":2}
        }))),
        line(stream_event(json!({"type":"message_stop"}))),
        line(json!({
            "type":"assistant",
            "session_id":"session-1",
            "message":{"id":"message-1","type":"message","role":"assistant","model":"claude-sonnet-4","stop_reason":"end_turn","content":[{"type":"text","text":text}]}
        })),
        line(json!({
            "type":"result",
            "subtype":"success",
            "is_error":false,
            "session_id":"session-1",
            "model":"claude-sonnet-4",
            "result":text,
            "usage":{"input_tokens":4,"output_tokens":2}
        })),
    ]
    .concat()
}

#[test]
fn arbitrary_chunks_emit_streamed_text_once_and_accept_full_confirmation() {
    let bytes = streamed_success("海🙂llo");
    let mut parser = ClaudeJsonlParser::new();
    let mut events = Vec::new();
    for byte in bytes {
        events.extend(parser.feed(&[byte]).expect("feed"));
    }

    assert_eq!(
        events,
        vec![ClaudeEvent::AssistantDelta("海🙂llo".to_owned())]
    );
    let outcome = parser.finish(0).expect("success");
    assert_eq!(outcome.assistant_text, "海🙂llo");
    assert_eq!(outcome.session_id, "session-1");
    assert_eq!(outcome.model.as_deref(), Some("claude-sonnet-4"));
    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(outcome.usage.expect("usage").output_tokens, Some(2));
    assert_eq!(
        outcome.stream_usage.expect("stream usage").output_tokens,
        Some(2)
    );
    assert_eq!(
        outcome.result_usage.expect("result usage").output_tokens,
        Some(2)
    );
}

#[test]
fn accepts_null_and_omitted_message_delta_stop_reason_before_final_stop() {
    let bytes = [
        line(init()),
        line(stream_event(json!({
            "type":"message_start",
            "message":{"id":"message-1","type":"message","role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":4,"output_tokens":1}}
        }))),
        line(stream_event(json!({
            "type":"content_block_start",
            "index":0,
            "content_block":{"type":"text","text":""}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta",
            "index":0,
            "delta":{"type":"text_delta","text":"answer"}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":0}))),
        line(stream_event(json!({
            "type":"message_delta",
            "delta":{"stop_reason":null},
            "usage":{"output_tokens":2}
        }))),
        line(stream_event(json!({
            "type":"message_delta",
            "delta":{},
            "usage":{"output_tokens":3}
        }))),
        line(stream_event(json!({
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":4}
        }))),
        line(stream_event(json!({"type":"message_stop"}))),
        line(json!({
            "type":"assistant",
            "session_id":"session-1",
            "message":{"id":"message-1","type":"message","role":"assistant","model":"claude-sonnet-4","stop_reason":"end_turn","content":[{"type":"text","text":"answer"}]}
        })),
        line(json!({
            "type":"result",
            "subtype":"success",
            "is_error":false,
            "session_id":"session-1",
            "model":"claude-sonnet-4",
            "result":"answer",
            "usage":{"input_tokens":4,"output_tokens":4}
        })),
    ]
    .concat();

    let mut parser = ClaudeJsonlParser::new();
    assert_eq!(
        parser.feed(&bytes).expect("feed"),
        vec![ClaudeEvent::AssistantDelta("answer".into())]
    );
    let outcome = parser.finish(0).expect("success");
    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(
        outcome.stream_usage.expect("stream usage").output_tokens,
        Some(4)
    );
    assert_eq!(
        outcome.result_usage.expect("result usage").output_tokens,
        Some(4)
    );
}

#[test]
fn rejects_nonempty_message_start_content_before_emission() {
    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(stream_event(json!({
            "type":"message_start",
            "message":{
                "id":"message-1",
                "type":"message",
                "role":"assistant",
                "content":[{"type":"text","text":"unexpected"}]
            }
        }))))
        .expect_err("nonempty message start content");
    assert_eq!(failure.code, ClaudeFailureCode::UnsupportedEvent);
    assert!(failure.partial_output.is_empty());
}

#[test]
fn rejects_result_usage_lower_than_stream_usage() {
    let bytes = [
        line(init()),
        line(stream_event(json!({
            "type":"message_start",
            "message":{"id":"message-1","type":"message","role":"assistant","usage":{"input_tokens":4,"output_tokens":5}}
        }))),
        line(stream_event(json!({
            "type":"content_block_start",
            "index":0,
            "content_block":{"type":"text","text":""}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta",
            "index":0,
            "delta":{"type":"text_delta","text":"answer"}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":0}))),
        line(stream_event(json!({
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":5}
        }))),
        line(stream_event(json!({"type":"message_stop"}))),
        line(json!({
            "type":"assistant",
            "session_id":"session-1",
            "message":{"id":"message-1","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"answer"}]}
        })),
        line(json!({
            "type":"result",
            "subtype":"success",
            "is_error":false,
            "session_id":"session-1",
            "result":"answer",
            "usage":{"input_tokens":4,"output_tokens":4}
        })),
    ]
    .concat();

    let mut parser = ClaudeJsonlParser::new();
    let failure = parser.feed(&bytes).expect_err("result usage regression");
    assert_eq!(failure.code, ClaudeFailureCode::InvalidUsage);
    assert_eq!(failure.partial_output, "answer");
}

#[test]
fn accepts_crlf_final_record_and_full_assistant_without_stream_partials() {
    let mut valid = Vec::new();
    for value in [
        init(),
        json!({
            "type":"assistant","session_id":"session-1",
            "message":{"type":"message","role":"assistant","model":"claude-sonnet-4","stop_reason":"end_turn","content":[{"type":"text","text":"complete"}]}
        }),
    ] {
        valid.extend(serde_json::to_vec(&value).expect("JSON"));
        valid.extend_from_slice(b"\r\n");
    }
    valid.extend(serde_json::to_vec(&json!({
        "type":"result","subtype":"success","is_error":false,"session_id":"session-1","result":"complete"
    })).expect("JSON"));
    let mut parser = ClaudeJsonlParser::new();
    assert_eq!(
        parser.feed(&valid).expect("feed"),
        vec![ClaudeEvent::AssistantDelta("complete".into())]
    );
    assert_eq!(
        parser.finish(0).expect("success").assistant_text,
        "complete"
    );
}

#[test]
fn ignores_bounded_thinking_and_redacted_thinking() {
    let bytes = [
        line(init()),
        line(stream_event(json!({
            "type":"message_start","message":{"type":"message","role":"assistant"}
        }))),
        line(stream_event(json!({
            "type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"private"}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" thought"}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":0}))),
        line(stream_event(json!({
            "type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"opaque"}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta","index":1,"delta":{"type":"redacted_thinking_delta","data":"more"}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":1}))),
        line(stream_event(json!({
            "type":"content_block_start","index":2,"content_block":{"type":"text","text":"prefix"}
        }))),
        line(stream_event(json!({
            "type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"visible"}
        }))),
        line(stream_event(json!({"type":"content_block_stop","index":2}))),
        line(stream_event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}))),
        line(stream_event(json!({"type":"message_stop"}))),
        line(json!({"type":"result","subtype":"success","is_error":false,"result":"prefixvisible"})),
    ]
    .concat();
    let mut parser = ClaudeJsonlParser::new();
    let events = parser.feed(&bytes).expect("feed");
    assert_eq!(
        events,
        vec![
            ClaudeEvent::AssistantDelta("prefix".into()),
            ClaudeEvent::AssistantDelta("visible".into())
        ]
    );
    assert_eq!(
        parser.finish(0).expect("success").assistant_text,
        "prefixvisible"
    );
}

#[test]
fn normal_refusal_text_is_not_inferred_as_failure() {
    let text = "I cannot comply with that request.";
    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&streamed_success(text)).expect("feed");
    let outcome = parser.finish(0).expect("success");
    assert_eq!(outcome.assistant_text, text);
}

#[test]
fn rejects_configuration_tools_parent_and_user_surfaces() {
    let mut parser = ClaudeJsonlParser::new();
    let failure = parser
        .feed(&line(json!({
            "type":"system","subtype":"init","session_id":"s","tools":["Bash"],"mcp_servers":[]
        })))
        .expect_err("configured tool");
    assert_eq!(failure.code, ClaudeFailureCode::ConfigurationRejected);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(
            json!({"type":"user","message":{"content":"injected"}}),
        ))
        .expect_err("user turn");
    assert_eq!(failure.code, ClaudeFailureCode::UnsupportedEvent);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"assistant","parent_tool_use_id":"parent",
            "message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"x"}]}
        })))
        .expect_err("subagent parent");
    assert_eq!(failure.code, ClaudeFailureCode::UnsupportedEvent);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"stream_event","parent_tool_use_id":"parent","event":{"type":"message_start","message":{"type":"message","role":"assistant"}}
        })))
        .expect_err("stream subagent parent");
    assert_eq!(failure.code, ClaudeFailureCode::UnsupportedEvent);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"result","subtype":"success","is_error":false,"result":"x","permission_denials":[{"tool_name":"Bash"}]
        })))
        .expect_err("permission denial");
    assert_eq!(failure.code, ClaudeFailureCode::UnsupportedEvent);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"assistant","error":{"kind":"provider"},
            "message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"must not emit"}]}
        })))
        .expect_err("structured provider error");
    assert_eq!(failure.code, ClaudeFailureCode::ProviderFailure);
    assert!(failure.partial_output.is_empty());
}

#[test]
fn rejects_model_session_and_terminal_conflicts() {
    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({"type":"stream_event","session_id":"other","event":{"type":"message_start","message":{"type":"message","role":"assistant"}}})))
        .expect_err("session conflict");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({"type":"stream_event","event":{"type":"message_start","message":{"type":"message","role":"assistant","model":"other"}}})))
        .expect_err("model conflict");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&streamed_success("done")).expect("stream");
    let failure = parser
        .feed(&line(json!({"type":"system","subtype":"init","session_id":"session-1","tools":[],"mcp_servers":[]})))
        .expect_err("event after result");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(stream_event(json!({
            "type":"message_start","message":{"id":"message-1","type":"message","role":"assistant"}
        }))))
        .expect("message start");
    let failure = parser
        .feed(&line(json!({
            "type":"assistant","session_id":"session-1",
            "message":{"id":"message-2","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"x"}]}
        })))
        .expect_err("message conflict");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    let failure = parser
        .feed(&line(json!({"type":"system","subtype":"unknown"})))
        .expect_err("unknown system");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({"type":"future_event"})))
        .expect_err("unknown event");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    for event in [
        json!({"type":"message_start","message":{"type":"message","role":"assistant"}}),
        json!({"type":"message_stop"}),
    ] {
        // No content block is valid for this terminal-conflict fixture; the
        // parser still fences message-stop ordering separately.
        parser
            .feed(&line(stream_event(event)))
            .expect("stream event");
    }
    let failure = parser
        .feed(&line(stream_event(json!({"type":"message_stop"}))))
        .expect_err("duplicate message stop");
    assert_eq!(failure.code, ClaudeFailureCode::Protocol);
}

#[test]
fn preserves_partial_for_provider_failure_truncation_and_nonzero_exit() {
    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(json!({
            "type":"assistant","session_id":"session-1",
            "message":{"type":"message","role":"assistant","stop_reason":null,"content":[{"type":"text","text":"partial"}]}
        })))
        .expect("partial assistant");
    let failure = parser
        .feed(&line(json!({"type":"result","subtype":"error_max_output_tokens","is_error":true,"result":"partial"})))
        .expect_err("truncation");
    assert_eq!(failure.code, ClaudeFailureCode::ProviderFailure);
    assert_eq!(failure.partial_output, "partial");

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(json!({
            "type":"assistant","message":{"type":"message","role":"assistant","stop_reason":"max_tokens","content":[{"type":"text","text":"truncated"}]}
        })))
        .expect("truncated assistant");
    parser
        .feed(&line(
            json!({"type":"result","subtype":"success","is_error":false,"result":"truncated"}),
        ))
        .expect("explicit result");
    let failure = parser.finish(0).expect_err("truncated stop reason");
    assert_eq!(failure.code, ClaudeFailureCode::Truncated);
    assert_eq!(failure.partial_output, "truncated");

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(json!({
            "type":"assistant","message":{"type":"message","role":"assistant","stop_reason":"refusal","content":[{"type":"text","text":"refusal text"}]}
        })))
        .expect("refusal assistant");
    parser
        .feed(&line(
            json!({"type":"result","subtype":"success","is_error":false,"result":"refusal text"}),
        ))
        .expect("explicit result");
    let failure = parser.finish(0).expect_err("refusal stop reason");
    assert_eq!(failure.code, ClaudeFailureCode::Refused);
    assert_eq!(failure.partial_output, "refusal text");
    assert!(!failure.detail.contains("partial"));

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&streamed_success("partial")).expect("stream");
    let failure = parser.finish(9).expect_err("nonzero exit");
    assert_eq!(failure.code, ClaudeFailureCode::NonZeroExit);
    assert_eq!(failure.partial_output, "partial");
}

#[test]
fn incomplete_and_invalid_usage_are_sticky_and_sanitized() {
    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(json!({
            "type":"assistant","message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"partial"}]}
        })))
        .expect("assistant");
    let failure = parser.finish(0).expect_err("missing result");
    assert_eq!(failure.code, ClaudeFailureCode::Incomplete);
    assert_eq!(failure.partial_output, "partial");
    assert_eq!(parser.feed(b"SECRET"), Err(failure.clone()));

    let mut parser = ClaudeJsonlParser::new();
    let mut bad = init();
    bad["session_id"] = json!("SECRET_SESSION");
    parser.feed(&line(bad)).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"result","subtype":"success","is_error":false,"result":"x","usage":{"output_tokens":-1}
        })))
        .expect_err("negative usage");
    assert_eq!(failure.code, ClaudeFailureCode::InvalidUsage);
    assert!(!failure.detail.contains("SECRET_SESSION"));

    let mut parser = ClaudeJsonlParser::new();
    let failure = parser
        .feed(b"{\"type\":\"system\",\"session_id\":\"SECRET\"}\xff\n")
        .expect_err("invalid UTF-8");
    assert_eq!(failure.code, ClaudeFailureCode::InvalidUtf8);
    assert!(!failure.detail.contains("SECRET"));
}

#[test]
fn usage_can_be_absent_and_limits_remain_bounded() {
    let mut parser = ClaudeJsonlParser::new();
    let bytes = [
        line(init()),
        line(json!({
            "type":"assistant","message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"answer"}]}
        })),
        line(json!({"type":"result","subtype":"success","is_error":false,"result":"answer"})),
    ]
    .concat();
    parser.feed(&bytes).expect("feed");
    assert!(parser.finish(0).expect("success").usage.is_none());

    let mut parser = ClaudeJsonlParser::new();
    parser.feed(&line(init())).expect("init");
    parser
        .feed(&line(stream_event(json!({
            "type":"message_start","message":{"type":"message","role":"assistant","usage":{"output_tokens":2}}
        }))))
        .expect("message start");
    let failure = parser
        .feed(&line(stream_event(json!({
            "type":"message_delta","usage":{"output_tokens":1},"delta":{}
        }))))
        .expect_err("decreasing stream usage");
    assert_eq!(failure.code, ClaudeFailureCode::InvalidUsage);

    let mut parser = ClaudeJsonlParser::new();
    let failure = parser
        .feed(&vec![b'x'; MAX_LINE_BYTES + 1])
        .expect_err("line limit");
    assert_eq!(failure.code, ClaudeFailureCode::LimitExceeded);

    let mut parser = ClaudeJsonlParser::new();
    let failure = parser
        .feed(&vec![b'\n'; MAX_TOTAL_BYTES + 1])
        .expect_err("total limit");
    assert_eq!(failure.code, ClaudeFailureCode::LimitExceeded);

    let mut parser = ClaudeJsonlParser::new();
    let mut event_bytes = [
        line(init()),
        line(stream_event(json!({
            "type":"message_start","message":{"type":"message","role":"assistant"}
        }))),
    ]
    .concat();
    for _ in 0..=MAX_EVENTS {
        event_bytes.extend(line(stream_event(json!({
            "type":"message_delta","delta":{}
        }))));
    }
    let failure = parser.feed(&event_bytes).expect_err("event limit");
    assert_eq!(failure.code, ClaudeFailureCode::LimitExceeded);

    let mut parser = ClaudeJsonlParser::new();
    let oversized = "x".repeat(MAX_TEXT_BYTES + 1);
    parser.feed(&line(init())).expect("init");
    let failure = parser
        .feed(&line(json!({
            "type":"assistant","message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":oversized}]}
        })))
        .expect_err("text limit");
    assert_eq!(failure.code, ClaudeFailureCode::LimitExceeded);
}
