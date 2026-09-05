use crate::projects::{CoreError, CoreResult};
use rusqlite::Connection;

pub(crate) fn configure(connection: &Connection) -> CoreResult<()> {
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
    )?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let sync: i64 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    if mode != "wal" || sync != 2 || keys != 1 {
        return Err(CoreError::new(
            "PersistenceUnavailable",
            "Required SQLite durability settings are unavailable.",
        ));
    }
    Ok(())
}

pub(crate) fn migrate(connection: &mut Connection) -> CoreResult<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > 1 {
        return Err(CoreError::new(
            "UnsupportedSchema",
            "This project needs a newer version of WebnovelStudio.",
        ));
    }
    if version == 0 {
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch(include_str!("001_projects.sql"))?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit().map_err(CoreError::uncertain)?;
    }
    Ok(())
}
