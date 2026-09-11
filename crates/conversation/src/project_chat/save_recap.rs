//! Read-only return recap from ordinary editor save receipts. No transcript
//! event, summary, source epoch or new copy of manuscript content is written.
use super::*;
use rusqlite::{OptionalExtension, params};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatDocumentSave {
    pub operation_id: String,
    pub head: Head,
    pub title: String,
    pub created_at: String,
    pub revision_id: Option<String>,
}

pub(super) fn read_document_saves(
    db: &rusqlite::Connection,
    access: &ProjectAccess,
    conversation_id: &str,
) -> CoreResult<Vec<ChatDocumentSave>> {
    // Latest save per ordinary document since this conversation was created.
    // The UI explicitly labels the bounded recent list. Namespace fencing
    // excludes historical receipt copies from duplication and recovery.
    let mut query = db.prepare(
        "WITH saves AS (
            SELECT r.operation_id,r.document_id,r.result_json,r.created_at,d.title,
                   ROW_NUMBER() OVER (PARTITION BY r.document_id ORDER BY r.rowid DESC) AS rank
            FROM command_receipts r JOIN documents d ON d.id=r.document_id
            JOIN project_conversations c ON c.id=? AND c.project_id=? AND c.operation_namespace=?
            WHERE r.operation_namespace=c.operation_namespace AND r.operation_kind='save'
              AND d.role='ordinary' AND d.trashed=0 AND r.created_at>=c.created_at
        ) SELECT operation_id,document_id,result_json,created_at,title FROM saves
          WHERE rank=1 ORDER BY created_at DESC,operation_id DESC LIMIT 20",
    )?;
    let rows = query
        .query_map(
            params![
                conversation_id,
                access.project_id,
                access.operation_namespace
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(|(operation_id, document_id, result_json, created_at, title)| {
        let result: StoredResult = serde_json::from_str(&result_json)?;
        if result.head.document_id != document_id || !valid_hash(&result.head.body_hash) {
            return Err(CoreError::new("InvalidProjectChat", "A saved document receipt has an invalid source identity."));
        }
        let version = parse_version(&result.head.version)?;
        let revision_id = db.query_row(
            "SELECT id FROM revisions WHERE document_id=? AND source_working_version=? AND body_hash=?",
            params![document_id, version, result.head.body_hash], |row| row.get(0),
        ).optional()?;
        Ok(ChatDocumentSave { operation_id, head: result.head, title, created_at, revision_id })
    }).collect()
}
