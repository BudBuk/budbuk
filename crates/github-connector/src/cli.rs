//! Demo CLI for the GitHub connector. Reuses the rest-connector engine and its
//! `render_table`/`init_tracing` helpers — this crate adds no HTTP code at all.

use connector_sdk::{Connector, Filter, Operator, Query, Value};
use rest_connector::cli::{init_tracing, render_table};
use rest_connector::RestConnector;

use crate::{github_spec, GithubConfig};

/// Build a config from environment variables, defaulting to public
/// `octocat/Hello-World` so the demo runs with no credentials.
pub fn build_config_from_env() -> GithubConfig {
    GithubConfig {
        base_url: std::env::var("GITHUB_API_URL")
            .unwrap_or_else(|_| "https://api.github.com".to_string()),
        owner: std::env::var("GITHUB_OWNER").unwrap_or_else(|_| "octocat".to_string()),
        repo: std::env::var("GITHUB_REPO").unwrap_or_else(|_| "Hello-World".to_string()),
        token: std::env::var("GITHUB_TOKEN").ok(),
    }
}

/// Build the GitHub connector for `cfg` and run the demo.
pub async fn run_at(cfg: &GithubConfig) -> anyhow::Result<()> {
    init_tracing();
    run_with(RestConnector::new(github_spec(cfg))).await
}

/// Run the demo against any connector: discovery, a scan of each table, and a
/// predicate-pushdown example on `issues`.
pub async fn run_with<C: Connector>(connector: C) -> anyhow::Result<()> {
    println!("budbuk github connector: {}\n", connector.name());
    let schemas = connector.discover().await?;

    println!("=== all tables (limit 5) ===\n");
    for schema in &schemas {
        let query = Query {
            limit: Some(5),
            ..Default::default()
        };
        match connector.fetch(&schema.name, &query).await {
            Ok(rows) => println!("{}\n", render_table(schema, &rows)),
            Err(e) => println!("── {} ──\n  (skipped: {e})\n", schema.name),
        }
    }

    println!("=== pushdown: issues WHERE state = 'closed' ===");
    let query = Query {
        filters: vec![Filter::new(
            "state",
            Operator::Eq,
            Value::Text("closed".into()),
        )],
        limit: Some(5),
        ..Default::default()
    };
    if let Some(schema) = schemas.iter().find(|s| s.name == "issues") {
        match connector.fetch("issues", &query).await {
            Ok(rows) => println!("{}", render_table(schema, &rows)),
            Err(e) => println!("  (error: {e})"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use connector_sdk::{ConnectorError, Result, Row, TableSchema};
    use serde_json::json;
    use serial_test::serial;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FakeConnector {
        schemas: Vec<TableSchema>,
        fetch_err: bool,
    }

    #[async_trait]
    impl Connector for FakeConnector {
        fn name(&self) -> &str {
            "github"
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

    #[tokio::test]
    async fn run_with_failing_connector_handles_errors() {
        let c = FakeConnector {
            schemas: vec![TableSchema::new("issues", vec![])],
            fetch_err: true,
        };
        run_with(c).await.unwrap();
    }

    #[tokio::test]
    async fn run_with_without_issues_skips_pushdown() {
        let c = FakeConnector {
            schemas: vec![TableSchema::new("repos", vec![])],
            fetch_err: false,
        };
        run_with(c).await.unwrap();
    }

    #[test]
    #[serial]
    fn env_config_defaults_and_overrides() {
        for k in [
            "GITHUB_API_URL",
            "GITHUB_OWNER",
            "GITHUB_REPO",
            "GITHUB_TOKEN",
        ] {
            std::env::remove_var(k);
        }
        let cfg = build_config_from_env();
        assert_eq!(cfg.owner, "octocat");
        assert_eq!(cfg.repo, "Hello-World");
        assert!(cfg.token.is_none());

        std::env::set_var("GITHUB_OWNER", "rust-lang");
        std::env::set_var("GITHUB_TOKEN", "ghp_x");
        let cfg = build_config_from_env();
        assert_eq!(cfg.owner, "rust-lang");
        assert_eq!(cfg.token.as_deref(), Some("ghp_x"));
        std::env::remove_var("GITHUB_OWNER");
        std::env::remove_var("GITHUB_TOKEN");
    }

    /// Mount an array response for a GET path.
    async fn mount(server: &MockServer, p: &str) {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "number": 1, "name": "r", "state": "open",
                 "user": {"login": "octocat"}, "owner": {"login": "octocat"}}
            ])))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn run_at_against_mock_github_succeeds() {
        let server = MockServer::start().await;
        mount(&server, "/users/octocat/repos").await;
        mount(&server, "/repos/octocat/Hello-World/issues").await;
        mount(&server, "/users/octocat/gists").await;
        mount(&server, "/users/octocat/orgs").await;

        let cfg = GithubConfig {
            base_url: server.uri(),
            ..GithubConfig::public("octocat", "Hello-World")
        };
        run_at(&cfg).await.unwrap();
    }

    #[tokio::test]
    async fn token_is_sent_as_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/octocat/repos"))
            .and(header("authorization", "Bearer ghp_x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])))
            .mount(&server)
            .await;

        let cfg = GithubConfig {
            base_url: server.uri(),
            token: Some("ghp_x".to_string()),
            ..GithubConfig::public("octocat", "Hello-World")
        };
        let rows = RestConnector::new(github_spec(&cfg))
            .fetch(
                "repos",
                &Query {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
