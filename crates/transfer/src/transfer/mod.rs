//! Staged project transfer operations.
//!
//! Backups use SQLite's online backup API from a separate read-only connection,
//! then package the resulting database and a small manifest in a stored ZIP.
//! Restore always creates a new project identity; it never replaces the source
//! project or reuses its active operation namespace.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the manifest vocabulary and size limits, `io` the staged
//! filesystem helpers, `heads` the database-head readers, `import` the storage
//! and manifest validators, `operations` the backup/recover/duplicate commands,
//! and `draft` the draft-projection and export preparation.

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, backup::Backup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use wns_documents::records::{CheckpointReason, CheckpointRequest};
use wns_kernel::validate_snapshot_json;
use wns_kernel::{
    CoreError, CoreResult, DocumentRole, Head, ProjectAccess, ProjectInfo, SourceEpoch,
    StoredResult,
};
use wns_storage::{CreationOrigin, read_creation_origin, write_creation_origin};
use wns_storage::{configure, migrate};
use wns_story::source_pins::AUTHOR_ROOM_AUDIENCE;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::host::{TransferFactory, TransferSource};

mod draft;
mod heads;
mod import;
mod io;
mod operations;
mod types;

pub use draft::*;
pub use heads::*;
pub(crate) use import::*;
pub use io::*;
pub use operations::*;
pub use types::*;
