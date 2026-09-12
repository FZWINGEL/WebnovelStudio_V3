//! Transport preference is separate from the author model and from any
//! already-accepted project request. Reading it never starts a process.
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use wns_kernel::{CoreError, CoreResult};
use wns_providers::preferences::parse_revision;
use wns_transfer::host::TransferFactory;

use crate::library::Library;


const KEY: &str = "codex-transport-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CodexTransport {
    #[default]
    Exec,
    AppServer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexTransportSettings {
    pub revision: String,
    pub transport: CodexTransport,
}

impl<F: TransferFactory> Library<F> {
    pub fn codex_transport_settings(&self) -> CoreResult<CodexTransportSettings> {
        let row: Option<(i64, i64, String)> = self
            .connection
            .query_row(
                "SELECT schema_version,revision,value_json FROM app_preferences WHERE key=?",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match row {
            None => Ok(CodexTransportSettings {
                revision: "0".into(),
                transport: CodexTransport::default(),
            }),
            Some((1, revision, json)) if revision >= 0 => Ok(CodexTransportSettings {
                revision: revision.to_string(),
                transport: serde_json::from_str(&json).map_err(|_| invalid())?,
            }),
            _ => Err(invalid()),
        }
    }

    pub fn save_codex_transport(
        &mut self,
        expected_revision: &str,
        transport: CodexTransport,
    ) -> CoreResult<CodexTransportSettings> {
        let expected = parse_revision(expected_revision)?;
        let current = self.codex_transport_settings()?;
        if parse_revision(&current.revision)? != expected {
            return Err(CoreError::new(
                "PreferenceConflict",
                "The Codex transport changed. Read Settings again before saving.",
            ));
        }
        let next = expected.checked_add(1).ok_or_else(invalid)?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO app_preferences(key,schema_version,revision,value_json,updated_at) VALUES(?,1,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET schema_version=excluded.schema_version,revision=excluded.revision,value_json=excluded.value_json,updated_at=excluded.updated_at",
            params![KEY, next, serde_json::to_string(&transport)?],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(CodexTransportSettings {
            revision: next.to_string(),
            transport,
        })
    }
}

fn invalid() -> CoreError {
    CoreError::new(
        "InvalidCodexTransport",
        "The stored Codex transport preference is invalid.",
    )
}
