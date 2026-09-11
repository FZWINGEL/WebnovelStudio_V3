//! L1 — provider boundaries.
//!
//! Pure protocol parsers and a bounded Windows Codex runtime. The native
//! application dispatches frozen packets and reports terminal results back to
//! the discussion actor; providers never own manuscript mutations.
//!
//! Extracted from `webnovel-core`. Before this crate existed, the packet
//! compiler sourced its profile constants from the provider modules while the
//! provider adapters reached up into the compiler for `ProviderBinding` — a
//! real cycle between the two layers. The provider vocabulary now lives here
//! (see [`vocabulary`]) and the compiler imports it downward, so the cycle is
//! gone by construction rather than by convention.
//!
//! Depends on `wns-kernel` only.

pub mod vocabulary;

pub mod adapter;
pub mod catalog;
pub mod claude_exec;
pub mod claude_profile;
#[cfg(windows)]
pub mod claude_runner;
#[cfg(windows)]
pub mod claude_runtime;
pub mod cli;
pub mod codex_app_server;
pub mod codex_catalog;
#[cfg(windows)]
pub mod codex_discovery;
pub mod codex_exec;
pub mod codex_profile;
#[cfg(windows)]
pub mod codex_runner;
#[cfg(windows)]
pub mod codex_runtime;
pub mod credentials;
pub mod endpoints;
pub mod http_request;
pub mod openai_compatible;
pub mod preferences;
