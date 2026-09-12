//! Conversation-owned run selection for consumers that do not need a full run.

use rusqlite::{Connection, OptionalExtension, params};
use wns_kernel::{CoreError, CoreResult};
use wns_story::run_vocabulary::CompletedDiscussionOutput;

/// Enumerate every run in insertion order. Consumers retain their own intent
/// filtering after materializing the run through the existing validated reader.
pub fn run_ids_at(connection: &Connection) -> CoreResult<Vec<String>> {
    let mut statement = connection.prepare("SELECT id FROM discussion_runs ORDER BY rowid")?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Find the immutable run identity for an exact project operation, including
/// operations whose intent differs from the caller's expected intent.
pub fn run_id_for_operation_at(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    operation_id: &str,
) -> CoreResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",
            params![project_id, operation_namespace, operation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(CoreError::from)
}

/// Read raw completed, delivered outputs in insertion order on the supplied
/// connection, including its current transaction. This deliberately does not
/// validate packets or interpret output: candidate consumers may skip unrelated
/// malformed output that a full `read_run` would reject.
pub fn completed_outputs_at(connection: &Connection) -> CoreResult<Vec<CompletedDiscussionOutput>> {
    let mut statement = connection.prepare(
        "SELECT id,packet_id,output_text FROM discussion_runs WHERE status='completed' AND dispatch_state='delivered' ORDER BY rowid",
    )?;
    Ok(statement
        .query_map([], |row| {
            Ok(CompletedDiscussionOutput {
                run_id: row.get(0)?,
                packet_id: row.get(1)?,
                output_text: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE discussion_runs (
                    id TEXT NOT NULL, packet_id TEXT NOT NULL, output_text TEXT NOT NULL,
                    status TEXT NOT NULL, dispatch_state TEXT NOT NULL,
                    project_id TEXT NOT NULL, operation_namespace TEXT NOT NULL,
                    operation_id TEXT NOT NULL
                )",
            )
            .unwrap();
        connection
    }

    fn insert(connection: &Connection, id: &str, status: &str, dispatch_state: &str) {
        connection
            .execute(
                "INSERT INTO discussion_runs VALUES (?, ?, ?, ?, ?, 'project', 'namespace', ?)",
                params![
                    id,
                    format!("missing-packet-{id}"),
                    "unparsed output",
                    status,
                    dispatch_state,
                    id
                ],
            )
            .unwrap();
    }

    #[test]
    fn selection_preserves_insertion_order_and_completed_delivered_predicate() {
        let connection = database();
        insert(&connection, "z-completed", "completed", "delivered");
        insert(&connection, "running", "running", "delivered");
        insert(&connection, "pending", "completed", "pending");
        insert(&connection, "a-completed", "completed", "delivered");

        assert_eq!(
            run_ids_at(&connection).unwrap(),
            ["z-completed", "running", "pending", "a-completed"]
        );
        let outputs = completed_outputs_at(&connection).unwrap();
        assert_eq!(
            outputs,
            ["z-completed", "a-completed"].map(|id| CompletedDiscussionOutput {
                run_id: id.into(),
                packet_id: format!("missing-packet-{id}"),
                output_text: "unparsed output".into(),
            })
        );
    }

    #[test]
    fn operation_lookup_matches_each_owner_component_exactly() {
        let connection = database();
        for (id, project_id, namespace, operation_id) in [
            ("other-project", "other", "namespace", "operation"),
            ("other-namespace", "project", "other", "operation"),
            ("other-operation", "project", "namespace", "other"),
            ("expected", "project", "namespace", "operation"),
        ] {
            connection
                .execute(
                    "INSERT INTO discussion_runs VALUES (?, 'packet', '', 'queued', 'pending', ?, ?, ?)",
                    params![id, project_id, namespace, operation_id],
                )
                .unwrap();
            assert_eq!(
                run_id_for_operation_at(&connection, project_id, namespace, operation_id).unwrap(),
                Some(id.into())
            );
        }
        for (project_id, namespace, operation_id) in [
            ("missing", "namespace", "operation"),
            ("project", "missing", "operation"),
            ("project", "namespace", "missing"),
        ] {
            assert_eq!(
                run_id_for_operation_at(&connection, project_id, namespace, operation_id).unwrap(),
                None
            );
        }
    }

    #[test]
    fn readers_observe_the_supplied_transaction_and_its_rollback() {
        let mut connection = database();
        let transaction = connection.transaction().unwrap();
        insert(&transaction, "uncommitted", "completed", "delivered");
        assert_eq!(run_ids_at(&transaction).unwrap(), ["uncommitted"]);
        assert_eq!(
            run_id_for_operation_at(&transaction, "project", "namespace", "uncommitted").unwrap(),
            Some("uncommitted".into())
        );
        assert_eq!(
            completed_outputs_at(&transaction).unwrap()[0].run_id,
            "uncommitted"
        );
        transaction.rollback().unwrap();
        assert!(run_ids_at(&connection).unwrap().is_empty());
        assert!(completed_outputs_at(&connection).unwrap().is_empty());
        assert_eq!(
            run_id_for_operation_at(&connection, "project", "namespace", "uncommitted").unwrap(),
            None
        );
    }
}
