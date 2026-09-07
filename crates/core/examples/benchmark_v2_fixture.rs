//! Alternating on-disk fixture setup comparison using the project's pinned SQLite.
use rusqlite::{Connection, types::Value};
use serde_json::json;
use std::{fs, time::Instant};
use uuid::Uuid;
use webnovel_core::v2_import::preview_v2_import;

fn logical_contents(connection: &Connection) -> Vec<(String, Vec<Vec<String>>)> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let sql = format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                table.replace('"', "\"\"")
            );
            let mut statement = connection.prepare(&sql).unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| {
                            row.get::<_, Value>(column)
                                .map(|value| format!("{value:?}"))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, rows)
        })
        .collect()
}

fn main() {
    let fixture = include_str!("../../../tests/fixtures/v2-import/schema8.sql");
    assert_eq!(fixture.matches("\nBEGIN;").count(), 1);
    assert_eq!(fixture.matches("\nCOMMIT;").count(), 1);
    let baseline = fixture.replace("\nBEGIN;", "").replace("\nCOMMIT;", "");
    let mut samples = Vec::new();
    let mut expected = None;
    for pair in 0..5 {
        for (variant, sql) in [("baseline", baseline.as_str()), ("transaction", fixture)] {
            let path =
                std::env::temp_dir().join(format!("v2-fixture-benchmark-{}.db", Uuid::new_v4()));
            let started = Instant::now();
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(sql).unwrap();
            let setup_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(connection.is_autocommit());
            assert_eq!(
                connection
                    .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                8
            );
            assert!(
                !connection
                    .prepare("PRAGMA foreign_key_check")
                    .unwrap()
                    .exists([])
                    .unwrap()
            );
            let contents = logical_contents(&connection);
            if let Some(expected) = &expected {
                assert_eq!(&contents, expected);
            } else {
                expected = Some(contents);
            }
            connection.close().unwrap();
            let before = fs::read(&path).unwrap();
            let started = Instant::now();
            preview_v2_import(&path, "p-alpha").unwrap();
            let preview_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(
                fs::read(&path).unwrap(),
                before,
                "Preview changed source bytes"
            );
            samples.push(json!({"pair": pair, "variant": variant, "setupMs": setup_ms, "previewMs": preview_ms}));
            fs::remove_file(path).unwrap();
        }
    }
    println!(
        "{}",
        json!({"sqliteVersion": rusqlite::version(), "samples": samples,
        "scope": "five alternating fresh file fixtures; identical logical rows; committed and closed before unchanged-source preview"})
    );
}
