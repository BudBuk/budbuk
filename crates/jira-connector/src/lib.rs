//! # jira-connector
//!
//! The Jira implementation of the `Connector` trait. One `JiraConnector` value
//! represents ONE Jira account (one "instance"); create several for several
//! accounts, each with its own `JiraConfig`.

pub mod cli;
pub mod client;
pub mod jql;
pub mod mock;

use async_trait::async_trait;
use connector_sdk::{Connector, ConnectorError, Query, Result, Row, TableSchema};

/// JQL for worklogs: scope to issues logged against recently, so we actually
/// find worklogs to show.
const DEFAULT_WORKLOGS_JQL: &str = "worklogDate >= -30d order by updated DESC";

/// Default row cap when the caller didn't specify a LIMIT.
const DEFAULT_LIMIT: usize = 50;

/// Configuration for a single Jira account. This is the per-instance data that
/// keeps the connector *code* account-agnostic: the same code serves any
/// account by receiving a different `JiraConfig`.
#[derive(Debug, Clone)]
pub struct JiraConfig {
    /// e.g. `https://your-domain.atlassian.net`
    pub base_url: String,
    /// The account email used with the API token (Jira Cloud basic auth).
    pub email: String,
    /// The API token (a secret — we'll harden how this is stored later).
    pub api_token: String,
    /// When true, serve canned sample data instead of calling the real API.
    pub mock: bool,
}

/// The Jira connector. Holds one account's config plus its HTTP client.
pub struct JiraConnector {
    config: JiraConfig,
    client: client::JiraClient,
}

impl JiraConnector {
    /// Create a connector for a real account.
    pub fn new(config: JiraConfig) -> Self {
        // Build the HTTP client from this account's credentials. It's unused in
        // mock mode, but constructing it is cheap.
        let client = client::JiraClient::new(
            config.base_url.clone(),
            config.email.clone(),
            config.api_token.clone(),
        );
        Self { config, client }
    }

    /// Create a connector that serves canned sample data — no credentials
    /// needed. Great for learning and tests.
    pub fn mock() -> Self {
        Self::new(JiraConfig {
            base_url: String::new(),
            email: String::new(),
            api_token: String::new(),
            mock: true,
        })
    }
}

// Implement the framework's `Connector` trait for our type. Once this block
// exists, the whole framework can drive `JiraConnector` like any other source.
#[async_trait]
impl Connector for JiraConnector {
    fn name(&self) -> &str {
        "jira"
    }

    async fn discover(&self) -> Result<Vec<TableSchema>> {
        // Mock mode returns a fixed schema. With a real account (next step)
        // we'll confirm/augment this by querying Jira's field metadata.
        Ok(mock::schemas())
    }

    async fn fetch(&self, table: &str, query: &Query) -> Result<Vec<Row>> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);

        // Mock mode: serve canned rows, honoring LIMIT.
        if self.config.mock {
            let mut rows = mock::rows_for(table)
                .ok_or_else(|| ConnectorError::UnknownTable(table.to_string()))?;
            rows.truncate(limit);
            return Ok(rows);
        }

        // Real mode: dispatch to the matching API call. Each arm `.await`s an
        // async HTTP request and returns neutral rows.
        match table {
            "projects" => self.client.projects(limit).await,
            "issues" => {
                // PUSHDOWN: build JQL from the query's filters + sort so Jira
                // does the filtering server-side instead of us over-fetching.
                let built = jql::build_issues_jql(&query.filters, &query.sort);
                tracing::debug!(target: "budbuk::jira", jql = %built.jql, "issues predicate pushdown");
                self.client.issues(&built.jql, limit).await
            }
            "users" => self.client.users(limit).await,
            "worklogs" => self.client.worklogs(DEFAULT_WORKLOGS_JQL, limit).await,
            other => Err(ConnectorError::UnknownTable(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_connector_reports_name_and_discovers_four_tables() {
        let c = JiraConnector::mock();
        assert_eq!(c.name(), "jira");
        let schemas = c.discover().await.unwrap();
        assert_eq!(schemas.len(), 4);
    }

    #[tokio::test]
    async fn mock_fetch_returns_rows_for_each_table() {
        let c = JiraConnector::mock();
        for table in ["projects", "issues", "users", "worklogs"] {
            let rows = c.fetch(table, &Query::default()).await.unwrap();
            assert!(!rows.is_empty(), "expected rows for {table}");
        }
    }

    #[tokio::test]
    async fn mock_fetch_honors_limit() {
        let c = JiraConnector::mock();
        let q = Query {
            limit: Some(1),
            ..Default::default()
        };
        let rows = c.fetch("issues", &q).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn mock_fetch_unknown_table_errors() {
        let c = JiraConnector::mock();
        let err = c.fetch("widgets", &Query::default()).await.unwrap_err();
        assert!(matches!(err, ConnectorError::UnknownTable(t) if t == "widgets"));
    }

    #[test]
    fn config_is_debuggable_and_cloneable() {
        let cfg = JiraConfig {
            base_url: "https://x.atlassian.net".into(),
            email: "a@b.c".into(),
            api_token: "secret".into(),
            mock: false,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.base_url, "https://x.atlassian.net");
        assert!(format!("{cfg:?}").contains("atlassian"));
        // Constructing via `new` builds the HTTP client without calling out.
        let _ = JiraConnector::new(cfg);
    }
}
