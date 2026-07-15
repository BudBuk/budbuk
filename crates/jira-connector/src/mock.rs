//! Canned sample data, so the connector runs without a real Jira account.
//! In the next step we replace these with real HTTP calls to Jira's API.

use connector_sdk::{Column, DataType, Row, TableSchema, Value};

/// Tiny helper to build a `Column` with less typing.
fn col(name: &str, data_type: DataType) -> Column {
    Column {
        name: name.to_string(),
        data_type,
    }
}

/// The four tables our Jira connector exposes, with their columns and types.
/// This is what schema discovery returns.
pub fn schemas() -> Vec<TableSchema> {
    use DataType::*;
    vec![
        TableSchema::new(
            "projects",
            vec![
                col("id", Integer),
                col("key", Text),
                col("name", Text),
                col("lead", Text),
            ],
        ),
        TableSchema::new(
            "issues",
            vec![
                col("key", Text),
                col("summary", Text),
                col("status", Text),
                col("assignee", Text),
                col("project", Text),
                col("created", Timestamp),
            ],
        ),
        TableSchema::new(
            "users",
            vec![
                col("account_id", Text),
                col("display_name", Text),
                col("email", Text),
                col("active", Bool),
            ],
        ),
        TableSchema::new(
            "worklogs",
            vec![
                col("id", Integer),
                col("issue_key", Text),
                col("author", Text),
                col("time_spent_seconds", Integer),
                col("started", Timestamp),
            ],
        ),
    ]
}

/// Canned rows for one table, or `None` if we don't recognize the table name.
/// The `match` picks a branch by the table string — like a powerful switch.
pub fn rows_for(table: &str) -> Option<Vec<Row>> {
    match table {
        "projects" => Some(vec![
            Row(vec![
                Value::Integer(10001),
                Value::Text("ENG".into()),
                Value::Text("Engineering".into()),
                Value::Text("Alice".into()),
            ]),
            Row(vec![
                Value::Integer(10002),
                Value::Text("OPS".into()),
                Value::Text("Operations".into()),
                Value::Text("Bob".into()),
            ]),
        ]),
        "issues" => Some(vec![
            Row(vec![
                Value::Text("ENG-1".into()),
                Value::Text("Set up CI".into()),
                Value::Text("Done".into()),
                Value::Text("Alice".into()),
                Value::Text("ENG".into()),
                Value::Timestamp("2026-07-01T09:00:00Z".into()),
            ]),
            Row(vec![
                Value::Text("ENG-2".into()),
                Value::Text("Write the FDW".into()),
                Value::Text("In Progress".into()),
                Value::Text("Carol".into()),
                Value::Text("ENG".into()),
                Value::Timestamp("2026-07-05T14:30:00Z".into()),
            ]),
            // Note the NULLs: this issue is unassigned and unestimated.
            Row(vec![
                Value::Text("OPS-7".into()),
                Value::Text("Rotate API keys".into()),
                Value::Text("Open".into()),
                Value::Null,
                Value::Text("OPS".into()),
                Value::Timestamp("2026-07-10T11:15:00Z".into()),
            ]),
        ]),
        "users" => Some(vec![
            Row(vec![
                Value::Text("acc-1".into()),
                Value::Text("Alice".into()),
                Value::Text("alice@example.com".into()),
                Value::Bool(true),
            ]),
            Row(vec![
                Value::Text("acc-2".into()),
                Value::Text("Bob".into()),
                Value::Text("bob@example.com".into()),
                Value::Bool(true),
            ]),
            Row(vec![
                Value::Text("acc-3".into()),
                Value::Text("Carol".into()),
                Value::Text("carol@example.com".into()),
                Value::Bool(false),
            ]),
        ]),
        "worklogs" => Some(vec![
            Row(vec![
                Value::Integer(1),
                Value::Text("ENG-1".into()),
                Value::Text("Alice".into()),
                Value::Integer(3600),
                Value::Timestamp("2026-07-02T10:00:00Z".into()),
            ]),
            Row(vec![
                Value::Integer(2),
                Value::Text("ENG-2".into()),
                Value::Text("Carol".into()),
                Value::Integer(7200),
                Value::Timestamp("2026-07-06T13:00:00Z".into()),
            ]),
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_expose_the_four_expected_tables() {
        let schemas = schemas();
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["projects", "issues", "users", "worklogs"]);
        // Spot-check column counts to guard against accidental drift.
        let issues = schemas.iter().find(|s| s.name == "issues").unwrap();
        assert_eq!(issues.columns.len(), 6);
    }

    #[test]
    fn rows_for_returns_data_for_each_known_table() {
        for table in ["projects", "issues", "users", "worklogs"] {
            let rows = rows_for(table).expect("known table has rows");
            assert!(!rows.is_empty(), "expected rows for {table}");
            // Every row must have the same width as the table's schema.
            let schema = schemas().into_iter().find(|s| s.name == table).unwrap();
            for row in &rows {
                assert_eq!(
                    row.0.len(),
                    schema.columns.len(),
                    "width mismatch in {table}"
                );
            }
        }
    }

    #[test]
    fn rows_for_unknown_table_is_none() {
        assert!(rows_for("nope").is_none());
    }
}
