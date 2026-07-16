//! Generating PostgreSQL DDL from discovered schemas.
//!
//! This powers `IMPORT FOREIGN SCHEMA`: given the [`TableSchema`]s a connector
//! reports from [`discover`](crate::Connector::discover), produce the
//! `CREATE FOREIGN TABLE` statements PostgreSQL should run. The FDW layer calls
//! these functions; the logic lives here so it's pure and unit-tested.

use crate::types::{DataType, TableSchema};

/// The PostgreSQL column type for a neutral [`DataType`].
///
/// `Timestamp` and `Json` map to `text` on purpose: the engine currently
/// materializes those values as strings (the FDW turns them into a text cell),
/// so the declared column type must be `text` to match. Upgrading them to
/// `timestamptz`/`jsonb` requires the FDW to emit typed cells first.
pub fn pg_type(data_type: DataType) -> &'static str {
    match data_type {
        DataType::Text => "text",
        DataType::Integer => "bigint",
        DataType::Float => "double precision",
        DataType::Bool => "boolean",
        DataType::Timestamp => "text",
        DataType::Json => "text",
    }
}

/// Which tables `IMPORT FOREIGN SCHEMA` should create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFilter {
    /// `IMPORT FOREIGN SCHEMA … ` — every discovered table.
    All,
    /// `… LIMIT TO (a, b)` — only the named tables.
    LimitTo(Vec<String>),
    /// `… EXCEPT (a, b)` — every table except the named ones.
    Except(Vec<String>),
}

impl ImportFilter {
    fn includes(&self, name: &str) -> bool {
        match self {
            ImportFilter::All => true,
            ImportFilter::LimitTo(names) => names.iter().any(|n| n == name),
            ImportFilter::Except(names) => !names.iter().any(|n| n == name),
        }
    }
}

/// Quote a SQL identifier (double quotes, doubling any embedded quote).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a SQL string literal (single quotes, doubling any embedded quote).
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Build a single `CREATE FOREIGN TABLE` statement for one discovered table.
///
/// The local table takes the connector's table name and carries an
/// `OPTIONS (object '<table>')` so the FDW's scan knows which table to fetch.
pub fn create_foreign_table_ddl(schema: &TableSchema, server: &str, local_schema: &str) -> String {
    let columns = schema
        .columns
        .iter()
        .map(|c| format!("{} {}", quote_ident(&c.name), pg_type(c.data_type)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE FOREIGN TABLE {}.{} ({}) SERVER {} OPTIONS (object {})",
        quote_ident(local_schema),
        quote_ident(&schema.name),
        columns,
        quote_ident(server),
        quote_literal(&schema.name),
    )
}

/// Build the `CREATE FOREIGN TABLE` statements for the discovered schemas that
/// pass `filter`. This is what an FDW's `import_foreign_schema` returns.
pub fn create_foreign_table_statements(
    schemas: &[TableSchema],
    server: &str,
    local_schema: &str,
    filter: &ImportFilter,
) -> Vec<String> {
    schemas
        .iter()
        .filter(|s| filter.includes(&s.name))
        .map(|s| create_foreign_table_ddl(s, server, local_schema))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Column;

    fn col(name: &str, data_type: DataType) -> Column {
        Column {
            name: name.to_string(),
            data_type,
        }
    }

    fn schemas() -> Vec<TableSchema> {
        vec![
            TableSchema::new(
                "charges",
                vec![
                    col("id", DataType::Text),
                    col("amount", DataType::Integer),
                    col("rate", DataType::Float),
                    col("paid", DataType::Bool),
                    col("created", DataType::Timestamp),
                    col("meta", DataType::Json),
                ],
            ),
            TableSchema::new("customers", vec![col("id", DataType::Text)]),
        ]
    }

    #[test]
    fn pg_type_maps_every_data_type() {
        assert_eq!(pg_type(DataType::Text), "text");
        assert_eq!(pg_type(DataType::Integer), "bigint");
        assert_eq!(pg_type(DataType::Float), "double precision");
        assert_eq!(pg_type(DataType::Bool), "boolean");
        assert_eq!(pg_type(DataType::Timestamp), "text");
        assert_eq!(pg_type(DataType::Json), "text");
    }

    #[test]
    fn ddl_has_expected_shape_and_types() {
        let ddl = create_foreign_table_ddl(&schemas()[0], "stripe", "stripe");
        assert_eq!(
            ddl,
            r#"CREATE FOREIGN TABLE "stripe"."charges" ("id" text, "amount" bigint, "rate" double precision, "paid" boolean, "created" text, "meta" text) SERVER "stripe" OPTIONS (object 'charges')"#
        );
    }

    #[test]
    fn identifiers_and_literals_are_quoted_safely() {
        let s = TableSchema::new("we\"ird", vec![col("a\"b", DataType::Text)]);
        let ddl = create_foreign_table_ddl(&s, "srv", "it's");
        assert!(ddl.contains(r#""it's"."we""ird""#));
        assert!(ddl.contains(r#""a""b" text"#));
        // The object literal doubles the single quote is N/A here (name has none),
        // but the table name with a quote is doubled in the identifier.
        assert!(ddl.contains("OPTIONS (object 'we\"ird')"));
    }

    #[test]
    fn literal_escapes_single_quotes() {
        let s = TableSchema::new("o'hara", vec![col("id", DataType::Text)]);
        let ddl = create_foreign_table_ddl(&s, "srv", "public");
        assert!(ddl.contains("OPTIONS (object 'o''hara')"));
    }

    #[test]
    fn statements_all_includes_every_table() {
        let out =
            create_foreign_table_statements(&schemas(), "stripe", "stripe", &ImportFilter::All);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains(r#""stripe"."charges""#));
        assert!(out[1].contains(r#""stripe"."customers""#));
    }

    #[test]
    fn statements_limit_to_selects_named_tables() {
        let out = create_foreign_table_statements(
            &schemas(),
            "stripe",
            "stripe",
            &ImportFilter::LimitTo(vec!["customers".into()]),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("customers"));
    }

    #[test]
    fn statements_except_omits_named_tables() {
        let out = create_foreign_table_statements(
            &schemas(),
            "stripe",
            "stripe",
            &ImportFilter::Except(vec!["charges".into()]),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("customers"));
    }

    #[test]
    fn import_filter_is_debuggable_and_comparable() {
        let f = ImportFilter::LimitTo(vec!["a".into()]);
        assert_eq!(f, ImportFilter::LimitTo(vec!["a".into()]));
        assert_ne!(f, ImportFilter::All);
        assert!(!format!("{f:?}").is_empty());
        let _ = f.clone();
    }
}
