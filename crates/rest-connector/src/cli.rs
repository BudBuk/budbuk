//! Demo CLI wiring for the REST engine, factored out of `main.rs` so it's
//! testable. `build_demo_spec` describes the public JSONPlaceholder API as a
//! [`SourceSpec`] — a hand-written spec is all it takes to get a working
//! connector, which is the whole reusability point.

use connector_sdk::{Connector, DataType, Filter, Operator, Query, Row, TableSchema, Value};

use crate::spec::{ColumnSpec, FilterParam, Pagination, SourceSpec, TableSpec};
use crate::RestConnector;

/// A hand-written spec for the public JSONPlaceholder API (no auth). Parameterized
/// by `base_url` so tests can point it at a mock server.
pub fn build_demo_spec(base_url: &str) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    SourceSpec {
        name: "jsonplaceholder".to_string(),
        base_url: base_url.to_string(),
        auth: Default::default(), // None
        tables: vec![
            TableSpec {
                name: "posts".to_string(),
                path: "/posts".to_string(),
                row_path: Default::default(), // Root
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("userId", "userId", DataType::Integer),
                    col("title", "title", DataType::Text),
                ],
                pagination: Pagination::Offset {
                    start_param: "_start".to_string(),
                    limit_param: "_limit".to_string(),
                    page_size: 20,
                },
                filters: vec![FilterParam {
                    column: "userId".to_string(),
                    param: "userId".to_string(),
                }],
            },
            TableSpec {
                name: "users".to_string(),
                path: "/users".to_string(),
                row_path: Default::default(),
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("username", "username", DataType::Text),
                    // Nested field extraction.
                    col("company", "company.name", DataType::Text),
                ],
                pagination: Pagination::None,
                filters: vec![],
            },
        ],
    }
}

/// Install a `tracing` subscriber (level via `RUST_LOG`, default `info`).
pub fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}

/// Build the demo connector against `base_url` and run the demo.
pub async fn run_at(base_url: &str) -> anyhow::Result<()> {
    init_tracing();
    run_with(RestConnector::new(build_demo_spec(base_url))).await
}

/// A tiny bundled OpenAPI document describing JSONPlaceholder — used to
/// demonstrate that an imported spec runs on the same engine.
pub const SAMPLE_OPENAPI: &str = r##"{
  "openapi": "3.0.0",
  "info": { "title": "JSONPlaceholder" },
  "servers": [{ "url": "https://jsonplaceholder.typicode.com" }],
  "paths": {
    "/posts": {
      "get": {
        "parameters": [{ "name": "userId", "in": "query", "schema": { "type": "integer" } }],
        "responses": { "200": { "content": { "application/json": {
          "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Post" } } } } } }
      }
    },
    "/users": {
      "get": {
        "responses": { "200": { "content": { "application/json": {
          "schema": { "type": "array", "items": { "$ref": "#/components/schemas/User" } } } } } }
      }
    }
  },
  "components": { "schemas": {
    "Post": { "type": "object", "properties": {
      "id": { "type": "integer" }, "userId": { "type": "integer" },
      "title": { "type": "string" }, "body": { "type": "string" } } },
    "User": { "type": "object", "properties": {
      "id": { "type": "integer" }, "name": { "type": "string" },
      "username": { "type": "string" }, "email": { "type": "string" } } }
  } }
}"##;

/// Import the bundled OpenAPI spec (overriding its base URL) and run the demo —
/// proving the OpenAPI importer feeds the same engine, end to end.
pub async fn run_openapi_at(base_url: &str) -> anyhow::Result<()> {
    init_tracing();
    let opts = crate::ImportOptions {
        base_url: Some(base_url.to_string()),
        ..Default::default()
    };
    let spec = SourceSpec::from_openapi_json(SAMPLE_OPENAPI, opts)?;
    println!(
        "(spec generated from OpenAPI: {} tables)\n",
        spec.tables.len()
    );
    run_with(RestConnector::new(spec)).await
}

/// Run the demo against any connector: discovery, a scan of each table, and a
/// predicate-pushdown example.
pub async fn run_with<C: Connector>(connector: C) -> anyhow::Result<()> {
    println!("budbuk rest-connector: {}\n", connector.name());
    let schemas = connector.discover().await?;

    println!("=== all tables (limit 3) ===\n");
    for schema in &schemas {
        let query = Query {
            limit: Some(3),
            ..Default::default()
        };
        match connector.fetch(&schema.name, &query).await {
            Ok(rows) => println!("{}\n", render_table(schema, &rows)),
            Err(e) => println!("── {} ──\n  (skipped: {e})\n", schema.name),
        }
    }

    println!("=== pushdown: posts WHERE userId = 1 ===");
    let query = Query {
        filters: vec![Filter::new("userId", Operator::Eq, Value::Integer(1))],
        limit: Some(3),
        ..Default::default()
    };
    if let Some(schema) = schemas.iter().find(|s| s.name == "posts") {
        match connector.fetch("posts", &query).await {
            Ok(rows) => println!("{}", render_table(schema, &rows)),
            Err(e) => println!("  (error: {e})"),
        }
    }
    Ok(())
}

/// Render a table's header and rows as an aligned text grid.
pub fn render_table(schema: &TableSchema, rows: &[Row]) -> String {
    let headers: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, value) in row.0.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(value.to_display_string().len());
            }
        }
    }

    let mut out = format!("── {} ({} rows) ──\n", schema.name, rows.len());
    out.push_str(&render_row(&headers, &widths));
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&render_row(&rule, &widths));
    for row in rows {
        let cells: Vec<String> = row.0.iter().map(|v| v.to_display_string()).collect();
        out.push_str(&render_row(&cells, &widths));
    }
    out.trim_end().to_string()
}

fn render_row(cells: &[String], widths: &[usize]) -> String {
    let padded: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:width$}", c, width = widths.get(i).copied().unwrap_or(0)))
        .collect();
    format!("| {} |\n", padded.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use connector_sdk::{Column, ConnectorError, Result};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A configurable fake connector to drive the run_with branches.
    struct FakeConnector {
        schemas: Vec<TableSchema>,
        fetch_err: bool,
    }

    #[async_trait]
    impl Connector for FakeConnector {
        fn name(&self) -> &str {
            "fake"
        }
        async fn discover(&self) -> Result<Vec<TableSchema>> {
            Ok(self.schemas.clone())
        }
        async fn fetch(&self, _table: &str, _query: &Query) -> Result<Vec<Row>> {
            if self.fetch_err {
                return Err(ConnectorError::Other("nope".into()));
            }
            Ok(vec![])
        }
    }

    fn posts_schema() -> TableSchema {
        TableSchema::new(
            "posts",
            vec![Column {
                name: "id".into(),
                data_type: DataType::Integer,
            }],
        )
    }

    #[test]
    fn build_demo_spec_defines_expected_tables() {
        let spec = build_demo_spec("https://example.com");
        assert_eq!(spec.name, "jsonplaceholder");
        assert!(spec.table("posts").is_some());
        assert!(spec.table("users").is_some());
    }

    #[test]
    fn render_table_produces_aligned_grid() {
        let schema = TableSchema::new(
            "posts",
            vec![
                Column {
                    name: "id".into(),
                    data_type: DataType::Integer,
                },
                Column {
                    name: "title".into(),
                    data_type: DataType::Text,
                },
            ],
        );
        let rows = vec![Row(vec![Value::Integer(1), Value::Text("hello".into())])];
        let out = render_table(&schema, &rows);
        assert!(out.contains("posts (1 rows)"));
        assert!(out.contains("hello"));
    }

    #[test]
    fn render_table_tolerates_rows_wider_than_schema() {
        let schema = TableSchema::new(
            "t",
            vec![Column {
                name: "c".into(),
                data_type: DataType::Text,
            }],
        );
        let rows = vec![Row(vec![Value::Text("a".into()), Value::Text("b".into())])];
        let out = render_table(&schema, &rows);
        assert!(out.contains('a') && out.contains('b'));
    }

    #[tokio::test]
    async fn run_with_failing_connector_handles_errors() {
        let c = FakeConnector {
            schemas: vec![posts_schema()],
            fetch_err: true,
        };
        run_with(c).await.unwrap();
    }

    #[tokio::test]
    async fn run_with_no_posts_skips_pushdown() {
        let other = TableSchema::new("users", vec![]);
        let c = FakeConnector {
            schemas: vec![other],
            fetch_err: false,
        };
        run_with(c).await.unwrap();
    }

    #[tokio::test]
    async fn run_at_against_mock_server_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/posts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "userId": 1, "title": "hello"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "name": "Alice", "username": "alice", "company": {"name": "Acme"}}
            ])))
            .mount(&server)
            .await;

        run_at(&server.uri()).await.unwrap();
    }

    #[test]
    fn sample_openapi_imports_to_expected_tables() {
        let spec = SourceSpec::from_openapi_json(SAMPLE_OPENAPI, Default::default()).unwrap();
        assert!(spec.table("posts").is_some());
        assert!(spec.table("users").is_some());
    }

    #[tokio::test]
    async fn run_openapi_against_mock_server_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/posts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "userId": 1, "title": "hello", "body": "b"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "name": "Alice", "username": "alice", "email": "a@x"}
            ])))
            .mount(&server)
            .await;

        run_openapi_at(&server.uri()).await.unwrap();
    }
}
