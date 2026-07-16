//! Demo CLI wiring for the GraphQL engine, factored out of `main.rs` so it's
//! testable. `build_demo_spec` describes the public Countries GraphQL API
//! (`countries.trevorblades.com`, no auth) as a [`GraphQlSpec`]; a bundled
//! introspection document demonstrates that a *generated* spec runs on the same
//! engine — the analog of the REST engine's OpenAPI demo.

use connector_sdk::{Connector, Query, Row, TableSchema};

use crate::spec::{ColumnSpec, GraphQlSpec, GraphQlTable, NodeShape, Pagination};
use crate::{GraphQlConnector, ImportOptions};
use connector_sdk::DataType;

/// A hand-written spec for the public Countries GraphQL API (no auth).
/// Parameterized by `endpoint` so tests can point it at a mock server.
pub fn build_demo_spec(endpoint: &str) -> GraphQlSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    GraphQlSpec {
        name: "countries".to_string(),
        endpoint: endpoint.to_string(),
        auth: Default::default(), // None
        tables: vec![GraphQlTable {
            name: "countries".to_string(),
            query: "query { countries { code name emoji continent { code name } } }".to_string(),
            data_pointer: "/countries".to_string(),
            shape: NodeShape::List,
            columns: vec![
                col("code", "code", DataType::Text),
                col("name", "name", DataType::Text),
                col("emoji", "emoji", DataType::Text),
                // Nested object → JSON.
                col("continent", "continent", DataType::Json),
            ],
            pagination: Pagination::None,
            filters: vec![],
        }],
    }
}

/// Install a `tracing` subscriber (level via `RUST_LOG`, default `info`).
pub fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}

/// Build the demo connector against `endpoint` and run the demo.
pub async fn run_at(endpoint: &str) -> anyhow::Result<()> {
    init_tracing();
    run_with(GraphQlConnector::new(build_demo_spec(endpoint))).await
}

/// A small bundled introspection document describing the Countries API — used to
/// demonstrate that a spec generated from schema introspection runs on the same
/// engine.
pub const SAMPLE_INTROSPECTION: &str = r#"{
  "__schema": {
    "queryType": { "name": "Query" },
    "types": [
      { "name": "Query", "fields": [
        { "name": "countries", "args": [], "type": { "kind": "NON_NULL", "name": null, "ofType":
          { "kind": "LIST", "name": null, "ofType":
            { "kind": "NON_NULL", "name": null, "ofType":
              { "kind": "OBJECT", "name": "Country", "ofType": null } } } } }
      ]},
      { "name": "Country", "fields": [
        { "name": "code", "args": [], "type": { "kind": "NON_NULL", "name": null, "ofType":
          { "kind": "SCALAR", "name": "ID", "ofType": null } } },
        { "name": "name", "args": [], "type": { "kind": "SCALAR", "name": "String", "ofType": null } },
        { "name": "emoji", "args": [], "type": { "kind": "SCALAR", "name": "String", "ofType": null } },
        { "name": "continent", "args": [], "type": { "kind": "OBJECT", "name": "Continent", "ofType": null } }
      ]},
      { "name": "Continent", "fields": [
        { "name": "code", "args": [], "type": { "kind": "SCALAR", "name": "String", "ofType": null } },
        { "name": "name", "args": [], "type": { "kind": "SCALAR", "name": "String", "ofType": null } }
      ]}
    ]
  }
}"#;

/// Generate a spec from the bundled introspection document (overriding its
/// endpoint) and run the demo — proving the generator feeds the same engine.
pub async fn run_introspect_at(endpoint: &str) -> anyhow::Result<()> {
    init_tracing();
    let opts = ImportOptions {
        endpoint: endpoint.to_string(),
        name: Some("countries".to_string()),
        ..Default::default()
    };
    let spec = GraphQlSpec::from_introspection_json(SAMPLE_INTROSPECTION, opts)?;
    println!(
        "(spec generated from introspection: {} tables)\n",
        spec.tables.len()
    );
    run_with(GraphQlConnector::new(spec)).await
}

/// Run the demo against any connector: discovery, then a scan of each table.
pub async fn run_with<C: Connector>(connector: C) -> anyhow::Result<()> {
    println!("budbuk graphql-connector: {}\n", connector.name());
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
    use connector_sdk::{Column, ConnectorError, Result, Value};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    fn countries_schema() -> TableSchema {
        TableSchema::new(
            "countries",
            vec![Column {
                name: "code".into(),
                data_type: DataType::Text,
            }],
        )
    }

    #[test]
    fn build_demo_spec_defines_countries_table() {
        let spec = build_demo_spec("https://x/graphql");
        assert_eq!(spec.name, "countries");
        assert!(spec.table("countries").is_some());
    }

    #[test]
    fn render_table_produces_aligned_grid() {
        let schema = TableSchema::new(
            "countries",
            vec![
                Column {
                    name: "code".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "name".into(),
                    data_type: DataType::Text,
                },
            ],
        );
        let rows = vec![Row(vec![
            Value::Text("US".into()),
            Value::Text("United States".into()),
        ])];
        let out = render_table(&schema, &rows);
        assert!(out.contains("countries (1 rows)"));
        assert!(out.contains("United States"));
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
            schemas: vec![countries_schema()],
            fetch_err: true,
        };
        run_with(c).await.unwrap();
    }

    #[tokio::test]
    async fn run_with_ok_connector_prints_tables() {
        let c = FakeConnector {
            schemas: vec![countries_schema()],
            fetch_err: false,
        };
        run_with(c).await.unwrap();
    }

    #[tokio::test]
    async fn run_at_against_mock_server_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [
                    {"code": "US", "name": "United States", "emoji": "🇺🇸",
                     "continent": {"code": "NA", "name": "North America"}}
                ]}
            })))
            .mount(&server)
            .await;
        run_at(&format!("{}/graphql", server.uri())).await.unwrap();
    }

    #[test]
    fn sample_introspection_imports_to_countries_table() {
        let spec = GraphQlSpec::from_introspection_json(
            SAMPLE_INTROSPECTION,
            ImportOptions {
                endpoint: "https://x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(spec.table("countries").is_some());
    }

    #[tokio::test]
    async fn run_introspect_against_mock_server_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"countries": [
                    {"code": "IN", "name": "India", "emoji": "🇮🇳",
                     "continent": {"code": "AS", "name": "Asia"}}
                ]}
            })))
            .mount(&server)
            .await;
        run_introspect_at(&format!("{}/graphql", server.uri()))
            .await
            .unwrap();
    }
}
