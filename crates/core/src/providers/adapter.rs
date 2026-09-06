//! Shared provider transport contracts.
//!
//! The transport boundary is deliberately independent of projects, documents,
//! and persistence.  A provider can return text or stream deltas, but it never
//! applies an edit or establishes story canon.

use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A chat message sent to an OpenAI-compatible endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
}

impl MessageRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Developer => "developer",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// The optional response shape requested from a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseFormat {
    #[default]
    PlainText,
    JsonObject,
}

/// Request options common to OpenAI-compatible chat APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub response_format: ResponseFormat,
    pub max_output_tokens: Option<u32>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            reasoning_effort: None,
            service_tier: None,
            response_format: ResponseFormat::PlainText,
            max_output_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub text: String,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Role(MessageRole),
    ContentDelta(String),
    ReasoningDelta(String),
    Usage(Usage),
}

/// Cooperative cancellation shared by the native command and transport.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    Configuration,
    Authentication,
    Http,
    Network,
    InvalidResponse,
    Protocol,
    LimitExceeded,
    Cancelled,
    Truncated,
    Refused,
    ToolCall,
    Incomplete,
}

/// A bounded, sanitized provider failure.  Raw response bodies and request
/// credentials are intentionally never carried here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub status: Option<u16>,
    pub detail: String,
    pub partial_text: String,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            status: None,
            detail: detail.into(),
            partial_text: String::new(),
        }
    }

    pub fn with_partial(mut self, partial_text: impl Into<String>) -> Self {
        self.partial_text = partial_text.into();
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.status {
            write!(
                formatter,
                "{:?} (HTTP {status}): {}",
                self.kind, self.detail
            )
        } else {
            write!(formatter, "{:?}: {}", self.kind, self.detail)
        }
    }
}

impl std::error::Error for ProviderError {}

/// A minimal async provider boundary suitable for the Tauri command layer.
/// Implementations must never mutate documents or persistence.
#[allow(async_fn_in_trait)]
pub trait ChatAdapter: Send + Sync {
    async fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
    ) -> Result<ChatResponse, ProviderError>;
    async fn stream(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, ProviderError>;
    async fn list_models(&self, cancel: &CancellationToken) -> Result<Vec<String>, ProviderError>;
}
