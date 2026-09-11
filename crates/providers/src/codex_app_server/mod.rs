//! Versioned contracts for the application-owned Codex app-server transport.
//! A retained process is not retained story context or permission to replay work.

use wns_kernel::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

pub const AUTHOR_PROFILE: &str = "codex-app-server.author.v1";
pub const MAINTENANCE_PROFILE: &str = "codex-app-server.maintenance.v1";
pub const ACCOUNTING_METHOD: &str = "utf8-byte-count/codex-app-server-application-cap-v1";

/// Non-secret identity of the process-level isolation and account contract.
/// The selected model descriptor remains in ProviderRuntimeIdentity.catalog_sha256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppServerRuntimeIdentity {
    pub account_sha256: String,
    pub security_config_sha256: String,
    pub restrictive_catalog_sha256: String,
}

impl AppServerRuntimeIdentity {
    pub fn is_valid(&self) -> bool {
        valid_hash(&self.account_sha256)
            && valid_hash(&self.security_config_sha256)
            && valid_hash(&self.restrictive_catalog_sha256)
    }
}

pub fn is_app_server(binding: &crate::vocabulary::ProviderBinding) -> bool {
    binding.provider_id == "codex"
        && matches!(
            binding.profile_version.as_str(),
            AUTHOR_PROFILE | MAINTENANCE_PROFILE
        )
}

/// Committed before writing turn/start. The provider thread has been created,
/// but this record alone cannot authorize another external start after recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppServerDispatch {
    pub server_generation: String,
    pub thread_id: String,
    pub rpc_id: String,
    pub packet_hash: String,
    pub request_hash: String,
}

impl AppServerDispatch {
    pub fn validate(&self) -> CoreResult<()> {
        if !valid_identifier(&self.server_generation)
            || !valid_identifier(&self.thread_id)
            || !valid_identifier(&self.rpc_id)
            || !valid_hash(&self.packet_hash)
            || !valid_hash(&self.request_hash)
        {
            return Err(invalid("The app-server dispatch identity is invalid."));
        }
        Ok(())
    }
}

/// Stable request construction shared by the driver and durable validator.
/// The full application packet is one text input; arbitrary caller-supplied
/// tools, files, instructions, and history cannot enter this envelope.
pub fn turn_request(
    dispatch: &AppServerDispatch,
    binding: &crate::vocabulary::ProviderBinding,
    packet: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": dispatch.rpc_id,
        "method": "turn/start",
        "params": {
            "threadId": dispatch.thread_id,
            "input": [{ "type": "text", "text": packet }],
            "model": binding.model_id,
            "effort": binding.reasoning,
            "serviceTierForTurn": binding.service_tier.as_deref().unwrap_or("default")
        }
    })
}

pub fn prepare_dispatch(
    server_generation: String,
    thread_id: String,
    rpc_id: String,
    binding: &crate::vocabulary::ProviderBinding,
    packet: &str,
) -> CoreResult<AppServerDispatch> {
    let mut dispatch = AppServerDispatch {
        server_generation,
        thread_id,
        rpc_id,
        packet_hash: wns_kernel::sha256_hex(packet.as_bytes()),
        request_hash: String::new(),
    };
    dispatch.request_hash = wns_kernel::sha256_hex(&serde_json::to_vec(&turn_request(
        &dispatch, binding, packet,
    ))?);
    dispatch.validate()?;
    Ok(dispatch)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppServerSubmission {
    NotSent,
    Uncertain,
    Acknowledged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppServerTerminal {
    Completed,
    Interrupted,
    Failed,
}

/// Connection state is distinct from settlement of this one request. Closing
/// a server after a failed interrupt can settle ownership without establishing
/// whether its upstream generation completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppServerConnectionSettlement {
    Reusable,
    Closed,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppServerDelivery {
    pub dispatch: Option<AppServerDispatch>,
    pub submission: AppServerSubmission,
    pub turn_id: Option<String>,
    pub terminal: Option<AppServerTerminal>,
    pub request_settled: bool,
    pub connection: AppServerConnectionSettlement,
}

impl AppServerDelivery {
    pub fn not_sent() -> Self {
        Self {
            dispatch: None,
            submission: AppServerSubmission::NotSent,
            turn_id: None,
            terminal: None,
            request_settled: true,
            connection: AppServerConnectionSettlement::Closed,
        }
    }

    pub fn validate(&self) -> CoreResult<()> {
        if let Some(dispatch) = &self.dispatch {
            dispatch.validate()?;
        }
        if self
            .turn_id
            .as_ref()
            .is_some_and(|id| !valid_identifier(id))
            || (self.submission != AppServerSubmission::NotSent && self.dispatch.is_none())
            || (self.submission == AppServerSubmission::Acknowledged && self.turn_id.is_none())
            || (self.submission != AppServerSubmission::Acknowledged && self.turn_id.is_some())
            || (self.terminal.is_some() && self.submission != AppServerSubmission::Acknowledged)
            || (self.connection == AppServerConnectionSettlement::Unresolved
                && self.request_settled)
            || (self.connection == AppServerConnectionSettlement::Reusable
                && (!self.request_settled
                    || (self.submission != AppServerSubmission::NotSent
                        && self.terminal.is_none())))
        {
            return Err(invalid("The app-server delivery evidence is inconsistent."));
        }
        Ok(())
    }

    pub fn completed(&self) -> bool {
        self.validate().is_ok()
            && self.submission == AppServerSubmission::Acknowledged
            && self.terminal == Some(AppServerTerminal::Completed)
            && self.request_settled
    }
}

pub fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

#[cfg(any(windows, test))]
pub mod auth;
#[cfg(windows)]
pub mod connection;
#[cfg(windows)]
pub mod launch;
pub mod protocol;
#[cfg(windows)]
pub mod runtime;

fn invalid(message: &str) -> CoreError {
    CoreError::new("InvalidAppServerDelivery", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acknowledged() -> AppServerDelivery {
        AppServerDelivery {
            dispatch: Some(AppServerDispatch {
                server_generation: "server-1".into(),
                thread_id: "thread-1".into(),
                rpc_id: "request-1".into(),
                packet_hash: "a".repeat(64),
                request_hash: "b".repeat(64),
            }),
            submission: AppServerSubmission::Acknowledged,
            turn_id: Some("turn-1".into()),
            terminal: Some(AppServerTerminal::Completed),
            request_settled: true,
            connection: AppServerConnectionSettlement::Reusable,
        }
    }

    #[test]
    fn acknowledgment_is_not_a_completed_or_settled_request() {
        let mut receipt = acknowledged();
        assert!(receipt.completed());
        receipt.terminal = None;
        assert!(receipt.validate().is_err());
        receipt.request_settled = false;
        receipt.connection = AppServerConnectionSettlement::Unresolved;
        assert!(receipt.validate().is_ok());
        assert!(!receipt.completed());
    }

    #[test]
    fn terminated_connection_can_settle_uncertain_submission_without_success() {
        let mut receipt = acknowledged();
        receipt.submission = AppServerSubmission::Uncertain;
        receipt.turn_id = None;
        receipt.terminal = None;
        receipt.connection = AppServerConnectionSettlement::Closed;
        assert!(receipt.validate().is_ok());
        assert!(!receipt.completed());
        receipt.dispatch = None;
        assert!(receipt.validate().is_err());
    }
}
