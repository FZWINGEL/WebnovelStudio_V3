use std::future::Future;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::runtime::Builder;

use serde_json::{Value, json};
use sha2::Digest;
use webnovel_core::context::PacketReceipt;
use webnovel_core::context::packet::{
    CompiledPacket, HTTP_INPUT_LIMIT_BYTES, HTTP_OUTPUT_LIMIT_BYTES, HTTP_PROFILE_VERSION,
    HTTP_TOKEN_ACCOUNTING_METHOD, HttpProviderBinding, HttpResponseFormat, PacketMessage,
    PacketOptions, ProviderBinding,
};
use webnovel_core::providers::adapter::{
    CancellationToken, ChatAdapter, ChatMessage, ChatRequest, HttpRequestStage, MessageRole,
    ProviderErrorKind, ResponseFormat, StreamEvent,
};
use webnovel_core::providers::http_request::prepare_request;
use webnovel_core::providers::openai_compatible::{
    OpenAiCompatibleAdapter, OpenAiCompatibleConfig, normalize_base_url,
};

fn run<T>(future: impl Future<Output = T>) -> T {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime")
        .block_on(future)
}

struct CapturedRequest {
    path: String,
    headers: String,
    body: Vec<u8>,
}

struct MockServer {
    url: String,
    captured: std::sync::mpsc::Receiver<CapturedRequest>,
    thread: JoinHandle<()>,
}

fn mock_server(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    chunks: Option<Vec<Vec<u8>>>,
) -> MockServer {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
    let address = listener.local_addr().expect("mock address");
    let content_type = content_type.to_owned();
    let (sender, captured) = std::sync::mpsc::channel();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept mock request");
        let request = read_request(&mut stream);
        sender.send(request).expect("send captured request");
        let reason = if status == 200 { "OK" } else { "Error" };
        let headers = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).expect("write headers");
        if let Some(chunks) = chunks {
            for chunk in chunks {
                stream.write_all(&chunk).expect("write chunk");
                stream.flush().expect("flush chunk");
            }
        } else {
            stream.write_all(&body).expect("write body");
        }
    });
    MockServer {
        url: format!("http://{address}"),
        captured,
        thread,
    }
}

fn stalled_server(send_headers: bool) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind stalled server");
    let address = listener.local_addr().expect("stalled address");
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept stalled request");
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("set stalled read timeout");
        let _ = read_request(&mut stream);
        if send_headers {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n",
                )
                .expect("write stalled headers");
        }
        let mut byte = [0_u8; 1];
        while stream.read(&mut byte).unwrap_or(0) > 0 {}
    });
    (format!("http://{address}"), thread)
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 2048];
    let header_end;
    loop {
        let count = stream.read(&mut chunk).expect("read request");
        assert!(count > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
        assert!(bytes.len() < 256 * 1024, "request headers too large");
    }
    let headers_end = bytes[..header_end]
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header separator")
        + 4;
    let header_text =
        String::from_utf8(bytes[..headers_end].to_vec()).expect("request headers UTF-8");
    let length = header_text
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("content-length")
                    .then_some(value.trim())
            })
        })
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < headers_end + length {
        let count = stream.read(&mut chunk).expect("read request body");
        assert!(count > 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let path = header_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned();
    CapturedRequest {
        path,
        headers: header_text,
        body: bytes[headers_end..headers_end + length].to_vec(),
    }
}

fn adapter(server: &MockServer, key: Option<&str>) -> OpenAiCompatibleAdapter {
    let config =
        OpenAiCompatibleConfig::new(&format!("{}/v1/", server.url), key.map(str::to_owned))
            .expect("valid mock endpoint");
    OpenAiCompatibleAdapter::new(config).expect("build adapter")
}

fn request() -> ChatRequest {
    let mut request = ChatRequest::new(
        "gpt-5.6-luna",
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Write one sentence.".to_owned(),
        }],
    );
    request.reasoning_effort = Some("xhigh".to_owned());
    request.max_output_tokens = Some(128);
    request.response_format = ResponseFormat::JsonObject;
    request
}

fn packet(server_url: &str, stream: bool) -> CompiledPacket {
    let binding = ProviderBinding {
        provider_id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
        model_id: "gpt-5.6-luna".into(),
        reasoning: Some("xhigh".into()),
        service_tier: Some("priority".into()),
        profile_version: HTTP_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: format!("{server_url}/v1"),
            config_revision: "1".into(),
            stream,
            response_format: HttpResponseFormat::Text,
        }),
    };
    CompiledPacket {
        messages: vec![PacketMessage {
            role: "user".into(),
            content: "Write one sentence.".into(),
        }],
        options: PacketOptions {
            model_id: binding.model_id.clone(),
            max_output_tokens: "128".into(),
            token_accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
            provider_binding: Some(binding),
        },
        receipt: PacketReceipt {
            lookup: None,
            packet_id: "packet".into(),
            session_id: "session".into(),
            snapshot_id: "snapshot".into(),
            invocation_ordinal: "1".into(),
            source_handles: Vec::new(),
            mandatory_source_handles: Vec::new(),
            guidance_handles: Vec::new(),
            conversation_message_ids: Vec::new(),
            omitted_discussion_turns: 0,
            safe_brief: None,
            coverage: Vec::new(),
            omissions: Vec::new(),
            navigation_views: Vec::new(),
            navigation_omissions: Vec::new(),
            reviewed_evidence: Vec::new(),
            reviewed_evidence_omissions: Vec::new(),
            reviewed_promises: Vec::new(),
            reviewed_summaries: Vec::new(),
            reviewed_promise_omissions: Vec::new(),
            reviewed_summary_omissions: Vec::new(),
            input_hash: "a".repeat(64),
            input_tokens: "1".into(),
            token_accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        },
    }
}

#[test]
fn endpoint_validation_normalizes_v1_without_leaking_credentials() {
    assert_eq!(
        normalize_base_url("http://localhost:9000")
            .unwrap()
            .as_str(),
        "http://localhost:9000/v1"
    );
    assert_eq!(
        normalize_base_url("https://example.test/api/v1/")
            .unwrap()
            .as_str(),
        "https://example.test/api/v1"
    );
    assert_eq!(
        normalize_base_url("http://localhost:9000/api/chat/")
            .unwrap()
            .as_str(),
        "http://localhost:9000/api/chat"
    );
    for invalid in [
        "ftp://example.test",
        "https://user:password@example.test",
        "https://example.test/v1?key=secret",
        "https://example.test/v1#secret",
        "https://example.test/v1 with-space",
    ] {
        assert!(
            normalize_base_url(invalid).is_err(),
            "accepted unsafe endpoint {invalid}"
        );
    }
    let config =
        OpenAiCompatibleConfig::new("http://localhost:9000/v1", Some("super-secret".to_owned()))
            .unwrap();
    let debug = format!("{config:?}");
    assert!(!debug.contains("super-secret"));
}

#[test]
fn complete_sends_only_selected_options_and_bearer() {
    let body = serde_json::to_vec(&json!({
        "id": "chat-1",
        "model": "gpt-5.6-luna",
        "choices": [{"message": {"role": "assistant", "content": "{\"answer\":\"done\"}", "refusal": null, "tool_calls": null}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
    }))
    .unwrap();
    let server = mock_server(200, "application/json", body, None);
    let adapter = adapter(&server, Some("secret-token"));
    let result = run(adapter.complete(&request(), &CancellationToken::new())).unwrap();
    assert_eq!(result.text, "{\"answer\":\"done\"}");
    assert_eq!(result.usage.input_tokens, Some(7));
    let captured = server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
    assert_eq!(captured.path, "/v1/chat/completions");
    assert!(
        captured
            .headers
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token")
    );
    let body: Value = serde_json::from_slice(&captured.body).unwrap();
    assert_eq!(body["model"], "gpt-5.6-luna");
    assert_eq!(body["reasoning_effort"], "xhigh");
    assert_eq!(body["max_completion_tokens"], 128);
    assert_eq!(body["response_format"]["type"], "json_object");
    assert!(body.get("service_tier").is_none());
}

#[test]
fn json_mode_rejects_markdown_or_non_object_content() {
    let body = serde_json::to_vec(&json!({
        "choices": [{"message": {"role": "assistant", "content": "```json\n[]\n```"}}]
    }))
    .unwrap();
    let server = mock_server(200, "application/json", body, None);
    let client = adapter(&server, None);
    let error = run(client.complete(&request(), &CancellationToken::new())).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    assert!(error.partial_text.contains("```json"));
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
}

#[test]
fn list_models_reads_openai_data_shape() {
    let body =
        serde_json::to_vec(&json!({"data": [{"id": "model-a"}, {"id": "model-b"}]})).unwrap();
    let server = mock_server(200, "application/json", body, None);
    let adapter = adapter(&server, None);
    let models = run(adapter.list_models(&CancellationToken::new())).unwrap();
    assert_eq!(models, ["model-a", "model-b"]);
    let captured = server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
    assert_eq!(captured.path, "/v1/models");
    assert!(!captured.headers.contains("Authorization:"));
}

#[test]
fn stream_handles_split_sse_multiline_reasoning_usage_and_done() {
    let sse = concat!(
        "data: {\"id\":\"1\",\"model\":\"model-a\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"refusal\":null,\"tool_calls\":null},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"world\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}\n\n",
        "data: [DONE]\n\n"
    ).as_bytes().to_vec();
    let chunks = sse.chunks(7).map(|chunk| chunk.to_vec()).collect();
    let server = mock_server(200, "text/event-stream", sse, Some(chunks));
    let adapter = adapter(&server, None);
    let mut stream_request = request();
    stream_request.response_format = ResponseFormat::PlainText;
    let mut events = Vec::new();
    let response = run(
        adapter.stream(&stream_request, &CancellationToken::new(), &mut |event| {
            events.push(event)
        }),
    )
    .unwrap();
    assert_eq!(response.text, "hello world");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert!(
        events.iter().any(
            |event| matches!(event, StreamEvent::ReasoningDelta(value) if value == "thinking")
        )
    );
    assert!(
        events.iter().any(
            |event| matches!(event, StreamEvent::Usage(usage) if usage.total_tokens == Some(5))
        )
    );
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
}

#[test]
fn stream_preserves_final_content_when_provider_stops_at_length() {
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"final partial\"},\"finish_reason\":\"length\"}]}\n\n",
        "data: [DONE]\n\n"
    )
    .as_bytes()
    .to_vec();
    let server = mock_server(200, "text/event-stream", sse, None);
    let adapter = adapter(&server, None);
    let mut stream_request = request();
    stream_request.response_format = ResponseFormat::PlainText;
    let mut events = Vec::new();
    let error = run(
        adapter.stream(&stream_request, &CancellationToken::new(), &mut |event| {
            events.push(event)
        }),
    )
    .expect_err("length termination must remain a failed stream");

    assert_eq!(error.kind, ProviderErrorKind::Incomplete);
    assert_eq!(error.partial_text, "final partial");
    assert!(events.iter().any(
        |event| matches!(event, StreamEvent::ContentDelta(value) if value == "final partial")
    ));
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
}

#[test]
fn truncated_stream_and_auth_error_are_distinct_and_redacted() {
    let server = mock_server(
        200,
        "text/event-stream",
        b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n".to_vec(),
        None,
    );
    let client = adapter(&server, None);
    let error = run(client.stream(&request(), &CancellationToken::new(), &mut |_| {})).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Truncated);
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();

    let server = mock_server(
        401,
        "application/json",
        b"secret-provider-error".to_vec(),
        None,
    );
    let client = adapter(&server, Some("auth-secret"));
    let error = run(client.complete(&request(), &CancellationToken::new())).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Authentication);
    assert_eq!(error.status, Some(401));
    assert!(!error.to_string().contains("secret-provider-error"));
    assert!(!error.to_string().contains("auth-secret"));
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
}

#[test]
fn pre_cancelled_requests_do_not_open_network() {
    let server = mock_server(200, "application/json", b"{}".to_vec(), None);
    let adapter = adapter(&server, None);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = run(adapter.complete(&request(), &cancel)).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    assert!(
        server
            .captured
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    drop(server);
}

#[test]
fn cancellation_interrupts_stalled_headers_and_sse_body() {
    for send_headers in [false, true] {
        let (url, server_thread) = stalled_server(send_headers);
        let config = OpenAiCompatibleConfig::new(&format!("{url}/v1"), None).unwrap();
        let client = std::sync::Arc::new(OpenAiCompatibleAdapter::new(config).unwrap());
        let cancel = CancellationToken::new();
        let worker_client = std::sync::Arc::clone(&client);
        let worker_cancel = cancel.clone();
        let worker = thread::spawn(move || {
            if send_headers {
                run(worker_client.stream(&request(), &worker_cancel, &mut |_| {}))
            } else {
                run(worker_client.complete(&request(), &worker_cancel))
            }
        });
        thread::sleep(Duration::from_millis(100));
        cancel.cancel();
        let error = worker
            .join()
            .expect("cancellation worker should exit")
            .expect_err("stalled provider should be cancelled");
        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        server_thread
            .join()
            .expect("stalled server should observe close");
    }
}

#[test]
fn stream_rejects_empty_malformed_and_non_text_completions() {
    let server = mock_server(200, "text/event-stream", b"data: [DONE]\n\n".to_vec(), None);
    let client = adapter(&server, None);
    let mut stream_request = request();
    stream_request.response_format = ResponseFormat::PlainText;
    let error = run(client.stream(&stream_request, &CancellationToken::new(), &mut |_| {}))
        .expect_err("DONE-only stream should fail closed");
    assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();

    let cases = [
        (r#"{"choices":[]}"#, ProviderErrorKind::Protocol),
        (r#"{"choices":{}}"#, ProviderErrorKind::Protocol),
        (
            r#"{"choices":[{"delta":{},"finish_reason":4}]}"#,
            ProviderErrorKind::Protocol,
        ),
        (
            r#"{"choices":[{"delta":{"content":"x"},"finish_reason":"mystery"}]}"#,
            ProviderErrorKind::Protocol,
        ),
        (
            r#"{"error":{"message":"do not expose"}}"#,
            ProviderErrorKind::Http,
        ),
        (
            r#"{"choices":[{"delta":{"refusal":"no"}}]}"#,
            ProviderErrorKind::Refused,
        ),
        (
            r#"{"choices":[{"delta":{"tool_calls":[{}]}}]}"#,
            ProviderErrorKind::ToolCall,
        ),
        (
            r#"{"choices":[{"delta":{"content":"partial"},"finish_reason":"length"}]}"#,
            ProviderErrorKind::Incomplete,
        ),
    ];
    for (payload, expected_kind) in cases {
        let body = format!("data: {payload}\n\ndata: [DONE]\n\n").into_bytes();
        let server = mock_server(200, "text/event-stream", body, None);
        let client = adapter(&server, None);
        let mut stream_request = request();
        stream_request.response_format = ResponseFormat::PlainText;
        let error = run(client.stream(&stream_request, &CancellationToken::new(), &mut |_| {}))
            .expect_err("malformed stream should fail closed");
        assert_eq!(error.kind, expected_kind, "payload {payload}");
        server
            .captured
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        server.thread.join().unwrap();
    }
}

#[test]
fn packet_stream_sends_the_compiled_body_and_reports_http_stages() {
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"compiled\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    )
    .as_bytes()
    .to_vec();
    let server = mock_server(200, "text/event-stream", sse, None);
    let packet = packet(&server.url, true);
    let expected = prepare_request(&packet.messages, &packet.options).unwrap();
    let adapter = adapter(&server, Some("packet-secret"));
    let mut events = Vec::new();
    let mut stages = Vec::new();
    let response = run(adapter.stream_packet_async(
        &packet,
        &CancellationToken::new(),
        &mut |event| events.push(event),
        &mut |stage| stages.push(stage),
    ))
    .unwrap();
    assert_eq!(response.text, "compiled");
    assert_eq!(
        stages,
        vec![
            HttpRequestStage::Submitted,
            HttpRequestStage::ResponseReceived
        ]
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StreamEvent::ContentDelta(value) if value == "compiled"))
    );
    let captured = server
        .captured
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.thread.join().unwrap();
    assert_eq!(captured.body, expected.body);
    let digest = sha2::Sha256::digest(&captured.body);
    assert_eq!(
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        expected.body_hash
    );
    assert!(!String::from_utf8_lossy(&captured.body).contains("packet-secret"));
}

#[test]
fn packet_stream_rejects_an_endpoint_binding_mismatch_before_network_io() {
    let server = mock_server(200, "text/event-stream", b"data: [DONE]\n\n".to_vec(), None);
    let packet = packet("http://127.0.0.1:1", true);
    let adapter = adapter(&server, None);
    let mut stages = Vec::new();
    let error = run(adapter.stream_packet_async(
        &packet,
        &CancellationToken::new(),
        &mut |_| {},
        &mut |stage| stages.push(stage),
    ))
    .expect_err("a packet for another endpoint must not be sent");
    assert_eq!(error.kind, ProviderErrorKind::Configuration);
    assert!(stages.is_empty());
    assert!(
        server
            .captured
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    drop(server);
}
