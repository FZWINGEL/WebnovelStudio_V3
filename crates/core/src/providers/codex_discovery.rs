//! Bounded native Codex app-server model discovery.
//!
//! This module owns only the read-only JSON-RPC handshake and paginated
//! `model/list` exchange. It never starts a thread or turn, reads credentials,
//! or exposes the app-server's command/process/filesystem methods.

#![cfg(windows)]

use super::cli::windows_process::{
    ChildLimits, ChildStream, ChildTermination, CliInvocation, ContainmentError, EnvironmentPolicy,
    InteractiveAction, StopSignal, spawn_interactive,
};
use super::codex_catalog::{CodexCatalog, CodexCatalogModel, parse_model_page};
use crate::projects::{CoreError, CoreResult};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
const DISCOVERY_STOP_GRACE: Duration = Duration::from_secs(2);
const DISCOVERY_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_PAGES: usize = 32;
const MAX_MODELS: usize = 256;
const MAX_LINE_BYTES: usize = 256 * 1024;
const DISCOVERY_PAGE_SIZE: u64 = 32;
const CLIENT_NAME: &str = "webnovelstudio-v3";
const CLIENT_TITLE: &str = "WebnovelStudio V3";
const CLIENT_VERSION: &str = "0.0.0";

pub fn discover(
    executable: &Path,
    cwd: &Path,
    cli_version: &str,
    executable_sha256: &str,
    discovered_at: &str,
) -> CoreResult<CodexCatalog> {
    let initial = json_line(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": CLIENT_NAME,
                "title": CLIENT_TITLE,
                "version": CLIENT_VERSION,
            }
        }
    }))?;
    let invocation = CliInvocation {
        executable: executable.to_owned(),
        arguments: discovery_arguments(),
        cwd: cwd.to_owned(),
        environment: EnvironmentPolicy::Inherit,
        packet: initial,
        limits: ChildLimits {
            overall: DISCOVERY_TIMEOUT,
            stop_grace: DISCOVERY_STOP_GRACE,
            max_total_output_bytes: DISCOVERY_OUTPUT_BYTES,
        },
    };
    let running = spawn_interactive(invocation).map_err(containment_error)?;
    let stop = StopSignal::new();
    // The initial packet is written before the observer starts receiving
    // output, so the first response is the initialize response (id 1).
    let mut session = DiscoverySession {
        awaiting_id: Some(1),
        ..DiscoverySession::default()
    };
    let outcome = running
        .finish_interactive(stop.clone(), |stream, bytes| {
            if stream != ChildStream::Stdout || session.error.is_some() {
                return InteractiveAction::KeepOpen;
            }
            let action = session.accept(bytes);
            if session.error.is_some() {
                stop.request_stop();
            }
            action
        })
        .map_err(containment_error)?;

    if let Some(error) = session.error {
        return Err(error);
    }
    if outcome.termination != ChildTermination::Completed
        || outcome.output.exit_code != Some(0)
        || outcome.output.truncated
        || !outcome.output.io_errors.is_empty()
    {
        return Err(CoreError::new(
            "CodexDiscoveryFailed",
            "Codex model discovery did not finish cleanly.",
        ));
    }
    if !session.line_buffer.is_empty() {
        return Err(CoreError::new(
            "CodexDiscoveryFailed",
            "Codex returned an incomplete discovery record.",
        ));
    }
    if !session.completed || session.models.is_empty() {
        return Err(CoreError::new(
            "CodexDiscoveryFailed",
            "Codex returned no complete model catalog.",
        ));
    }
    let catalog = CodexCatalog {
        cli_version: cli_version.to_owned(),
        executable_sha256: executable_sha256.to_owned(),
        discovered_at: discovered_at.to_owned(),
        models: session.models,
    };
    catalog.validate()?;
    Ok(catalog)
}

fn discovery_arguments() -> Vec<OsString> {
    [
        "app-server",
        "--strict-config",
        "-c",
        "mcp_servers={}",
        "-c",
        "features.plugins=false",
        "-c",
        "features.apps=false",
        "--listen",
        "stdio://",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn json_line(value: &Value) -> CoreResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| {
        CoreError::new(
            "CodexDiscoveryFailed",
            "Codex discovery request could not be encoded.",
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn containment_error(_error: ContainmentError) -> CoreError {
    CoreError::new(
        "CodexDiscoveryFailed",
        "The Codex discovery process could not be contained or cleaned up.",
    )
}

#[derive(Default)]
struct DiscoverySession {
    line_buffer: Vec<u8>,
    awaiting_id: Option<u64>,
    next_id: u64,
    pages: usize,
    models: Vec<CodexCatalogModel>,
    model_ids: HashSet<String>,
    cursors: HashSet<String>,
    completed: bool,
    error: Option<CoreError>,
}

impl DiscoverySession {
    fn accept(&mut self, bytes: &[u8]) -> InteractiveAction {
        self.line_buffer.extend_from_slice(bytes);
        if self.line_buffer.len() > MAX_LINE_BYTES {
            self.fail("Codex discovery returned an oversized JSONL record.");
            return InteractiveAction::Close;
        }
        let mut pending = Vec::new();
        let mut close = false;
        while let Some(position) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.line_buffer.drain(..=position).collect::<Vec<_>>();
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            match self.accept_line(line) {
                InteractiveAction::KeepOpen => {}
                InteractiveAction::Send(bytes) => pending.extend(bytes),
                InteractiveAction::Close => close = true,
            }
            if self.error.is_some() {
                break;
            }
        }
        if self.error.is_some() || close {
            InteractiveAction::Close
        } else if pending.is_empty() {
            InteractiveAction::KeepOpen
        } else {
            InteractiveAction::Send(pending)
        }
    }

    fn accept_line(&mut self, line: &[u8]) -> InteractiveAction {
        let value: Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => {
                self.fail("Codex discovery returned an invalid JSONL record.");
                return InteractiveAction::Close;
            }
        };
        let Some(object) = value.as_object() else {
            self.fail("Codex discovery returned a non-object JSON-RPC record.");
            return InteractiveAction::Close;
        };
        if object.get("method").is_some() && object.get("id").is_none() {
            return InteractiveAction::KeepOpen;
        }
        let Some(id) = object.get("id").and_then(Value::as_u64) else {
            self.fail("Codex discovery returned a response without a numeric ID.");
            return InteractiveAction::Close;
        };
        if self.completed {
            self.fail("Codex discovery returned a response after completion.");
            return InteractiveAction::Close;
        }
        if object.get("error").is_some() {
            self.fail("Codex rejected the model discovery request.");
            return InteractiveAction::Close;
        }
        if self.awaiting_id != Some(id) {
            self.fail("Codex discovery returned an unexpected response ID.");
            return InteractiveAction::Close;
        }
        if id == 1 {
            self.awaiting_id = Some(2);
            self.next_id = 2;
            let initialized = json!({"jsonrpc":"2.0","method":"initialized","params":{}});
            let list = json!({
                "jsonrpc":"2.0",
                "id": self.next_id,
                "method":"model/list",
                "params":{"limit":DISCOVERY_PAGE_SIZE,"includeHidden":false}
            });
            return match join_lines(&[initialized, list]) {
                Ok(bytes) => InteractiveAction::Send(bytes),
                Err(error) => {
                    self.error = Some(error);
                    InteractiveAction::Close
                }
            };
        }
        self.accept_page(&value)
    }

    fn accept_page(&mut self, value: &Value) -> InteractiveAction {
        self.pages = self.pages.saturating_add(1);
        if self.pages > MAX_PAGES {
            self.fail("Codex discovery exceeded the pagination limit.");
            return InteractiveAction::Close;
        }
        let page = match parse_model_page(value.to_string().as_bytes()) {
            Ok(page) => page,
            Err(_) => {
                self.fail("Codex returned an invalid model discovery page.");
                return InteractiveAction::Close;
            }
        };
        if let Err(message) = validate_raw_page(value) {
            self.fail(message);
            return InteractiveAction::Close;
        }
        if self.models.len().saturating_add(page.models.len()) > MAX_MODELS {
            self.fail("Codex discovery returned too many models.");
            return InteractiveAction::Close;
        }
        for model in page.models {
            if !self.model_ids.insert(model.model_id.clone()) {
                self.fail("Codex discovery returned a duplicate model ID.");
                return InteractiveAction::Close;
            }
            self.models.push(model);
        }
        let Some(cursor) = page.next_cursor else {
            self.completed = true;
            return InteractiveAction::Close;
        };
        if !self.cursors.insert(cursor.clone()) {
            self.fail("Codex discovery returned a pagination cursor cycle.");
            return InteractiveAction::Close;
        }
        self.next_id = self.next_id.saturating_add(1);
        self.awaiting_id = Some(self.next_id);
        match json_line(&json!({
            "jsonrpc":"2.0",
            "id": self.next_id,
            "method":"model/list",
            "params":{"limit":DISCOVERY_PAGE_SIZE,"includeHidden":false,"cursor":cursor}
        })) {
            Ok(bytes) => InteractiveAction::Send(bytes),
            Err(error) => {
                self.error = Some(error);
                InteractiveAction::Close
            }
        }
    }

    fn fail(&mut self, detail: &str) {
        self.error = Some(CoreError::new("CodexDiscoveryFailed", detail));
    }
}

fn join_lines(values: &[Value]) -> CoreResult<Vec<u8>> {
    let mut output = Vec::new();
    for value in values {
        output.extend(json_line(value)?);
    }
    Ok(output)
}

fn validate_raw_page(value: &Value) -> Result<(), &'static str> {
    let object = value
        .as_object()
        .ok_or("Codex model page was not an object.")?;
    let result = object
        .get("result")
        .and_then(Value::as_object)
        .unwrap_or(object);
    let data = result
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex model page has no data array.")?;
    let mut ids = HashSet::new();
    for item in data {
        let entry = item
            .as_object()
            .ok_or("Codex model entry was not an object.")?;
        let id = entry
            .get("model")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("Codex model entry has no valid ID.")?;
        if !ids.insert(id) {
            return Err("Codex model page contains a duplicate model ID.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_a_request_when_notifications_share_the_response_chunk() {
        let mut session = DiscoverySession {
            awaiting_id: Some(1),
            ..DiscoverySession::default()
        };
        let bytes = format!(
            "{}\n{}\n{}\n",
            json!({"jsonrpc":"2.0","method":"remoteControl/status/changed","params":{}}),
            json!({"jsonrpc":"2.0","id":1,"result":{}}),
            json!({"jsonrpc":"2.0","method":"remoteControl/status/changed","params":{}}),
        );
        let action = session.accept(bytes.as_bytes());
        let InteractiveAction::Send(request) = action else {
            panic!("initialize response should schedule the next request");
        };
        let text = String::from_utf8(request).unwrap();
        assert!(text.contains("\"method\":\"initialized\""));
        assert!(text.contains("\"method\":\"model/list\""));
        assert_eq!(session.awaiting_id, Some(2));
    }

    #[test]
    fn rejects_duplicate_ids_inside_a_page_before_catalog_merge() {
        let mut session = DiscoverySession {
            awaiting_id: Some(2),
            ..DiscoverySession::default()
        };
        let page = json!({
            "jsonrpc":"2.0",
            "id":2,
            "result":{"data":[
                {"model":"same-model","displayName":"One"},
                {"model":"same-model","displayName":"Two"}
            ]}
        });
        assert!(matches!(
            session.accept_line(page.to_string().as_bytes()),
            InteractiveAction::Close
        ));
        assert!(session.error.is_some());
        assert!(session.models.is_empty());
    }

    #[test]
    fn consumes_a_terminal_page_followed_by_a_notification_in_one_chunk() {
        let mut session = DiscoverySession {
            awaiting_id: Some(2),
            ..DiscoverySession::default()
        };
        let bytes = format!(
            "{}\n{}\n",
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "result":{"data":[{"model":"model-one","displayName":"Model One"}]}
            }),
            json!({"jsonrpc":"2.0","method":"remoteControl/status/changed","params":{}}),
        );
        assert!(matches!(
            session.accept(bytes.as_bytes()),
            InteractiveAction::Close
        ));
        assert!(session.error.is_none());
        assert!(session.completed);
        assert!(session.line_buffer.is_empty());
    }

    #[test]
    fn rejects_a_response_after_the_terminal_page() {
        let mut session = DiscoverySession {
            awaiting_id: Some(2),
            ..DiscoverySession::default()
        };
        let first = json!({
            "jsonrpc":"2.0",
            "id":2,
            "result":{"data":[{"model":"model-one"}]}
        });
        assert!(matches!(
            session.accept(format!("{}\n", first).as_bytes()),
            InteractiveAction::Close
        ));
        let duplicate = json!({
            "jsonrpc":"2.0",
            "id":2,
            "result":{"data":[]}
        });
        assert!(matches!(
            session.accept(format!("{}\n", duplicate).as_bytes()),
            InteractiveAction::Close
        ));
        assert!(session.error.is_some());
    }
}
