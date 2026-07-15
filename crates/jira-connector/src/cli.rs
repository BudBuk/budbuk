//! CLI orchestration, factored out of `main.rs` so it can be unit-tested.
//!
//! `main.rs` stays a thin shell that just calls [`run`]; all real logic lives
//! here (and is covered by tests).

use std::time::Duration;

use connector_sdk::{
    Cache, CachedConnector, Connector, Filter, Operator, Query, Row, SortKey, TableSchema, Value,
};

use crate::{JiraConfig, JiraConnector};

/// How long a cached result stays fresh in the demo.
const DEMO_TTL: Duration = Duration::from_secs(300);
/// How long a stale result is still served (while refreshing) in the demo.
const DEMO_STALE_WINDOW: Duration = Duration::from_secs(60);

/// Build the base connector from environment variables (real mode) or fall back
/// to mock mode when they're absent. Returns the connector, its cache namespace
/// (account identity), and a human-readable mode label.
pub fn build_connector_from_env() -> (JiraConnector, String, &'static str) {
    let base_url = std::env::var("JIRA_BASE_URL").ok();
    let email = std::env::var("JIRA_USER_EMAIL").ok();
    let api_token = std::env::var("JIRA_API_TOKEN").ok();

    if let (Some(base_url), Some(email), Some(api_token)) = (base_url, email, api_token) {
        let namespace = base_url.clone();
        let config = JiraConfig {
            base_url,
            email,
            api_token,
            mock: false,
        };
        (JiraConnector::new(config), namespace, "REAL")
    } else {
        (JiraConnector::mock(), "mock".to_string(), "MOCK")
    }
}

/// Entry point used by `main`: build from env, then run the demo.
pub async fn run() -> anyhow::Result<()> {
    let (base, namespace, mode) = build_connector_from_env();
    run_with(base, namespace, mode).await
}

/// Run the demo against an already-built connector. Wraps it in a cache and
/// exercises schema discovery, caching, and predicate pushdown. Generic over
/// the connector so tests can inject mock and failing connectors.
pub async fn run_with<C: Connector + 'static>(
    base: C,
    namespace: String,
    mode: &str,
) -> anyhow::Result<()> {
    let connector =
        CachedConnector::new(base, Cache::new(), namespace, DEMO_TTL, DEMO_STALE_WINDOW);

    println!("budbuk — connector: {} [{} mode]\n", connector.name(), mode);

    let schemas = connector.discover().await?;

    println!("=== all tables (unfiltered, limit 3) ===\n");
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

    println!("=== pushdown demo: issues WHERE project = 'JHOC' ORDER BY created DESC ===");
    let filtered = Query {
        filters: vec![Filter::new(
            "project",
            Operator::Eq,
            Value::Text("JHOC".into()),
        )],
        sort: vec![SortKey {
            column: "created".into(),
            descending: true,
        }],
        projection: None,
        limit: Some(5),
    };
    if let Some(issues_schema) = schemas.iter().find(|s| s.name == "issues") {
        match connector.fetch("issues", &filtered).await {
            Ok(rows) => {
                println!("{}", render_table(issues_schema, &rows));
                let all_jhoc = rows.iter().all(|r| project_cell(r) == "JHOC");
                println!("\n  all rows have project=JHOC? {all_jhoc}");
            }
            Err(e) => println!("  (error: {e})"),
        }
    }

    Ok(())
}

/// The `project` column of an issues row (index 4), rendered as a string.
fn project_cell(row: &Row) -> String {
    row.0
        .get(4)
        .map(|v| v.to_display_string())
        .unwrap_or_default()
}

/// Render a table's header and rows as an aligned text grid. Pure — it returns
/// the string rather than printing, so it's trivial to unit-test.
pub fn render_table(schema: &TableSchema, rows: &[Row]) -> String {
    let headers: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();

    // Column widths = max of header and every cell in that column.
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

/// Render one row of cells, each padded to its column width.
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
    use connector_sdk::{Column, ConnectorError, DataType, Result};
    use serial_test::serial;

    /// One configurable fake connector, used to drive every `run_with` branch.
    /// Consolidating the scenarios into a single type means both method bodies
    /// are exercised (no dead code): `discover` succeeds or errors per flag,
    /// and `fetch` succeeds or errors per flag.
    struct FakeConnector {
        schemas: Vec<TableSchema>,
        discover_err: bool,
        fetch_err: bool,
    }

    /// A minimal 6-column `issues` schema for tests.
    fn issues_schema() -> TableSchema {
        TableSchema::new(
            "issues",
            vec![
                Column {
                    name: "key".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "summary".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "status".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "assignee".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "project".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "created".into(),
                    data_type: DataType::Timestamp,
                },
            ],
        )
    }

    /// Discovers `issues` but fails every fetch (drives the fetch error arms).
    fn fake_fetch_fails() -> FakeConnector {
        FakeConnector {
            schemas: vec![issues_schema()],
            discover_err: false,
            fetch_err: true,
        }
    }

    /// Exposes only a non-`issues` table, returning no rows (drives the "no
    /// issues schema" branch and the empty-table render path).
    fn fake_without_issues() -> FakeConnector {
        FakeConnector {
            schemas: vec![TableSchema::new(
                "projects",
                vec![Column {
                    name: "key".into(),
                    data_type: DataType::Text,
                }],
            )],
            discover_err: false,
            fetch_err: false,
        }
    }

    /// Fails discovery (drives the `?` error path).
    fn fake_discovery_fails() -> FakeConnector {
        FakeConnector {
            schemas: vec![],
            discover_err: true,
            fetch_err: false,
        }
    }

    #[async_trait]
    impl Connector for FakeConnector {
        fn name(&self) -> &str {
            "fake"
        }
        async fn discover(&self) -> Result<Vec<TableSchema>> {
            if self.discover_err {
                return Err(ConnectorError::Network("down".into()));
            }
            Ok(self.schemas.clone())
        }
        async fn fetch(&self, _table: &str, _query: &Query) -> Result<Vec<Row>> {
            if self.fetch_err {
                return Err(ConnectorError::Other("nope".into()));
            }
            Ok(vec![])
        }
    }

    #[test]
    fn render_table_produces_aligned_grid() {
        let schema = TableSchema::new(
            "issues",
            vec![
                Column {
                    name: "key".into(),
                    data_type: DataType::Text,
                },
                Column {
                    name: "n".into(),
                    data_type: DataType::Integer,
                },
            ],
        );
        let rows = vec![Row(vec![Value::Text("K-1".into()), Value::Integer(5)])];
        let out = render_table(&schema, &rows);
        assert!(out.contains("issues (1 rows)"));
        assert!(out.contains("| key"));
        assert!(out.contains("K-1"));
    }

    #[test]
    fn render_table_tolerates_rows_wider_than_schema() {
        // A row with more cells than there are columns exercises the defensive
        // width lookups (get_mut / get returning None).
        let schema = TableSchema::new(
            "t",
            vec![Column {
                name: "c".into(),
                data_type: DataType::Text,
            }],
        );
        let rows = vec![Row(vec![
            Value::Text("aa".into()),
            Value::Text("bb".into()),
        ])];
        let out = render_table(&schema, &rows);
        assert!(out.contains("aa"));
        assert!(out.contains("bb"));
    }

    #[test]
    fn project_cell_reads_index_four_or_empty() {
        let full = Row(vec![
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Text("ENG".into()),
        ]);
        assert_eq!(project_cell(&full), "ENG");
        assert_eq!(project_cell(&Row(vec![])), "");
    }

    #[tokio::test]
    async fn run_with_mock_succeeds() {
        run_with(JiraConnector::mock(), "mock".to_string(), "MOCK")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_with_failing_connector_handles_errors_gracefully() {
        // fetch errors are caught per-table, so the run still completes Ok.
        run_with(fake_fetch_fails(), "ns".to_string(), "TEST")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_with_connector_without_issues_skips_pushdown_demo() {
        // No `issues` schema → the pushdown block is skipped; empty rows render.
        run_with(fake_without_issues(), "ns".to_string(), "TEST")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_with_propagates_discovery_error() {
        let err = run_with(fake_discovery_fails(), "ns".to_string(), "TEST")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("down"));
    }

    #[test]
    #[serial]
    fn build_connector_from_env_real_mode() {
        std::env::set_var("JIRA_BASE_URL", "https://x.atlassian.net");
        std::env::set_var("JIRA_USER_EMAIL", "a@b.c");
        std::env::set_var("JIRA_API_TOKEN", "tok");
        let (_, namespace, mode) = build_connector_from_env();
        assert_eq!(namespace, "https://x.atlassian.net");
        assert_eq!(mode, "REAL");
        std::env::remove_var("JIRA_BASE_URL");
        std::env::remove_var("JIRA_USER_EMAIL");
        std::env::remove_var("JIRA_API_TOKEN");
    }

    #[test]
    #[serial]
    fn build_connector_from_env_mock_mode() {
        std::env::remove_var("JIRA_BASE_URL");
        std::env::remove_var("JIRA_USER_EMAIL");
        std::env::remove_var("JIRA_API_TOKEN");
        let (_, namespace, mode) = build_connector_from_env();
        assert_eq!(namespace, "mock");
        assert_eq!(mode, "MOCK");
    }

    #[tokio::test]
    #[serial]
    async fn run_falls_back_to_mock_without_env() {
        std::env::remove_var("JIRA_BASE_URL");
        std::env::remove_var("JIRA_USER_EMAIL");
        std::env::remove_var("JIRA_API_TOKEN");
        run().await.unwrap();
    }
}
