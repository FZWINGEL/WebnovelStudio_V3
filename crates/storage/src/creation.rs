//! The project-creation record, kept beside the database it describes.
//!
//! Identity of the library operation that installed this independent folder.
//! It lives here — not with the project actor — because the transfer crate
//! writes and reads it while staging a recovery, and a re-export through the
//! legacy crate would make transfer depend on the crate being decomposed.
//! `webnovel_core::projects` re-exports all three, so the actor's own
//! `create_staged` path is unchanged.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use wns_kernel::{CoreError, CoreResult, ProjectInfo, check_id};

/// Identity of the library operation that installed this independent folder.
/// Kept beside the database so registry recovery does not require a schema upgrade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreationOrigin {
    pub operation_namespace: String,
    pub operation_id: String,
}

pub fn write_creation_origin(path: &Path, origin: &CreationOrigin) -> CoreResult<()> {
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    let mut file = File::create_new(path.join("creation.json"))?;
    file.write_all(&serde_json::to_vec(origin)?)?;
    file.sync_all()?;
    Ok(())
}

pub fn read_creation_origin(path: &Path) -> CoreResult<CreationOrigin> {
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(path.join("creation.json"))?
        .take(4097)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(CoreError::new(
            "InvalidProject",
            "Invalid project creation record.",
        ));
    }
    let origin: CreationOrigin = serde_json::from_slice(&bytes)?;
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    Ok(origin)
}

/// The project's identity and the version of that record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMetadata {
    pub project: ProjectInfo,
    pub metadata_version: String,
}
