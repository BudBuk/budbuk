//! Pure SQL generation for the shadow-table sync engine.
//!
//! Kept separate from the I/O so it can be unit-tested. A synced table is
//! materialized into `shadow."<source>__<table>"`, using the same
//! `DataType → PostgreSQL type` mapping as `IMPORT FOREIGN SCHEMA`.

use connector_sdk::{pg_type, Row, TableSchema, Value};

/// The (unqualified) shadow table name for a source's table.
pub fn shadow_table(source_id: &str, table: &str) -> String {
    format!("{source_id}__{table}")
}

/// Quote a SQL identifier (double quotes, doubling any embedded quote).
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// `CREATE TABLE shadow."<id>__<table>" (...)` with typed columns.
pub fn create_shadow_ddl(source_id: &str, table: &str, schema: &TableSchema) -> String {
    let columns = schema
        .columns
        .iter()
        .map(|c| format!("{} {}", quote_ident(&c.name), pg_type(c.data_type)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE TABLE shadow.{} ({})",
        quote_ident(&shadow_table(source_id, table)),
        columns
    )
}

/// Render one neutral [`Value`] as a SQL literal.
pub fn value_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Text(s) => quote_literal(s),
        Value::Integer(n) => n.to_string(),
        // Non-finite floats have no SQL numeric literal — store NULL.
        Value::Float(f) if f.is_finite() => f.to_string(),
        Value::Float(_) => "NULL".to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Timestamp(s) => quote_literal(s),
        Value::Json(j) => quote_literal(&j.to_string()),
    }
}

/// A single multi-row `INSERT`, or `None` if there are no rows.
pub fn insert_rows_sql(
    source_id: &str,
    table: &str,
    schema: &TableSchema,
    rows: &[Row],
) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let columns = schema
        .columns
        .iter()
        .map(|c| quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let tuples = rows
        .iter()
        .map(|row| {
            let cells = row
                .0
                .iter()
                .map(value_literal)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({cells})")
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "INSERT INTO shadow.{} ({}) VALUES {}",
        quote_ident(&shadow_table(source_id, table)),
        columns,
        tuples
    ))
}

/// `SELECT "c1"::text, "c2"::text FROM shadow.<t> LIMIT n` — every column cast
/// to text so the API can return rows generically.
pub fn select_text_sql(source_id: &str, table: &str, schema: &TableSchema, limit: usize) -> String {
    let cols = if schema.columns.is_empty() {
        "*".to_string()
    } else {
        schema
            .columns
            .iter()
            .map(|c| format!("{}::text", quote_ident(&c.name)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "SELECT {} FROM shadow.{} LIMIT {}",
        cols,
        quote_ident(&shadow_table(source_id, table)),
        limit
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use connector_sdk::{Column, DataType};

    fn schema() -> TableSchema {
        TableSchema::new(
            "projects",
            vec![
                Column {
                    name: "id".into(),
                    data_type: DataType::Integer,
                },
                Column {
                    name: "name".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "rate".into(),
                    data_type: DataType::Float,
                },
                Column {
                    name: "active".into(),
                    data_type: DataType::Bool,
                },
                Column {
                    name: "created".into(),
                    data_type: DataType::Timestamp,
                },
                Column {
                    name: "meta".into(),
                    data_type: DataType::Json,
                },
            ],
        )
    }

    #[test]
    fn shadow_table_name() {
        assert_eq!(shadow_table("s1", "projects"), "s1__projects");
    }

    #[test]
    fn create_ddl_has_types_and_quoting() {
        let ddl = create_shadow_ddl("s1", "projects", &schema());
        assert_eq!(
            ddl,
            r#"CREATE TABLE shadow."s1__projects" ("id" bigint, "name" text, "rate" double precision, "active" boolean, "created" text, "meta" text)"#
        );
    }

    #[test]
    fn value_literal_covers_every_variant() {
        assert_eq!(value_literal(&Value::Null), "NULL");
        assert_eq!(value_literal(&Value::Text("a'b".into())), "'a''b'");
        assert_eq!(value_literal(&Value::Integer(7)), "7");
        assert_eq!(value_literal(&Value::Float(1.5)), "1.5");
        assert_eq!(value_literal(&Value::Float(f64::NAN)), "NULL");
        assert_eq!(value_literal(&Value::Float(f64::INFINITY)), "NULL");
        assert_eq!(value_literal(&Value::Bool(true)), "true");
        assert_eq!(value_literal(&Value::Bool(false)), "false");
        assert_eq!(value_literal(&Value::Timestamp("2026".into())), "'2026'");
        assert_eq!(
            value_literal(&Value::Json(serde_json::json!({"k": 1}))),
            "'{\"k\":1}'"
        );
    }

    #[test]
    fn insert_sql_builds_multi_row_and_empty_is_none() {
        let rows = vec![
            Row(vec![
                Value::Integer(1),
                Value::Text("app".into()),
                Value::Null,
                Value::Bool(true),
                Value::Timestamp("t".into()),
                Value::Json(serde_json::json!([1])),
            ]),
            Row(vec![
                Value::Integer(2),
                Value::Text("lib".into()),
                Value::Float(2.0),
                Value::Bool(false),
                Value::Null,
                Value::Null,
            ]),
        ];
        let sql = insert_rows_sql("s1", "projects", &schema(), &rows).unwrap();
        assert!(sql.starts_with(r#"INSERT INTO shadow."s1__projects" ("id", "name", "rate", "active", "created", "meta") VALUES "#));
        assert!(sql.contains("(1, 'app', NULL, true, 't', '[1]')"));
        assert!(sql.contains("(2, 'lib', 2, false, NULL, NULL)"));
        assert!(insert_rows_sql("s1", "projects", &schema(), &[]).is_none());
    }

    #[test]
    fn select_casts_columns_to_text() {
        let sql = select_text_sql("s1", "projects", &schema(), 50);
        assert!(sql.starts_with(r#"SELECT "id"::text, "name"::text"#));
        assert!(sql.ends_with(r#"FROM shadow."s1__projects" LIMIT 50"#));
        // Empty schema falls back to `*`.
        let empty = TableSchema::new("t", vec![]);
        assert!(select_text_sql("s1", "t", &empty, 5).contains("SELECT * FROM"));
    }
}
